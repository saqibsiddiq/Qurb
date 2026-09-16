//! The metadata index: SQLite in WAL mode.
//!
//! Holds paths, chunk lists, and chunk reference counts. Chunk *payloads* live
//! in the [`crate::cas`] store on the filesystem; see
//! ../../docs/decisions/0003-sqlite-plus-cas.md for why the two are separate.
//!
//! # Reference counting
//!
//! Deduplication means one chunk may belong to many files, so a chunk can only
//! be deleted once nothing points at it. That count is maintained by database
//! triggers rather than by Rust code, deliberately: a trigger fires as part of
//! the same transaction as the row change, so the count cannot drift because a
//! code path forgot to decrement. [`Db::audit_refcounts`] re-derives the counts
//! from scratch and is used by tests and verification to prove that holds.

use crate::error::{Error, Result};
use qurb_sync::{Content, DeviceId, FileVersion, VersionVector};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// Schema migrations, applied in order. `user_version` records how many have
/// run, so an existing database picks up only what it is missing.
///
/// Migrations are append-only. Editing one that has already shipped would leave
/// databases in the field at a schema nobody can reproduce.
const MIGRATIONS: &[&str] = &[V1, V2];

const V1: &str = r#"
CREATE TABLE IF NOT EXISTS chunks (
    hash            BLOB PRIMARY KEY,
    size            INTEGER NOT NULL,   -- plaintext bytes
    stored_size     INTEGER NOT NULL,   -- bytes on disk after compress + encrypt
    refcount        INTEGER NOT NULL DEFAULT 0,
    unreferenced_at INTEGER,            -- unix seconds when refcount last hit 0
    created_at      INTEGER NOT NULL
) STRICT;

-- Garbage collection scans by this; without the index it degrades to a full
-- table scan on a table with one row per chunk in the library.
CREATE INDEX IF NOT EXISTS idx_chunks_collectable
    ON chunks (unreferenced_at) WHERE refcount = 0;

CREATE TABLE IF NOT EXISTS files (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL UNIQUE,
    size         INTEGER NOT NULL,
    content_hash BLOB NOT NULL,
    mtime_ns     INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER              -- tombstone; NULL means live
) STRICT;

