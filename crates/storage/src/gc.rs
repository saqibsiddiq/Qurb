//! Garbage collection.
//!
//! Deduplication is what makes deletion hard. A chunk may be referenced by many
//! files and by many versions of the same file, so "delete the file" cannot
//! mean "delete its chunks". Getting this wrong destroys data belonging to a
//! file nobody touched, and does it silently.
//!
//! Collection runs in two stages, separated because they answer different
//! questions.
//!
//! **Stage 1 — expire tombstones.** A deleted file keeps its row and its chunk
//! references for the retention window, which is what makes undelete possible.
//! Past that window the row and its links are removed. Removing the links is
//! what actually releases the chunks, via the trigger in [`crate::db`].
//!
//! **Stage 2 — reclaim chunks.** Any chunk now referenced by nothing, and
//! unreferenced for longer than the retention window, is removed from disk.
//! The second condition matters for chunks orphaned by an *edit* rather than a
//! deletion: overwriting a file releases the old version's chunks, and the
//! retention window is what keeps the previous version recoverable.
//!
//! The two windows compose rather than overlap. A deleted file's chunks are
//! held for the retention period as a tombstone, and then for the retention
//! period again as unreferenced chunks, so the worst case before space is
//! actually reclaimed is twice the configured window. That is the safe
//! direction to err in for a storage product — it costs disk, where the other
//! direction costs data — but it is a real cost and worth knowing about when
//! choosing the window.
//!
//! # Concurrency
//!
//! The deletions run inside a single `BEGIN IMMEDIATE` transaction, which takes
//! SQLite's one write lock and therefore excludes any concurrent writer for the
//! duration. Without that, a writer could add a reference to a chunk between
//! this code deciding it was unreferenced and removing it from disk — leaving
//! an index entry pointing at nothing.
//!
//! Payloads are removed from disk before the transaction commits. If the
//! process dies in that window, the affected rows survive with `refcount = 0`
//! and the next collection removes them; [`crate::cas::Cas::remove`] tolerates
//! an already-absent file. No live file is ever exposed, because every chunk
//! touched here had no references at all.

use crate::cas::Cas;
use crate::db::{self, Db};
use crate::error::Result;
use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GcStats {
    /// Deleted files whose retention window expired and are now gone for good.
    pub tombstones_expired: usize,
    pub chunks_removed: usize,
    /// On-disk bytes freed.
    pub bytes_reclaimed: u64,
}

/// Reclaim space, keeping anything deleted or superseded within `retention`.
pub fn collect(db: &mut Db, cas: &Cas, retention: Duration) -> Result<GcStats> {
    let cutoff = db::now() - retention.as_secs() as i64;
    let mut stats = GcStats::default();

    let tx = db
        .conn_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    // Stage 1. The cascade on file_chunks fires the delete trigger, which
    // decrements each chunk's refcount and starts its retention clock.
    stats.tombstones_expired = tx.execute(
        "DELETE FROM files WHERE deleted_at IS NOT NULL AND deleted_at <= ?1",
        rusqlite::params![cutoff],
    )?;

    // Stage 2. Chunks released long enough ago to be past recovery.
    //
    // Both stages share one cutoff, computed before either ran. A chunk
    // released by stage 1 above therefore carries `unreferenced_at` of roughly
    // now, so it is only collected here when the retention window is zero.
    // With any real window it waits for a later pass.
    let candidates: Vec<(blake3::Hash, u64)> = {
        let mut stmt = tx.prepare(
            "SELECT hash, stored_size FROM chunks
              WHERE refcount = 0 AND unreferenced_at IS NOT NULL AND unreferenced_at <= ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![cutoff], |r| {
            let raw: Vec<u8> = r.get(0)?;
            Ok((db::to_hash(&raw), r.get::<_, i64>(1)? as u64))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };

    {
        let mut del = tx.prepare("DELETE FROM chunks WHERE hash = ?1")?;
        for (hash, stored_size) in &candidates {
            del.execute(rusqlite::params![hash.as_bytes().as_slice()])?;
            cas.remove(hash)?;
            stats.chunks_removed += 1;
            stats.bytes_reclaimed += stored_size;
        }
    }

    tx.commit()?;
    Ok(stats)
}

/// Remove payloads on disk that the index does not know about.
///
/// These are the residue of writes interrupted between storing a payload and
/// recording it, and of collections interrupted before their commit. Wasted
/// space rather than corruption, so this is separate from [`collect`] and safe
/// to run rarely.
///
/// Runs under the write lock for the same reason [`collect`] does: a chunk
/// being written concurrently would otherwise look orphaned.
pub fn sweep_orphans(db: &mut Db, cas: &Cas) -> Result<GcStats> {
    let mut stats = GcStats::default();
    let tx = db
        .conn_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let mut known = tx.prepare("SELECT 1 FROM chunks WHERE hash = ?1")?;
    for hash in cas.iter_hashes()? {
        let indexed: bool = known
            .exists(rusqlite::params![hash.as_bytes().as_slice()])?;
        if !indexed {
            stats.bytes_reclaimed += cas.stored_size(&hash).unwrap_or(0);
            cas.remove(&hash)?;
            stats.chunks_removed += 1;
        }
    }
    drop(known);

    tx.commit()?;
    Ok(stats)
}
