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
const MIGRATIONS: &[&str] = &[V1, V2, V3, V4, V5];

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

/// The devices this one trusts.
///
/// Until now `DeviceId` and the network fingerprint were unrelated: version
/// vectors counted against one, connections authenticated the other, and
/// nothing tied them together. A device could authenticate as itself and then
/// claim any history it liked.
///
/// This table is the binding. A row says: the device whose certificate hashes
/// to this fingerprint is the one whose changes count under this device id.
/// Rows are written only by pairing, which requires an out-of-band exchange.
const V3: &str = r#"
CREATE TABLE IF NOT EXISTS peers (
    device_id   BLOB PRIMARY KEY,
    fingerprint BLOB NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    paired_at   INTEGER NOT NULL,
    last_seen   INTEGER
) STRICT;

CREATE INDEX IF NOT EXISTS idx_peers_fingerprint ON peers (fingerprint);
"#;

const V4: &str = r#"
-- Whether this device is holding the file's bytes, or only knows about it.
--
-- A storage cap frees space by deleting the file from the folder and keeping
-- the index entry. Without this column the next scan would find the file gone
-- and tombstone it -- turning "I am short of space" into "the user deleted it"
-- and propagating that to every other device. The whole cap rests on this
-- distinction.
ALTER TABLE files ADD COLUMN materialised INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS idx_files_evicted ON files (materialised)
    WHERE materialised = 0 AND deleted_at IS NULL;

-- Which other devices are known to have taken delivery of which content.
--
-- Evicting the only copy of a file destroys it. This device may only drop
-- content it has watched another device receive: a row here is written when a
-- transfer of that exact content hash completed, in either direction. No row
-- means no eviction, which is the safe default for a device that has never
-- synced.
--
-- Keyed by content hash rather than by path, because the question at eviction
-- time is "do these bytes exist elsewhere", and a renamed file is the same
-- bytes.
CREATE TABLE IF NOT EXISTS replicas (
    content_hash BLOB NOT NULL,
    device_id    BLOB NOT NULL,
    at           INTEGER NOT NULL,
    PRIMARY KEY (content_hash, device_id)
) STRICT;

-- When the file was last read or written here. Eviction takes the coldest
-- first, and a file nobody has opened is the cheapest one to lose.
ALTER TABLE files ADD COLUMN touched_at INTEGER NOT NULL DEFAULT 0;
"#;

