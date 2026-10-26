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
/// Which version of the index this build writes: how many migrations it has.
/// Reported beside the app's version, because two builds agreeing on this is
/// what lets one open the other's index (decision 0047).
pub const SCHEMA_VERSION: usize = MIGRATIONS.len();

const MIGRATIONS: &[&str] =
    &[V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15, V16, V17, V18, V19, V20, V21, V22, V23, V24, V25, V26, V27, V28];

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

const V11: &str = r#"
-- Deliveries this device has taken, kept for good.
--
-- A delivery is taken once, keyed by its content. Until now the only record of
-- having taken one was the received file's own row -- and once the person
-- deleted the file, that row was a tombstone, which garbage collection expires
-- after the retention window. The sender goes on offering what it sent for as
-- long as it keeps the entry, so a week after somebody deleted a file they had
-- been sent, it arrived again.
--
-- And a delivery filed outside the folder, as an ordinary file in Downloads
-- (decision 0037), has no row at all. This table is the record for both, and
-- nothing expires it: one short row per file somebody sent, which is not a
-- cost worth managing.
CREATE TABLE IF NOT EXISTS taken (
    content_hash BLOB PRIMARY KEY,
    -- Who sent it, where known.
    sender       BLOB,
    taken_at     INTEGER NOT NULL,
    -- Where it went: a path in the folder, or a file in Downloads.
    filed_as     TEXT NOT NULL
) STRICT;

-- Everything already taken into this device's vault, deleted or not, so that
-- nothing an existing device has received becomes deliverable again.
INSERT OR IGNORE INTO taken (content_hash, sender, taken_at, filed_as)
SELECT content_hash, modified_by, updated_at, path FROM files
 WHERE scope = (SELECT device_id FROM local WHERE id = 1)
   AND content_hash != zeroblob(32);
"#;

const V12: &str = r#"
-- Holding another device's vault for it (decision 0036).
--
-- On the holder, a row in another device's vault is one of two things: a
-- file this device sent there, waiting to be collected, or that device's own
-- file, which this device keeps for it. Scope alone cannot tell them apart,
-- and the difference is whether the bytes may be released once the owner has
-- them -- right for a send, and the loss of the backup for a held file.
ALTER TABLE files ADD COLUMN held INTEGER NOT NULL DEFAULT 0;

-- On the owner: the devices allowed to hold this device's vault, and so to
-- be shown it. Nothing else is shown another device's vault.
CREATE TABLE IF NOT EXISTS holders (
    device_id BLOB PRIMARY KEY,
    since     INTEGER NOT NULL
) STRICT;
"#;

const V13: &str = r#"
-- Recently deleted: files this device took out of the folder because they were
-- deleted -- on another device, or here through qurb -- kept in `trash/` in
-- the store directory for a while instead of unlinked.
--
-- Under single-copy storage the file in the folder is the only copy of its
-- bytes on this device, so before this a deletion anywhere removed every copy
-- everywhere the moment it synced. A row here is one file's worth of second
-- chance; see decision 0042.
CREATE TABLE IF NOT EXISTS trash (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL,
    -- NULL for the shared area, a device id for that device's vault.
    scope        BLOB,
    content_hash BLOB NOT NULL,
    size         INTEGER NOT NULL,
    deleted_at   INTEGER NOT NULL,
    -- The device whose deletion this was, where there is one.
    deleted_by   BLOB,
    -- Why, where "deleted" is not the whole story: the other version of a
    -- conflict somebody settled.
    why          TEXT
) STRICT;
"#;

const V14: &str = r#"
-- Which devices a folder in the shared area is shared with (decision 0044).
--
-- Derived, not authoritative: the rules are small files under .qurb-sharing/
-- that sync like any other, and these tables are rebuilt from them whenever
-- they change, so that what a device may see can be answered in SQL along
-- with everything else a query already filters on. A folder with no row here
-- is shared with every device.
CREATE TABLE IF NOT EXISTS shares (
    folder TEXT PRIMARY KEY
) STRICT;

CREATE TABLE IF NOT EXISTS share_members (
    folder    TEXT NOT NULL REFERENCES shares (folder) ON DELETE CASCADE,
    device_id BLOB NOT NULL,
    PRIMARY KEY (folder, device_id)
) STRICT;

-- What the rule files looked like when the tables were last built, so that
-- rebuilding is skipped when nothing changed.
CREATE TABLE IF NOT EXISTS shares_stamp (
    id    INTEGER PRIMARY KEY CHECK (id = 1),
    stamp TEXT NOT NULL
) STRICT;
"#;

const V15: &str = r#"
-- Folders this device keeps only remotely (decision 0045, brief §29): their
-- files are listed and fetched when asked for, and not downloaded because
-- another device changed them. This device's choice alone -- never synced.
CREATE TABLE IF NOT EXISTS remote_folders (
    folder TEXT PRIMARY KEY
) STRICT;
"#;

const V16: &str = r#"
-- What kind of device each peer is: 'phone', 'computer' or 'replica'
-- (decision 0053). A phone's copy is one tap in its settings from gone, so it
-- does not count as the other copy that lets a device free its own. No row for
-- a device that has not said yet; it is asked at the next sync. Tables rather
-- than columns, so that running this again is harmless.
CREATE TABLE IF NOT EXISTS peer_kinds (
    device_id BLOB PRIMARY KEY,
    kind      TEXT NOT NULL
) STRICT;

