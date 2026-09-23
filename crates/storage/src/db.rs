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
use std::fmt;
use std::path::Path;

/// Schema migrations, applied in order. `user_version` records how many have
/// run, so an existing database picks up only what it is missing.
///
/// Migrations are append-only. Editing one that has already shipped would leave
/// databases in the field at a schema nobody can reproduce.
const MIGRATIONS: &[&str] = &[V1, V2, V3, V4, V5, V6, V7, V8, V9, V10];

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

const V6: &str = r#"
-- Which peers have already been told what this device holds.
--
-- `Got` is sent when a transfer completes, which covers everything from now
-- on and nothing from before. A device that received a file last week holds it
-- just as truly, but never said so, so the device that made it goes on
-- counting it as delivered nowhere -- a number that would stay wrong for the
-- life of the file, because a file both devices already have is never
-- transferred again.
--
-- So a device also reports content it is merely holding. This records what it
-- has already reported to whom, because the statement is worth making once and
-- not on every sweep for every file.
CREATE TABLE IF NOT EXISTS reported (
    device_id    BLOB NOT NULL,
    content_hash BLOB NOT NULL,
    at           INTEGER NOT NULL,
    PRIMARY KEY (device_id, content_hash)
) STRICT;
"#;

const V7: &str = r#"
-- Which vault a file belongs to.
--
-- NULL is the shared area: a path every paired device converges on, which is
-- what every file was before this column existed and what every file still is
-- unless something says otherwise. A device id means the file belongs to that
-- device's private vault: other devices may send content into it and may not
-- read it back.
--
-- The privacy has to be a property rather than a drawing, so this column is
-- consulted when answering a peer -- for the tree it is shown, for the
-- manifests it may resolve, and for the chunks it may fetch. A device that
-- cannot see a path cannot reach its bytes either, which is the part a user
-- interface could not have enforced on its own.
ALTER TABLE files ADD COLUMN scope BLOB;

-- Answering "what may this device see" is a per-request question, so it must
-- not be a scan of every file.
CREATE INDEX IF NOT EXISTS idx_files_scope ON files (scope);
"#;

const V8: &str = r#"
-- One path per place, rather than one path anywhere.
--
-- `path` carried a column-level UNIQUE, which was right when there was one
-- shared namespace and is wrong now there are vaults: a device could not hold
-- `photo.jpg` for two different phones, nor hold one for a phone while having
-- its own in the shared area.
--
-- A column-level UNIQUE cannot be dropped in SQLite, so the table is rebuilt.
-- Migrations run with foreign keys off for exactly this reason -- `file_chunks`
-- cascades from `files`, and dropping the old table with them on would take
-- every chunk reference with it.
--
-- The replacement is two *partial* unique indexes rather than UNIQUE(scope,
-- path), because SQL treats NULLs as distinct: under that constraint two
-- shared rows with the same path would both be allowed, which is the bug this
-- is meant to prevent.
CREATE TABLE files_rebuilt (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL,
    size         INTEGER NOT NULL,
    content_hash BLOB NOT NULL,
    mtime_ns     INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER,
    vector       BLOB NOT NULL DEFAULT x'',
    modified_by  BLOB,
    materialised INTEGER NOT NULL DEFAULT 1,
    touched_at   INTEGER NOT NULL DEFAULT 0,
    wanted       INTEGER NOT NULL DEFAULT 0,
    scope        BLOB
) STRICT;

-- `id` is preserved, so every `file_chunks.file_id` stays valid.
INSERT INTO files_rebuilt
    (id, path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
     vector, modified_by, materialised, touched_at, wanted, scope)
SELECT id, path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
       vector, modified_by, materialised, touched_at, wanted, scope
  FROM files;

DROP TABLE files;
ALTER TABLE files_rebuilt RENAME TO files;

CREATE UNIQUE INDEX idx_files_shared_path ON files (path) WHERE scope IS NULL;
CREATE UNIQUE INDEX idx_files_vault_path ON files (scope, path) WHERE scope IS NOT NULL;