CREATE INDEX IF NOT EXISTS idx_files_deleted ON files (deleted_at)
    WHERE deleted_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS file_chunks (
    file_id    INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    seq        INTEGER NOT NULL,
    chunk_hash BLOB NOT NULL REFERENCES chunks (hash),
    PRIMARY KEY (file_id, seq)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_file_chunks_hash ON file_chunks (chunk_hash);

-- The two triggers below are the entire reference counting mechanism.
CREATE TRIGGER IF NOT EXISTS file_chunks_after_insert
AFTER INSERT ON file_chunks BEGIN
    UPDATE chunks
       SET refcount = refcount + 1,
           unreferenced_at = NULL
     WHERE hash = NEW.chunk_hash;
END;

CREATE TRIGGER IF NOT EXISTS file_chunks_after_delete
AFTER DELETE ON file_chunks BEGIN
    UPDATE chunks
       SET refcount = refcount - 1,
           -- Start the retention clock the moment the last reference goes, so
           -- a chunk stays recoverable for a window after the file that used
           -- it was deleted or rewritten.
           unreferenced_at = CASE WHEN refcount - 1 <= 0
                                  THEN unixepoch()
                                  ELSE NULL END
     WHERE hash = OLD.chunk_hash;
END;

-- Rewriting which chunk a row points at would silently skew both counts. It is
-- never a legitimate operation: chunk lists are replaced wholesale.
CREATE TRIGGER IF NOT EXISTS file_chunks_no_hash_update
BEFORE UPDATE OF chunk_hash ON file_chunks BEGIN
    SELECT RAISE(ABORT, 'file_chunks.chunk_hash is immutable; delete and reinsert');
END;
"#;

/// Version vectors, and this device's identity.
///
/// Until now the index described one machine's files. These columns are what
/// let it describe a *history* that another device can compare against.
const V2: &str = r#"
-- An empty vector decodes to "no device has changed this", which is the right
-- reading for rows written before this column existed.
ALTER TABLE files ADD COLUMN vector BLOB NOT NULL DEFAULT x'';
ALTER TABLE files ADD COLUMN modified_by BLOB;

-- This device's identity, and the counter it stamps onto its own changes.
-- One row, enforced.
CREATE TABLE IF NOT EXISTS local (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    device_id BLOB NOT NULL,
    counter   INTEGER NOT NULL DEFAULT 0
) STRICT;

-- Finding content we already hold under some other name, so adopting a peer's
-- version of a file we already have costs a lookup instead of a transfer.
CREATE INDEX IF NOT EXISTS idx_files_content ON files (content_hash)
    WHERE deleted_at IS NULL;
"#;

pub struct Db {
    conn: Connection,
}

/// A row from the `chunks` table.
#[derive(Debug, Clone)]
pub struct ChunkRow {
    pub hash: blake3::Hash,
    pub size: u64,
    pub stored_size: u64,
    pub refcount: i64,
    pub unreferenced_at: Option<i64>,
}

/// A logical file as the index sees it.
#[derive(Debug, Clone)]
pub struct FileRow {
    pub id: i64,
    pub path: String,
    pub size: u64,
    pub content_hash: blake3::Hash,
    pub mtime_ns: i64,
    pub deleted_at: Option<i64>,
    /// What this version has seen. Empty for rows predating the column.
    pub vector: VersionVector,
    /// Which device last changed it, if known.
    pub modified_by: Option<DeviceId>,
    pub updated_at: i64,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // WAL lets readers proceed during a write. NORMAL synchronous is the
        // documented safe pairing with WAL: durable against process crash,
        // and against power loss it can lose only the most recent commits,
        // which the sync engine can recover from peers.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Required for the file_chunks -> files cascade, and off by default.
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // SQLite permits one writer at a time, and this system genuinely has
        // several: the engine writing, the peer server reading, and garbage
        // collection taking the write lock for its deletions. Without a busy
        // timeout the loser of a race gets an immediate "database is locked"
        // rather than waiting its turn.
        //
        // Set explicitly rather than left to the library's default, because the
        // behaviour under contention is a design decision and should be visible
        // as one.
        conn.busy_timeout(std::time::Duration::from_secs(10))?;

        let applied: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, migration) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
            conn.execute_batch(migration)?;
            conn.pragma_update(None, "user_version", (i + 1) as i64)?;
        }

        let db = Self { conn };
        db.ensure_local_identity()?;
        Ok(db)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    // -- chunks --------------------------------------------------------------

    pub fn chunk(&self, hash: &blake3::Hash) -> Result<Option<ChunkRow>> {
        self.conn
            .query_row(
                "SELECT hash, size, stored_size, refcount, unreferenced_at
                   FROM chunks WHERE hash = ?1",
                params![hash.as_bytes().as_slice()],
                chunk_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn has_chunk(&self, hash: &blake3::Hash) -> Result<bool> {
        Ok(self.chunk(hash)?.is_some())
    }

    /// Record a chunk that has just been written to the store.
    ///
    /// Inserted with `refcount = 0` and the retention clock already running, so
    /// that a crash between writing the payload and linking it to a file leaves
    /// a chunk that garbage collection will eventually reclaim rather than an
    /// orphan that lives forever.
    pub fn insert_chunk(&self, hash: &blake3::Hash, size: u64, stored_size: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO chunks (hash, size, stored_size, refcount, unreferenced_at, created_at)
             VALUES (?1, ?2, ?3, 0, unixepoch(), unixepoch())
             ON CONFLICT (hash) DO NOTHING",
            params![hash.as_bytes().as_slice(), size as i64, stored_size as i64],
        )?;
        Ok(())
    }

    pub fn chunk_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM chunks", [], |r| r.get(0))?)
    }

    /// Total plaintext and on-disk bytes across all known chunks.
    pub fn size_totals(&self) -> Result<(u64, u64)> {
        let (a, b): (i64, i64) = self.conn.query_row(
            "SELECT coalesce(sum(size), 0), coalesce(sum(stored_size), 0) FROM chunks",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((a as u64, b as u64))
    }

    // -- files ---------------------------------------------------------------

    pub fn file_by_path(&self, path: &str) -> Result<Option<FileRow>> {
        self.conn
            .query_row(
                &format!("SELECT {FILE_COLUMNS} FROM files WHERE path = ?1"),
                params![path],
                file_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn chunk_hashes_for(&self, file_id: i64) -> Result<Vec<blake3::Hash>> {
        let mut stmt = self
            .conn
            .prepare("SELECT chunk_hash FROM file_chunks WHERE file_id = ?1 ORDER BY seq")?;
        let rows = stmt.query_map(params![file_id], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(to_hash(&raw))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    pub fn live_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM files WHERE deleted_at IS NULL ORDER BY path")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Live paths at or beneath `prefix`, treated as a directory.
    ///
    /// A filesystem watcher reporting a removal cannot say whether the path was
    /// a file or a directory — it is gone either way. The index is what
    /// remembers, so removal is resolved by asking what used to live there.
    ///
    /// The trailing separator matters: a prefix of `docs` must match
    /// `docs/notes.txt` but not `docstring.md`.
    pub fn live_paths_under(&self, prefix: &str) -> Result<Vec<String>> {
        let pattern = format!("{}/%", prefix.trim_end_matches('/'));
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE deleted_at IS NULL AND (path = ?1 OR path LIKE ?2 ESCAPE '\\')
              ORDER BY path",
        )?;
        let rows = stmt.query_map(rusqlite::params![prefix, escape_like(&pattern)], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    // -- local identity and version vectors ----------------------------------

    /// Create this device's identity if it does not already have one.
    fn ensure_local_identity(&self) -> Result<()> {
        use rand::RngCore;
        let mut id = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut id);
        self.conn.execute(
            "INSERT INTO local (id, device_id, counter) VALUES (1, ?1, 0)
             ON CONFLICT (id) DO NOTHING",
            params![id.as_slice()],
        )?;
        Ok(())
    }

    /// This device's identity.
    ///
    /// Generated once, on first open, and stable for the life of the store. It
    /// is what every version vector counts against, so it must never change:
    /// a new identity would make every existing version look like it came from
    /// a device nobody has heard of.
    pub fn local_device(&self) -> Result<DeviceId> {
        let raw: Vec<u8> = self
            .conn
            .query_row("SELECT device_id FROM local WHERE id = 1", [], |r| r.get(0))?;
        to_device(raw).ok_or_else(|| Error::Corrupt {
            detail: "local device id is not 32 bytes".into(),
        })
    }

    /// Allocate the next counter for a local change.
    ///
    /// Monotonic across the whole store rather than per file. A per-file
    /// counter would be smaller, but this one doubles as a cheap "has anything
    /// changed here since you last asked" for a peer.
    pub fn next_counter(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "UPDATE local SET counter = counter + 1 WHERE id = 1 RETURNING counter",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// The version of one path, tombstones included.
    pub fn version(&self, path: &str) -> Result<Option<FileVersion>> {
        Ok(self.file_by_path(path)?.map(|row| row_to_version(&row)))
    }

    /// Every path this device knows about, tombstones included.
    ///
    /// Tombstones are part of the answer, not noise. A peer that is not told
    /// about a deletion still has the file, offers it back, and the deletion
    /// undoes itself.
    pub fn all_versions(&self) -> Result<Vec<FileVersion>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {FILE_COLUMNS} FROM files ORDER BY path"))?;
        let rows = stmt.query_map([], file_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row_to_version(&row?));
        }
        Ok(out)
    }

    /// Stamp a path with a version vector.
    ///
    /// Used both for local changes, where the caller has just allocated a
    /// counter, and for versions adopted from a peer, where the vector is
    /// taken verbatim.
    pub fn set_version(
        &self,
        path: &str,
        vector: &VersionVector,
        modified_by: &DeviceId,
        modified_at: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET vector = ?2, modified_by = ?3, updated_at = ?4 WHERE path = ?1",
            params![path, vector.encode(), modified_by.as_bytes().as_slice(), modified_at],
        )?;
        Ok(())
    }

    /// The vector a local change to `path` should carry.
    ///
    /// Takes whatever the path already knew and advances this device's entry,
    /// so the new version supersedes the old one instead of being concurrent
    /// with it.
    pub fn next_local_vector(&self, path: &str) -> Result<(VersionVector, DeviceId)> {
        let device = self.local_device()?;
        let mut vector = self
            .file_by_path(path)?
            .map(|row| row.vector)
            .unwrap_or_default();
        vector.set(device, self.next_counter()?);
        Ok((vector, device))
    }

    /// A live path holding exactly this content, if there is one.
    pub fn live_path_with_content(&self, hash: &blake3::Hash) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT path FROM files
                  WHERE content_hash = ?1 AND deleted_at IS NULL LIMIT 1",
                params![hash.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Any path whose chunk list describes this content, tombstoned included.
    ///
    /// "Do we have these bytes?" is a question about chunks, not about names. A
    /// deleted file's chunks survive the retention window, so a path being gone
    /// does not mean its content is.
    ///
    /// This matters more than it sounds. Renaming a directory arrives as a set
    /// of additions and a set of deletions, applied in path order — so whether
    /// the old path is still live when the new one is written depends on how the
    /// two names happen to sort. Asking only about live paths made a rename to
    /// an alphabetically later name re-transfer the entire tree, and a rename to
    /// an earlier one free.
    ///
    /// Live paths are preferred, so the common case still reads a file that is
    /// certainly intact.
    pub fn any_path_with_content(&self, hash: &blake3::Hash) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT path FROM files
                  WHERE content_hash = ?1
                  ORDER BY (deleted_at IS NULL) DESC
                  LIMIT 1",
                params![hash.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// A different live path that a case-insensitive filesystem could not keep
    /// apart from `path`.
    ///
    /// Folding uses SQLite's `lower()`, which is ASCII-only. That misses
    /// collisions in other scripts — Turkish dotted I, for one — and catches the
    /// cases that actually occur in practice. A full Unicode fold belongs with
    /// proper normalisation work, which this is not.
    pub fn live_path_colliding_with(&self, path: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT path FROM files
                  WHERE deleted_at IS NULL AND lower(path) = lower(?1) AND path <> ?1
                  LIMIT 1",
                params![path],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Live paths whose content includes this chunk.
    ///
    /// The question repair asks: a chunk has been found damaged, so which files
    /// stopped being readable because of it?
    pub fn live_paths_using_chunk(&self, hash: &blake3::Hash) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT f.path
               FROM files f
               JOIN file_chunks fc ON fc.file_id = f.id
              WHERE fc.chunk_hash = ?1 AND f.deleted_at IS NULL
              ORDER BY f.path",
        )?;
        let rows = stmt.query_map(params![hash.as_bytes().as_slice()], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    // -- integrity -----------------------------------------------------------

    /// Re-derive every reference count from `file_chunks` and report any chunk
    /// whose stored count disagrees.
    ///
    /// The triggers should make this impossible. That is exactly why it is
    /// worth checking: the cost of the mechanism being subtly wrong is silent
    /// data loss, so the invariant is tested rather than assumed.
    pub fn audit_refcounts(&self) -> Result<Vec<RefcountDrift>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.hash, c.refcount,
                    (SELECT count(*) FROM file_chunks fc WHERE fc.chunk_hash = c.hash)
               FROM chunks c
              WHERE c.refcount <> (SELECT count(*) FROM file_chunks fc
                                    WHERE fc.chunk_hash = c.hash)",
        )?;
        let rows = stmt.query_map([], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(RefcountDrift { hash: to_hash(&raw), stored: r.get(1)?, actual: r.get(2)? })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
pub struct RefcountDrift {
    pub hash: blake3::Hash,
    pub stored: i64,
    pub actual: i64,
}

fn chunk_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChunkRow> {
    let raw: Vec<u8> = r.get(0)?;
    Ok(ChunkRow {
        hash: to_hash(&raw),
        size: r.get::<_, i64>(1)? as u64,
        stored_size: r.get::<_, i64>(2)? as u64,
        refcount: r.get(3)?,
        unreferenced_at: r.get(4)?,
    })
}

fn file_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileRow> {
    let raw: Vec<u8> = r.get(3)?;
    let encoded: Vec<u8> = r.get(6)?;
    let modified_by: Option<Vec<u8>> = r.get(7)?;
    Ok(FileRow {
        id: r.get(0)?,
        path: r.get(1)?,
        size: r.get::<_, i64>(2)? as u64,
        content_hash: to_hash(&raw),
        mtime_ns: r.get(4)?,
        deleted_at: r.get(5)?,
        // A vector that will not decode means the row was written by something
        // that is not this code. Treating it as empty is the conservative
        // reading: the version looks maximally old, so it loses to anything
        // real rather than silently winning.
        vector: VersionVector::decode(&encoded).unwrap_or_default(),
        modified_by: modified_by.and_then(to_device),
        updated_at: r.get(8)?,
    })
}

fn to_device(raw: Vec<u8>) -> Option<DeviceId> {
    let bytes: [u8; 32] = raw.try_into().ok()?;
    Some(DeviceId::from_bytes(bytes))
}

const FILE_COLUMNS: &str =
    "id, path, size, content_hash, mtime_ns, deleted_at, vector, modified_by, updated_at";

fn row_to_version(row: &FileRow) -> FileVersion {
    let content = if row.deleted_at.is_some() {
        Content::Deleted
    } else {
        Content::File { hash: *row.content_hash.as_bytes(), size: row.size }
    };
    FileVersion {
        path: row.path.clone(),
        content,
        vector: row.vector.clone(),
        // A row written before vectors existed has no recorded author. Naming
        // this device would be a lie; the all-zero id reads as "unknown", and
        // the only thing it affects is a conflict filename.
        modified_by: row.modified_by.unwrap_or(DeviceId::from_bytes([0; 32])),
        modified_at: row.updated_at,
    }
}

/// Rebuild a hash from its 32-byte database representation.
///
/// A short row means the database was written by something that is not this
/// code, which is not a condition we can sensibly continue from.
pub(crate) fn to_hash(raw: &[u8]) -> blake3::Hash {
    let mut bytes = [0u8; 32];
    assert_eq!(raw.len(), 32, "chunk hash column must hold exactly 32 bytes");
    bytes.copy_from_slice(raw);
    blake3::Hash::from(bytes)
}

/// Escape LIKE's wildcards so a path containing `%` or `_` matches literally.
///
/// Without this, a directory named `100%` would match far more than itself.
fn escape_like(pattern: &str) -> String {
    // The trailing "/%" added by the caller is the intended wildcard, so
    // escape the body and re-attach it.
    let (body, suffix) = match pattern.strip_suffix("/%") {
        Some(body) => (body, "/%"),
        None => (pattern, ""),
    };
    let escaped: String = body
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            other => vec![other],
        })
        .collect();
    format!("{escaped}{suffix}")
}

/// Seconds since the unix epoch, as the database records them.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[allow(dead_code)]
fn _assert_error_conversion(e: rusqlite::Error) -> Error {
    e.into()
}