-- Facts about this device: its own kind, told to devices that ask, and
-- whether a phone has already had a computer chosen to keep its vault by
-- default -- once, so a person who later removes that choice is not overruled.
CREATE TABLE IF NOT EXISTS local_facts (
    name  TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
"#;

const V17: &str = r#"
-- Copies this device has confirmed by asking the device recorded as holding
-- them (decision 0055). A copy is recorded on the word of the device that
-- made the file, or of a report, and stays recorded after that device frees
-- it. Asked once per device and content, a few at each sync.
CREATE TABLE IF NOT EXISTS confirmed (
    device_id    BLOB NOT NULL,
    content_hash BLOB NOT NULL,
    at           INTEGER NOT NULL,
    PRIMARY KEY (device_id, content_hash)
) STRICT;
"#;

const V18: &str = r#"
-- Deliveries this device has taken, one row per send (decision 0059).
--
-- `taken`, from V11, is keyed by content: a delivery of bytes this device had
-- taken once was never taken again. That kept an old send from arriving
-- again each time its sender reappeared, and also refused every new send of
-- the same file -- by the same person after deleting the first, or by a phone
-- set up again -- silently. A send is identified instead by who sent it,
-- under what name, and which version of that name: offered again, it is the
-- same row; sent again, it is a new version and a new row.
--
-- `taken` stays, read-only, for what was taken before this: matched by
-- content, and only for versions made before it was taken.
CREATE TABLE IF NOT EXISTS deliveries (
    sender       BLOB NOT NULL,
    path         TEXT NOT NULL,
    vector       BLOB NOT NULL,
    content_hash BLOB NOT NULL,
    taken_at     INTEGER NOT NULL,
    filed_as     TEXT NOT NULL,
    -- Whether the sender has been told, which is what lets it stop waiting.
    -- Per send: told about the bytes once, a sender of the same file again
    -- would never be told about the second.
    acknowledged INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (sender, path, vector)
) STRICT;
"#;

const V19: &str = r#"
-- Where a send's bytes are read from (decision 0060).
--
-- A send used to keep its own sealed copy of the file in the chunk store
-- until the recipient had it, and then on until space ran short: 1.6 GB for
-- one video on a phone. qurb keeps no copy now. The send records where the
-- file is, and its chunks are read from there when the recipient collects
-- them, checked against the hashes taken when it was sent. A file changed or
-- deleted before then calls the send off.
--
-- `source` is an absolute path, or on a phone a `content://` document the
-- app may read again later. `temporary` marks a copy qurb itself made -- of a
-- file Android's share sheet lent only briefly -- deleted once collected.
CREATE TABLE IF NOT EXISTS send_sources (
    file_id   INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    source    TEXT NOT NULL,
    size      INTEGER NOT NULL,
    mtime_ns  INTEGER NOT NULL,
    temporary INTEGER NOT NULL DEFAULT 0
) STRICT;
"#;

const V20: &str = r#"
-- Who a peer is to this device (decision 0060), for a device of another
-- person: 'guest', visiting this computer, or 'host', a computer this device
-- visits. No row is one of the same person's devices, as every peer was
-- before. A guest or a host holds a different key, and is shown only what is
-- addressed to it. A table of its own, like `peer_kinds`, rather than a
-- column, so that the migration can run again safely.
CREATE TABLE IF NOT EXISTS peer_relations (
    device_id BLOB PRIMARY KEY,
    relation  TEXT NOT NULL
) STRICT;

-- The secret a guest and the computer it visits meet under: what both
-- announce with at the rendezvous service, which matches the devices of one
-- person by a group derived from their key, and a guest does not have it.
CREATE TABLE IF NOT EXISTS meetings (
    device_id BLOB PRIMARY KEY,
    secret    BLOB NOT NULL
) STRICT;
"#;

const V21: &str = r#"
-- This device's vault, sealed for a keeper of another person (decision 0060):
-- a computer this device visits as a guest, which keeps its files without
-- being able to read them. For each file and keeper, the sealed file it was
-- shown -- its hash and size -- made from which version of the plain file.
CREATE TABLE IF NOT EXISTS sealed_views (
    holder       BLOB NOT NULL,
    path         TEXT NOT NULL,
    content_hash BLOB NOT NULL,
    sealed_hash  BLOB NOT NULL,
    sealed_size  INTEGER NOT NULL,
    PRIMARY KEY (holder, path)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_sealed_views_hash ON sealed_views (holder, sealed_hash);

-- The parts of a sealed file, in order: the sealed header, then each plain
-- chunk sealed. What the keeper is told when it asks for the file's chunk
-- list, and how a sealed chunk it asks for is found again: by reading the
-- plain chunk and sealing it, the same way every time.
CREATE TABLE IF NOT EXISTS sealed_parts (
    holder      BLOB NOT NULL,
    sealed_hash BLOB NOT NULL,
    seq         INTEGER NOT NULL,
    sealed_id   BLOB NOT NULL,
    chunk_hash  BLOB,
    header      BLOB,
    PRIMARY KEY (holder, sealed_hash, seq)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_sealed_parts_id ON sealed_parts (holder, sealed_id);

-- On a computer: the person each guest device belongs to, as that device said
-- when it visited -- an identifier derived from that person's key for this
-- computer alone. A guest's folder is kept under it, so that a phone set up
-- again with the same key finds the folder it had (decision 0060).
CREATE TABLE IF NOT EXISTS visit_persons (
    device_id BLOB PRIMARY KEY,
    person    BLOB NOT NULL
) STRICT;

-- On a guest: files of its vault fetched back from the computer keeping them,
-- and when. Such a file stays a day before it is let go of again; one added
-- here goes as soon as the computer has it (decision 0060).
CREATE TABLE IF NOT EXISTS kept_opened (
    path TEXT PRIMARY KEY,
    at   INTEGER NOT NULL
) STRICT;
"#;

const V22: &str = r#"
-- Files of this device's own vault that live outside its folder, and are read
-- where they lie (decision 0062): on a phone, WhatsApp's folder, through the
-- grant the person gave. The chunks are recorded and none is stored. They are
-- read from `source` whenever asked for, and checked against their hashes, for
-- as long as the file is there -- unlike a send's, which is let go of once
-- collected. `size` and `mtime_ns` are as the folder was last listed, which is
-- how a change is noticed.
CREATE TABLE IF NOT EXISTS outside_sources (
    file_id  INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    source   TEXT NOT NULL,
    size     INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL
) STRICT;
"#;

/// The folders to back up chosen on either side, the phone or a computer
/// keeping its vault (the owner's item 10, 2026-10-14): each choice, ticked or
/// not, with when it was made, so that the later one wins wherever it was made.
///
/// New tables rather than new columns, so that running it again -- an index
/// rewound by a build undone and then redone -- keeps every choice, an untick
/// included: `INSERT OR IGNORE` takes the V23 rows, all of them ticks then,
/// only where there is nothing yet. V23's two tables are left, unused.
/// A counter every change to a file's listing moves (decision 0072): what lets
/// a device asking for this one's file list be sent only what changed since
/// the list it holds.
///
/// Kept by triggers, so that no write can forget it. **A migration that
/// rebuilds `files` must create the triggers again**; one that does not would
/// leave lists sent whole, not wrong -- the asker checks what it rebuilt
/// against the list's fingerprint and fetches it whole on any difference.
const V27: &str = r#"
CREATE TABLE IF NOT EXISTS list_counter (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    n  INTEGER NOT NULL
) STRICT;
INSERT OR IGNORE INTO list_counter (id, n) VALUES (1, (SELECT coalesce(max(id), 0) FROM files));
-- Beside `files` rather than a column of it, so that this can run again on an
-- index that has it, as every migration here can. Written in the triggers by a
-- delete and an insert, not `INSERT OR REPLACE`: inside a trigger SQLite puts
-- the conflict rule of the statement that fired it in place of the trigger's
-- own, and a row replaced in `files` under the same id then collided here.
CREATE TABLE IF NOT EXISTS listed (
    file_id INTEGER PRIMARY KEY,
    changed INTEGER NOT NULL
) STRICT;
INSERT OR IGNORE INTO listed (file_id, changed) SELECT id, id FROM files;
CREATE INDEX IF NOT EXISTS idx_listed_changed ON listed (changed);
CREATE TRIGGER IF NOT EXISTS files_listed_insert AFTER INSERT ON files BEGIN
    UPDATE list_counter SET n = n + 1 WHERE id = 1;
    DELETE FROM listed WHERE file_id = NEW.id;
    INSERT INTO listed (file_id, changed) VALUES (NEW.id, (SELECT n FROM list_counter WHERE id = 1));
END;
-- Only what a listing shows: not when a file was last opened, nor whether its
-- bytes are here.
CREATE TRIGGER IF NOT EXISTS files_listed_update AFTER UPDATE ON files
WHEN NEW.path IS NOT OLD.path OR NEW.size IS NOT OLD.size
    OR NEW.content_hash IS NOT OLD.content_hash OR NEW.deleted_at IS NOT OLD.deleted_at
    OR NEW.vector IS NOT OLD.vector OR NEW.modified_by IS NOT OLD.modified_by
    OR NEW.updated_at IS NOT OLD.updated_at OR NEW.scope IS NOT OLD.scope
    OR NEW.held IS NOT OLD.held
BEGIN
    UPDATE list_counter SET n = n + 1 WHERE id = 1;
    DELETE FROM listed WHERE file_id = NEW.id;
    INSERT INTO listed (file_id, changed) VALUES (NEW.id, (SELECT n FROM list_counter WHERE id = 1));
END;
CREATE TRIGGER IF NOT EXISTS files_listed_delete AFTER DELETE ON files BEGIN
    DELETE FROM listed WHERE file_id = OLD.id;
END;
"#;

/// Each device's own name and picture, set on it and shown on every device
/// paired with it (the owner's list of 2026-10-26, item 7).
const V28: &str = r#"
-- This device's: what it tells a device that asks.
CREATE TABLE IF NOT EXISTS own_profile (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    name       TEXT NOT NULL,
    picture    BLOB,
    changed_at INTEGER NOT NULL
) STRICT;

-- Each paired device's, as it last said: asked again with when, so that it
-- answers only what changed since.
CREATE TABLE IF NOT EXISTS peer_profiles (
    device_id  BLOB PRIMARY KEY,
    name       TEXT NOT NULL,
    picture    BLOB,
    changed_at INTEGER NOT NULL
) STRICT;
"#;

/// Files whose size and time cannot yet be trusted to say they are unchanged
/// (`Store::put_file`, *Too recent to trust*).
const V26: &str = r#"
-- Read within a few seconds of being written: a second write of the same
-- size in the same tick of the filesystem's clock would look like none. Read
-- again on each scan until they are older than that, then let go of.
CREATE TABLE IF NOT EXISTS racy (
    path TEXT PRIMARY KEY
) STRICT;
"#;

const V25: &str = r#"
-- On a computer: each phone's choices.
CREATE TABLE IF NOT EXISTS phone_choices (
    device_id  BLOB NOT NULL,
    folder     TEXT NOT NULL,
    chosen     INTEGER NOT NULL,
    changed_at INTEGER NOT NULL,
    PRIMARY KEY (device_id, folder)
) STRICT;
INSERT OR IGNORE INTO phone_choices (device_id, folder, chosen, changed_at)
    SELECT device_id, folder, 1, 0 FROM backup_choices;

-- On a phone: its own choices, made here or on a computer.
CREATE TABLE IF NOT EXISTS own_choices (
    folder     TEXT PRIMARY KEY,
    chosen     INTEGER NOT NULL,
    changed_at INTEGER NOT NULL
) STRICT;
INSERT OR IGNORE INTO own_choices (folder, chosen, changed_at)
    SELECT folder, 1, 0 FROM chosen_here;
"#;

/// A file, live or not, whose chunks add up to its size: asked for every file
/// a device serves. It must be found through an index (V24).
const ANY_FILE_WITH_CONTENT: &str = "SELECT id FROM files f
  WHERE content_hash = ?1
    AND size = (SELECT coalesce(sum(c.size), 0)
                  FROM file_chunks fc JOIN chunks c ON c.hash = fc.chunk_hash
                 WHERE fc.file_id = f.id)
  ORDER BY deleted_at IS NOT NULL, id
  LIMIT 1";

/// A path with this content, live first. Also through an index (V24).
const ANY_PATH_WITH_CONTENT: &str = "SELECT path FROM files
  WHERE content_hash = ?1
  ORDER BY (deleted_at IS NULL) DESC
  LIMIT 1";

/// Every row's content findable without reading them all.
///
/// The index on `content_hash` covered live rows only, and the questions that
/// accept a deleted row too -- `any_file_with_content`, asked for every file
/// a device serves, and `any_path_with_content` -- could not use it: each read
/// the whole of `files`. Measured on 2026-10-14 with the scale example, a first
/// sync took 2.4 ms a file at 5,000 files, 3.6 at 10,000 and 4.8 at 20,000,
/// most of it in that scan, under the store's lock (decision 0065). One index
/// over every row serves both kinds of question.
const V24: &str = r#"
DROP INDEX IF EXISTS idx_files_content;
CREATE INDEX IF NOT EXISTS idx_files_content ON files (content_hash);
"#;

/// Folders a phone backs up, chosen on a computer (decision 0063).
const V23: &str = r#"
-- On a phone: its own folders, two levels deep, as it last listed them; what
-- a computer keeping its vault is shown when it asks.
CREATE TABLE IF NOT EXISTS own_folders (
    path  TEXT PRIMARY KEY,
    files INTEGER NOT NULL,
    bytes INTEGER NOT NULL
) STRICT;

-- On a phone: the folders a computer keeping its vault last said were chosen,
-- kept so that a run with the computer away still backs them up.
CREATE TABLE IF NOT EXISTS chosen_here (
    folder TEXT PRIMARY KEY
) STRICT;

-- On a computer: each of its person's phones' folders, as the phone last said.
CREATE TABLE IF NOT EXISTS known_folders (
    device_id BLOB NOT NULL,
    path      TEXT NOT NULL,
    files     INTEGER NOT NULL,
    bytes     INTEGER NOT NULL,
    seen_at   INTEGER NOT NULL,
    PRIMARY KEY (device_id, path)
) STRICT;

-- On a computer: the folders the person ticked for each phone.
CREATE TABLE IF NOT EXISTS backup_choices (
    device_id BLOB NOT NULL,
    folder    TEXT NOT NULL,
    PRIMARY KEY (device_id, folder)
) STRICT;
"#;

/// A copy this device could ask for: on a device it is paired with, and not
/// known to be out of reach (decision 0055). For a query over `replicas r`.
///
/// What a person is shown as "on another device". A copy recorded for a device
/// never paired with this one -- the device that made a file, reached through
/// another -- or for one since removed is a copy this device cannot get back.
///
/// Only one of this person's own devices: a guest's copy, or a host's, is
/// another person's, and nothing here can ask for it back (decision 0060).
///
/// Except a computer of another person this device keeps its own vault on
/// (decision 0060): that copy is sealed, and this device can fetch it back.
const ASKABLE: &str = "r.private = 0 AND EXISTS (SELECT 1 FROM peers p WHERE p.device_id = r.device_id) AND (NOT EXISTS (SELECT 1 FROM peer_relations pr WHERE pr.device_id = r.device_id) OR EXISTS (SELECT 1 FROM holders h WHERE h.device_id = r.device_id))";

/// A copy held by a device not known to be a phone (decision 0053): the other
/// copy that lets this device free its own. For a query over `replicas r`.
const SAFE_ELSEWHERE: &str = "NOT EXISTS (SELECT 1 FROM peer_kinds k
                                  WHERE k.device_id = r.device_id AND k.kind = 'phone')";

/// Excludes qurb's own bookkeeping from anything a person is shown or a
/// storage cap may drop: the sharing rules themselves, and the folders kept
/// outside the vault's own (decision 0062) -- WhatsApp's files are not the
/// person's files in qurb, and are shown as a backup instead. For a query
/// over `files` with no alias. Both prefixes are 14 characters.
const NOT_RULES: &str =
    "substr(path, 1, 14) <> '.qurb-sharing/' AND substr(path, 1, 14) <> '.qurb-outside/'";
/// The same, for a query that calls `files` `f`.
const NOT_RULES_F: &str =
    "substr(f.path, 1, 14) <> '.qurb-sharing/' AND substr(f.path, 1, 14) <> '.qurb-outside/'";

/// Where folders kept outside the vault's own go in it (decision 0062), each
/// under a name of its own beneath this.
pub const OUTSIDE: &str = ".qurb-outside";

/// A shared-area row's `path` lies outside every folder shared without the
/// device in SQL parameter `device` -- the per-device half of what it may see
/// (decision 0044). A NULL device is in no folder's list.
fn shared_with(path: &str, device: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM shares s
                      WHERE ({path} = s.folder OR substr({path}, 1, length(s.folder) + 1) = s.folder || '/')
                        AND NOT EXISTS (SELECT 1 FROM share_members m
                                         WHERE m.folder = s.folder AND m.device_id = {device}))"
    )
}

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
        let applied: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        // Written by a newer build: this one does not know what that index
        // holds, and guessing would be how an index gets damaged. Said, rather
        // than opened (decision 0047).
        if applied as usize > MIGRATIONS.len() {
            return Err(Error::Corrupt {
                detail: format!(
                    "this index was written by a newer qurb (schema {applied}; this build knows up to {}) — \
                     run the newer build, or the one it was upgraded from",
                    MIGRATIONS.len()
                ),
            });
        }
        // About to be migrated: a copy first, as it was, so an upgrade can be
        // undone by putting the older build back with its index. Migrations
        // only go forward; this is the way back. One copy, the latest.
        if applied > 0 && (applied as usize) < MIGRATIONS.len() {
            let before = path.with_file_name(format!("index.before-schema-{}.db", MIGRATIONS.len()));
            let _ = std::fs::remove_file(&before);
            conn.execute("VACUUM INTO ?1", params![before.to_string_lossy()])?;
        }
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
            // One transaction for a migration and its number. Apart, a crash
            // between them left a migration applied but not recorded, and the
            // next open ran it again -- which an `ADD COLUMN` cannot survive,
            // so the store would never open again. Found when the crash test
            // failed under load, the kill landing during the first migrations.
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(migration)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
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

    /// A file's chunks in order, each with its plain length.
    pub fn chunks_with_sizes(&self, file_id: i64) -> Result<Vec<(blake3::Hash, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT fc.chunk_hash, c.size FROM file_chunks fc JOIN chunks c ON c.hash = fc.chunk_hash
              WHERE fc.file_id = ?1 ORDER BY fc.seq",
        )?;
        let rows = stmt.query_map(params![file_id], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok((to_hash(&raw), r.get::<_, i64>(1)? as u64))
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

    /// Where a live send's chunk can be read from: the send's original file,
    /// the offset of the chunk in it, and its length (decision 0060).
    ///
    /// Only a send still waiting -- not yet collected -- is read from: after
    /// that the file is the person's own again, and qurb has no business
    /// opening it.
    pub fn locate_send_chunk(&self, hash: &blake3::Hash) -> Result<Option<(String, u64, u64)>> {
        let found = self.conn.query_row(
            "WITH holder AS (
                 SELECT fc.file_id AS id
                   FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                   JOIN send_sources s ON s.file_id = f.id
                  WHERE fc.chunk_hash = ?1 AND f.deleted_at IS NULL
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
             SELECT (SELECT source FROM send_sources WHERE file_id = (SELECT id FROM holder)),
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

    /// Where a chunk of a file kept outside the folder can be read from: the
    /// file, the chunk's offset in it, and its length (decision 0062).
    pub fn locate_outside_chunk(&self, hash: &blake3::Hash) -> Result<Option<(String, u64, u64)>> {
        let found = self.conn.query_row(
            "WITH holder AS (
                 SELECT fc.file_id AS id
                   FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                   JOIN outside_sources o ON o.file_id = f.id
                  WHERE fc.chunk_hash = ?1 AND f.deleted_at IS NULL
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
             SELECT (SELECT source FROM outside_sources WHERE file_id = (SELECT id FROM holder)),
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

    /// This device's own folders, as just listed: replacing what was there
    /// (decision 0063).
    pub fn set_own_folders(&self, folders: &[FolderSummary]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM own_folders", [])?;
        for folder in folders {
            tx.execute(
                "INSERT INTO own_folders (path, files, bytes) VALUES (?1, ?2, ?3)",
                params![folder.path, folder.files as i64, folder.bytes as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// This device's own folders, as last listed.
    pub fn own_folders(&self) -> Result<Vec<FolderSummary>> {
        let mut stmt = self.conn.prepare("SELECT path, files, bytes FROM own_folders ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            Ok(FolderSummary { path: row.get(0)?, files: row.get::<_, i64>(1)? as u64, bytes: row.get::<_, i64>(2)? as u64 })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Choose, or stop choosing, one of this device's folders to back up,
    /// here (item 10): the later choice wins wherever it was made. Every
    /// choice inside it is set the same way, so that the folder is all
    /// backed up, or none of it ([`crate::chosen`]).
    pub fn choose_here(&self, folder: &str, chosen: bool, at_ms: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO own_choices (folder, chosen, changed_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (folder) DO UPDATE SET chosen = excluded.chosen, changed_at = excluded.changed_at",
            params![folder, chosen as i64, at_ms],
        )?;
        tx.execute(
            "UPDATE own_choices SET chosen = ?2, changed_at = ?3
              WHERE substr(folder, 1, length(?1) + 1) = ?1 || '/'",
            params![folder, chosen as i64, at_ms],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every choice made for this device, ticked or not, with when.
    pub fn choices_here(&self) -> Result<Vec<Choice>> {
        let mut stmt = self.conn.prepare("SELECT folder, chosen, changed_at FROM own_choices ORDER BY folder")?;
        let rows = stmt.query_map([], |r| Ok(Choice { folder: r.get(0)?, chosen: r.get::<_, i64>(1)? != 0, changed_at: r.get(2)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// What a computer keeping this device's vault said was chosen, taken
    /// where it is later than what is here.
    pub fn merge_choices_here(&self, theirs: &[Choice]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for choice in theirs {
            tx.execute(
                "INSERT INTO own_choices (folder, chosen, changed_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (folder) DO UPDATE SET chosen = excluded.chosen, changed_at = excluded.changed_at
                  WHERE excluded.changed_at > own_choices.changed_at",
                params![choice.folder, choice.chosen as i64, choice.changed_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The folders chosen for this device, wherever they were chosen.
    pub fn chosen_here(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT folder FROM own_choices WHERE chosen = 1 ORDER BY folder")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// A phone's folders as it just said, replacing what it said before.
    pub fn set_known_folders(&self, device: &DeviceId, folders: &[FolderSummary], seen_at: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM known_folders WHERE device_id = ?1", params![device.as_bytes().as_slice()])?;
        for folder in folders {
            tx.execute(
                "INSERT INTO known_folders (device_id, path, files, bytes, seen_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![device.as_bytes().as_slice(), folder.path, folder.files as i64, folder.bytes as i64, seen_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// A phone's folders as it last said, and when.
    pub fn known_folders(&self, device: &DeviceId) -> Result<(Vec<FolderSummary>, Option<i64>)> {
        let mut stmt = self.conn.prepare(
            "SELECT path, files, bytes, seen_at FROM known_folders WHERE device_id = ?1 ORDER BY path",
        )?;
        let mut seen = None;
        let rows = stmt.query_map(params![device.as_bytes().as_slice()], |row| {
            Ok((
                FolderSummary { path: row.get(0)?, files: row.get::<_, i64>(1)? as u64, bytes: row.get::<_, i64>(2)? as u64 },
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut folders = Vec::new();
        for row in rows {
            let (folder, at) = row?;
            seen = Some(at);
            folders.push(folder);
        }
        Ok((folders, seen))
    }

    /// Tick, or untick, a phone's folder to back up, here, now, and every
    /// choice inside it the same way, as on the phone ([`choose_here`](Self::choose_here)).
    pub fn choose_folder(&self, device: &DeviceId, folder: &str, chosen: bool) -> Result<()> {
        let at = now_ms();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO phone_choices (device_id, folder, chosen, changed_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (device_id, folder) DO UPDATE SET chosen = excluded.chosen, changed_at = excluded.changed_at",
            params![device.as_bytes().as_slice(), folder, chosen as i64, at],
        )?;
        tx.execute(
            "UPDATE phone_choices SET chosen = ?3, changed_at = ?4
              WHERE device_id = ?1 AND substr(folder, 1, length(?2) + 1) = ?2 || '/'",
            params![device.as_bytes().as_slice(), folder, chosen as i64, at],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every choice made for a phone, ticked or not, with when.
    pub fn choices_for(&self, device: &DeviceId) -> Result<Vec<Choice>> {
        let mut stmt = self.conn.prepare(
            "SELECT folder, chosen, changed_at FROM phone_choices WHERE device_id = ?1 ORDER BY folder",
        )?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice()], |r| {
            Ok(Choice { folder: r.get(0)?, chosen: r.get::<_, i64>(1)? != 0, changed_at: r.get(2)? })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// What a phone said it chose, taken where it is later than what is here.
    pub fn merge_choices_for(&self, device: &DeviceId, theirs: &[Choice]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for choice in theirs {
            tx.execute(
                "INSERT INTO phone_choices (device_id, folder, chosen, changed_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (device_id, folder) DO UPDATE SET chosen = excluded.chosen, changed_at = excluded.changed_at
                  WHERE excluded.changed_at > phone_choices.changed_at",
                params![device.as_bytes().as_slice(), choice.folder, choice.chosen as i64, choice.changed_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The folders ticked for a phone to back up, wherever they were ticked.
    pub fn chosen_for(&self, device: &DeviceId) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT folder FROM phone_choices WHERE device_id = ?1 AND chosen = 1 ORDER BY folder",
        )?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice()], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Remember where a file kept outside the folder is read from, as it was
    /// last listed (decision 0062).
    pub fn note_outside_source(&self, file_id: i64, source: &str, size: u64, mtime_ns: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO outside_sources (file_id, source, size, mtime_ns)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (file_id) DO UPDATE SET
                 source = excluded.source, size = excluded.size, mtime_ns = excluded.mtime_ns",
            params![file_id, source, size as i64, mtime_ns],
        )?;
        Ok(())
    }

    /// Stop reading a file kept outside the folder: it has gone from there.
    pub fn forget_outside_source(&self, file_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM outside_sources WHERE file_id = ?1", params![file_id])?;
        Ok(())
    }

    /// The live files of this device's own vault beneath `prefix`, with where
    /// each is read from while it is still there to read (decision 0062).
    pub fn outside_under(&self, prefix: &str) -> Result<Vec<OutsideRow>> {
        let me = self.local_device()?;
        let pattern = format!("{}/%", prefix.trim_end_matches('/'));
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.path, f.size, f.content_hash, o.source, o.size, o.mtime_ns
               FROM files f
               LEFT JOIN outside_sources o ON o.file_id = f.id
              WHERE f.scope = ?1 AND f.deleted_at IS NULL AND f.path LIKE ?2 ESCAPE '\\'
              ORDER BY f.path",
        )?;
        let rows = stmt.query_map(params![me.as_bytes().as_slice(), escape_like(&pattern)], |r| {
            let hash: Vec<u8> = r.get(3)?;
            Ok(OutsideRow {
                file_id: r.get(0)?,
                path: r.get(1)?,
                size: r.get::<_, i64>(2)? as u64,
                content: to_hash(&hash),
                source: match r.get::<_, Option<String>>(4)? {
                    Some(source) => Some(OutsideSource {
                        source,
                        size: r.get::<_, i64>(5)? as u64,
                        mtime_ns: r.get(6)?,
                    }),
                    None => None,
                },
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Remember where a send's bytes are to be read from (decision 0060).
    pub fn note_send_source(
        &self,
        file_id: i64,
        source: &str,
        size: u64,
        mtime_ns: i64,
        temporary: bool,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO send_sources (file_id, source, size, mtime_ns, temporary)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (file_id) DO UPDATE SET
                 source = excluded.source, size = excluded.size,
                 mtime_ns = excluded.mtime_ns, temporary = excluded.temporary",
            params![file_id, source, size as i64, mtime_ns, temporary as i64],
        )?;
        Ok(())
    }

    /// Every send read from where it is, with whether its recipient has
    /// collected it yet.
    pub fn send_sources(&self) -> Result<Vec<SendSource>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.path, f.scope, f.content_hash, f.deleted_at IS NOT NULL,
                    s.source, s.size, s.mtime_ns, s.temporary,
                    EXISTS (SELECT 1 FROM replicas r
                             WHERE r.content_hash = f.content_hash AND r.device_id = f.scope)
               FROM send_sources s
               JOIN files f ON f.id = s.file_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, bool>(8)?,
                r.get::<_, bool>(9)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (file_id, path, scope, hash, deleted, source, size, mtime_ns, temporary, collected) =
                row?;
            let Some(to) = scope.and_then(to_device) else { continue };
            out.push(SendSource {
                file_id,
                path,
                to,
                content: blake3::Hash::from(<[u8; 32]>::try_from(hash).unwrap_or([0; 32])),
                source,
                size: size as u64,
                mtime_ns,
                temporary,
                done: deleted || collected,
            });
        }
        Ok(out)
    }

    /// A send's file was found not to hold what was sent: recorded as a size
    /// it cannot have, which the next check reads as changed.
    pub fn send_source_changed(&self, source: &str) -> Result<()> {
        self.conn.execute("UPDATE send_sources SET size = -1 WHERE source = ?1", params![source])?;
        Ok(())
    }

    /// Stop reading a send from where it was.
    pub fn forget_send_source(&self, file_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM send_sources WHERE file_id = ?1", params![file_id])?;
        Ok(())
    }

    /// The send's file has been seen unchanged since it was sent, at this
    /// modification time.
    pub fn send_source_seen(&self, file_id: i64, mtime_ns: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE send_sources SET mtime_ns = ?2 WHERE file_id = ?1",
            params![file_id, mtime_ns],
        )?;
        Ok(())
    }

    /// Total size of every live file, as the user would count it.
    ///
    /// Distinct from the plaintext total in [`size_totals`](Self::size_totals),
    /// which sums *chunks* and therefore counts shared content once. Three
    /// copies of one file are 3x here and 1x there, and the gap between the two
    /// is exactly what deduplication saved.
    pub fn live_bytes(&self) -> Result<u64> {
        let total: i64 = self.conn.query_row(
            // Not the sharing rules: bookkeeping, not a person's files.
            &format!(
                "SELECT coalesce(sum(size), 0) FROM files
                  WHERE deleted_at IS NULL
                    AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))
                    AND {NOT_RULES}"
            ),
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
    /// Other devices holding these bytes that a device may free its own copy
    /// for: any but a phone, whose copy one tap in its settings can erase
    /// (decision 0053). On 2026-10-05 a laptop had freed 18 files because a
    /// phone kept them, and the phone's data was cleared.
    pub fn safe_copies_elsewhere(&self, content: &blake3::Hash) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            &format!(
                "SELECT count(*) FROM replicas r
                  WHERE r.content_hash = ?1 AND r.private = 0 AND {SAFE_ELSEWHERE}"
            ),
            params![content.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Whether a device a phone may free its copy for (as
    /// [`safe_copies_elsewhere`](Self::safe_copies_elsewhere)) said, asked,
    /// that it holds these bytes ([`note_held`](Self::note_held)). A record
    /// on any other word may be stale: a laptop that freed shared files
    /// before decision 0053, on the strength of a phone's copy, is still
    /// recorded by that phone as keeping them. What a phone deletes its last
    /// copy on the strength of (decision 0073, item 2).
    pub fn confirmed_safe_elsewhere(&self, content: &blake3::Hash) -> Result<bool> {
        let found: bool = self.conn.query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM replicas r
                                  JOIN confirmed c ON c.device_id = r.device_id AND c.content_hash = r.content_hash
                                 WHERE r.content_hash = ?1 AND r.private = 0 AND {SAFE_ELSEWHERE})"
            ),
            params![content.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(found)
    }

    /// Other devices holding these bytes that this device could ask for them:
    /// paired, and not known to be out of reach ([`ASKABLE`]).
    pub fn replica_count(&self, content: &blake3::Hash) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            &format!("SELECT count(*) FROM replicas r WHERE r.content_hash = ?1 AND {ASKABLE}"),
            params![content.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Content to ask `device` about, not asked about before: what a sync
    /// with it asks, a few at a time (decision 0055). Three kinds:
    ///
    /// - files freed here that it is recorded as holding, on the word of
    ///   whoever made them -- which nothing withdrew when that device freed
    ///   its own;
    /// - files whose copy there is marked out of reach. Removing a device
    ///   marks all of them, and pairing it again restored none: on
    ///   2026-10-08 a laptop called three files the only copy, minutes after
    ///   the phone it had removed and paired again had synced them;
    /// - shared files made by a device no longer paired, with nothing
    ///   recorded about `device`. Holdings are reported to a file's maker, so
    ///   with the maker gone nobody reports them: the same day a phone set up
    ///   again called three of its old photos the only copy, which the laptop
    ///   also had.
    ///
    /// And on a phone, shared files it still has the bytes of that `device`
    /// is recorded as holding: leaving the shared area deletes those only
    /// once a computer has said so (decision 0073, item 2).
    pub fn unconfirmed_holdings(&self, device: &DeviceId, limit: usize) -> Result<Vec<blake3::Hash>> {
        // Asked first: a phone holds them back from leaving the shared area
        // until answered, and the rest of what it asks about -- its freed
        // files, WhatsApp's tens of thousands among them -- would keep them
        // waiting for weeks at a few a pass.
        let (leaving, first) = match self.local_kind()?.as_deref() == Some("phone") {
            true => (
                "OR (f.scope IS NULL AND f.materialised = 1 AND r.private = 0)",
                "ORDER BY max(f.scope IS NULL AND f.materialised = 1) DESC",
            ),
            false => ("", ""),
        };
        // Not another person's device: what it holds is in its own vault,
        // which its server rightly shows nobody, and is never a copy this
        // device could ask for (decision 0060). Asking would only learn no.
        let mut stmt = self.conn.prepare(&format!(
            "SELECT f.content_hash
               FROM files f
              WHERE f.deleted_at IS NULL
                AND NOT EXISTS (SELECT 1 FROM peer_relations pr WHERE pr.device_id = ?1)
                AND (f.scope IS NULL OR f.scope = (SELECT device_id FROM local WHERE id = 1))
                AND NOT EXISTS (
                      SELECT 1 FROM confirmed c
                       WHERE c.device_id = ?1 AND c.content_hash = f.content_hash
                    )
                AND (
                      EXISTS (
                        SELECT 1 FROM replicas r
                         WHERE r.content_hash = f.content_hash AND r.device_id = ?1
                           AND ((f.materialised = 0 AND r.private = 0) OR r.private = 1 {leaving})
                      )
                   OR (f.scope IS NULL
                       AND f.modified_by != (SELECT device_id FROM local WHERE id = 1)
                       AND NOT EXISTS (SELECT 1 FROM peers p WHERE p.device_id = f.modified_by)
                       AND NOT EXISTS (
                             SELECT 1 FROM replicas r
                              WHERE r.content_hash = f.content_hash AND r.device_id = ?1
                           ))
                    )
              GROUP BY f.content_hash {first}
              LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice(), limit as i64], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(to_hash(&raw))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Files of this device's own vault -- its Private Vault, and WhatsApp's
    /// and chosen folders' kept where they lie -- with no record of `device`
    /// holding them: up to `limit`, chosen at random, to ask it about
    /// (`qurb_peer::confirm_kept`). At random, so that files it has not
    /// collected yet, asked and answered no, do not stand in front of the
    /// rest pass after pass.
    pub fn kept_unconfirmed(&self, device: &DeviceId, limit: usize) -> Result<Vec<blake3::Hash>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT content_hash FROM (
                 SELECT f.content_hash
                   FROM files f
                  WHERE f.scope = (SELECT device_id FROM local WHERE id = 1)
                    AND f.deleted_at IS NULL
                    AND NOT EXISTS (
                          SELECT 1 FROM replicas r
                           WHERE r.content_hash = f.content_hash AND r.device_id = ?1 AND r.private = 0
                        )
                  ORDER BY random()
                  LIMIT ?2
             )",
        )?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice(), limit as i64], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(to_hash(&raw))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// `device`, asked, said it holds this content: a copy this device can ask
    /// for, whatever its record said before, and recorded if there was none.
    pub fn note_held(&self, device: &DeviceId, content: &blake3::Hash) -> Result<()> {
        let id = device.as_bytes().as_slice();
        let content = content.as_bytes().as_slice();
        self.conn.execute(
            "INSERT INTO replicas (content_hash, device_id, at, private)
             VALUES (?2, ?1, unixepoch(), 0)
             ON CONFLICT (content_hash, device_id) DO UPDATE SET private = 0",
            params![id, content],
        )?;
        self.note_asked(id, content)
    }

    /// `device`, asked or fetched from, said it does not hold this content: a
    /// copy this device cannot ask for, as a removed device's is
    /// ([`forget_peer`](Self::forget_peer)). Marked rather than deleted, for
    /// the same reason; a later report of holding it marks it back.
    pub fn note_not_held(&self, device: &DeviceId, content: &blake3::Hash) -> Result<()> {
        let id = device.as_bytes().as_slice();
        let content = content.as_bytes().as_slice();
        self.conn.execute(
            "UPDATE replicas SET private = 1 WHERE device_id = ?1 AND content_hash = ?2",
            params![id, content],
        )?;
        self.note_asked(id, content)
    }

    /// Asked once, whatever the answer: not asked again unless the device is
    /// removed and paired again.
    fn note_asked(&self, device: &[u8], content: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO confirmed (device_id, content_hash, at) VALUES (?1, ?2, unixepoch())
             ON CONFLICT (device_id, content_hash) DO UPDATE SET at = excluded.at",
            params![device, content],
        )?;
        Ok(())
    }

    /// The other devices holding these bytes that would hand them back: the
    /// ones a details screen names under "on these devices". Most recently
    /// reported first. Vault deliveries are left out for the same reason as in
    /// [`replica_count`](Self::replica_count).
    pub fn holders_of_content(&self, content: &blake3::Hash) -> Result<Vec<DeviceId>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT r.device_id FROM replicas r
              WHERE r.content_hash = ?1 AND {ASKABLE}
              ORDER BY r.at DESC"
        ))?;
        let rows = stmt.query_map(params![content.as_bytes().as_slice()], |r| r.get::<_, Vec<u8>>(0))?;
        let mut holders = Vec::new();
        for raw in rows {
            if let Some(id) = to_device(raw?) {
                holders.push(id);
            }
        }
        Ok(holders)
    }

    /// What this device could free without losing anything: files in its
    /// folder whose bytes are here and that another device is known to hold
    /// (brief §20, "space that can safely be freed"). The size and number of
    /// all of them, and the `largest` biggest, which is where freeing a little
    /// saves the most.
    ///
    /// The same test [`Store::free_local`](crate::Store::free_local) applies
    /// before it frees anything, so a file offered here is one it will free.
    pub fn freeable(&self, largest: usize) -> Result<Freeable> {
        let condition = format!(
            "f.deleted_at IS NULL
             AND (f.scope IS NULL OR f.scope = (SELECT device_id FROM local WHERE id = 1))
             AND {NOT_RULES_F}
             AND f.materialised = 1
             AND EXISTS (SELECT 1 FROM replicas r
                          WHERE r.content_hash = f.content_hash AND r.private = 0
                            AND {SAFE_ELSEWHERE})"
        );
        let (count, bytes): (i64, i64) = self.conn.query_row(
            &format!("SELECT count(*), coalesce(sum(f.size), 0) FROM files f WHERE {condition}"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT f.path, f.size, f.mtime_ns, f.scope IS NOT NULL FROM files f
              WHERE {condition}
              ORDER BY f.size DESC, f.path
              LIMIT ?1"
        ))?;
        let files = stmt
            .query_map(params![largest as i64], |r| {
                Ok(FolderEntry {
                    path: r.get(0)?,
                    size: r.get::<_, i64>(1)? as u64,
                    mtime_ns: r.get(2)?,
                    availability: Availability::Here,
                    private: r.get::<_, i64>(3)? != 0,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(Freeable { count: count as usize, bytes: bytes as u64, files })
    }

    /// Note that a path was just read or written here.
    pub fn touch(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET touched_at = unixepoch() WHERE path = ?1
              AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))",
            params![path],
        )?;
        Ok(())
    }

    /// A page of live files, with everything a listing needs, in path order.
    ///
    /// The shared area only. A device's own vault is a different list with a
    /// different meaning, and mixing the two would put content somebody sent
    /// you in the middle of your own folder listing.
    ///
    /// `under` restricts to one directory and everything beneath it, spelled
    /// the way [`Db::live_paths_under`] spells it, so that a listing and a
    /// deletion agree about what "in this folder" means.
    pub fn listing(&self, under: Option<&str>, limit: usize, offset: usize) -> Result<Vec<Listed>> {
        let pattern = under.map(|u| format!("{}/%", u.trim_end_matches('/')));
        let exact = under.map(|u| u.trim_end_matches('/').to_string());
        let mut stmt = self.conn.prepare(
            "SELECT path, size, updated_at, materialised, content_hash
               FROM files
              WHERE deleted_at IS NULL
                AND scope IS NULL
                AND substr(path, 1, 14) <> '.qurb-sharing/'
                AND (?1 IS NULL OR path = ?1 OR path LIKE ?2 ESCAPE '\\')
              ORDER BY path
              LIMIT ?3 OFFSET ?4",
        )?;
        // Escaped, so that a folder called `a_b` does not also list `axb`.
        let pattern = pattern.map(|p| escape_like(&p));
        let rows =
            stmt.query_map(params![exact, pattern, limit as i64, offset as i64], listed_row)?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// How many live files the shared area holds, for paging.
    pub fn live_count(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            &format!("SELECT count(*) FROM files WHERE deleted_at IS NULL AND scope IS NULL AND {NOT_RULES}"),
            [],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Live files whose path contains `text`, case-insensitively.
    ///
    /// By path, not by content. Searching contents would mean an index of what
    /// every file says — a second database, and a much larger promise than this
    /// product has made. Searching names is what people do most of the time and
    /// is answerable from what the index already holds.
    ///
    /// SQLite folds case for ASCII only, so `CAFÉ` does not match `café`. Said
    /// plainly rather than papered over: the alternative is carrying a
    /// Unicode-aware collation for a feature nobody has asked to be perfect.
    ///
    /// Newest first, because a search is usually for something recent.
    pub fn search(&self, text: &str, limit: usize) -> Result<Vec<Listed>> {
        // `%` and `_` in what somebody typed are literal, not wildcards. A
        // search for `report_final` must not match `reportXfinal`.
        let escaped = text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        let mut stmt = self.conn.prepare(
            "SELECT path, size, updated_at, materialised, content_hash
               FROM files
              WHERE deleted_at IS NULL
                AND scope IS NULL
                AND substr(path, 1, 14) <> '.qurb-sharing/'
                AND path LIKE ?1 ESCAPE '\\'
              ORDER BY updated_at DESC, path
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], listed_row)?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Live files this device could drop the bytes of, coldest first.
    ///
    /// Three conditions, all of them necessary:
    ///
    /// - **Materialised.** There is nothing to free in a file already evicted.
    /// - **Known to be elsewhere.** At least one other device has taken
    ///   delivery of this exact content, and it is not a phone (decision
    ///   0053). Without that this is the only copy, or the only one besides a
    ///   phone's, and dropping it is not eviction but deletion.
    /// - **Not a tombstone.** Deleted files are the garbage collector's
    ///   problem, not the cap's.
    ///
    /// Ordered by last touch, oldest first, then by size largest first so that
    /// among equally cold files the one that frees the most goes first.
    pub fn evictable(&self) -> Result<Vec<(String, u64, blake3::Hash)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT f.path, f.size, f.content_hash
               FROM files f
              WHERE f.deleted_at IS NULL
                AND f.materialised = 1
                AND (f.scope IS NULL OR f.scope = (SELECT device_id FROM local WHERE id = 1))
                -- A rule file is never freed: unreadable, it would close its
                -- folder to everybody.
                AND substr(f.path, 1, 14) <> '.qurb-sharing/'
                AND EXISTS (
                      SELECT 1 FROM replicas r
                       WHERE r.content_hash = f.content_hash AND r.private = 0
                         AND {SAFE_ELSEWHERE}
                    )
              ORDER BY f.touched_at ASC, f.size DESC"
        ))?;
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
             WHERE path = ?1 AND deleted_at IS NULL
               AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))",
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
                  WHERE path = ?1 AND deleted_at IS NULL
                    AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))",
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
                AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))
              ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Bytes of live files this device is actually holding.
    pub fn materialised_bytes(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT coalesce(sum(size), 0) FROM files
              WHERE deleted_at IS NULL AND materialised = 1
                AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Ask for a file's bytes back. Acted on the next time a peer is reachable.
    pub fn want(&self, path: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE files SET wanted = 1
             WHERE path = ?1 AND deleted_at IS NULL AND materialised = 0
               AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))",
            params![path],
        )?;
        Ok(changed > 0)
    }

    /// Whether somebody asked for this path's bytes.
    pub fn is_wanted(&self, path: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM files
                             WHERE path = ?1 AND wanted = 1 AND deleted_at IS NULL
                               AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1)))",
            params![path],
            |r| r.get(0),
        )?)
    }

    /// Keep `folder` only remotely on this device, or stop.
    pub fn set_kept_remotely(&self, folder: &str, remote: bool) -> Result<()> {
        match remote {
            true => self.conn.execute(
                "INSERT OR IGNORE INTO remote_folders (folder) VALUES (?1)",
                params![folder],
            )?,
            false => self.conn.execute("DELETE FROM remote_folders WHERE folder = ?1", params![folder])?,
        };
        Ok(())
    }

    /// The folders this device keeps only remotely.
    pub fn remote_folders(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT folder FROM remote_folders ORDER BY folder")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Whether `path` is in a folder this device keeps only remotely.
    pub fn kept_remotely(&self, path: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM remote_folders
                             WHERE ?1 = folder OR substr(?1, 1, length(folder) + 1) = folder || '/')",
            params![path],
            |r| r.get(0),
        )?)
    }

    /// Live shared-area paths in `folder`, and whether each is here.
    pub fn shared_under(&self, folder: &str) -> Result<Vec<(String, bool)>> {
        let pattern = escape_like(&format!("{}/%", folder.trim_matches('/')));
        let mut stmt = self.conn.prepare(
            "SELECT path, materialised FROM files
              WHERE scope IS NULL AND deleted_at IS NULL AND path LIKE ?1 ESCAPE '\\'
              ORDER BY path",
        )?;
        let rows = stmt.query_map(params![pattern], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Paths asked for that this device is not holding yet.
    pub fn wanted_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE wanted = 1 AND materialised = 0 AND deleted_at IS NULL
                AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))
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
        let mut stmt = self.conn.prepare(&format!(
            "SELECT f.path, f.size
               FROM files f
              WHERE f.deleted_at IS NULL
                AND (f.scope IS NULL OR f.scope = (SELECT device_id FROM local WHERE id = 1))
                AND f.modified_by = (SELECT device_id FROM local WHERE id = 1)
                AND {NOT_RULES_F}
                AND NOT EXISTS (
                      SELECT 1 FROM replicas r WHERE r.content_hash = f.content_hash
                    )
              ORDER BY f.updated_at DESC, f.path"
        ))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Everything whose bytes no other device is known to hold: what would be
    /// gone if this device were wiped, largest first, with whether each is
    /// private (decision 0053). Wherever it came from -- made here, sent here,
    /// or left here when another device freed its copy -- and including sends
    /// not collected yet; not what this device keeps for another, whose owner
    /// has it.
    pub fn only_here(&self) -> Result<Vec<(String, u64, bool)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT f.path, f.size, f.scope IS NOT NULL
               FROM files f
              WHERE f.deleted_at IS NULL
                AND f.held = 0
                AND {NOT_RULES_F}
                AND NOT EXISTS (SELECT 1 FROM replicas r WHERE r.content_hash = f.content_hash)
              ORDER BY f.size DESC, f.path"
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? != 0))
        })?;
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

    /// Whether `device` has been told this content is held here.
    pub fn was_reported(&self, device: &DeviceId, content: &blake3::Hash) -> Result<bool> {
        let told: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM reported WHERE device_id = ?1 AND content_hash = ?2
             )",
            params![device.as_bytes().as_slice(), content.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(told)
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

    /// This device's own name and picture, as it tells devices that ask,
    /// once one has been set here (the owner's list of 2026-10-26, item 7).
    pub fn own_profile(&self) -> Result<Option<Profile>> {
        self.conn
            .query_row("SELECT name, picture, changed_at FROM own_profile WHERE id = 1", [], |r| {
                Ok(Profile { name: r.get(0)?, picture: r.get(1)?, changed_at: r.get(2)? })
            })
            .optional()
            .map_err(Into::into)
    }

    /// Set this device's name, its picture, or both, now: `picture` `Some(None)`
    /// takes the picture away. A device asking next is told.
    pub fn set_own_profile(&self, name: Option<&str>, picture: Option<Option<&[u8]>>) -> Result<Profile> {
        let now = now_ms();
        let current = self.own_profile()?;
        let profile = Profile {
            name: name.map(str::to_string).or_else(|| current.as_ref().map(|p| p.name.clone())).unwrap_or_default(),
            picture: match picture {
                Some(picture) => picture.map(<[u8]>::to_vec),
                None => current.and_then(|p| p.picture),
            },
            // Later than the last, whatever the clock says: a device asking
            // with the last one's time must hear of this one.
            changed_at: now,
        };
        self.conn.execute(
            "INSERT INTO own_profile (id, name, picture, changed_at) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, picture = excluded.picture,
                 changed_at = max(excluded.changed_at, own_profile.changed_at + 1)",
            params![profile.name, profile.picture, profile.changed_at],
        )?;
        Ok(self.own_profile()?.unwrap_or(profile))
    }

    /// A paired device's name and picture, as it last said.
    pub fn peer_profile(&self, device: &DeviceId) -> Result<Option<Profile>> {
        self.conn
            .query_row(
                "SELECT name, picture, changed_at FROM peer_profiles WHERE device_id = ?1",
                params![device.as_bytes().as_slice()],
                |r| Ok(Profile { name: r.get(0)?, picture: r.get(1)?, changed_at: r.get(2)? }),
            )
            .optional()
            .map_err(Into::into)
    }

    /// When a paired device's picture last changed, if it has one: whether a
    /// circle drawn from it is still right, without reading the picture.
    pub fn peer_picture_at(&self, device: &DeviceId) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT changed_at FROM peer_profiles WHERE device_id = ?1 AND picture IS NOT NULL",
                params![device.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// What a paired device said its name and picture are: kept, and its name
    /// put where every list of devices reads it -- a device renaming itself
    /// was known by its pairing name for good until this.
    pub fn set_peer_profile(&self, device: &DeviceId, profile: &Profile) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let newer = tx.execute(
            "INSERT INTO peer_profiles (device_id, name, picture, changed_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (device_id) DO UPDATE SET name = excluded.name, picture = excluded.picture,
                 changed_at = excluded.changed_at
              WHERE excluded.changed_at > peer_profiles.changed_at",
            params![device.as_bytes().as_slice(), profile.name, profile.picture, profile.changed_at],
        )? > 0;
        if newer && !profile.name.trim().is_empty() {
            tx.execute(
                "UPDATE peers SET name = ?2 WHERE device_id = ?1",
                params![device.as_bytes().as_slice(), profile.name.trim()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The live files of this device's own vault at `prefix`: the file of
    /// that name, or every file in the folder of that name, by path; `""`
    /// for all of them. What a
    /// folder of the phone's folder on a computer is, changed from the phone
    /// (the owner's list of 2026-10-26, item 13).
    pub fn own_vault_paths_under(&self, prefix: &str) -> Result<Vec<String>> {
        let me = self.local_device()?;
        let prefix = prefix.trim_matches('/');
        let pattern = format!("{prefix}/%");
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE deleted_at IS NULL AND scope = ?3
                AND (?1 = '' OR path = ?1 OR path LIKE ?2 ESCAPE '\\')
              ORDER BY path",
        )?;
        let rows = stmt.query_map(params![prefix, escape_like(&pattern), me.as_bytes().as_slice()], |r| r.get(0))?;
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
                    "SELECT {FILE_COLUMNS} FROM files WHERE scope IS NULL AND {} ORDER BY path",
                    shared_with("path", "NULL")
                ))?;
                let rows = stmt.query_map([], file_row)?;
                rows.map(|row| Ok(row_to_version(&row?))).collect()
            }
            Audience::Guest(asker) => {
                // Only what is its own: sends into its vault, and what this
                // computer keeps for its person -- sealed, by them, so nothing
                // here can read it. Never the shared area, never anything kept
                // for anyone else (decision 0060).
                let person = self.person_of(asker)?.unwrap_or(*asker);
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {FILE_COLUMNS}, held FROM files
                      WHERE (scope = ?1 AND held = 0) OR (scope = ?2 AND held = 1)
                      ORDER BY path"
                ))?;
                let rows = stmt.query_map(params![asker.as_bytes().as_slice(), person.as_bytes().as_slice()], |r| {
                    Ok((file_row(r)?, r.get::<_, i64>(FILE_COLUMN_COUNT)? != 0))
                })?;
                let mut out = Vec::new();
                for row in rows {
                    let (row, held) = row?;
                    let area = if held { qurb_sync::Area::Held } else { qurb_sync::Area::Sent };
                    out.push(row_to_version(&row).in_area(area));
                }
                Ok(out)
            }
            Audience::Device(asker) => {
                // Two queries rather than one, so that each version is marked
                // with where it came from. The asker cannot tell a shared file
                // from something sent to it privately by looking at the path,
                // and it has exactly one chance to file it correctly.
                let mut out = Vec::new();
                // The shared area, less any folder shared without it.
                let mut shared = self.conn.prepare(&format!(
                    "SELECT {FILE_COLUMNS} FROM files WHERE scope IS NULL AND {} ORDER BY path",
                    shared_with("path", "?1")
                ))?;
                for row in shared.query_map(params![asker.as_bytes().as_slice()], file_row)? {
                    out.push(row_to_version(&row?));
                }
                // Its vault: what was sent to it, and what this device keeps
                // for it, told apart so that it neither collects its own files
                // as deliveries nor mistakes a send for a backup.
                let mut theirs = self.conn.prepare(&format!(
                    "SELECT {FILE_COLUMNS}, held FROM files WHERE scope = ?1 ORDER BY path"
                ))?;
                let rows = theirs.query_map(params![asker.as_bytes().as_slice()], |r| {
                    Ok((file_row(r)?, r.get::<_, i64>(FILE_COLUMN_COUNT)? != 0))
                })?;
                for row in rows {
                    let (row, held) = row?;
                    let area = if held { qurb_sync::Area::Held } else { qurb_sync::Area::Sent };
                    out.push(row_to_version(&row).in_area(area));
                }

                // And this device's own vault, for a device allowed to hold it
                // (decision 0036). Tombstones included: deleting is the one
                // instruction a holder acts on.
                //
                // Less the folders kept outside it while they are not offered
                // (decision 0062): not on mobile data. A holder does nothing
                // about a file missing from this list, so leaving them out
                // costs nothing but the wait.
                if self.is_holder(asker)? {
                    let me = self.local_device()?;
                    let outside = match self.outside_offered()? {
                        true => "",
                        false => "AND substr(path, 1, 14) <> '.qurb-outside/'",
                    };
                    let mut mine = self.conn.prepare(&format!(
                        "SELECT {FILE_COLUMNS} FROM files WHERE scope = ?1 {outside} ORDER BY path"
                    ))?;
                    for row in mine.query_map(params![me.as_bytes().as_slice()], file_row)? {
                        out.push(row_to_version(&row?).in_area(qurb_sync::Area::Hold));
                    }
                }
                Ok(out)
            }
        }
    }

    /// The folders at the top of the shared area, by name.
    pub fn top_folders(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT DISTINCT substr(path, 1, instr(path, '/') - 1) AS folder FROM files
              WHERE scope IS NULL AND deleted_at IS NULL AND instr(path, '/') > 0 AND {NOT_RULES}
              ORDER BY folder"
        ))?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Every version in the shared area, sharing rules or not.
    pub fn shared_versions(&self) -> Result<Vec<FileVersion>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE scope IS NULL ORDER BY path"
        ))?;
        let rows = stmt.query_map([], file_row)?;
        rows.map(|row| Ok(row_to_version(&row?))).collect()
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

    /// Whether the last thing recorded about `path` is this same failure.
    pub fn failed_last_with(&self, path: &str, detail: &str) -> Result<bool> {
        let last: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT kind, detail FROM activity WHERE path = ?1 ORDER BY at DESC, id DESC LIMIT 1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(matches!(last, Some((kind, Some(said))) if kind == Event::Failed.as_str() && said == detail))
    }

    /// Forget that `path` did not arrive, once that has been settled another
    /// way. The failures say something no longer true, and on a phone's Home
    /// they were the most recent thing it had to show.
    pub fn forget_failures(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM activity WHERE kind = ?1 AND path = ?2",
            params![Event::Failed.as_str(), path],
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
            // Never a file of a folder kept outside a vault (decision 0062):
            // a phone's WhatsApp, kept by this computer out of sight.
            "SELECT id, at, kind, path, size, device, detail
               FROM activity
              WHERE (?1 IS NULL OR id < ?1)
                AND (path IS NULL OR substr(path, 1, 14) <> '.qurb-outside/')
              ORDER BY at DESC, id DESC
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![before, limit as i64], activity_row)?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// A page of everything in this device's folder: the shared area and its
    /// own vault, in path order, with where each file's bytes are.
    ///
    /// One query, availability included, because a phone lists its files on
    /// every screen and a query per file would make a large library slow to
    /// open.
    pub fn folder_listing(&self, limit: usize, offset: usize) -> Result<Vec<FolderEntry>> {
        let tail = format!("LIMIT {} OFFSET {}", limit as i64, offset as i64);
        self.folder_entries("1", params![], &tail)
    }

    /// Everything in this device's folder beneath `dir`, at any depth, in path
    /// order: what a file browser needs to show one directory, since the index
    /// is flat and a directory is only the paths that begin with it. The empty
    /// string is the whole folder.
    pub fn folder_entries_under(&self, dir: &str) -> Result<Vec<FolderEntry>> {
        let dir = dir.trim_matches('/');
        if dir.is_empty() {
            return self.folder_entries("1", params![], "");
        }
        let pattern = escape_like(&format!("{dir}/%"));
        self.folder_entries("f.path LIKE ?1 ESCAPE '\\'", params![pattern], "")
    }

    /// One file in this device's folder, with where its bytes are. The shared
    /// area first, as [`folder_row`](Self::folder_row) looks.
    pub fn folder_entry(&self, path: &str) -> Result<Option<FolderEntry>> {
        let mut found = self.folder_entries("f.path = ?1", params![path], "")?;
        found.sort_by_key(|entry| entry.private);
        Ok(found.into_iter().next())
    }

    /// Files in this device's folder whose path contains `text`, ignoring
    /// case as far as SQLite's LIKE does (ASCII).
    pub fn folder_search(&self, text: &str, limit: usize) -> Result<Vec<FolderEntry>> {
        let escaped = text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        self.folder_entries(
            "f.path LIKE ?1 ESCAPE '\\'",
            params![format!("%{escaped}%")],
            &format!("LIMIT {}", limit as i64),
        )
    }

    /// The one query behind every folder listing: live files in the shared
    /// area or this device's own vault, availability included, filtered by
    /// `condition`. One query rather than one per file, because a phone lists
    /// on every screen and a file picker lists while someone waits.
    fn folder_entries(
        &self,
        condition: &str,
        values: &[&dyn rusqlite::ToSql],
        tail: &str,
    ) -> Result<Vec<FolderEntry>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT f.path, f.size,
                    -- A file listed without its bytes was given no time
                    -- before 2026-10-07; when it last changed here stands in.
                    CASE WHEN f.mtime_ns = 0 THEN f.updated_at * 1000000000 ELSE f.mtime_ns END,
                    f.materialised, f.scope IS NOT NULL,
                    EXISTS (SELECT 1 FROM replicas r
                             WHERE r.content_hash = f.content_hash AND {ASKABLE})
               FROM files f
              WHERE f.deleted_at IS NULL
                AND (f.scope IS NULL OR f.scope = (SELECT device_id FROM local WHERE id = 1))
                AND {NOT_RULES_F}
                AND ({condition})
              ORDER BY f.path
              {tail}"
        ))?;
        let rows = stmt.query_map(values, |r| {
            Ok(FolderEntry {
                path: r.get(0)?,
                size: r.get::<_, i64>(1)? as u64,
                mtime_ns: r.get(2)?,
                availability: Availability::of(r.get::<_, i64>(3)? != 0, r.get::<_, i64>(5)? != 0),
                private: r.get::<_, i64>(4)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// One entry, by its id.
    /// What happened, newest first, told once each (the owner's list of
    /// 2026-10-26, item 8: "it shows 1file 3 times"): `limit` entries of
    /// [`activity`](Self::activity) after [`told_once`] has gone through it.
    /// More are read than are shown, so that a page is not short for what
    /// was folded together.
    pub fn activity_told(&self, limit: usize, before: Option<i64>) -> Result<Vec<Activity>> {
        let mut told = told_once(self.activity(limit.saturating_mul(4).max(limit), before)?);
        told.truncate(limit);
        Ok(told)
    }

    /// Forget what happened more than [`ACTIVITY_KEPT`] ago: history is kept
    /// for 30 days (the owner's list of 2026-10-26, item 8).
    pub fn forget_old_activity(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM activity WHERE at < ?1", params![now() - ACTIVITY_KEPT])?)
    }

    pub fn activity_entry(&self, id: i64) -> Result<Option<Activity>> {
        self.conn
            .query_row(
                "SELECT id, at, kind, path, size, device, detail FROM activity WHERE id = ?1",
                params![id],
                activity_row,
            )
            .optional()
            .map_err(Into::into)
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
                AND f.held = 0
                AND f.scope != (SELECT device_id FROM local WHERE id = 1)
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
                AND f.held = 0
                AND f.scope != (SELECT device_id FROM local WHERE id = 1)
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

    /// Whether `device` has told this one it holds this content, in any way.
    ///
    /// The same test [`pending_deliveries`](Self::pending_deliveries) uses to
    /// decide a send is no longer waiting: any record of the device holding
    /// the bytes, private or not.
    pub fn device_holds(&self, content: &blake3::Hash, device: &DeviceId) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM replicas WHERE content_hash = ?1 AND device_id = ?2
             )",
            params![content.as_bytes().as_slice(), device.as_bytes().as_slice()],
            |r| r.get(0),
        )?)
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

    /// A live row for a path that is materialised in *this device's folder*.
    ///
    /// The folder holds two kinds of file: content shared with every device,
    /// and content somebody sent to this one. Both are written into it, because
    /// a person asked for a file and should find a file — so anything that
    /// walks the folder and asks the index "do I know about this" has to look
    /// in both places.
    ///
    /// Shared first, because it is overwhelmingly the common case and because a
    /// path that is somehow in both should be treated as the shared one: that
    /// is what every other device believes about it.
    ///
    /// Live rows only, on both sides. A shared *tombstone* at the path does not
    /// count: a file of that name deleted long ago says nothing about a file
    /// somebody sent here since, and answering with the tombstone made the
    /// received file impossible to delete.
    pub fn in_folder(&self, path: &str) -> Result<Option<FileRow>> {
        Ok(self.folder_row(path)?.map(|(row, _)| row))
    }

    /// The same, and which area it is in: `None` for the shared area, this
    /// device's id for its own vault. What a write to the row has to name, so
    /// that it lands on the row that was found rather than on a namesake.
    pub fn folder_row(&self, path: &str) -> Result<Option<(FileRow, Option<DeviceId>)>> {
        if let Some(shared) = self.file_by_path(path)?.filter(|f| f.deleted_at.is_none()) {
            return Ok(Some((shared, None)));
        }
        let me = self.local_device()?;
        Ok(self.own_vault_row(path)?.map(|row| (row, Some(me))))
    }

    /// A live row for a path in this device's own vault: something sent here.
    pub fn own_vault_row(&self, path: &str) -> Result<Option<FileRow>> {
        let me = self.local_device()?;
        self.live_row_in(path, Some(&me))
    }

    /// The live row for a path in one area: `None` for the shared area, a
    /// device id for that device's vault.
    ///
    /// For a caller that already knows which area it means. A path can have a
    /// row in several, and a write that looked the path up without saying
    /// which would find a namesake.
    pub fn live_row_in(&self, path: &str, area: Option<&DeviceId>) -> Result<Option<FileRow>> {
        let Some(owner) = area else {
            return Ok(self.file_by_path(path)?.filter(|f| f.deleted_at.is_none()));
        };
        self.conn
            .query_row(
                &format!(
                    "SELECT {FILE_COLUMNS} FROM files
                      WHERE path = ?1 AND scope = ?2 AND deleted_at IS NULL"
                ),
                params![path, owner.as_bytes().as_slice()],
                file_row,
            )
            .optional()
            .map_err(Into::into)
    }

    /// The deleted row for a path in `owner`'s vault, while it still lists
    /// `content`: what a file deleted without its bytes here is put back from
    /// ([`Store::restore_from_trash`](crate::Store::restore_from_trash)).
    pub fn deleted_row_in(&self, path: &str, owner: &DeviceId, content: &blake3::Hash) -> Result<Option<FileRow>> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {FILE_COLUMNS} FROM files
                      WHERE path = ?1 AND scope = ?2 AND deleted_at IS NOT NULL AND content_hash = ?3"
                ),
                params![path, owner.as_bytes().as_slice(), content.as_bytes().as_slice()],
                file_row,
            )
            .optional()
            .map_err(Into::into)
    }

    /// A chunk's size in bytes, before compression, if the index knows it.
    pub fn chunk_size(&self, hash: &blake3::Hash) -> Result<Option<u64>> {
        self.conn
            .query_row("SELECT size FROM chunks WHERE hash = ?1", params![hash.as_bytes().as_slice()], |r| {
                r.get::<_, i64>(0)
            })
            .optional()
            .map(|size| size.map(|s| s as u64))
            .map_err(Into::into)
    }

    /// Where the index stands: the rows changed through this connection, and
    /// the count SQLite moves whenever another connection commits. Equal
    /// twice, nothing in the index changed in between -- which is what lets a
    /// summary that reads every row be kept and given again (the phone's
    /// backups, read on every refresh of its Home).
    pub fn change_mark(&self) -> Result<(u64, i64)> {
        let theirs: i64 = self.conn.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        Ok((self.conn.total_changes(), theirs))
    }

    /// Every live path materialised in this device's folder, shared or sent
    /// here. What a scan of the folder should expect to find.
    ///
    /// Used to notice deletions: a path the index lists and the walk did not
    /// find is gone. Before this included received files, deleting one went
    /// unnoticed and it stayed in the index for ever.
    ///
    /// Files kept where they lie (`OUTSIDE`, decision 0062) are left out:
    /// they are never in the folder, and until 2026-10-15 the scan asked
    /// about each of them in turn -- 37,500 questions on the S23, holding
    /// the engine over a second every time the app came back.
    pub fn folder_paths(&self) -> Result<Vec<String>> {
        let me = self.local_device()?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT DISTINCT path FROM files
              WHERE deleted_at IS NULL AND (scope IS NULL OR scope = ?1)
                AND substr(path, 1, {len}) <> '{OUTSIDE}/'
              ORDER BY path",
            len = OUTSIDE.len() + 1,
        ))?;
        let rows = stmt.query_map(params![me.as_bytes().as_slice()], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// The same, beneath one path. See [`Db::live_paths_under`].
    pub fn folder_paths_under(&self, prefix: &str) -> Result<Vec<String>> {
        let me = self.local_device()?;
        let pattern = format!("{}/%", prefix.trim_end_matches('/'));
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT path FROM files
              WHERE deleted_at IS NULL
                AND (scope IS NULL OR scope = ?3)
                AND (path = ?1 OR path LIKE ?2 ESCAPE '\\')
              ORDER BY path",
        )?;
        let rows = stmt.query_map(
            params![prefix.trim_end_matches('/'), escape_like(&pattern), me.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Whether this device has already taken this send (decision 0059).
    ///
    /// A send is who sent it, under what name, and which version of that
    /// name. Offered again, by a sender that reappears, it is the same send,
    /// and taking it again would bring back a file the person deleted. Sent
    /// again, it is a new version, and a new delivery even when the bytes are
    /// the same.
    ///
    /// What was taken before sends were told apart is recorded by content
    /// alone. It answers only for a version made before it was taken -- that
    /// is the old send offered again -- so that nothing already received
    /// arrives twice, and a file sent again since still does. The two clocks
    /// compared are different devices', which is the cost of a record that
    /// never held more.
    pub fn delivery_taken(&self, version: &FileVersion) -> Result<bool> {
        let Some(hash) = version.content.hash() else { return Ok(false) };
        let taken: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM deliveries WHERE sender = ?1 AND path = ?2 AND vector = ?3
             ) OR EXISTS (
                 SELECT 1 FROM taken
                  WHERE content_hash = ?4
                    AND (sender IS NULL OR sender = ?1)
                    AND taken_at >= ?5
             )",
            params![
                version.modified_by.as_bytes().as_slice(),
                version.path,
                version.vector.encode(),
                hash.as_slice(),
                version.modified_at
            ],
            |r| r.get(0),
        )?;
        Ok(taken)
    }

    /// Whether the sender of this send is still to be told it was taken.
    ///
    /// For a send recorded as one, its own flag. For one taken before sends
    /// were told apart, whether these bytes were ever reported to `peer`,
    /// which was the record then.
    pub fn unacknowledged(&self, version: &FileVersion, peer: &DeviceId) -> Result<bool> {
        let Some(hash) = version.content.hash() else { return Ok(false) };
        let flag: Option<bool> = self
            .conn
            .query_row(
                "SELECT acknowledged FROM deliveries WHERE sender = ?1 AND path = ?2 AND vector = ?3",
                params![version.modified_by.as_bytes().as_slice(), version.path, version.vector.encode()],
                |r| r.get(0),
            )
            .optional()?;
        match flag {
            Some(told) => Ok(!told),
            None => Ok(self.delivery_taken(version)?
                && !self.was_reported(peer, &blake3::Hash::from(*hash))?),
        }
    }

    /// The sender of this send has been told it was taken.
    pub fn acknowledge(&self, version: &FileVersion) -> Result<()> {
        self.conn.execute(
            "UPDATE deliveries SET acknowledged = 1 WHERE sender = ?1 AND path = ?2 AND vector = ?3",
            params![version.modified_by.as_bytes().as_slice(), version.path, version.vector.encode()],
        )?;
        Ok(())
    }

    /// The last time this device sent these bytes to `device`, if it has:
    /// the name it sent them under and when (decision 0059).
    ///
    /// Asked before sending, so that sending a file again is a choice the
    /// person makes knowing it went before. Tombstones count -- a send let go
    /// of after it arrived was still sent.
    pub fn sent_before(
        &self,
        content: &blake3::Hash,
        device: &DeviceId,
    ) -> Result<Option<(String, i64)>> {
        self.conn
            .query_row(
                "SELECT path, updated_at FROM files
                  WHERE scope = ?1 AND content_hash = ?2 AND held = 0
                  ORDER BY updated_at DESC LIMIT 1",
                params![device.as_bytes().as_slice(), content.as_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Sizes of everything this device has sent to `device`, live or not.
    ///
    /// A file whose size is not among them was certainly never sent there,
    /// so only the rest need hashing before [`sent_before`](Self::sent_before)
    /// can be asked -- a folder of a thousand photos is not read twice to
    /// find the one sent last week.
    pub fn sizes_sent_to(&self, device: &DeviceId) -> Result<std::collections::HashSet<u64>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT size FROM files WHERE scope = ?1 AND held = 0",
        )?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice()], |r| r.get::<_, i64>(0))?;
        rows.map(|r| r.map(|n| n as u64).map_err(Into::into)).collect()
    }

    /// A new send of `content` to `device`: the device's earlier delivery of
    /// the same bytes says nothing about this one.
    ///
    /// Collected is judged by a record of the device holding the bytes. Left
    /// in place, the record from the first time would count the new send as
    /// collected the moment it was made -- and a sender that lets go of what
    /// has arrived would let go of it before it had. Only a record that the
    /// device holds them in its own vault is dropped; one that it holds them
    /// in the shared area is a copy it can hand back, whatever was sent.
    pub fn sending_again(&self, content: &blake3::Hash, device: &DeviceId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM replicas WHERE content_hash = ?1 AND device_id = ?2 AND private = 1",
            params![content.as_bytes().as_slice(), device.as_bytes().as_slice()],
        )?;
        Ok(())
    }

    /// The file sent to `device` that this chunk is part of, if any: its path
    /// and size.
    ///
    /// How a sender turns "that device asked for this chunk" into "that device
    /// is collecting report.pdf". A chunk can be part of several files; any
    /// live one sent to that device will do, since the question is only which
    /// send to show moving.
    pub fn sent_file_holding(
        &self,
        chunk: &blake3::Hash,
        device: &DeviceId,
    ) -> Result<Option<(String, u64)>> {
        self.conn
            .query_row(
                "SELECT f.path, f.size FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                  WHERE fc.chunk_hash = ?1 AND f.scope = ?2 AND f.deleted_at IS NULL
                    AND f.held = 0
                  LIMIT 1",
                params![chunk.as_bytes().as_slice(), device.as_bytes().as_slice()],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Let `device` hold this device's own vault, and so be shown it.
    pub fn add_holder(&self, device: &DeviceId) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO holders (device_id, since) VALUES (?1, unixepoch())",
            params![device.as_bytes().as_slice()],
        )?;
        Ok(())
    }

    /// Stop letting `device` hold this device's vault. What it already holds
    /// it keeps -- nothing here reaches into another device -- but it is shown
    /// nothing more.
    pub fn remove_holder(&self, device: &DeviceId) -> Result<bool> {
        let n = self.conn.execute(
            "DELETE FROM holders WHERE device_id = ?1",
            params![device.as_bytes().as_slice()],
        )?;
        Ok(n > 0)
    }

    /// The devices allowed to hold this device's vault.
    pub fn holders(&self) -> Result<Vec<DeviceId>> {
        let mut stmt = self.conn.prepare("SELECT device_id FROM holders ORDER BY since")?;
        let rows = stmt.query_map([], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok(to_device(raw))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?.into_iter().flatten().collect())
    }

    /// Whether `device` may hold this device's vault.
    pub fn is_holder(&self, device: &DeviceId) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM holders WHERE device_id = ?1)",
            params![device.as_bytes().as_slice()],
            |r| r.get(0),
        )?)
    }

    /// Whether this device keeps these bytes for `device`, as opposed to having
    /// sent them to it.
    pub fn holds_for(&self, content: &blake3::Hash, device: &DeviceId) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM files
                  WHERE content_hash = ?1 AND scope = ?2 AND held = 1 AND deleted_at IS NULL
             )",
            params![content.as_bytes().as_slice(), device.as_bytes().as_slice()],
            |r| r.get(0),
        )?)
    }

    /// Whether the live row at `path` in `owner`'s vault is kept for it.
    pub fn is_held(&self, path: &str, owner: &DeviceId) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM files
                  WHERE path = ?1 AND scope = ?2 AND held = 1 AND deleted_at IS NULL
             )",
            params![path, owner.as_bytes().as_slice()],
            |r| r.get(0),
        )?)
    }

    /// Mark a row in `owner`'s vault as kept for it rather than sent to it.
    ///
    /// Sticky on purpose: once this device holds a file for its owner, nothing
    /// short of the owner deleting it makes the bytes releasable again.
    pub fn mark_held(&self, path: &str, owner: &DeviceId) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET held = 1 WHERE path = ?1 AND scope = ?2",
            params![path, owner.as_bytes().as_slice()],
        )?;
        Ok(())
    }

    /// Remember, for good, that this device took this send (decision 0059).
    ///
    /// The first record stands: a send is taken once, so a second call for the
    /// same one changes nothing.
    pub fn note_taken(&self, version: &FileVersion, filed_as: &str) -> Result<()> {
        let Some(hash) = version.content.hash() else { return Ok(()) };
        self.conn.execute(
            "INSERT OR IGNORE INTO deliveries
                 (sender, path, vector, content_hash, taken_at, filed_as)
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)",
            params![
                version.modified_by.as_bytes().as_slice(),
                version.path,
                version.vector.encode(),
                hash.as_slice(),
                filed_as
            ],
        )?;
        Ok(())
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
        if let Audience::Guest(guest) = audience {
            // What was sent to it, and what is kept for its person, and
            // nothing else, however the bytes are shared with anything here.
            let person = self.person_of(guest)?.unwrap_or(*guest);
            let visible: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM file_chunks fc JOIN files f ON f.id = fc.file_id
                      WHERE fc.chunk_hash = ?1 AND (f.scope = ?2 OR f.scope = ?3) LIMIT 1",
                    params![hash.as_bytes().as_slice(), guest.as_bytes().as_slice(), person.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .optional()?;
            return Ok(visible.is_some());
        }
        let owner = audience.device().map(|d| d.as_bytes().to_vec());
        let visible: Option<i64> = self
            .conn
            .query_row(
                &format!(
                    "SELECT 1
                       FROM file_chunks fc
                       JOIN files f ON f.id = fc.file_id
                      WHERE fc.chunk_hash = ?1
                        AND ((f.scope IS NULL AND {})
                             OR f.scope = ?2
                             OR (f.scope = (SELECT device_id FROM local WHERE id = 1)
                                 AND EXISTS (SELECT 1 FROM holders h WHERE h.device_id = ?2)))
                      LIMIT 1",
                    shared_with("f.path", "?2")
                ),
                params![hash.as_bytes().as_slice(), owner],
                |r| r.get(0),
            )
            .optional()?;
        if visible.is_some() {
            return Ok(true);
        }
        match audience.device() {
            Some(device) => self.kept_for_earlier(device, Some(hash), None),
            None => Ok(false),
        }
    }

    /// Whether a device may resolve this content hash to a chunk list.
    /// Whether a live file of the shared area, outside every folder shared
    /// without `device`, has this content: what a guest of this computer may
    /// fetch of its shared folder, live, though it is never sent the list
    /// (decision 0073, item 6). Its own person's phones see the shared area
    /// by [`content_visible_to`](Self::content_visible_to) already.
    pub fn shared_content_reachable(&self, content: &blake3::Hash, device: &DeviceId) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                &format!(
                    "SELECT 1 FROM files
                      WHERE content_hash = ?1 AND scope IS NULL AND deleted_at IS NULL AND {}
                      LIMIT 1",
                    shared_with("path", "?2")
                ),
                params![content.as_bytes().as_slice(), device.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// [`shared_content_reachable`](Self::shared_content_reachable), for one
    /// chunk of such a file.
    pub fn shared_chunk_reachable(&self, hash: &blake3::Hash, device: &DeviceId) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                &format!(
                    "SELECT 1 FROM file_chunks fc JOIN files f ON f.id = fc.file_id
                      WHERE fc.chunk_hash = ?1 AND f.scope IS NULL AND f.deleted_at IS NULL AND {}
                      LIMIT 1",
                    shared_with("f.path", "?2")
                ),
                params![hash.as_bytes().as_slice(), device.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// The live files of the shared area under `dir` (every one, at any
    /// depth; `""` for all of it) that `device` may see -- outside every
    /// folder shared without it -- as `(path, size, mtime_ns, content)`.
    pub fn shared_entries_under(&self, dir: &str, device: &DeviceId) -> Result<Vec<(String, u64, i64, blake3::Hash)>> {
        self.shared_entries_where(dir, device, "1")
    }

    /// The same, only the files whose bytes are in this device's folder: the
    /// shared folder as it is on disk, which a phone reaching it is shown
    /// (the owner's list of 2026-10-26, item 14). A file listed with no bytes
    /// here -- freed, or on no device since -- is not in the folder; the
    /// owner's laptop listed 17 such, its folder empty, and the S23 showed
    /// them all.
    pub fn shared_entries_here(&self, dir: &str, device: &DeviceId) -> Result<Vec<(String, u64, i64, blake3::Hash)>> {
        self.shared_entries_where(dir, device, "materialised = 1")
    }

    fn shared_entries_where(
        &self,
        dir: &str,
        device: &DeviceId,
        condition: &str,
    ) -> Result<Vec<(String, u64, i64, blake3::Hash)>> {
        let dir = dir.trim_matches('/');
        let mut stmt = self.conn.prepare(&format!(
            "SELECT path, size, mtime_ns, content_hash FROM files
              WHERE scope IS NULL AND deleted_at IS NULL AND {condition}
                AND (?1 = '' OR substr(path, 1, length(?1) + 1) = ?1 || '/')
                AND {}
              ORDER BY path",
            shared_with("path", "?2")
        ))?;
        let rows = stmt.query_map(params![dir, device.as_bytes().as_slice()], |r| {
            let hash: Vec<u8> = r.get(3)?;
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)?, hash))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (path, size, mtime, hash) = row?;
            let Ok(hash) = <[u8; 32]>::try_from(hash.as_slice()) else { continue };
            out.push((path, size, mtime, blake3::Hash::from(hash)));
        }
        Ok(out)
    }

    /// Whether `path` in the shared area lies outside every folder shared
    /// without `device`: whether that device may see it, or put something
    /// there.
    pub fn shared_path_reachable(&self, path: &str, device: &DeviceId) -> Result<bool> {
        let reachable: bool = self.conn.query_row(
            &format!("SELECT {}", shared_with("?1", "?2")),
            params![path.trim_matches('/'), device.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(reachable)
    }

    pub fn content_visible_to(
        &self,
        content: &blake3::Hash,
        audience: Audience<'_>,
    ) -> Result<bool> {
        if matches!(audience, Audience::Ourselves) {
            return Ok(true);
        }
        if let Audience::Guest(guest) = audience {
            let person = self.person_of(guest)?.unwrap_or(*guest);
            let visible: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM files WHERE content_hash = ?1 AND (scope = ?2 OR scope = ?3) LIMIT 1",
                    params![content.as_bytes().as_slice(), guest.as_bytes().as_slice(), person.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .optional()?;
            return Ok(visible.is_some());
        }
        let owner = audience.device().map(|d| d.as_bytes().to_vec());
        let visible: Option<i64> = self
            .conn
            .query_row(
                &format!(
                    "SELECT 1 FROM files
                      WHERE content_hash = ?1
                        AND ((scope IS NULL AND {})
                             OR scope = ?2
                             OR (scope = (SELECT device_id FROM local WHERE id = 1)
                                 AND EXISTS (SELECT 1 FROM holders h WHERE h.device_id = ?2)))
                      LIMIT 1",
                    shared_with("path", "?2")
                ),
                params![content.as_bytes().as_slice(), owner],
                |r| r.get(0),
            )
            .optional()?;
        if visible.is_some() {
            return Ok(true);
        }
        match audience.device() {
            Some(device) => self.kept_for_earlier(device, None, Some(content)),
            None => Ok(false),
        }
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
                AND f.held = 0
                AND f.scope != (SELECT device_id FROM local WHERE id = 1)
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
    ///
    /// Only a row whose chunk list is complete -- its chunks add up to its
    /// size. A file deleted from the folder keeps its row as a tombstone but
    /// loses the references to payloads that left with it, and an empty or
    /// partial list read back as this content assembles the wrong bytes: found
    /// when a deleted file came back with the same content, and the device
    /// that had deleted it failed to take it, every sync, as "corrupt".
    pub fn any_file_with_content(&self, hash: &blake3::Hash) -> Result<Option<i64>> {
        self.conn
            .query_row(
                ANY_FILE_WITH_CONTENT,
                params![hash.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn any_path_with_content(&self, hash: &blake3::Hash) -> Result<Option<String>> {
        self.conn
            .query_row(
                ANY_PATH_WITH_CONTENT,
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
            "SELECT p.device_id, p.fingerprint, p.name, p.paired_at, p.last_seen, pr.relation
               FROM peers p LEFT JOIN peer_relations pr ON pr.device_id = p.device_id
              ORDER BY p.name",
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
                "SELECT p.device_id, p.fingerprint, p.name, p.paired_at, p.last_seen, pr.relation
                   FROM peers p LEFT JOIN peer_relations pr ON pr.device_id = p.device_id
                  WHERE p.fingerprint = ?1",
                params![fingerprint.as_slice()],
                peer_row,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Stop trusting `device`, and stop counting on it for anything.
    ///
    /// More than the trust row, because other tables go on promising things
    /// about a device after it stops being one this device will talk to:
    ///
    /// - `holders`: it no longer keeps this device's own vault.
    /// - `replicas`: every copy it was known to hold becomes one this device
    ///   cannot ask for -- the meaning `private` already has. Left counting as
    ///   an ordinary copy, it would let freeing space drop a file whose only
    ///   other copy is on a device nothing will connect to again. Marked rather
    ///   than deleted, because the same rows are what say a send was
    ///   collected, and a collected send must not reappear as waiting.
    /// - `reported`: what it was told this device holds; told again if it is
    ///   ever paired again.
    /// - `confirmed`: what it was asked about holding; asked again if it is
    ///   ever paired again, which is how its copies come to count again
    ///   (decision 0055).
    ///
    /// Returns whether it was trusted.
    pub fn forget_peer(&self, device: &DeviceId) -> Result<bool> {
        let id = device.as_bytes().as_slice();
        let tx = self.conn.unchecked_transaction()?;
        let n = tx.execute("DELETE FROM peers WHERE device_id = ?1", params![id])?;
        tx.execute("DELETE FROM holders WHERE device_id = ?1", params![id])?;
        tx.execute("UPDATE replicas SET private = 1 WHERE device_id = ?1", params![id])?;
        tx.execute("DELETE FROM reported WHERE device_id = ?1", params![id])?;
        tx.execute("DELETE FROM confirmed WHERE device_id = ?1", params![id])?;
        tx.execute("DELETE FROM meetings WHERE device_id = ?1", params![id])?;
        tx.execute("DELETE FROM peer_relations WHERE device_id = ?1", params![id])?;
        tx.execute("DELETE FROM visit_persons WHERE device_id = ?1", params![id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Record another person's device, with the secret the two meet under
    /// (decision 0060): a guest of this computer, or a computer this device
    /// visits.
    pub fn trust_visitor(
        &self,
        device: &DeviceId,
        fingerprint: &[u8; 32],
        name: &str,
        relation: Relation,
        secret: &[u8; 32],
    ) -> Result<()> {
        self.trust_peer(device, fingerprint, name)?;
        self.conn.execute(
            "INSERT INTO peer_relations (device_id, relation) VALUES (?1, ?2)
             ON CONFLICT (device_id) DO UPDATE SET relation = excluded.relation",
            params![device.as_bytes().as_slice(), relation.as_str()],
        )?;
        self.conn.execute(
            "INSERT INTO meetings (device_id, secret) VALUES (?1, ?2)
             ON CONFLICT (device_id) DO UPDATE SET secret = excluded.secret",
            params![device.as_bytes().as_slice(), secret.as_slice()],
        )?;
        Ok(())
    }

    /// This device's own vault, tombstones included, with each row's id: what
    /// a keeper of another person is shown, sealed (decision 0060).
    pub fn own_vault_rows(&self) -> Result<Vec<(FileRow, FileVersion)>> {
        let me = self.local_device()?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE scope = ?1 AND held = 0 ORDER BY path"
        ))?;
        let rows = stmt.query_map(params![me.as_bytes().as_slice()], file_row)?;
        let mut out = Vec::new();
        for row in rows {
            let row = row?;
            let version = row_to_version(&row);
            out.push((row, version));
        }
        Ok(out)
    }

    /// The sealed view of `path` made for `holder`: the plain content it was
    /// made from, the sealed file's hash and its size.
    pub fn sealed_view(&self, holder: &DeviceId, path: &str) -> Result<Option<(blake3::Hash, blake3::Hash, u64)>> {
        self.conn
            .query_row(
                "SELECT content_hash, sealed_hash, sealed_size FROM sealed_views WHERE holder = ?1 AND path = ?2",
                params![holder.as_bytes().as_slice(), path],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?, r.get::<_, i64>(2)?)),
            )
            .optional()?
            .map(|(c, s, n)| Ok((to_hash(&c), to_hash(&s), n as u64)))
            .transpose()
    }

    /// The plain file a sealed one shown to `holder` was made from: its path
    /// and content.
    pub fn unsealed(&self, holder: &DeviceId, sealed: &blake3::Hash) -> Result<Option<(String, blake3::Hash)>> {
        self.conn
            .query_row(
                "SELECT path, content_hash FROM sealed_views WHERE holder = ?1 AND sealed_hash = ?2 LIMIT 1",
                params![holder.as_bytes().as_slice(), sealed.as_bytes().as_slice()],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?
            .map(|(p, c)| Ok((p, to_hash(&c))))
            .transpose()
    }

    /// Record a sealed view, replacing the one made of an earlier version.
    pub fn put_sealed_view(
        &self,
        holder: &DeviceId,
        path: &str,
        content: &blake3::Hash,
        sealed: &blake3::Hash,
        size: u64,
        parts: &[([u8; 32], SealedPart)],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let h = holder.as_bytes().as_slice();
        tx.execute(
            "INSERT INTO sealed_views (holder, path, content_hash, sealed_hash, sealed_size)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (holder, path) DO UPDATE SET content_hash = excluded.content_hash,
                 sealed_hash = excluded.sealed_hash, sealed_size = excluded.sealed_size",
            params![h, path, content.as_bytes().as_slice(), sealed.as_bytes().as_slice(), size as i64],
        )?;
        tx.execute(
            "DELETE FROM sealed_parts WHERE holder = ?1 AND sealed_hash = ?2",
            params![h, sealed.as_bytes().as_slice()],
        )?;
        for (seq, (id, part)) in parts.iter().enumerate() {
            let (chunk, header) = match part {
                SealedPart::Chunk(chunk) => (Some(chunk.as_bytes().to_vec()), None),
                SealedPart::Header(header) => (None, Some(header.clone())),
            };
            tx.execute(
                "INSERT INTO sealed_parts (holder, sealed_hash, seq, sealed_id, chunk_hash, header)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    h,
                    sealed.as_bytes().as_slice(),
                    seq as i64,
                    id.as_slice(),
                    chunk,
                    header
                ],
            )?;
        }
        // Parts of sealed files no view points at any more.
        tx.execute(
            "DELETE FROM sealed_parts WHERE holder = ?1
               AND sealed_hash NOT IN (SELECT sealed_hash FROM sealed_views WHERE holder = ?1)",
            params![h],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The sealed chunk ids of a sealed file shown to `holder`, in order.
    pub fn sealed_manifest(&self, holder: &DeviceId, sealed: &blake3::Hash) -> Result<Option<Vec<[u8; 32]>>> {
        let mut stmt = self.conn.prepare(
            "SELECT sealed_id FROM sealed_parts WHERE holder = ?1 AND sealed_hash = ?2 ORDER BY seq",
        )?;
        let ids: Vec<Vec<u8>> = stmt
            .query_map(params![holder.as_bytes().as_slice(), sealed.as_bytes().as_slice()], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        if ids.is_empty() {
            return Ok(None);
        }
        Ok(Some(ids.into_iter().filter_map(|id| <[u8; 32]>::try_from(id).ok()).collect()))
    }

    /// What a sealed chunk shown to `holder` is: the plain chunk it seals, or
    /// the sealed header itself.
    pub fn sealed_part(&self, holder: &DeviceId, id: &[u8; 32]) -> Result<Option<SealedPart>> {
        let found = self
            .conn
            .query_row(
                "SELECT chunk_hash, header FROM sealed_parts WHERE holder = ?1 AND sealed_id = ?2 LIMIT 1",
                params![holder.as_bytes().as_slice(), id.as_slice()],
                |r| Ok((r.get::<_, Option<Vec<u8>>>(0)?, r.get::<_, Option<Vec<u8>>>(1)?)),
            )
            .optional()?;
        Ok(match found {
            Some((_, Some(header))) => Some(SealedPart::Header(header)),
            Some((Some(chunk), None)) => Some(SealedPart::Chunk(to_hash(&chunk))),
            _ => None,
        })
    }

    /// Files of this device's vault whose bytes are here and which a computer
    /// of another person keeping that vault is known to hold (decision 0060):
    /// what this device lets go of, the guest having chosen to keep nothing
    /// on the phone.
    pub fn kept_by_hosts(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path FROM files f
              WHERE f.scope = (SELECT device_id FROM local WHERE id = 1)
                AND f.held = 0 AND f.deleted_at IS NULL AND f.materialised = 1
                AND EXISTS (
                      SELECT 1 FROM replicas r
                        JOIN holders h ON h.device_id = r.device_id
                        JOIN peer_relations pr ON pr.device_id = r.device_id
                       WHERE r.content_hash = f.content_hash AND r.private = 0
                         AND pr.relation = 'host'
                    )
                AND NOT EXISTS (
                      SELECT 1 FROM kept_opened o
                       WHERE o.path = f.path AND o.at > unixepoch() - ?1
                    )
              ORDER BY f.path",
        )?;
        let rows = stmt.query_map(params![KEPT_OPENED_FOR], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// A file of this device's vault was fetched back from a computer keeping
    /// it, to be opened: it stays [`KEPT_OPENED_FOR`] before it goes again.
    pub fn note_kept_opened(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kept_opened (path, at) VALUES (?1, unixepoch())
             ON CONFLICT (path) DO UPDATE SET at = excluded.at",
            params![path],
        )?;
        self.conn.execute("DELETE FROM kept_opened WHERE at <= unixepoch() - ?1", params![KEPT_OPENED_FOR])?;
        Ok(())
    }

    /// The person a guest device belongs to, as it said when it visited.
    pub fn person_of(&self, device: &DeviceId) -> Result<Option<DeviceId>> {
        self.conn
            .query_row(
                "SELECT person FROM visit_persons WHERE device_id = ?1",
                params![device.as_bytes().as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map(|raw| raw.and_then(to_device))
            .map_err(Into::into)
    }

    /// The paired devices a guest's person has visited this computer with.
    pub fn devices_of_person(&self, person: &DeviceId) -> Result<Vec<DeviceId>> {
        let mut stmt = self.conn.prepare(
            "SELECT v.device_id FROM visit_persons v JOIN peers p ON p.device_id = v.device_id
              WHERE v.person = ?1",
        )?;
        let rows = stmt.query_map(params![person.as_bytes().as_slice()], |r| r.get::<_, Vec<u8>>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?.into_iter().filter_map(to_device).collect())
    }

    /// The sealed names of what this computer keeps for a guest's person,
    /// with each sealed file's hash and size.
    pub fn kept_entries(&self, person: &DeviceId) -> Result<Vec<(String, blake3::Hash, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, content_hash, size FROM files
              WHERE scope = ?1 AND held = 1 AND deleted_at IS NULL ORDER BY path",
        )?;
        let rows = stmt.query_map(params![person.as_bytes().as_slice()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?, r.get::<_, i64>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (path, hash, size) = row?;
            out.push((path, to_hash(&hash), size as u64));
        }
        Ok(out)
    }

    /// Just the sealed names of what is kept for a guest's person.
    pub fn kept_names(&self, person: &DeviceId) -> Result<Vec<String>> {
        Ok(self.kept_entries(person)?.into_iter().map(|(name, _, _)| name).collect())
    }

    /// How much this computer keeps for a guest's person, in bytes as the
    /// guest sealed them (decision 0060).
    pub fn kept_for_person(&self, person: &DeviceId) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT coalesce(sum(size), 0) FROM files WHERE scope = ?1 AND held = 1 AND deleted_at IS NULL",
            params![person.as_bytes().as_slice()],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    /// Record the person a guest device belongs to.
    pub fn set_person(&self, device: &DeviceId, person: &[u8; 32]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO visit_persons (device_id, person) VALUES (?1, ?2)
             ON CONFLICT (device_id) DO UPDATE SET person = excluded.person",
            params![device.as_bytes().as_slice(), person.as_slice()],
        )?;
        Ok(())
    }

    /// Who `device` is to this one, if it is paired.
    pub fn relation_of(&self, device: &DeviceId) -> Result<Option<Relation>> {
        self.conn
            .query_row(
                "SELECT pr.relation FROM peers p
                   LEFT JOIN peer_relations pr ON pr.device_id = p.device_id
                  WHERE p.device_id = ?1",
                params![device.as_bytes().as_slice()],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map(|found| found.map(|word| word.map_or(Relation::Own, |w| Relation::parse(&w))))
            .map_err(Into::into)
    }

    /// Every device of another person, with the secret this device meets it
    /// under.
    pub fn meetings(&self) -> Result<Vec<(TrustedPeer, [u8; 32])>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.device_id, p.fingerprint, p.name, p.paired_at, p.last_seen, pr.relation, m.secret
               FROM peers p JOIN meetings m ON m.device_id = p.device_id
               LEFT JOIN peer_relations pr ON pr.device_id = p.device_id
              ORDER BY p.name",
        )?;
        let rows = stmt.query_map([], |r| {
            let secret: Vec<u8> = r.get(6)?;
            Ok((peer_row(r)?, secret))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (peer, secret) = row?;
            if let Ok(secret) = <[u8; 32]>::try_from(secret) {
                out.push((peer, secret));
            }
        }
        Ok(out)
    }

        /// A stamp of the rule files as the index has them: changes whenever any
    /// is added, changed or deleted. Asked by path range, which the path
    /// index serves, rather than by a pattern it cannot.
    pub fn sharing_stamp(&self) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT coalesce(group_concat(path || ':' || hex(content_hash) || ':'
                                          || coalesce(deleted_at, ''), '|'), '')
               FROM (SELECT path, content_hash, deleted_at FROM files
                      WHERE scope IS NULL AND path >= '.qurb-sharing/' AND path < '.qurb-sharing0'
                      ORDER BY path)",
            [],
            |r| r.get(0),
        )?)
    }

    /// The stamp the share tables were last built from.
    pub fn shares_stamp(&self) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT stamp FROM shares_stamp WHERE id = 1", [], |r| r.get(0))
            .optional()
            .map_err(Into::into)
    }

    /// Live rule files in the shared area.
    pub fn live_rule_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE scope IS NULL AND deleted_at IS NULL
                AND path >= '.qurb-sharing/' AND path < '.qurb-sharing0'
              ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Replace the share tables with `rules`, built from the rule files whose
    /// stamp is `stamp`.
    pub fn replace_shares(&self, rules: &qurb_sync::sharing::Rules, stamp: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM share_members", [])?;
        tx.execute("DELETE FROM shares", [])?;
        for (folder, members) in rules.iter() {
            tx.execute("INSERT INTO shares (folder) VALUES (?1)", params![folder])?;
            for device in members {
                tx.execute(
                    "INSERT INTO share_members (folder, device_id) VALUES (?1, ?2)",
                    params![folder, device.as_bytes().as_slice()],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO shares_stamp (id, stamp) VALUES (1, ?1)
             ON CONFLICT (id) DO UPDATE SET stamp = excluded.stamp",
            params![stamp],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The rules as the share tables have them.
    pub fn shares(&self) -> Result<qurb_sync::sharing::Rules> {
        let mut rules: std::collections::BTreeMap<String, std::collections::BTreeSet<DeviceId>> =
            std::collections::BTreeMap::new();
        let mut folders = self.conn.prepare("SELECT folder FROM shares")?;
        for folder in folders.query_map([], |r| r.get::<_, String>(0))? {
            rules.insert(folder?, Default::default());
        }
        let mut members = self.conn.prepare("SELECT folder, device_id FROM share_members")?;
        for row in members.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))? {
            let (folder, raw) = row?;
            if let (Some(set), Some(device)) = (rules.get_mut(&folder), to_device(raw)) {
                set.insert(device);
            }
        }
        Ok(qurb_sync::sharing::Rules::new(rules))
    }

    /// Live paths in the folder -- the shared area and this device's own
    /// vault -- containing `text`, in path order.
    pub fn folder_paths_containing(&self, text: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files
              WHERE deleted_at IS NULL
                AND (scope IS NULL OR scope = (SELECT device_id FROM local WHERE id = 1))
                AND substr(path, 1, 14) <> '.qurb-sharing/'
                AND instr(path, ?1) > 0
              ORDER BY path",
        )?;
        let rows = stmt.query_map(params![text], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Remember a file just moved to the trash, and return its id -- which is
    /// also its name under `trash/`.
    pub fn add_trash(&self, entry: &NewTrash<'_>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO trash (path, scope, content_hash, size, deleted_at, deleted_by, why)
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5, ?6)",
            params![
                entry.path,
                entry.scope.map(|d| d.as_bytes().to_vec()),
                entry.content.as_bytes().as_slice(),
                entry.size as i64,
                entry.by.map(|d| d.as_bytes().to_vec()),
                entry.why,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Everything in the trash, most recently deleted first.
    pub fn trash(&self) -> Result<Vec<Trashed>> {
        self.trash_where("1", [])
    }

    pub fn trash_entry(&self, id: i64) -> Result<Option<Trashed>> {
        Ok(self.trash_where("id = ?1", [id])?.into_iter().next())
    }

    /// Entries deleted before `cutoff` (unix seconds).
    pub fn trash_before(&self, cutoff: i64) -> Result<Vec<Trashed>> {
        self.trash_where("deleted_at < ?1", [cutoff])
    }

    fn trash_where<P: rusqlite::Params>(&self, condition: &str, values: P) -> Result<Vec<Trashed>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, path, scope, content_hash, size, deleted_at, deleted_by, why
               FROM trash WHERE {condition} ORDER BY deleted_at DESC, id DESC"
        ))?;
        let rows = stmt.query_map(values, |r| {
            let hash: Vec<u8> = r.get(3)?;
            Ok(Trashed {
                id: r.get(0)?,
                path: r.get(1)?,
                scope: r.get::<_, Option<Vec<u8>>>(2)?.and_then(to_device),
                content: blake3::Hash::from(<[u8; 32]>::try_from(hash).unwrap_or([0; 32])),
                size: r.get::<_, i64>(4)? as u64,
                deleted_at: r.get(5)?,
                deleted_by: r.get::<_, Option<Vec<u8>>>(6)?.and_then(to_device),
                why: r.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    pub fn remove_trash(&self, id: i64) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM trash WHERE id = ?1", params![id])? > 0)
    }

    /// The entries for one file of `owner`'s vault: `content` at `path`.
    pub fn trash_of(&self, path: &str, owner: &DeviceId, content: &blake3::Hash) -> Result<Vec<Trashed>> {
        self.trash_where(
            "path = ?1 AND scope = ?2 AND content_hash = ?3",
            params![path, owner.as_bytes().as_slice(), content.as_bytes().as_slice()],
        )
    }

    /// The paths in this device's own vault whose last word is a deletion:
    /// all that a device keeping the vault could have put back (decision
    /// 0073), asked once a sync rather than once a file.
    pub fn own_deleted_paths(&self) -> Result<std::collections::HashSet<String>> {
        let me = self.local_device()?;
        let mut stmt = self.conn.prepare(
            "SELECT path FROM files f
              WHERE scope = ?1 AND deleted_at IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM files l WHERE l.path = f.path AND l.scope = ?1 AND l.deleted_at IS NULL)",
        )?;
        let rows = stmt.query_map(params![me.as_bytes().as_slice()], |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// What the trash is costing on disk.
    pub fn trash_bytes(&self) -> Result<u64> {
        let n: i64 = self.conn.query_row("SELECT coalesce(sum(size), 0) FROM trash", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    /// A name for every device this one has known: a trusted one by the name
    /// it has now, and one since removed by the name history recorded when it
    /// was paired or removed.
    ///
    /// History is kept about devices that may no longer be paired, and a
    /// short id is not what anybody called them.
    pub fn device_names(&self) -> Result<std::collections::HashMap<DeviceId, String>> {
        let mut names = std::collections::HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT device, detail FROM activity
              WHERE kind IN ('paired', 'removed') AND device IS NOT NULL AND detail IS NOT NULL
              ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (raw, name) = row?;
            if let Some(id) = to_device(raw) {
                names.insert(id, name);
            }
        }
        for peer in self.trusted_peers()? {
            names.insert(peer.device_id, peer.name);
        }
        Ok(names)
    }

    /// Files freed here whose only other known copy is on `device`.
    ///
    /// Asked before removing it: once removed, these have nowhere to come back
    /// from.
    pub fn only_kept_by(&self, device: &DeviceId) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path FROM files f
              WHERE f.deleted_at IS NULL
                AND f.materialised = 0
                AND (f.scope IS NULL OR f.scope = (SELECT device_id FROM local WHERE id = 1))
                AND EXISTS (SELECT 1 FROM replicas r
                             WHERE r.content_hash = f.content_hash
                               AND r.device_id = ?1 AND r.private = 0)
                AND NOT EXISTS (SELECT 1 FROM replicas r
                                 WHERE r.content_hash = f.content_hash
                                   AND r.device_id != ?1 AND r.private = 0)
              ORDER BY f.path",
        )?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice()], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// What is filed in `device`'s vault here, live: each path, its size, and
    /// whether it is that device's own file kept for it (`true`) rather than
    /// one this device sent it.
    pub fn vault_contents(&self, device: &DeviceId) -> Result<Vec<(String, u64, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, size, held FROM files
              WHERE scope = ?1 AND deleted_at IS NULL
              ORDER BY path",
        )?;
        let rows = stmt.query_map(params![device.as_bytes().as_slice()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? != 0))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Record what kind of device a peer is: `phone`, `computer` or
    /// `replica`. Anything else is not recorded.
    pub fn set_peer_kind(&self, device: &DeviceId, kind: &str) -> Result<()> {
        if !matches!(kind, "phone" | "computer" | "replica") {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO peer_kinds (device_id, kind) VALUES (?1, ?2)
             ON CONFLICT (device_id) DO UPDATE SET kind = excluded.kind",
            params![device.as_bytes().as_slice(), kind],
        )?;
        Ok(())
    }

    /// What kind of device a peer is, if it has said.
    pub fn peer_kind(&self, device: &DeviceId) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT kind FROM peer_kinds WHERE device_id = ?1",
                params![device.as_bytes().as_slice()],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    fn local_fact(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM local_facts WHERE name = ?1", params![name], |r| r.get(0))
            .optional()?)
    }

    fn set_local_fact(&self, name: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO local_facts (name, value) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            params![name, value],
        )?;
        Ok(())
    }

    /// This device's own kind, as it tells devices that ask.
    pub fn local_kind(&self) -> Result<Option<String>> {
        self.local_fact("kind")
    }

    /// Every row of the shared area, live or a tombstone, as `(id, path,
    /// content, live, materialised)`: what a phone leaving it goes through
    /// (decision 0073, item 2).
    pub fn shared_rows(&self) -> Result<Vec<(i64, String, blake3::Hash, bool, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, content_hash, deleted_at IS NULL, materialised FROM files WHERE scope IS NULL ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| {
            let hash: Vec<u8> = r.get(2)?;
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, hash, r.get::<_, bool>(3)?, r.get::<_, bool>(4)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, hash, live, materialised) = row?;
            let Ok(hash) = <[u8; 32]>::try_from(hash.as_slice()) else { continue };
            out.push((id, path, blake3::Hash::from(hash), live, materialised));
        }
        Ok(out)
    }

    /// Whether the live file at `path` is still read from a folder outside
    /// the vault (decision 0062): a backup whose original is still there.
    pub fn read_from_outside(&self, path: &str) -> Result<bool> {
        let found: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM outside_sources o JOIN files f ON f.id = o.file_id
                             WHERE f.path = ?1 AND f.deleted_at IS NULL)",
            params![path],
            |r| r.get(0),
        )?;
        Ok(found)
    }

    /// Forget a row here and nowhere else: no tombstone is written, so no
    /// other device is told anything, and what pointed at it goes with it
    /// (its chunks' references, where it was read from). For a phone letting
    /// go of the shared area, whose files the computers keep (decision 0073,
    /// item 2): a file not listed is not a file deleted.
    pub fn forget_row(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM files WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Set by whatever opens the store for good: the phone app, the daemon, a
    /// replica.
    pub fn set_local_kind(&self, kind: &str) -> Result<()> {
        self.set_local_fact("kind", kind)
    }

    /// Whether a computer has already been chosen, by default, to keep this
    /// phone's vault.
    pub fn holders_defaulted(&self) -> Result<bool> {
        Ok(self.local_fact("holders_defaulted")?.is_some())
    }

    pub fn set_holders_defaulted(&self) -> Result<()> {
        self.set_local_fact("holders_defaulted", "1")
    }

    /// Whether the folders kept outside the vault's own are offered to the
    /// devices keeping this vault now (decision 0062). Only on a network that
    /// does not count what it carries: WhatsApp's folder was 9.65 GB on the
    /// S23, and must not go over mobile data. Off until the app says so.
    pub fn outside_offered(&self) -> Result<bool> {
        Ok(self.local_fact("outside_offered")?.as_deref() == Some("1"))
    }

    /// Where the list counter stands (`V27`): a listing changed since a list
    /// was read moves it past what it was then.
    pub fn list_mark(&self) -> Result<u64> {
        Ok(self.conn.query_row("SELECT n FROM list_counter WHERE id = 1", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Which listings changed after `since`, by where each is listed -- 0 the
    /// shared area, 2 this device's own vault, 1 any other vault -- and path:
    /// the class a list's entry carries in its area (decision 0072).
    pub fn listed_since(&self, since: u64) -> Result<ChangedListings> {
        let me = self.local_device()?;
        let mut statement = self.conn.prepare(
            "SELECT f.path, f.scope FROM listed l JOIN files f ON f.id = l.file_id WHERE l.changed > ?1",
        )?;
        let since = since.min(i64::MAX as u64) as i64;
        let rows = statement.query_map(params![since], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
        })?;
        let mut out = std::collections::HashSet::new();
        for row in rows {
            let (path, scope) = row?;
            let class = match scope {
                None => 0,
                Some(scope) if scope.as_slice() == me.as_bytes().as_slice() => 2,
                Some(_) => 1,
            };
            out.insert((class, path));
        }
        Ok(out)
    }

    /// Run `read` inside one read of the index, so that what it reads is all
    /// of one moment.
    pub fn at_once<T>(&self, read: impl FnOnce(&Db) -> Result<T>) -> Result<T> {
        let snapshot = self.conn.unchecked_transaction()?;
        let out = read(self)?;
        snapshot.commit()?;
        Ok(out)
    }

    /// Mark `path`'s size and time as too recent to trust, or trusted again
    /// (`Store::put_file`, *Too recent to trust*).
    pub fn set_racy(&self, path: &str, racy: bool) -> Result<()> {
        match racy {
            true => self.conn.execute("INSERT OR IGNORE INTO racy (path) VALUES (?1)", params![path])?,
            false => self.conn.execute("DELETE FROM racy WHERE path = ?1", params![path])?,
        };
        Ok(())
    }

    /// Whether `path`'s size and time are too recent to trust.
    pub fn is_racy(&self, path: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT 1 FROM racy WHERE path = ?1", params![path], |_| Ok(())).optional()?.is_some())
    }

    /// Every path whose size and time are too recent to trust: few, so a scan
    /// asks once rather than once a file.
    pub fn racy_paths(&self) -> Result<std::collections::HashSet<String>> {
        let mut statement = self.conn.prepare("SELECT path FROM racy")?;
        let paths = statement.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(paths)
    }

    pub fn set_outside_offered(&self, offered: bool) -> Result<()> {
        self.set_local_fact("outside_offered", if offered { "1" } else { "0" })
    }

    /// What this device keeps for `owner` of a folder that device keeps
    /// outside its vault's own, beneath `prefix`: how many files, how many
    /// bytes (decision 0062). On a computer, the WhatsApp backup of a phone.
    pub fn held_outside_for(&self, owner: &DeviceId, prefix: &str) -> Result<(u64, u64)> {
        let pattern = format!("{}/%", prefix.trim_end_matches('/'));
        Ok(self.conn.query_row(
            "SELECT count(*), coalesce(sum(size), 0) FROM files
              WHERE scope = ?1 AND held = 1 AND deleted_at IS NULL AND path LIKE ?2 ESCAPE '\\'",
            params![owner.as_bytes().as_slice(), escape_like(&pattern)],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        )?)
    }

    /// Everything this device keeps for `owner`, live: files and bytes.
    pub fn held_for(&self, owner: &DeviceId) -> Result<(u64, u64)> {
        Ok(self.conn.query_row(
            "SELECT count(*), coalesce(sum(size), 0) FROM files
              WHERE scope = ?1 AND held = 1 AND deleted_at IS NULL",
            params![owner.as_bytes().as_slice()],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        )?)
    }

    /// Every file this device keeps for `owner`, live, with its content: what
    /// a phone browsing its folder here may fetch (decision 0067, step 4).
    pub fn held_entries(&self, owner: &DeviceId) -> Result<Vec<(String, u64, i64, blake3::Hash)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, size, mtime_ns, content_hash FROM files
              WHERE scope = ?1 AND held = 1 AND deleted_at IS NULL
              ORDER BY path",
        )?;
        let rows = stmt.query_map(params![owner.as_bytes().as_slice()], |r| {
            let hash: Vec<u8> = r.get(3)?;
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)?, hash))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (path, size, mtime, hash) = row?;
            let Ok(hash) = <[u8; 32]>::try_from(hash.as_slice()) else { continue };
            out.push((path, size, mtime, blake3::Hash::from(hash)));
        }
        Ok(out)
    }

    /// This person's devices that `device` replaced: the same name and kind,
    /// paired before it and last seen by the time it was paired -- the same
    /// phone, its data cleared and set up again (the owner's item 6 of
    /// 2026-10-14). What is kept for them is that phone's too, and it may
    /// reach it from its folder (decision 0067).
    pub fn earlier_selves(&self, device: &DeviceId) -> Result<Vec<DeviceId>> {
        let peers = self.trusted_peers()?;
        let Some(me) = peers.iter().find(|p| p.device_id == *device) else { return Ok(Vec::new()) };
        let kind = self.peer_kind(device)?;
        if me.relation != Relation::Own || kind.is_none() {
            return Ok(Vec::new());
        }
        let mut earlier = Vec::new();
        for p in &peers {
            if p.device_id != *device
                && p.relation == Relation::Own
                && p.name == me.name
                && p.paired_at < me.paired_at
                && p.last_seen.is_none_or(|seen| seen <= me.paired_at)
                && self.peer_kind(&p.device_id)? == kind
            {
                earlier.push(p.device_id);
            }
        }
        Ok(earlier)
    }

    /// Whether content is kept here for one of `device`'s earlier selves,
    /// which [`earlier_selves`](Self::earlier_selves) lets it reach.
    fn kept_for_earlier(&self, device: &DeviceId, chunk: Option<&blake3::Hash>, content: Option<&blake3::Hash>) -> Result<bool> {
        for earlier in self.earlier_selves(device)? {
            let found: Option<i64> = match (chunk, content) {
                (Some(chunk), _) => self
                    .conn
                    .query_row(
                        "SELECT 1 FROM file_chunks fc JOIN files f ON f.id = fc.file_id
                          WHERE fc.chunk_hash = ?1 AND f.scope = ?2 AND f.held = 1 LIMIT 1",
                        params![chunk.as_bytes().as_slice(), earlier.as_bytes().as_slice()],
                        |r| r.get(0),
                    )
                    .optional()?,
                (None, Some(content)) => self
                    .conn
                    .query_row(
                        "SELECT 1 FROM files WHERE content_hash = ?1 AND scope = ?2 AND held = 1 LIMIT 1",
                        params![content.as_bytes().as_slice(), earlier.as_bytes().as_slice()],
                        |r| r.get(0),
                    )
                    .optional()?,
                (None, None) => None,
            };
            if found.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Every file this device keeps for `owner`, live: path, size, and when it
    /// last changed (decision 0067, a person's folder).
    pub fn held_files(&self, owner: &DeviceId) -> Result<Vec<(String, u64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, size, mtime_ns FROM files
              WHERE scope = ?1 AND held = 1 AND deleted_at IS NULL
              ORDER BY path",
        )?;
        let rows = stmt.query_map(params![owner.as_bytes().as_slice()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)?))
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Files this device keeps for `owner` beneath `prefix`, live: name from
    /// the prefix, size, and when it last changed (decision 0063).
    pub fn held_under(&self, owner: &DeviceId, prefix: &str) -> Result<Vec<(String, u64, i64)>> {
        let prefix = prefix.trim_end_matches('/');
        let pattern = format!("{prefix}/%");
        let mut stmt = self.conn.prepare(
            "SELECT path, size, mtime_ns FROM files
              WHERE scope = ?1 AND held = 1 AND deleted_at IS NULL AND path LIKE ?2 ESCAPE '\\'
              ORDER BY path",
        )?;
        let rows = stmt.query_map(params![owner.as_bytes().as_slice(), escape_like(&pattern)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (path, size, mtime) = row?;
            out.push((path[prefix.len() + 1..].to_string(), size, mtime));
        }
        Ok(out)
    }

    /// The folders kept outside beneath `prefix`, counted: every file and its
    /// bytes, those a computer is known to hold, and those kept there only,
    /// gone from the outside folder (decision 0062).
    pub fn outside_summary(&self, prefix: &str) -> Result<OutsideSummary> {
        let me = self.local_device()?;
        let pattern = format!("{}/%", prefix.trim_end_matches('/'));
        Ok(self.conn.query_row(
            &format!(
                "WITH rows AS (
                     SELECT f.size,
                            EXISTS (SELECT 1 FROM replicas r
                                     WHERE r.content_hash = f.content_hash AND r.private = 0
                                       AND {SAFE_ELSEWHERE}) AS held,
                            NOT EXISTS (SELECT 1 FROM outside_sources o WHERE o.file_id = f.id) AS only_kept
                       FROM files f
                      WHERE f.scope = ?1 AND f.deleted_at IS NULL AND f.path LIKE ?2 ESCAPE '\\'
                 )
                 SELECT count(*), coalesce(sum(size), 0),
                        coalesce(sum(held), 0), coalesce(sum(CASE WHEN held THEN size ELSE 0 END), 0),
                        coalesce(sum(only_kept), 0)
                   FROM rows"
            ),
            params![me.as_bytes().as_slice(), escape_like(&pattern)],
            |r| {
                Ok(OutsideSummary {
                    files: r.get::<_, i64>(0)? as u64,
                    bytes: r.get::<_, i64>(1)? as u64,
                    held_files: r.get::<_, i64>(2)? as u64,
                    held_bytes: r.get::<_, i64>(3)? as u64,
                    only_kept: r.get::<_, i64>(4)? as u64,
                })
            },
        )?)
    }

    /// The files under `prefix` in this device's own vault that no device
    /// it may count on holds yet -- what a backup still has to give the
    /// computer keeping it, as [`outside_summary`](Self::outside_summary)
    /// counts them -- the first `limit` by path, with how many there are and
    /// their bytes: what Home and the notice name (the owner's list of
    /// 2026-10-25).
    pub fn outside_waiting(&self, prefix: &str, limit: usize) -> Result<(Vec<(String, u64)>, u64, u64)> {
        let me = self.local_device()?;
        let pattern = escape_like(&format!("{}/%", prefix.trim_end_matches('/')));
        let waiting = format!(
            "FROM files f
              WHERE f.scope = ?1 AND f.deleted_at IS NULL AND f.path LIKE ?2 ESCAPE '\\'
                AND NOT EXISTS (SELECT 1 FROM replicas r
                                 WHERE r.content_hash = f.content_hash AND r.private = 0
                                   AND {SAFE_ELSEWHERE})"
        );
        let (files, bytes): (i64, i64) = self.conn.query_row(
            &format!("SELECT count(*), coalesce(sum(f.size), 0) {waiting}"),
            params![me.as_bytes().as_slice(), pattern],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut stmt = self.conn.prepare(&format!("SELECT f.path, f.size {waiting} ORDER BY f.path LIMIT ?3"))?;
        let rows = stmt.query_map(params![me.as_bytes().as_slice(), pattern, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        Ok((rows.collect::<std::result::Result<_, _>>()?, files as u64, bytes as u64))
    }

    /// A live file of this device's own vault made of `chunk`: what a device
    /// collecting that chunk is taking, for saying so (the owner's list of
    /// 2026-10-25). Through `idx_file_chunks_hash`.
    pub fn own_file_with_chunk(&self, chunk: &blake3::Hash) -> Result<Option<String>> {
        let me = self.local_device()?;
        self.conn
            .query_row(
                "SELECT f.path FROM file_chunks fc JOIN files f ON f.id = fc.file_id
                  WHERE fc.chunk_hash = ?1 AND f.scope = ?2 AND f.deleted_at IS NULL
                  LIMIT 1",
                params![chunk.as_bytes().as_slice(), me.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
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

    /// Whether a live file in the folder holds this chunk, by the index alone:
    /// [`locate_chunk`](Self::locate_chunk)'s question, asked for a yes or no.
    pub fn chunk_in_folder(&self, hash: &blake3::Hash) -> Result<bool> {
        let held: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                  WHERE fc.chunk_hash = ?1 AND f.deleted_at IS NULL AND f.materialised = 1
             )",
            params![hash.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(held)
    }

    /// Whether a send still waiting reads this chunk from its file
    /// (decision 0060).
    pub fn chunk_in_send(&self, hash: &blake3::Hash) -> Result<bool> {
        let held: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                   JOIN send_sources s ON s.file_id = f.id
                  WHERE fc.chunk_hash = ?1 AND f.deleted_at IS NULL
             )",
            params![hash.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(held)
    }

    /// Whether a chunk is part of a live file kept outside the folder, read
    /// where it lies (decision 0062).
    pub fn chunk_outside(&self, hash: &blake3::Hash) -> Result<bool> {
        let held: bool = self.conn.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM file_chunks fc
                   JOIN files f ON f.id = fc.file_id
                   JOIN outside_sources o ON o.file_id = f.id
                  WHERE fc.chunk_hash = ?1 AND f.deleted_at IS NULL
             )",
            params![hash.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        Ok(held)
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
    /// Who it is to this device (decision 0060).
    pub relation: Relation,
}

/// Who a paired device is to this one (decision 0060).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// One of the same person's devices, holding the same key.
    Own,
    /// Another person's device, visiting this computer.
    Guest,
    /// A computer this device visits, of another person.
    Host,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Own => "own",
            Relation::Guest => "guest",
            Relation::Host => "host",
        }
    }

    /// Unknown words read as `Own`'s opposite: a peer whose relation this
    /// build cannot read is shown no more than a guest is.
    pub fn parse(word: &str) -> Self {
        match word {
            "own" => Relation::Own,
            "host" => Relation::Host,
            _ => Relation::Guest,
        }
    }

    /// Whether it is another person's device.
    pub fn is_other_person(self) -> bool {
        self != Relation::Own
    }
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
        relation: r.get::<_, Option<String>>(5)?.map_or(Relation::Own, |w| Relation::parse(&w)),
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

/// Where a file's bytes are, from this device's point of view.
///
/// More than two values. A file that is here and also on another device, and
/// a file that is here and nowhere else in the world, look identical to
/// anything that only checks whether the bytes are on disk -- and offering to
/// free the second is offering to delete it. So do, from the other side, a
/// file freed here that another device has and one no device has any more
/// (decision 0055). Decided here, once, so that the desktop's window and the
/// phone's app cannot disagree about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// In the folder, openable now, and another device holds it too.
    Here,
    /// Known about, freed locally, and another device has it.
    Elsewhere,
    /// In the folder, and no other device is known to hold it. While this is
    /// true, losing this device loses the file.
    OnlyHere,
    /// Known about, not here, and no device this one can ask is known to hold
    /// it (decision 0055): freed here on the strength of a copy since lost,
    /// or kept by a device since removed. Listed, so that it is not mistaken
    /// for a file never made, and offered to nobody as something to fetch.
    Nowhere,
}

impl Availability {
    /// From whether the bytes are here, and whether another device that will
    /// hand them back is known to hold them.
    pub fn of(here: bool, elsewhere: bool) -> Self {
        match (here, elsewhere) {
            (true, true) => Availability::Here,
            (true, false) => Availability::OnlyHere,
            (false, true) => Availability::Elsewhere,
            (false, false) => Availability::Nowhere,
        }
    }
}

/// One file in this device's folder, as an app lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub path: String,
    pub size: u64,
    pub mtime_ns: i64,
    pub availability: Availability,
    /// In this device's own vault rather than the shared area.
    pub private: bool,
}

/// What this device could free without losing anything. See
/// [`Db::freeable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Freeable {
    /// How many files, and their size, in all.
    pub count: usize,
    pub bytes: u64,
    /// The largest of them, biggest first.
    pub files: Vec<FolderEntry>,
}

/// Which listings changed, by where each is listed and path (`Db::listed_since`).
pub type ChangedListings = std::collections::HashSet<(u8, String)>;

/// One row of a listing: what a file browser needs and nothing more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub path: String,
    pub size: u64,
    /// Unix seconds, when this device last changed its mind about the file.
    pub updated_at: i64,
    /// Whether the bytes are in the folder here, or only known about.
    pub here: bool,
    pub content: blake3::Hash,
}

fn listed_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Listed> {
    let raw: Vec<u8> = r.get(4)?;
    Ok(Listed {
        path: r.get(0)?,
        size: r.get::<_, i64>(1)? as u64,
        updated_at: r.get(2)?,
        here: r.get::<_, i64>(3)? != 0,
        content: to_hash(&raw),
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

/// A file about to go into the trash. See [`Db::add_trash`].
#[derive(Debug, Clone)]
pub struct NewTrash<'a> {
    pub path: &'a str,
    pub scope: Option<&'a DeviceId>,
    pub content: &'a blake3::Hash,
    pub size: u64,
    pub by: Option<&'a DeviceId>,
    pub why: Option<&'a str>,
}

/// How long a file fetched back from a computer keeping it stays here before
/// it is let go of again, in seconds: a day, so that one opened is there to
/// open again that day (decision 0060).
pub const KEPT_OPENED_FOR: i64 = 24 * 3600;

/// One part of a sealed file shown to a keeper of another person (decision
/// 0060): its start, sealed as it is, or a plain chunk that is read and sealed
/// again whenever it is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealedPart {
    Header(Vec<u8>),
    Chunk(blake3::Hash),
}

/// A send read from where its file is (decision 0060). See
/// [`Db::send_sources`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendSource {
    pub file_id: i64,
    /// The name the recipient sees.
    pub path: String,
    pub to: DeviceId,
    pub content: blake3::Hash,
    /// Where the bytes are read from.
    pub source: String,
    /// The file's size and modification time when it was sent, or last seen
    /// unchanged.
    pub size: u64,
    pub mtime_ns: i64,
    /// A copy qurb made, to be deleted once collected.
    pub temporary: bool,
    /// Collected, or taken back: nothing more is read from it.
    pub done: bool,
}

/// A file of this device's own vault kept outside its folder (decision 0062).
/// See [`Db::outside_under`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutsideRow {
    pub file_id: i64,
    pub path: String,
    pub size: u64,
    pub content: blake3::Hash,
    /// Where it is read from, while it is still there: `None` once it has
    /// gone from there and is kept by another device only.
    pub source: Option<OutsideSource>,
}

/// A folder chosen, or no longer chosen, to back up, and when, in
/// milliseconds since the epoch: the later choice wins (item 10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub folder: String,
    pub chosen: bool,
    pub changed_at: i64,
}

/// Milliseconds since the epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One of a phone's folders and what is in it, everything beneath it
/// counted (decision 0063).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderSummary {
    /// From the root of the phone's shared storage: `DCIM`, `DCIM/Camera`.
    pub path: String,
    pub files: u64,
    pub bytes: u64,
}

/// Counted by [`Db::outside_summary`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutsideSummary {
    pub files: u64,
    pub bytes: u64,
    /// Known to be held by a device this one may count on: a computer.
    pub held_files: u64,
    pub held_bytes: u64,
    /// Gone from the outside folder, and kept by that computer only.
    pub only_kept: u64,
}

/// Where a file kept outside the folder is read from, as it was last listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutsideSource {
    pub source: String,
    pub size: u64,
    pub mtime_ns: i64,
}

/// A file in the trash: recently deleted, and still restorable here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    /// Also its file name under `trash/` in the store directory.
    pub id: i64,
    /// Where it was.
    pub path: String,
    pub scope: Option<DeviceId>,
    pub content: blake3::Hash,
    pub size: u64,
    /// Unix seconds.
    pub deleted_at: i64,
    /// The device whose deletion this was, where known.
    pub deleted_by: Option<DeviceId>,
    /// Why, where "deleted" is not the whole story.
    pub why: Option<String>,
}

fn to_device(raw: Vec<u8>) -> Option<DeviceId> {
    let bytes: [u8; 32] = raw.try_into().ok()?;
    Some(DeviceId::from_bytes(bytes))
}

const FILE_COLUMNS: &str =
    "id, path, size, content_hash, mtime_ns, deleted_at, vector, modified_by, updated_at";

/// How many columns [`FILE_COLUMNS`] names, so a query can select one more after
/// them and read it by position.
const FILE_COLUMN_COUNT: usize = 9;

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
        area: qurb_sync::Area::Shared,
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

/// A device's name and picture, as it set them (the owner's list of
/// 2026-10-26, item 7). `changed_at` in milliseconds, for asking what changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// A small JPEG, as the device chose it, or none.
    pub picture: Option<Vec<u8>>,
    pub changed_at: i64,
}

/// How long history is kept: 30 days (the owner's list of 2026-10-26, item 8).
pub const ACTIVITY_KEPT: i64 = 30 * 24 * 3600;

/// One line for each thing that happened to a file, newest first (item 8 of
/// the owner's list of 2026-10-26). A file sent is recorded as it is put in
/// the other device's vault and again when that device collects it, and,
/// copied in from elsewhere first, as stored as well: three lines for one
/// send, which is what the S23 showed. Told once here:
///
/// - a send collected is one line, the collection, which says it arrived;
/// - a file stored here shortly before it was sent is the send's, not a line
///   of its own;
/// - the same thing to the same file, with the same device, again within a
///   minute is one line, the latest.
pub fn told_once(rows: Vec<Activity>) -> Vec<Activity> {
    const SAME_SEND: i64 = 10 * 60;
    const AGAIN: i64 = 60;
    let mut out: Vec<Activity> = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(path) = row.path.as_deref() else {
            out.push(row);
            continue;
        };
        let later = |kinds: &[&str], within: i64, same_device: bool| {
            out.iter().any(|newer| {
                newer.path.as_deref() == Some(path)
                    && kinds.contains(&newer.kind.as_str())
                    && newer.at - row.at <= within
                    && (!same_device || newer.device == row.device)
            })
        };
        let folded = match row.kind {
            Event::Sent => later(&["collected"], i64::MAX, true),
            Event::Stored => later(&["sent", "collected"], SAME_SEND, false),
            _ => false,
        } || later(&[row.kind.as_str()], AGAIN, true);
        if !folded {
            out.push(row);
        }
    }
    out
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
    /// A send was taken back before the other device collected it.
    Cancelled,
    /// A device stopped being trusted here.
    Removed,
    /// A file moved into this device's Private Vault, or out of it to every
    /// device (decision 0057). The detail says which.
    Moved,
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
            Event::Cancelled => "cancelled",
            Event::Removed => "removed",
            Event::Moved => "moved",
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
            "cancelled" => Event::Cancelled,
            "removed" => Event::Removed,
            "moved" => Event::Moved,
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
    /// A device of another person: a guest of this computer, or a computer
    /// this device visits (decision 0060). Sees only what was sent to it --
    /// never the shared area, never a vault kept for anyone.
    Guest(&'a DeviceId),
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
            Audience::Device(device) | Audience::Guest(device) => Some(device),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How SQLite would answer `sql`, in its own words.
    fn plan(db: &Db, sql: &str) -> String {
        let mut stmt = db.conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let rows = stmt.query_map([vec![0u8; 32]], |r| r.get::<_, String>(3)).unwrap();
        rows.map(|r| r.unwrap()).collect::<Vec<_>>().join(" | ")
    }

    /// A choice made on either side wins where it is the later one, an untick
    /// as much as a tick, on the phone and on the computer alike (item 10).
    #[test]
    fn the_later_choice_wins_wherever_it_was_made() {
        let phone = Db::open_in_memory().unwrap();
        phone.choose_here("DCIM/Camera", true, 1_000).unwrap();
        phone.choose_here("Documents", true, 1_000).unwrap();
        // The computer's word: Camera unticked later, Documents ticked earlier
        // (so not taken), Download newly ticked.
        phone
            .merge_choices_here(&[
                Choice { folder: "DCIM/Camera".into(), chosen: false, changed_at: 2_000 },
                Choice { folder: "Documents".into(), chosen: false, changed_at: 500 },
                Choice { folder: "Download".into(), chosen: true, changed_at: 1_500 },
            ])
            .unwrap();
        assert_eq!(phone.chosen_here().unwrap(), vec!["Documents".to_string(), "Download".to_string()]);

        let computer = Db::open_in_memory().unwrap();
        let device = DeviceId::from_bytes([3; 32]);
        computer.merge_choices_for(&device, &phone.choices_here().unwrap()).unwrap();
        assert_eq!(computer.chosen_for(&device).unwrap(), vec!["Documents".to_string(), "Download".to_string()]);
        // Ticked on the computer now: later than anything the phone said.
        computer.choose_folder(&device, "DCIM/Camera", true).unwrap();
        phone.merge_choices_here(&computer.choices_for(&device).unwrap()).unwrap();
        assert!(phone.chosen_here().unwrap().contains(&"DCIM/Camera".to_string()));
    }

    /// One line for each thing that happened to a file (the owner's list of
    /// 2026-10-26, item 8): a send copied in, put in the laptop's vault and
    /// collected was three lines, and is one; the same thing again within a
    /// minute is one; anything else stays as it was.
    #[test]
    fn a_file_sent_is_told_once() {
        let laptop = DeviceId::from_bytes([4; 32]);
        let row = |id: i64, at: i64, kind: Event, path: &str, device: Option<DeviceId>| Activity {
            id,
            at,
            kind,
            path: Some(path.to_string()),
            size: Some(10),
            device,
            detail: None,
        };
        // Newest first, as the history is read.
        let told = told_once(vec![
            row(9, 1_000, Event::Collected, "IMG_1.jpg", Some(laptop)),
            row(8, 990, Event::Sent, "IMG_1.jpg", Some(laptop)),
            row(7, 985, Event::Stored, "IMG_1.jpg", None),
            row(6, 900, Event::Received, "notes.txt", Some(laptop)),
            row(5, 880, Event::Received, "notes.txt", Some(laptop)),
            row(4, 500, Event::Sent, "big.mkv", Some(laptop)),
            row(3, 100, Event::Stored, "diary.txt", None),
            row(2, 50, Event::Deleted, "diary.txt", None),
        ]);
        let kept: Vec<i64> = told.iter().map(|a| a.id).collect();
        assert_eq!(kept, vec![9, 6, 4, 3, 2], "{told:?}");
    }

    /// History older than 30 days is forgotten, and nothing newer.
    #[test]
    fn history_is_kept_thirty_days() {
        let db = Db::open_in_memory().unwrap();
        db.record(Event::Stored, Some("new.txt"), Some(1), None, None).unwrap();
        db.record(Event::Stored, Some("old.txt"), Some(1), None, None).unwrap();
        db.conn.execute("UPDATE activity SET at = at - ?1 WHERE path = 'old.txt'", params![ACTIVITY_KEPT + 60]).unwrap();
        assert_eq!(db.forget_old_activity().unwrap(), 1);
        let left: Vec<String> = db.activity(10, None).unwrap().into_iter().filter_map(|a| a.path).collect();
        assert_eq!(left, vec!["new.txt".to_string()]);
    }

    /// Ticking or unticking a folder sets every choice inside it the same
    /// way, on the phone and on the computer, so that it is all of it or none
    /// ([`crate::chosen`]); a folder whose name only starts the same is not
    /// inside it.
    #[test]
    fn a_choice_takes_everything_inside_it() {
        let phone = Db::open_in_memory().unwrap();
        phone.choose_here("Download/ADM", false, 1_000).unwrap();
        phone.choose_here("Download/ADM/2025", true, 1_000).unwrap();
        phone.choose_here("Downloads", false, 1_000).unwrap();
        phone.choose_here("Download", true, 2_000).unwrap();
        let mut rows = phone.choices_here().unwrap();
        rows.sort_by(|a, b| a.folder.cmp(&b.folder));
        assert_eq!(
            rows.iter().map(|c| (c.folder.as_str(), c.chosen, c.changed_at)).collect::<Vec<_>>(),
            vec![("Download", true, 2_000), ("Download/ADM", true, 2_000), ("Download/ADM/2025", true, 2_000), ("Downloads", false, 1_000)]
        );

        let computer = Db::open_in_memory().unwrap();
        let device = DeviceId::from_bytes([3; 32]);
        computer.merge_choices_for(&device, &rows).unwrap();
        computer.choose_folder(&device, "Download", false).unwrap();
        let theirs = computer.choices_for(&device).unwrap();
        assert!(theirs.iter().filter(|c| c.folder.starts_with("Download/") || c.folder == "Download").all(|c| !c.chosen));
        assert!(theirs.iter().any(|c| c.folder == "Downloads" && !c.chosen));
        // Taken back to the phone: the later word, everything inside included.
        phone.merge_choices_here(&theirs).unwrap();
        assert!(phone.chosen_here().unwrap().is_empty());
    }

    /// The questions asked for every file a device serves find their rows
    /// through an index, not by reading all of `files`. With a scan, a first
    /// sync's time a file grew with the library: 2.4 ms at 5,000 files, 4.8 at
    /// 20,000 (decision 0065, V24).
    #[test]
    fn content_is_found_through_an_index() {
        let db = Db::open_in_memory().unwrap();
        for sql in [ANY_FILE_WITH_CONTENT, ANY_PATH_WITH_CONTENT] {
            let plan = plan(&db, sql);
            assert!(plan.contains("USING INDEX idx_files_content"), "{plan}");
            assert!(!plan.contains("SCAN f") && !plan.contains("SCAN files"), "{plan}");
        }
    }
}