const V5: &str = r#"
-- A standing request to have this file's bytes back.
--
-- `qurb fetch` runs in a different process from the daemon that does the
-- fetching, and the daemon may not even be running -- or may have no peer
-- reachable -- at the moment the person asks. Writing the request to the index
-- rather than sending it anywhere means it survives both: whenever a peer next
-- becomes reachable, the file comes back without being asked again.
ALTER TABLE files ADD COLUMN wanted INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_files_wanted ON files (wanted)
    WHERE wanted = 1 AND deleted_at IS NULL;
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

    /// Where a chunk's bytes can be found inside a live file.
    ///
    /// Returns the logical path, the byte offset within it, and the length —
    /// enough to read the plaintext straight from the file the user can see,
    /// without keeping a second encrypted copy of it.
    ///
    /// The offset is not stored; it is the running sum of the sizes of the
    /// chunks before this one in the same file, which the chunk list already
    /// determines. Scoped to a single holder first, so the window function runs
    /// over one file's chunks rather than every chunk in the index.
    ///
    /// `None` when no live file provides it — the content was deleted, or this
    /// device never materialised it (a replica holds chunks and no tree).
    pub fn locate_chunk(&self, hash: &blake3::Hash) -> Result<Option<(String, u64, u64)>> {
        let found = self.conn.query_row(
            "WITH holder AS (
                 SELECT fc.file_id AS id
                   FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                  WHERE fc.chunk_hash = ?1 AND f.deleted_at IS NULL AND f.materialised = 1
                  LIMIT 1
             ),
             laid_out AS (
                 SELECT fc.chunk_hash,
                        c.size,
                        COALESCE(
                            SUM(c.size) OVER (ORDER BY fc.seq
                                              ROWS BETWEEN UNBOUNDED PRECEDING
                                                       AND 1 PRECEDING),
                            0) AS offset
                   FROM file_chunks fc
                   JOIN chunks c ON c.hash = fc.chunk_hash
                  WHERE fc.file_id = (SELECT id FROM holder)
             )
             SELECT (SELECT path FROM files WHERE id = (SELECT id FROM holder)),
                    l.offset,
                    l.size
               FROM laid_out l
              WHERE l.chunk_hash = ?1
              LIMIT 1",
            params![hash.as_bytes().as_slice()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64)),
        );

        match found {
            Ok(located) => Ok(Some(located)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Total size of every live file, as the user would count it.
    ///
    /// Distinct from the plaintext total in [`size_totals`](Self::size_totals),
    /// which sums *chunks* and therefore counts shared content once. Three
    /// copies of one file are 3x here and 1x there, and the gap between the two
    /// is exactly what deduplication saved.
    pub fn live_bytes(&self) -> Result<u64> {
        let total: i64 = self.conn.query_row(
            "SELECT coalesce(sum(size), 0) FROM files WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(total as u64)
    }

    /// Record that `device` is known to hold these bytes.
    ///
    /// Written when a transfer of this content completes, in either direction:
    /// a peer that fetched it from us has it, and a peer we fetched it from had
    /// it. This is what makes eviction safe -- see [`Db::evictable`].
    pub fn note_replica(&self, content: &blake3::Hash, device: &DeviceId) -> Result<()> {
        self.conn.execute(
            "INSERT INTO replicas (content_hash, device_id, at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT (content_hash, device_id) DO UPDATE SET at = excluded.at",
            params![content.as_bytes().as_slice(), device.as_bytes().as_slice()],
        )?;
        Ok(())
    }

    /// How many other devices are known to hold these bytes.
    pub fn replica_count(&self, content: &blake3::Hash) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT count(*) FROM replicas WHERE content_hash = ?1",
            params![content.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Note that a path was just read or written here.
    pub fn touch(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET touched_at = unixepoch() WHERE path = ?1",
            params![path],
        )?;
        Ok(())
    }

    /// Live files this device could drop the bytes of, coldest first.
    ///
    /// Three conditions, all of them necessary:
    ///
    /// - **Materialised.** There is nothing to free in a file already evicted.
    /// - **Known to be elsewhere.** At least one other device has taken
    ///   delivery of this exact content. Without that this is the only copy,
    ///   and dropping it is not eviction but deletion.
    /// - **Not a tombstone.** Deleted files are the garbage collector's
    ///   problem, not the cap's.
    ///
    /// Ordered by last touch, oldest first, then by size largest first so that
    /// among equally cold files the one that frees the most goes first.
    pub fn evictable(&self) -> Result<Vec<(String, u64, blake3::Hash)>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.size, f.content_hash
               FROM files f
              WHERE f.deleted_at IS NULL
                AND f.materialised = 1
                AND EXISTS (SELECT 1 FROM replicas r WHERE r.content_hash = f.content_hash)
              ORDER BY f.touched_at ASC, f.size DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let raw: Vec<u8> = r.get(2)?;
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, to_hash(&raw)))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Mark a path as held or not held here. Returns whether anything changed.
    pub fn set_materialised(&self, path: &str, held: bool) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE files SET materialised = ?2 WHERE path = ?1 AND deleted_at IS NULL",
            params![path, held as i64],
        )?;
        Ok(changed > 0)
    }

    /// Whether this device holds the bytes of a live path.
    ///
    /// `None` when the path is not live here at all, which is a different
    /// question from "evicted" and must not be confused with it.
    pub fn is_materialised(&self, path: &str) -> Result<Option<bool>> {
        self.conn
            .query_row(
                "SELECT materialised FROM files WHERE path = ?1 AND deleted_at IS NULL",
                params![path],
                |r| Ok(r.get::<_, i64>(0)? != 0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Live paths whose bytes this device has dropped.
    pub fn evicted_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE deleted_at IS NULL AND materialised = 0
              ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Bytes of live files this device is actually holding.
    pub fn materialised_bytes(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT coalesce(sum(size), 0) FROM files
              WHERE deleted_at IS NULL AND materialised = 1",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Ask for a file's bytes back. Acted on the next time a peer is reachable.
    pub fn want(&self, path: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE files SET wanted = 1 WHERE path = ?1 AND deleted_at IS NULL",
            params![path],
        )?;
        Ok(changed > 0)
    }

    /// Paths asked for that this device is not holding yet.
    pub fn wanted_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE wanted = 1 AND materialised = 0 AND deleted_at IS NULL
              ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Live files this device made that no other device is known to hold.
    ///
    /// The honest answer to "has my photo reached the desktop yet". A file
    /// counts as outstanding while this device is the only known holder of its
    /// content — which is exactly the condition under which losing this device
    /// would lose the file.
    ///
    /// Restricted to content this device *made*. A file received from
    /// somewhere else is not this device's to deliver, and counting it would
    /// make a phone that has merely not finished downloading look like a phone
    /// with a backlog to push.
    ///
    /// Newest first, because that is the order someone recognises: the thing
    /// they just shared is the thing they are asking about.
    pub fn undelivered(&self) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.size
               FROM files f
              WHERE f.deleted_at IS NULL
                AND f.modified_by = (SELECT device_id FROM local WHERE id = 1)
                AND NOT EXISTS (
                      SELECT 1 FROM replicas r WHERE r.content_hash = f.content_hash
                    )
              ORDER BY f.updated_at DESC, f.path",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
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

    // -- trusted peers -------------------------------------------------------

    /// Record a device as trusted.
    ///
    /// Binds a device id to a network fingerprint. Both are unique: one device
    /// cannot hold two identities, and one identity cannot serve two devices.
    /// Re-pairing an already-known device updates its name and fingerprint
    /// rather than creating a second row, so replacing a device's certificate
    /// does not leave a stale identity trusted forever.
    pub fn trust_peer(
        &self,
        device: &DeviceId,
        fingerprint: &[u8; 32],
        name: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO peers (device_id, fingerprint, name, paired_at)
             VALUES (?1, ?2, ?3, unixepoch())
             ON CONFLICT (device_id) DO UPDATE SET
                 fingerprint = excluded.fingerprint,
                 name = excluded.name",
            params![device.as_bytes().as_slice(), fingerprint.as_slice(), name],
        )?;
        Ok(())
    }

    pub fn trusted_peers(&self) -> Result<Vec<TrustedPeer>> {
        let mut stmt = self.conn.prepare(
            "SELECT device_id, fingerprint, name, paired_at, last_seen
               FROM peers ORDER BY name",
        )?;
        let rows = stmt.query_map([], peer_row)?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// The device behind a fingerprint, if it is one we trust.
    ///
    /// What turns an authenticated connection into a known device: TLS proves
    /// the peer holds the key behind a fingerprint, and this says whose device
    /// that is.
    pub fn peer_by_fingerprint(&self, fingerprint: &[u8; 32]) -> Result<Option<TrustedPeer>> {
        self.conn
            .query_row(
                "SELECT device_id, fingerprint, name, paired_at, last_seen
                   FROM peers WHERE fingerprint = ?1",
                params![fingerprint.as_slice()],
                peer_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn forget_peer(&self, device: &DeviceId) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM peers WHERE device_id = ?1", params![device.as_bytes().as_slice()])?;
        Ok(n > 0)
    }

    pub fn mark_peer_seen(&self, fingerprint: &[u8; 32]) -> Result<()> {
        self.conn.execute(
            "UPDATE peers SET last_seen = unixepoch() WHERE fingerprint = ?1",
            params![fingerprint.as_slice()],
        )?;
        Ok(())
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

/// A device this one has paired with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedPeer {
    pub device_id: DeviceId,
    pub fingerprint: [u8; 32],
    /// What the user calls it. Chosen by the peer, so display-only — never
    /// used to decide anything.
    pub name: String,
    pub paired_at: i64,
    pub last_seen: Option<i64>,
}

fn peer_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrustedPeer> {
    let id: Vec<u8> = r.get(0)?;
    let fp: Vec<u8> = r.get(1)?;
    Ok(TrustedPeer {
        device_id: to_device(id).expect("device_id column holds 32 bytes"),
        fingerprint: fp.try_into().expect("fingerprint column holds 32 bytes"),
        name: r.get(2)?,
        paired_at: r.get(3)?,
        last_seen: r.get(4)?,
    })
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