CREATE INDEX idx_files_deleted ON files (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_files_content ON files (content_hash) WHERE deleted_at IS NULL;
CREATE INDEX idx_files_evicted ON files (materialised)
    WHERE materialised = 0 AND deleted_at IS NULL;
CREATE INDEX idx_files_wanted ON files (wanted) WHERE wanted = 1 AND deleted_at IS NULL;
CREATE INDEX idx_files_scope ON files (scope);
"#;

const V9: &str = r#"
-- Not every copy elsewhere is a copy you can ask for back.
--
-- A device that collected content into its *private vault* holds the bytes,
-- but this device may not read another device's vault -- that is the whole
-- point of a vault. Counting such a delivery as "the content exists
-- elsewhere" would let the storage cap drop a shared-area file whose only
-- other copy is behind a door this device cannot open. That is data loss
-- wearing eviction's clothes.
--
-- Existing rows default to 0: every replica recorded before vaults existed
-- was an ordinary shared-area delivery, and treating them as such is both
-- true and the conservative reading.
ALTER TABLE replicas ADD COLUMN private INTEGER NOT NULL DEFAULT 0;
"#;

const V10: &str = r#"
-- What happened, in order.
--
-- The daemon has always known what it did and never written it down, so the
-- only account of a sync was the log of whichever process happened to be
-- running. That answers nothing after a restart, and "why is my file not
-- here?" is a question about the past.
--
-- One row per event, with the pieces an interface needs to render a line
-- without joining anything: what kind of thing, which path, how big, which
-- other device, and a sentence for the cases where the rest is not enough.
--
-- This adds no privacy exposure that the index did not already have: `files`
-- has held every path in plaintext since V1, and this table holds no content.
-- It is pruned on the same schedule as everything else -- see `Db::prune_activity`.
CREATE TABLE IF NOT EXISTS activity (
    id     INTEGER PRIMARY KEY,
    at     INTEGER NOT NULL,
    kind   TEXT NOT NULL,
    path   TEXT,
    size   INTEGER,
    -- The device at the other end, where there is one: who sent it, who took
    -- it, who we paired with.
    device BLOB,
    -- Free text, for the cases a kind cannot carry on its own: the reason a
    -- transfer failed, the name a conflict was filed under.
    detail TEXT
) STRICT;

-- Newest first is the only order anything asks for, and `id` breaks ties
-- within a second so that paging cannot repeat or skip a row.
CREATE INDEX IF NOT EXISTS idx_activity_recent ON activity (at DESC, id DESC);

-- "What happened to this file" is the other question, and it is asked about
-- one path at a time.
CREATE INDEX IF NOT EXISTS idx_activity_path ON activity (path, at DESC);
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
        // Deliberately off until the migrations have run, and on afterwards.
        //
        // A migration that rebuilds a table has to drop the old one, and
        // `file_chunks` cascades from `files` — dropping it with foreign keys
        // enforced would delete every chunk reference in the store. SQLite's
        // own documented procedure for a schema change of that shape is to
        // turn them off around it, and migrations are the only place schema
        // changes happen.
        conn.pragma_update(None, "foreign_keys", "OFF")?;

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

        // On for the life of the connection, and checked once: a migration that
        // rebuilt a table and got a reference wrong would otherwise be
        // discovered later, as missing content rather than as an error here.
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let broken: i64 =
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
        if broken > 0 {
            return Err(Error::Corrupt {
                detail: format!("{broken} broken reference(s) after migrating the index"),
            });
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
                &format!("SELECT {FILE_COLUMNS} FROM files WHERE path = ?1 AND scope IS NULL"),
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
            "SELECT coalesce(sum(size), 0) FROM files
              WHERE deleted_at IS NULL AND scope IS NULL",
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
        self.record_replica(content, device, false)
    }

    /// Record that `device` took these bytes into its own private vault.
    ///
    /// Distinguished from an ordinary delivery because this device cannot ask
    /// for them back: a vault is readable only by the device that owns it. The
    /// row is enough to release content held *for* that device, and not enough
    /// to evict anything from this device's shared area. See the `V9`
    /// migration for what goes wrong when the two are conflated.
    ///
    /// A device that later acquires the same content in the shared area is
    /// upgraded to an ordinary replica; the reverse never happens, because
    /// knowing less than before is not something a delivery can teach us.
    pub fn note_replica_in_vault(&self, content: &blake3::Hash, device: &DeviceId) -> Result<()> {
        self.record_replica(content, device, true)
    }

    fn record_replica(&self, content: &blake3::Hash, device: &DeviceId, private: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO replicas (content_hash, device_id, at, private)
             VALUES (?1, ?2, unixepoch(), ?3)
             ON CONFLICT (content_hash, device_id) DO UPDATE SET
                 at = excluded.at,
                 private = min(replicas.private, excluded.private)",
            params![content.as_bytes().as_slice(), device.as_bytes().as_slice(), private as i64],
        )?;
        Ok(())
    }

    /// How many other devices hold these bytes somewhere we could ask for them.
    ///
    /// Vault deliveries are excluded on purpose: they are copies that exist and
    /// cannot be retrieved, which is no help to a device deciding whether it is
    /// safe to drop its own.
    pub fn replica_count(&self, content: &blake3::Hash) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT count(*) FROM replicas WHERE content_hash = ?1 AND private = 0",
            params![content.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Note that a path was just read or written here.
    pub fn touch(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET touched_at = unixepoch() WHERE path = ?1 AND scope IS NULL",
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
                AND f.scope IS NULL
                AND EXISTS (
                      SELECT 1 FROM replicas r
                       WHERE r.content_hash = f.content_hash AND r.private = 0
                    )
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
            "UPDATE files SET materialised = ?2
             WHERE path = ?1 AND deleted_at IS NULL AND scope IS NULL",
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
                "SELECT materialised FROM files
                  WHERE path = ?1 AND deleted_at IS NULL AND scope IS NULL",
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
              WHERE deleted_at IS NULL AND materialised = 0 AND scope IS NULL
              ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Bytes of live files this device is actually holding.
    pub fn materialised_bytes(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT coalesce(sum(size), 0) FROM files
              WHERE deleted_at IS NULL AND materialised = 1 AND scope IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Ask for a file's bytes back. Acted on the next time a peer is reachable.
    pub fn want(&self, path: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE files SET wanted = 1
             WHERE path = ?1 AND deleted_at IS NULL AND scope IS NULL",
            params![path],
        )?;
        Ok(changed > 0)
    }

    /// Paths asked for that this device is not holding yet.
    pub fn wanted_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE wanted = 1 AND materialised = 0 AND deleted_at IS NULL AND scope IS NULL
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
                AND f.scope IS NULL
                AND f.modified_by = (SELECT device_id FROM local WHERE id = 1)
                AND NOT EXISTS (
                      SELECT 1 FROM replicas r WHERE r.content_hash = f.content_hash
                    )
              ORDER BY f.updated_at DESC, f.path",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Content `device` made and this one holds, that it has not been told
    /// about yet.
    ///
    /// The peer's own "waiting to be delivered" list is made of files it made
    /// that it believes nobody else has. This is the answer to that, for the
    /// files it is wrong about — everything it made that is sitting here.
    ///
    /// Limited, because on a library where one device made everything this
    /// would otherwise be the whole library in one pass. What is left over is
    /// picked up next time.
    pub fn unreported_to(&self, device: &DeviceId, limit: usize) -> Result<Vec<blake3::Hash>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT f.content_hash
               FROM files f
              WHERE f.deleted_at IS NULL
                AND f.materialised = 1
                AND f.modified_by = ?1
                AND NOT EXISTS (
                      SELECT 1 FROM reported r
                       WHERE r.device_id = ?1 AND r.content_hash = f.content_hash
                    )
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice(), limit as i64], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(to_hash(&raw))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Remember that `device` has been told this content is held here.
    pub fn note_reported(&self, device: &DeviceId, content: &blake3::Hash) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reported (device_id, content_hash, at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT (device_id, content_hash) DO UPDATE SET at = excluded.at",
            params![device.as_bytes().as_slice(), content.as_bytes().as_slice()],
        )?;
        Ok(())
    }

    pub fn live_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT path FROM files
                  WHERE deleted_at IS NULL AND scope IS NULL ORDER BY path",
            )?;
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
              WHERE deleted_at IS NULL AND scope IS NULL
                AND (path = ?1 OR path LIKE ?2 ESCAPE '\\')
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

    /// The versions an audience is entitled to know about.
    ///
    /// See [`Audience`]. The shared area is visible to everyone; a vault is
    /// visible only to the device that owns it, which is the whole point and
    /// the reason this is a database query rather than something an interface
    /// chooses not to draw.
    pub fn versions_for(&self, audience: Audience<'_>) -> Result<Vec<FileVersion>> {
        match audience {
            Audience::Ourselves => self.all_versions(),
            Audience::Unplaced => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {FILE_COLUMNS} FROM files WHERE scope IS NULL ORDER BY path"
                ))?;
                let rows = stmt.query_map([], file_row)?;
                rows.map(|row| Ok(row_to_version(&row?))).collect()
            }
            Audience::Device(asker) => {
                // Two queries rather than one, so that each version is marked
                // with where it came from. The asker cannot tell a shared file
                // from something sent to it privately by looking at the path,
                // and it has exactly one chance to file it correctly.
                let mut out = Vec::new();
                let mut shared = self.conn.prepare(&format!(
                    "SELECT {FILE_COLUMNS} FROM files WHERE scope IS NULL ORDER BY path"
                ))?;
                for row in shared.query_map([], file_row)? {
                    out.push(row_to_version(&row?));
                }
                let mut theirs = self.conn.prepare(&format!(
                    "SELECT {FILE_COLUMNS} FROM files WHERE scope = ?1 ORDER BY path"
                ))?;
                for row in theirs.query_map(params![asker.as_bytes().as_slice()], file_row)? {
                    out.push(row_to_version(&row?).into_private());
                }
                Ok(out)
            }
        }
    }

    /// Whether any live file claims this path, in the shared area or in any
    /// vault. Asked before filing a delivery, because one path is one file on
    /// disk however many index rows point at it.
    pub fn live_path_anywhere(&self, path: &str) -> Result<bool> {
        let taken: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM files WHERE path = ?1 AND deleted_at IS NULL)",
            params![path],
            |r| r.get(0),
        )?;
        Ok(taken)
    }

    /// Write down that something happened.
    ///
    /// Best-effort by construction: it returns a `Result`, and every caller in
    /// this workspace logs and continues rather than failing the operation it
    /// was describing. A sync that worked must not be reported as failed
    /// because the note about it could not be written.
    pub fn record(
        &self,
        kind: Event,
        path: Option<&str>,
        size: Option<u64>,
        device: Option<&DeviceId>,
        detail: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO activity (at, kind, path, size, device, detail)
             VALUES (unixepoch(), ?1, ?2, ?3, ?4, ?5)",
            params![
                kind.as_str(),
                path,
                size.map(|n| n as i64),
                device.map(|d| d.as_bytes().to_vec()),
                detail,
            ],
        )?;
        Ok(())
    }

    /// The shorter form for the common case: a kind and a path.
    pub fn note(&self, kind: Event, path: &str) -> Result<()> {
        self.record(kind, Some(path), None, None, None)
    }

    /// What happened, newest first.
    ///
    /// `before` pages backwards: pass the `id` of the oldest row already shown
    /// and the next page follows it. By `id` rather than by time because two
    /// events in the same second are indistinguishable by time, and a page
    /// boundary landing between them would repeat or skip one.
    pub fn activity(&self, limit: usize, before: Option<i64>) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, at, kind, path, size, device, detail
               FROM activity
              WHERE ?1 IS NULL OR id < ?1
              ORDER BY at DESC, id DESC
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![before, limit as i64], activity_row)?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// What happened to one path, newest first.
    ///
    /// This is the answer to "why is my file not here?" — the question the
    /// product specification asks for and the one a log cannot answer after a
    /// restart.
    pub fn activity_for(&self, path: &str, limit: usize) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, at, kind, path, size, device, detail
               FROM activity
              WHERE path = ?1
              ORDER BY at DESC, id DESC
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![path, limit as i64], activity_row)?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Drop history past its window, and cap what is left.
    ///
    /// Two limits rather than one because they fail differently. Age alone
    /// lets a busy week grow the table without bound; a count alone lets a
    /// quiet device keep rows from years ago. Returns how many rows went.
    pub fn prune_activity(&self, keep_for: std::time::Duration, keep_at_most: usize) -> Result<usize> {
        let cutoff = now() - keep_for.as_secs() as i64;
        let mut gone = self
            .conn
            .execute("DELETE FROM activity WHERE at <= ?1", params![cutoff])?;
        gone += self.conn.execute(
            "DELETE FROM activity
              WHERE id NOT IN (
                    SELECT id FROM activity ORDER BY at DESC, id DESC LIMIT ?1
                  )",
            params![keep_at_most as i64],
        )?;
        Ok(gone)
    }

    /// Files sitting in somebody's vault here that they have not taken yet:
    /// what a send is still waiting on, newest first.
    ///
    /// "Not taken yet" means no record of that device holding those bytes, of
    /// either kind. A delivery they collected into their own vault counts as
    /// collected, which is the whole point of recording it.
    ///
    /// The sender's side of a delivery: a file put in somebody's vault is not
    /// in the watched folder, so nothing in the ordinary change stream will
    /// ever mention it. This is what the daemon asks in order to know there is
    /// somebody to wake, and what `qurb status` asks in order to say so.
    pub fn pending_deliveries(&self) -> Result<Vec<(String, u64, DeviceId)>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.size, f.scope
               FROM files f
              WHERE f.deleted_at IS NULL
                AND f.scope IS NOT NULL
                AND NOT EXISTS (
                      SELECT 1 FROM replicas r
                       WHERE r.content_hash = f.content_hash
                         AND r.device_id = f.scope
                    )
              ORDER BY f.updated_at DESC, f.path",
        )?;
        let rows = stmt.query_map([], |r| {
            let raw: Vec<u8> = r.get(2)?;
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, to_device(raw)))
        })?;
        Ok(rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|(path, size, who)| who.map(|w| (path, size, w)))
            .collect())
    }

    /// The same question, answered as the set of devices to wake.
    pub fn awaiting_collection(&self) -> Result<Vec<DeviceId>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT f.scope
               FROM files f
              WHERE f.deleted_at IS NULL
                AND f.scope IS NOT NULL
                AND NOT EXISTS (
                      SELECT 1 FROM replicas r
                       WHERE r.content_hash = f.content_hash
                         AND r.device_id = f.scope
                    )",
        )?;
        let rows = stmt.query_map([], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(to_device(raw))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?.into_iter().flatten().collect())
    }

    /// The path a given device's vault holds this content under, here.
    ///
    /// For describing a delivery after the fact: the sender knows the name it
    /// used, and a content hash on its own makes an unreadable history line.
    pub fn vault_path_for(&self, content: &blake3::Hash, device: &DeviceId) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT path FROM files
                  WHERE content_hash = ?1 AND scope = ?2 AND deleted_at IS NULL
                  LIMIT 1",
                params![content.as_bytes().as_slice(), device.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Whether this device has already taken delivery of these bytes.
    ///
    /// Tombstones count. A delivery the user accepted and then deleted has
    /// been taken, and offering it again every time the sender reappears would
    /// make deleting a received file impossible.
    pub fn vault_knows(&self, content: &blake3::Hash) -> Result<bool> {
        let me = self.local_device()?;
        let known: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM files WHERE content_hash = ?1 AND scope = ?2
             )",
            params![content.as_bytes().as_slice(), me.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(known)
    }

    /// Whether a device may fetch the bytes of this chunk.
    ///
    /// Answered from the files that reference it: a chunk is reachable if any
    /// file the asker may see uses it. Tombstones count, because a peer that
    /// learned of a version before it was deleted may still be fetching it.
    ///
    /// Deduplication makes this the right shape rather than an awkward one. If
    /// the same bytes appear in both the shared area and somebody's vault, the
    /// shared copy already entitles everyone to them, and pretending otherwise
    /// would refuse content the asker can obtain a different way.
    pub fn chunk_visible_to(&self, hash: &blake3::Hash, audience: Audience<'_>) -> Result<bool> {
        if matches!(audience, Audience::Ourselves) {
            return Ok(true);
        }
        let owner = audience.device().map(|d| d.as_bytes().to_vec());
        let visible: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1
                   FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                  WHERE fc.chunk_hash = ?1
                    AND (f.scope IS NULL OR f.scope = ?2)
                  LIMIT 1",
                params![hash.as_bytes().as_slice(), owner],
                |r| r.get(0),
            )
            .optional()?;
        Ok(visible.is_some())
    }

    /// Whether a device may resolve this content hash to a chunk list.
    pub fn content_visible_to(
        &self,
        content: &blake3::Hash,
        audience: Audience<'_>,
    ) -> Result<bool> {
        if matches!(audience, Audience::Ourselves) {
            return Ok(true);
        }
        let owner = audience.device().map(|d| d.as_bytes().to_vec());
        let visible: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM files
                  WHERE content_hash = ?1 AND (scope IS NULL OR scope = ?2)
                  LIMIT 1",
                params![content.as_bytes().as_slice(), owner],
                |r| r.get(0),
            )
            .optional()?;
        Ok(visible.is_some())
    }

    /// Whether the only way `device` could have got this content from here was
    /// out of its own vault.
    ///
    /// Asked when a peer reports a delivery, to decide which kind of record to
    /// write. If this device also holds the content in the shared area then the
    /// peer has it somewhere ordinary and reachable; if the only live file with
    /// those bytes is the one sitting in that device's vault, the copy now
    /// exists behind a door this device cannot open.
    ///
    /// Content this device does not hold at all answers `false`: the peer got
    /// it from somewhere else, and nothing here says that somewhere was
    /// private.
    pub fn delivery_is_vault_only(&self, content: &blake3::Hash, device: &DeviceId) -> Result<bool> {
        let shared: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM files
                  WHERE content_hash = ?1 AND deleted_at IS NULL AND scope IS NULL
             )",
            params![content.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        if shared {
            return Ok(false);
        }
        let theirs: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM files
                  WHERE content_hash = ?1 AND deleted_at IS NULL AND scope = ?2
             )",
            params![content.as_bytes().as_slice(), device.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(theirs)
    }

    /// Chunks held only on another device's behalf, which that device has.
    ///
    /// Two conditions, and the second is the one that makes this safe to run
    /// without asking. The chunk must belong to a vault entry whose content
    /// another device is recorded as holding — and **no** live file anywhere in
    /// this store may still need it from the chunk store. A chunk shared with a
    /// file that has no other way to be read is left alone, whatever else
    /// references it.
    ///
    /// "No other way to be read" means: not materialised in this device's
    /// folder, and no record of the content living somewhere reachable. Two
    /// records count as reachable, and the distinction is the whole reason
    /// `replicas.private` exists:
    ///
    /// - an ordinary replica (`private = 0`) -- some device holds it in the
    ///   shared area and will hand it back on request;
    /// - a vault delivery to the very device whose vault the entry is in --
    ///   the recipient has their own copy, so this one was only ever a
    ///   courtesy.
    ///
    /// A vault delivery to *someone else* counts as neither, because this
    /// device cannot read another device's vault. Phrased per chunk rather
    /// than per file because deduplication means one payload can serve
    /// several, and the most cautious file wins.
    pub fn releasable_held_chunks(&self) -> Result<Vec<blake3::Hash>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT fc.chunk_hash
               FROM file_chunks fc
               JOIN files f ON f.id = fc.file_id
              WHERE f.deleted_at IS NULL
                AND f.scope IS NOT NULL
                AND EXISTS (
                      SELECT 1 FROM replicas r
                       WHERE r.content_hash = f.content_hash
                         AND r.device_id = f.scope
                    )
                AND NOT EXISTS (
                      SELECT 1
                        FROM file_chunks other
                        JOIN files g ON g.id = other.file_id
                       WHERE other.chunk_hash = fc.chunk_hash
                         AND g.deleted_at IS NULL
                         AND g.materialised = 0
                         AND NOT EXISTS (
                               SELECT 1 FROM replicas r2
                                WHERE r2.content_hash = g.content_hash
                                  AND (r2.private = 0 OR r2.device_id = g.scope)
                             )
                    )",
        )?;
        let rows = stmt.query_map([], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(to_hash(&raw))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Put a path in a device's private vault, or back in the shared area.
    pub fn set_scope(&self, path: &str, vault: Option<&DeviceId>) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE files SET scope = ?2 WHERE path = ?1",
            params![path, vault.map(|d| d.as_bytes().to_vec())],
        )?;
        Ok(changed > 0)
    }

    /// Which vault a path belongs to, if any.
    pub fn scope_of(&self, path: &str) -> Result<Option<DeviceId>> {
        let raw: Option<Option<Vec<u8>>> = self
            .conn
            .query_row("SELECT scope FROM files WHERE path = ?1", params![path], |r| r.get(0))
            .optional()?;
        Ok(raw.flatten().map(|bytes| DeviceId::from_bytes(to_hash(&bytes).into())))
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
            "UPDATE files SET vector = ?2, modified_by = ?3, updated_at = ?4
              WHERE path = ?1 AND scope IS NULL",
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
    /// Any file holding exactly this content, by id rather than by name.
    ///
    /// Content is resolved to a row directly because a path is no longer a
    /// unique handle: the same name can be in the shared area and in a vault,
    /// and going by way of the path would find whichever the namespace filter
    /// happened to allow — which for content lookups is the wrong question
    /// entirely. Whether this device *holds the bytes* has nothing to do with
    /// which namespace they sit in.
    pub fn any_file_with_content(&self, hash: &blake3::Hash) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM files
                  WHERE content_hash = ?1
                  ORDER BY deleted_at IS NOT NULL, id
                  LIMIT 1",
                params![hash.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

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
        let new = self
            .conn
            .query_row(
                "SELECT 1 FROM peers WHERE device_id = ?1",
                params![device.as_bytes().as_slice()],
                |_| Ok(()),
            )
            .optional()?
            .is_none();
        self.conn.execute(
            "INSERT INTO peers (device_id, fingerprint, name, paired_at)
             VALUES (?1, ?2, ?3, unixepoch())
             ON CONFLICT (device_id) DO UPDATE SET
                 fingerprint = excluded.fingerprint,
                 name = excluded.name",
            params![device.as_bytes().as_slice(), fingerprint.as_slice(), name],
        )?;
        // Only a genuinely new device is worth a line in the history. Trust is
        // re-asserted on a schedule, and a device that appeared once a day
        // would fill the list with an event nobody made happen.
        if new {
            let _ = self.record(Event::Paired, None, None, Some(device), Some(name));
        }
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
                  WHERE deleted_at IS NULL AND scope IS NULL
                    AND lower(path) = lower(?1) AND path <> ?1
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

fn activity_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Activity> {
    let kind: String = r.get(2)?;
    let device: Option<Vec<u8>> = r.get(5)?;
    Ok(Activity {
        id: r.get(0)?,
        at: r.get(1)?,
        kind: Event::parse(&kind),
        path: r.get(3)?,
        size: r.get::<_, Option<i64>>(4)?.map(|n| n as u64),
        device: device.and_then(to_device),
        detail: r.get(6)?,
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
        // Set by the caller that knows the audience: the same row is private
        // to one device and invisible to every other.
        private: false,
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

/// One thing that happened, in a form an interface can render directly.
///
/// Deliberately flat. An enum with a payload per variant reads better in Rust
/// and turns every query into a match; what the two screens this exists for
/// actually do is show a line and a time, so the row is the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: i64,
    /// Unix seconds.
    pub at: i64,
    pub kind: Event,
    pub path: Option<String>,
    pub size: Option<u64>,
    /// The device at the other end, where there is one.
    pub device: Option<DeviceId>,
    /// A sentence for what the rest cannot carry: why something failed, what a
    /// conflict was filed as.
    pub detail: Option<String>,
}

/// What kind of thing happened.
///
/// Stored as text rather than as an integer so that a database opened by hand
/// — which is how most of this project's debugging happens — reads as
/// sentences instead of as a legend to look up. An unknown string from a newer
/// build is kept rather than dropped, because losing history to a downgrade is
/// worse than showing one unfamiliar word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A file changed here and was stored.
    Stored,
    /// A path was deleted here.
    Deleted,
    /// A version arrived from another device.
    Received,
    /// A file was put in another device's vault.
    Sent,
    /// That device collected it.
    Collected,
    /// A local copy was dropped to stay under the storage limit.
    Evicted,
    /// A dropped file's contents came back.
    Restored,
    /// Two concurrent edits; both kept.
    Conflicted,
    /// A device was paired with.
    Paired,
    /// Something went wrong that a person may need to know about.
    Failed,
    /// Written by a build that knew a kind this one does not.
    Other(String),
}

impl Event {
    pub fn as_str(&self) -> &str {
        match self {
            Event::Stored => "stored",
            Event::Deleted => "deleted",
            Event::Received => "received",
            Event::Sent => "sent",
            Event::Collected => "collected",
            Event::Evicted => "evicted",
            Event::Restored => "restored",
            Event::Conflicted => "conflicted",
            Event::Paired => "paired",
            Event::Failed => "failed",
            Event::Other(word) => word,
        }
    }

    fn parse(word: &str) -> Self {
        match word {
            "stored" => Event::Stored,
            "deleted" => Event::Deleted,
            "received" => Event::Received,
            "sent" => Event::Sent,
            "collected" => Event::Collected,
            "evicted" => Event::Evicted,
            "restored" => Event::Restored,
            "conflicted" => Event::Conflicted,
            "paired" => Event::Paired,
            "failed" => Event::Failed,
            other => Event::Other(other.to_string()),
        }
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who is asking, for the purpose of what they may see.
///
/// Three cases, and conflating any two of them is a bug with a security
/// consequence — which is why this is an enum rather than an `Option`.
#[derive(Debug, Clone, Copy)]
pub enum Audience<'a> {
    /// This device itself. Sees everything it holds: an interface showing
    /// somebody their own files is not a peer.
    Ourselves,
    /// A peer whose device this store recognises. Sees the shared area and
    /// that device's own vault, and never anybody else's.
    Device(&'a DeviceId),
    /// A peer that authenticated but whose device is not recorded here. Sees
    /// the shared area only.
    ///
    /// It owns no vault as far as this device knows, so it is shown none — but
    /// it is not refused outright, because the connection already proved it is
    /// trusted and the shared area is what trust entitles a device to. Being
    /// stricter here would break a paired device whose bookkeeping is
    /// incomplete, and buy nothing: vaults are still invisible to it.
    Unplaced,
}

impl<'a> Audience<'a> {
    fn device(&self) -> Option<&'a DeviceId> {
        match self {
            Audience::Device(device) => Some(device),
            _ => None,
        }
    }
}
