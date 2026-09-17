//! The storage API: files in, files out, with deduplication underneath.
//!
//! [`Store`] ties the three lower layers together — the chunker, the encrypted
//! chunk store on disk, and the SQLite index — and owns the ordering rules that
//! keep them consistent with each other.

use crate::cas::Cas;
use crate::chunker::{self, Manifest};
use crate::db::{self, Db};
use crate::error::{Error, Result};
use crate::format::{self, ChunkKey};
use crate::gc::{self, GcStats};
use qurb_sync::{Content, DeviceId, FileVersion};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

pub struct Store {
    cas: Cas,
    db: Db,
    key: ChunkKey,
}

/// Whose change this is.
///
/// The distinction matters because a version vector records *who* saw what. A
/// change made here advances this device's counter; a version received from a
/// peer keeps the vector it arrived with. Stamping a received version as local
/// would claim this device had seen changes it has not, and would make its
/// history dominate versions it should have conflicted with.
enum Stamp<'a> {
    Local,
    Remote(&'a FileVersion),
}

/// Whether the "content already matches" short-circuit may be taken.
///
/// That short-circuit assumes matching content means the payloads are still on
/// disk, which is true on every path except repair — which has just discarded
/// them on purpose. Taking it there would leave the file exactly as broken as
/// before while reporting success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Payloads {
    /// Skip the work if the index already agrees. The hot path.
    TrustIndex,
    /// Write every chunk, whatever the index believes.
    Rewrite,
}

/// What a write actually cost.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PutStats {
    pub chunks_total: usize,
    /// Chunks whose payload had to be written; the rest were already held.
    pub chunks_written: usize,
    pub bytes_written: u64,
    pub bytes_deduplicated: u64,
    /// The file was already stored with identical content; only metadata moved.
    pub unchanged: bool,
}

impl Store {
    /// Open or create a store rooted at `root`.
    ///
    /// Layout: `<root>/index.db` for metadata, `<root>/chunks/` for payloads.
    pub fn open(root: &Path, key: ChunkKey) -> Result<Self> {
        std::fs::create_dir_all(root).map_err(|e| Error::io(root, e))?;
        let cas = Cas::open(root.join("chunks"))?;
        let db = Db::open(&root.join("index.db"))?;
        Ok(Self { cas, db, key })
    }

    /// Where this store lives, so another handle can be opened on it.
    pub fn root(&self) -> &Path {
        // The CAS sits directly under the store root.
        self.cas.root().parent().expect("the chunk store has a parent")
    }

    /// The key this store was opened with.
    pub fn chunk_key(&self) -> ChunkKey {
        self.key.clone()
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn cas(&self) -> &Cas {
        &self.cas
    }

    /// Store a file from disk under a logical path, as a change made here.
    ///
    /// The file is mapped once and both chunked and stored from the same
    /// mapping. An earlier version chunked the file and then read it again into
    /// a heap buffer, which read every byte twice and held the whole file in
    /// memory — invisible at test sizes and 100k files' worth of waste at scale.
    pub fn put_file(&mut self, logical_path: &str, source: &Path) -> Result<PutStats> {
        let file = std::fs::File::open(source).map_err(|e| Error::io(source, e))?;
        let meta = file.metadata().map_err(|e| Error::io(source, e))?;
        let mtime_ns = mtime_from(&meta);

        // Mapping a zero-length file fails, and there is nothing to map anyway.
        if meta.len() == 0 {
            let manifest = chunker::chunk_bytes(&[]);
            return self.put_manifest(logical_path, &manifest, &[], mtime_ns, Stamp::Local, Payloads::TrustIndex);
        }

        let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| Error::io(source, e))?;
        let manifest = chunker::chunk_bytes(&mmap);
        self.put_manifest(logical_path, &manifest, &mmap, mtime_ns, Stamp::Local, Payloads::TrustIndex)
    }

    /// Store an in-memory buffer under a logical path, as a change made here.
    pub fn put_bytes(&mut self, logical_path: &str, data: &[u8], mtime_ns: i64) -> Result<PutStats> {
        let manifest = chunker::chunk_bytes(data);
        self.put_manifest(logical_path, &manifest, data, mtime_ns, Stamp::Local, Payloads::TrustIndex)
    }

    /// Take on a version decided elsewhere, keeping the vector it arrived with.
    ///
    /// `data` is required for content and ignored for a tombstone. Adopting a
    /// tombstone for a path this device has never seen still records a row:
    /// without it, the next comparison would look like the peer inventing a
    /// file we had deleted.
    pub fn adopt(
        &mut self,
        version: &FileVersion,
        data: Option<&[u8]>,
        mtime_ns: i64,
    ) -> Result<PutStats> {
        match &version.content {
            Content::File { .. } => {
                let data = data.ok_or_else(|| Error::NotFound {
                    path: format!("{} (no content supplied)", version.path),
                })?;
                let manifest = chunker::chunk_bytes(data);
                self.put_manifest(
                    &version.path,
                    &manifest,
                    data,
                    mtime_ns,
                    Stamp::Remote(version),
                    Payloads::TrustIndex,
                )
            }
            Content::Deleted => {
                self.tombstone(&version.path, Stamp::Remote(version))?;
                Ok(PutStats::default())
            }
        }
    }

    /// Take on a version whose content is already written at `source`.
    ///
    /// The streaming counterpart of [`adopt`](Self::adopt): the file is mapped
    /// rather than read into memory, so peak heap does not depend on its size.
    /// The caller has usually just written it chunk by chunk, and handing back
    /// a buffer it never needed would undo the point of doing so.
    pub fn adopt_file(
        &mut self,
        version: &FileVersion,
        source: &Path,
        mtime_ns: i64,
    ) -> Result<PutStats> {
        let file = std::fs::File::open(source).map_err(|e| Error::io(source, e))?;
        let meta = file.metadata().map_err(|e| Error::io(source, e))?;

        if meta.len() == 0 {
            let manifest = chunker::chunk_bytes(&[]);
            return self.put_manifest(
                &version.path,
                &manifest,
                &[],
                mtime_ns,
                Stamp::Remote(version),
                Payloads::TrustIndex,
            );
        }

        let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| Error::io(source, e))?;
        let manifest = chunker::chunk_bytes(&mmap);
        self.put_manifest(
            &version.path,
            &manifest,
            &mmap,
            mtime_ns,
            Stamp::Remote(version),
            Payloads::TrustIndex,
        )
    }

    /// Record a version vector without touching content.
    ///
    /// The case this exists for: two devices reached the same bytes
    /// independently. Nothing needs to move, but leaving the versions
    /// concurrent would make the next edit on either side raise a conflict over
    /// content that never disagreed. See
    /// ../../docs/decisions/0009-conflict-edge-cases.md.
    pub fn merge_version(&mut self, version: &FileVersion) -> Result<()> {
        self.db.set_version(
            &version.path,
            &version.vector,
            &version.modified_by,
            version.modified_at,
        )
    }

    /// # Ordering
    ///
    /// Chunk payloads are written to disk *before* the index is told they
    /// exist. A crash between the two leaves chunks nothing references, which
    /// garbage collection reclaims. The opposite order would leave index
    /// entries pointing at payloads that were never written — a dangling
    /// reference, which is unrecoverable without a peer to re-fetch from.
    ///
    /// Orphaned data is a cost. Dangling references are a corruption. When only
    /// one is avoidable, prefer the cost.
    fn put_manifest(
        &mut self,
        logical_path: &str,
        manifest: &Manifest,
        data: &[u8],
        mtime_ns: i64,
        stamp: Stamp<'_>,
        payloads: Payloads,
    ) -> Result<PutStats> {
        let mut stats = PutStats { chunks_total: manifest.chunks.len(), ..Default::default() };

        // Short-circuit an unchanged file: the content hash already matches, so
        // the chunk list in the index is by definition still correct.
        if payloads == Payloads::TrustIndex {
        if let Some(existing) = self.db.file_by_path(logical_path)? {
            if existing.deleted_at.is_none() && existing.content_hash == manifest.file_hash {
                self.db.conn().execute(
                    "UPDATE files SET mtime_ns = ?1, updated_at = unixepoch() WHERE id = ?2",
                    rusqlite::params![mtime_ns, existing.id],
                )?;
                // A local write of identical bytes is not a change and must not
                // advance the clock. A version adopted from a peer still has to
                // record the history it arrived with, even though no data moved.
                if let Stamp::Remote(version) = stamp {
                    self.merge_version(version)?;
                }
                stats.unchanged = true;
                stats.bytes_deduplicated = manifest.size;
                return Ok(stats);
            }
        }
        }

        // Allocate the vector before opening the transaction: computing it
        // needs its own write, and a counter that skips a value on rollback is
        // harmless -- vectors need to be monotonic, not dense.
        let (vector, modified_by, modified_at) = match stamp {
            Stamp::Local => {
                let (v, d) = self.db.next_local_vector(logical_path)?;
                (v, d, db::now())
            }
            Stamp::Remote(version) => {
                (version.vector.clone(), version.modified_by, version.modified_at)
            }
        };

        // Phase 1: get every payload on disk. Distinct hashes only -- a file
        // that repeats a chunk internally should not write it twice.
        let mut seen = HashSet::new();
        for c in &manifest.chunks {
            if !seen.insert(c.hash) {
                stats.bytes_deduplicated += c.len as u64;
                continue;
            }

            if payloads == Payloads::TrustIndex
                && self.db.has_chunk(&c.hash)?
                && self.cas.contains(&c.hash)
            {
                stats.bytes_deduplicated += c.len as u64;
                continue;
            }

            let plaintext = &data[c.offset as usize..c.offset as usize + c.len as usize];
            let sealed = format::seal(&self.key, plaintext)?;
            self.cas.put(&c.hash, &sealed)?;
            self.db.insert_chunk(&c.hash, c.len as u64, sealed.len() as u64)?;

            stats.chunks_written += 1;
            stats.bytes_written += sealed.len() as u64;
        }

        // Phase 2: point the index at them, atomically. Replacing the chunk
        // list releases references to the previous version's chunks, which
        // starts their retention clock via the delete trigger.
        //
        // `Immediate` takes the write lock up front. Without it the transaction
        // starts as a reader and upgrades on its first write, leaving a window
        // in which the checks below are already stale.
        let tx = self
            .db
            .conn_mut()
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        // Re-establish every chunk *inside* the transaction.
        //
        // Phase 1 decided which chunks needed writing, and that decision is
        // only as good as the moment it was made. Garbage collection running in
        // another connection can remove a chunk between the check and this
        // point — it only ever removes chunks nothing references, which is
        // exactly what a chunk we are about to reference looks like until we
        // reference it.
        //
        // The write lock makes collection impossible from here to the commit,
        // so anything confirmed present now stays present. Ordinarily this
        // finds everything already in place and costs one query per chunk.
        {
            let mut exists = tx.prepare("SELECT 1 FROM chunks WHERE hash = ?1")?;
            let mut insert = tx.prepare(
                "INSERT INTO chunks (hash, size, stored_size, refcount, unreferenced_at, created_at)
                 VALUES (?1, ?2, ?3, 0, unixepoch(), unixepoch())
                 ON CONFLICT (hash) DO NOTHING",
            )?;

            let mut rechecked = HashSet::new();
            for c in &manifest.chunks {
                if !rechecked.insert(c.hash) {
                    continue;
                }
                let indexed: bool = exists.exists(rusqlite::params![c.hash.as_bytes().as_slice()])?;
                if indexed && self.cas.contains(&c.hash) {
                    continue;
                }

                // Vanished under us. Write it again, still holding the lock.
                let plaintext = &data[c.offset as usize..c.offset as usize + c.len as usize];
                let sealed = format::seal(&self.key, plaintext)?;
                self.cas.put(&c.hash, &sealed)?;
                insert.execute(rusqlite::params![
                    c.hash.as_bytes().as_slice(),
                    c.len as i64,
                    sealed.len() as i64
                ])?;

                stats.chunks_written += 1;
                stats.bytes_written += sealed.len() as u64;
            }
        }
        let file_id: i64 = tx.query_row(
            "INSERT INTO files
                 (path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
                  vector, modified_by)
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5, NULL, ?6, ?7)
             ON CONFLICT (path) DO UPDATE SET
                 size = excluded.size,
                 content_hash = excluded.content_hash,
                 mtime_ns = excluded.mtime_ns,
                 updated_at = excluded.updated_at,
                 deleted_at = NULL,
                 vector = excluded.vector,
                 modified_by = excluded.modified_by
             RETURNING id",
            rusqlite::params![
                logical_path,
                manifest.size as i64,
                manifest.file_hash.as_bytes().as_slice(),
                mtime_ns,
                modified_at,
                vector.encode(),
                modified_by.as_bytes().as_slice(),
            ],
            |r| r.get(0),
        )?;

        tx.execute("DELETE FROM file_chunks WHERE file_id = ?1", rusqlite::params![file_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO file_chunks (file_id, seq, chunk_hash) VALUES (?1, ?2, ?3)",
            )?;
            for (seq, c) in manifest.chunks.iter().enumerate() {
                stmt.execute(rusqlite::params![
                    file_id,
                    seq as i64,
                    c.hash.as_bytes().as_slice()
                ])?;
            }
        }
        tx.commit()?;

        Ok(stats)
    }

    /// Write a stored file's contents to `out`, a chunk at a time.
    ///
    /// Peak memory is one chunk — at most 2 MiB — however large the file is.
    /// That matters more than it sounds: an iOS FileProvider extension runs
    /// under a ceiling in the tens of megabytes, so a path that assembles a
    /// whole file in memory works on a desktop and is killed on a phone.
    ///
    /// Verification is the same as [`read_file`](Self::read_file): every chunk
    /// against its own hash, and the whole against the file hash. The second is
    /// not redundant — correct chunks in the wrong order would pass the first.
    ///
    /// The whole-file check necessarily comes after the last byte has been
    /// written, so a caller streaming to a destination that matters should
    /// write somewhere temporary and move it once this returns.
    pub fn read_file_into(&self, logical_path: &str, out: &mut impl std::io::Write) -> Result<u64> {
        let file = self
            .db
            .file_by_path(logical_path)?
            .filter(|f| f.deleted_at.is_none())
            .ok_or_else(|| Error::NotFound { path: logical_path.to_string() })?;

        let mut whole = blake3::Hasher::new();
        let mut written = 0u64;

        for hash in self.db.chunk_hashes_for(file.id)? {
            let plaintext = self.read_chunk(&hash)?;
            whole.update(&plaintext);
            out.write_all(&plaintext).map_err(|e| Error::io(logical_path, e))?;
            written += plaintext.len() as u64;
        }

        if whole.finalize() != file.content_hash {
            return Err(Error::ChunkCorrupt { hash: file.content_hash.to_hex().to_string() });
        }
        Ok(written)
    }

    /// Reassemble a stored file, verifying it end to end.
    ///
    /// Every chunk is checked against its own hash, and the reassembled whole
    /// against the file hash. The second check is not redundant: correct chunks
    /// in the wrong order would pass the first.
    pub fn read_file(&self, logical_path: &str) -> Result<Vec<u8>> {
        let file = self
            .db
            .file_by_path(logical_path)?
            .filter(|f| f.deleted_at.is_none())
            .ok_or_else(|| Error::NotFound { path: logical_path.to_string() })?;

        let mut out = Vec::with_capacity(file.size as usize);
        let mut whole = blake3::Hasher::new();

        for hash in self.db.chunk_hashes_for(file.id)? {
            let plaintext = self.read_chunk(&hash)?;
            whole.update(&plaintext);
            out.extend_from_slice(&plaintext);
        }

        if whole.finalize() != file.content_hash {
            return Err(Error::ChunkCorrupt { hash: file.content_hash.to_hex().to_string() });
        }
        Ok(out)
    }

    /// Fetch one chunk: read, decrypt, decompress, and check it is what it
    /// claims to be.
    pub fn read_chunk(&self, hash: &blake3::Hash) -> Result<Vec<u8>> {
        let stored = self.cas.get(hash)?;
        let plaintext = format::open(&self.key, &stored, &hash.to_hex())?;
        if blake3::hash(&plaintext) != *hash {
            return Err(Error::ChunkCorrupt { hash: hash.to_hex().to_string() });
        }
        Ok(plaintext)
    }

    /// Mark a file deleted.
    ///
    /// The row survives as a tombstone and keeps its chunk references, so the
    /// content stays restorable. Only when garbage collection expires the
    /// tombstone are those references released. See
    /// ../../docs/CODEBASE.md section 2.1 for why deletion is not simply a
    /// matter of removing rows.
    pub fn delete_file(&mut self, logical_path: &str) -> Result<()> {
        if self
            .db
            .file_by_path(logical_path)?
            .is_none_or(|f| f.deleted_at.is_some())
        {
            return Err(Error::NotFound { path: logical_path.to_string() });
        }
        self.tombstone(logical_path, Stamp::Local)
    }

    /// Write a tombstone, creating the row if this device never held the file.
    fn tombstone(&mut self, logical_path: &str, stamp: Stamp<'_>) -> Result<()> {
        let (vector, modified_by, modified_at) = match stamp {
            Stamp::Local => {
                let (v, d) = self.db.next_local_vector(logical_path)?;
                (v, d, db::now())
            }
            Stamp::Remote(version) => {
                (version.vector.clone(), version.modified_by, version.modified_at)
            }
        };

        self.db.conn().execute(
            "INSERT INTO files
                 (path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
                  vector, modified_by)
             VALUES (?1, 0, zeroblob(32), 0, unixepoch(), ?2, ?2, ?3, ?4)
             ON CONFLICT (path) DO UPDATE SET
                 deleted_at = excluded.updated_at,
                 updated_at = excluded.updated_at,
                 vector = excluded.vector,
                 modified_by = excluded.modified_by",
            rusqlite::params![
                logical_path,
                modified_at,
                vector.encode(),
                modified_by.as_bytes().as_slice(),
            ],
        )?;
        Ok(())
    }

    /// Undo a deletion, provided garbage collection has not yet expired it.
    pub fn restore_file(&mut self, logical_path: &str) -> Result<()> {
        let (vector, device) = self.db.next_local_vector(logical_path)?;
        let n = self.db.conn().execute(
            "UPDATE files SET deleted_at = NULL, updated_at = unixepoch(),
                              vector = ?2, modified_by = ?3
              WHERE path = ?1 AND deleted_at IS NOT NULL",
            rusqlite::params![
                logical_path,
                vector.encode(),
                device.as_bytes().as_slice()
            ],
        )?;
        if n == 0 {
            return Err(Error::NotFound { path: logical_path.to_string() });
        }
        Ok(())
    }

    /// The chunks making up a given whole-file hash, in order.
    ///
    /// What lets a peer transfer only the parts of a file the other side is
    /// missing, instead of the whole thing. Answered from any live path holding
    /// that content, since the chunk list depends on the bytes and not on the
    /// name they are filed under.
    pub fn chunk_hashes_for_content(
        &self,
        content: &blake3::Hash,
    ) -> Result<Option<Vec<blake3::Hash>>> {
        let Some(path) = self.db.any_path_with_content(content)? else {
            return Ok(None);
        };
        let Some(file) = self.db.file_by_path(&path)? else {
            return Ok(None);
        };
        Ok(Some(self.db.chunk_hashes_for(file.id)?))
    }

    /// Reassemble content this device holds, by hash rather than by name.
    ///
    /// Returns `None` when the content is not here — either unknown, or known
    /// but with chunks that have since been collected. Verifies the result
    /// against the hash that was asked for, so a caller can write it without
    /// checking again.
    pub fn read_content(&self, content: &blake3::Hash) -> Result<Option<Vec<u8>>> {
        let Some(chunks) = self.chunk_hashes_for_content(content)? else {
            return Ok(None);
        };

        let mut out = Vec::new();
        for chunk in chunks {
            if !self.has_chunk(&chunk)? {
                // Known content whose payloads are gone: reclaimed after the
                // retention window, or damaged. Either way it must be fetched.
                return Ok(None);
            }
            out.extend_from_slice(&self.read_chunk(&chunk)?);
        }

        if blake3::hash(&out) != *content {
            return Err(Error::ChunkCorrupt { hash: content.to_hex().to_string() });
        }
        Ok(Some(out))
    }

    /// Stream content this device holds, by hash rather than by name.
    ///
    /// Returns `None` without writing anything when the content is not here, so
    /// a caller can fall back to fetching it. Verifies the result against the
    /// hash asked for — but only once the last byte has been written, so a
    /// caller must not treat the destination as finished until this returns.
    pub fn read_content_into(
        &self,
        content: &blake3::Hash,
        out: &mut impl std::io::Write,
    ) -> Result<Option<u64>> {
        let Some(chunks) = self.chunk_hashes_for_content(content)? else {
            return Ok(None);
        };
        // Checked before anything is written, so the caller's `None` really does
        // mean nothing happened.
        for chunk in &chunks {
            if !self.has_chunk(chunk)? {
                return Ok(None);
            }
        }

        let mut whole = blake3::Hasher::new();
        let mut written = 0u64;
        for chunk in chunks {
            let plaintext = self.read_chunk(&chunk)?;
            whole.update(&plaintext);
            out.write_all(&plaintext).map_err(|e| Error::io("<destination>", e))?;
            written += plaintext.len() as u64;
        }

        if whole.finalize() != *content {
            return Err(Error::ChunkCorrupt { hash: content.to_hex().to_string() });
        }
        Ok(Some(written))
    }

    /// Whether this device holds a chunk, in the index and on disk both.
    pub fn has_chunk(&self, hash: &blake3::Hash) -> Result<bool> {
        Ok(self.db.has_chunk(hash)? && self.cas.contains(hash))
    }

    /// Every path this device knows about, tombstones included: what it would
    /// advertise to a peer.
    pub fn tree(&self) -> Result<Vec<FileVersion>> {
        self.db.all_versions()
    }

    /// This device's identity.
    pub fn device_id(&self) -> Result<DeviceId> {
        self.db.local_device()
    }

    pub fn list(&self) -> Result<Vec<String>> {
        self.db.live_paths()
    }

    /// Reclaim space. See [`crate::gc`].
    pub fn gc(&mut self, retention: Duration) -> Result<GcStats> {
        gc::collect(&mut self.db, &self.cas, retention)
    }

    /// Write a file's payloads back, whatever the index currently believes.
    ///
    /// For repair only. Every other path trusts the index when the content hash
    /// matches, which is what makes an unchanged file cost a stat rather than a
    /// read — but repair has just thrown a payload away on purpose, and needs
    /// the write to happen anyway.
    pub fn rewrite_payloads(
        &mut self,
        version: &FileVersion,
        data: &[u8],
        mtime_ns: i64,
    ) -> Result<PutStats> {
        let manifest = chunker::chunk_bytes(data);
        self.put_manifest(
            &version.path,
            &manifest,
            data,
            mtime_ns,
            Stamp::Remote(version),
            Payloads::Rewrite,
        )
    }

    /// Throw away a chunk's payload, keeping the index entry.
    ///
    /// Used by repair when a payload is found damaged. The index row stays,
    /// because files still reference it and the reference count is still
    /// correct — what is gone is the bytes. A later write of the same content
    /// will notice the payload is absent and store it again.
    pub fn discard_payload(&mut self, hash: &blake3::Hash) -> Result<()> {
        self.cas.remove(hash)
    }

    /// Remove payloads on disk the index does not know about. See
    /// [`crate::gc::sweep_orphans`].
    pub fn sweep_orphans(&mut self) -> Result<GcStats> {
        gc::sweep_orphans(&mut self.db, &self.cas)
    }

    /// Check the index and the chunk store agree with each other.
    ///
    /// `deep` additionally decrypts and re-hashes every chunk, which is the
    /// only way to detect silent disk corruption but costs a full read of the
    /// store.
    pub fn verify(&self, deep: bool) -> Result<VerifyReport> {
        let mut report = VerifyReport::default();

        for drift in self.db.audit_refcounts()? {
            report.refcount_drift.push(drift);
        }

        let mut stmt = self.db.conn().prepare("SELECT hash FROM chunks")?;
        let indexed: Vec<blake3::Hash> = stmt
            .query_map([], |r| {
                let raw: Vec<u8> = r.get(0)?;
                Ok(db::to_hash(&raw))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let indexed_set: HashSet<_> = indexed.iter().copied().collect();

        for hash in &indexed {
            if !self.cas.contains(hash) {
                report.missing.push(*hash);
            } else if deep {
                match self.read_chunk(hash) {
                    Ok(_) => {}
                    Err(Error::ChunkCorrupt { .. }) | Err(Error::Decrypt { .. }) => {
                        report.corrupt.push(*hash)
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        for hash in self.cas.iter_hashes()? {
            if !indexed_set.contains(&hash) {
                report.orphaned.push(hash);
            }
        }

        Ok(report)
    }
}

/// The outcome of [`Store::verify`].
#[derive(Debug, Default)]
pub struct VerifyReport {
    /// Referenced by the index, absent from the store. **Data loss.**
    pub missing: Vec<blake3::Hash>,
    /// Present on disk but unknown to the index. Wasted space, not corruption;
    /// normal after a crash mid-write.
    pub orphaned: Vec<blake3::Hash>,
    /// Present but does not decrypt, or does not hash to its own name.
    pub corrupt: Vec<blake3::Hash>,
    /// Stored reference counts that disagree with the links they describe.
    pub refcount_drift: Vec<db::RefcountDrift>,
}

impl VerifyReport {
    /// Whether anything found threatens data rather than merely wasting space.
    pub fn is_healthy(&self) -> bool {
        self.missing.is_empty() && self.corrupt.is_empty() && self.refcount_drift.is_empty()
    }
}

fn mtime_from(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}
