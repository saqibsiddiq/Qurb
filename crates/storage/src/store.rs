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
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Store {
    cas: Cas,
    db: Db,
    key: ChunkKey,
    /// Where this device's materialised files live, if it has any.
    ///
    /// A syncing device writes every file to a folder the user can see, so the
    /// plaintext is already on disk — and keeping an encrypted copy of it in
    /// the chunk store as well costs a second copy of everything the user
    /// owns. With this set, the tree *is* the payload store for content it
    /// holds, and the chunk store keeps only what the tree does not provide.
    ///
    /// `None` for a storage-only replica, which has no tree and must therefore
    /// keep every payload itself. That is not a special case bolted on: a
    /// replica is precisely the device whose content is not materialised.
    tree: Option<PathBuf>,
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
        Ok(Self { cas, db, key, tree: None })
    }

    /// Tell the store where materialised files live.
    ///
    /// With a tree, content the user can already see is not stored a second
    /// time: `read_chunk` reads those bytes back out of the file. Without one —
    /// a storage-only replica — every payload is kept in the chunk store, which
    /// is the only copy that exists there.
    ///
    /// The root is the *sync* folder, not the store directory: the store lives
    /// inside it, at `<root>/.qurb`.
    pub fn in_tree(mut self, root: impl Into<PathBuf>) -> Self {
        self.tree = Some(root.into());
        self
    }

    /// Whether this store materialises files.
    pub fn has_tree(&self) -> bool {
        self.tree.is_some()
    }

    /// Whether the tree holds a readable file at this logical path.
    ///
    /// The question is not "is there a tree" but "will these bytes still be
    /// reachable afterwards". A path the tree does not actually have must keep
    /// its payloads in the chunk store, or the content is gone.
    fn supplies(&self, logical_path: &str) -> bool {
        self.tree
            .as_ref()
            .map(|tree| tree.join(logical_path).is_file())
            .unwrap_or(false)
    }

    /// Read a chunk's plaintext out of the file that holds it.
    ///
    /// Returns `None` when no live file provides it, which is the ordinary
    /// case for a replica and for content whose file has been deleted.
    ///
    /// The bytes are verified against the hash asked for. A file the user has
    /// edited since it was indexed will not match, and saying so is right:
    /// the content that hash names is genuinely no longer there, and returning
    /// the new bytes under the old name would corrupt whatever asked.
    fn chunk_from_tree(&self, hash: &blake3::Hash) -> Result<Option<Vec<u8>>> {
        let Some(tree) = &self.tree else { return Ok(None) };
        let Some((path, offset, len)) = self.db.locate_chunk(hash)? else { return Ok(None) };

        let full = tree.join(&path);
        let mut file = match std::fs::File::open(&full) {
            Ok(file) => file,
            // Gone or unreadable: not an error here, just not a source.
            Err(_) => return Ok(None),
        };

        use std::io::{Read, Seek, SeekFrom};
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return Ok(None);
        }
        let mut buffer = vec![0u8; len as usize];
        if file.read_exact(&mut buffer).is_err() {
            return Ok(None);
        }

        if blake3::hash(&buffer) != *hash {
            // The file changed under us. The index will catch up on the next
            // scan; until then this chunk simply has no source here.
            return Ok(None);
        }
        Ok(Some(buffer))
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

        // Whether the file being recorded is the one the tree holds at this
        // path -- in which case the tree supplies the payloads and the chunk
        // store need not.
        //
        // Checked by path rather than assumed, because `put_file` accepts any
        // source: importing from elsewhere on the disk copies into the tree
        // first, but nothing in the type system says so, and skipping the
        // write for a file that is *not* in the tree would lose the content
        // entirely.
        let materialised = self.supplies(logical_path);

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

            // Where the tree already holds these bytes, do not store them
            // again. The file the user can see *is* the payload; a second
            // encrypted copy of it would double what every synced file costs,
            // for content that is already on this disk and already readable.
            //
            // `stored_size` is recorded as zero for such a chunk, which is
            // true: it occupies nothing of its own.
            if materialised {
                self.db.insert_chunk(&c.hash, c.len as u64, 0)?;
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

                // A tree-backed chunk is deliberately absent from the CAS, so
                // "indexed but no payload" is its normal state, not evidence
                // that collection took it. Re-assert the index row -- that much
                // *can* have been collected -- and never write the payload.
                if materialised {
                    if !indexed {
                        insert.execute(rusqlite::params![
                            c.hash.as_bytes().as_slice(),
                            c.len as i64,
                            0i64
                        ])?;
                    }
                    continue;
                }

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
        // Whether this device is holding the bytes afterwards. A store with a
        // folder holds what the folder has; a replica holds chunks and so
        // always holds it. Recorded on every write so that a file coming back
        // — re-created by the user, or fetched after being dropped for the
        // storage cap — stops being marked as evicted without anyone having to
        // remember to clear it.
        let holding = materialised || self.tree.is_none();

        let file_id: i64 = tx.query_row(
            "INSERT INTO files
                 (path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
                  vector, modified_by, materialised, touched_at, wanted)
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5, NULL, ?6, ?7, ?8, unixepoch(), 0)
             ON CONFLICT (path) WHERE scope IS NULL DO UPDATE SET
                 size = excluded.size,
                 content_hash = excluded.content_hash,
                 mtime_ns = excluded.mtime_ns,
                 updated_at = excluded.updated_at,
                 deleted_at = NULL,
                 vector = excluded.vector,
                 modified_by = excluded.modified_by,
                 materialised = excluded.materialised,
                 touched_at = excluded.touched_at,
                 wanted = CASE WHEN excluded.materialised = 1 THEN 0 ELSE wanted END
             RETURNING id",
            rusqlite::params![
                logical_path,
                manifest.size as i64,
                manifest.file_hash.as_bytes().as_slice(),
                mtime_ns,
                modified_at,
                vector.encode(),
                modified_by.as_bytes().as_slice(),
                holding as i64,
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
        // The tree first when the chunk store does not hold it. Ordered this
        // way round -- cheap membership test, then the file -- so a replica and
        // any content the tree cannot supply take the original path unchanged.
        if !self.cas.contains(hash) {
            if let Some(plaintext) = self.chunk_from_tree(hash)? {
                return Ok(plaintext);
            }
        }

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
    ///
    /// Content the *folder* was holding is the exception, and has to be. Those
    /// bytes left with the file, so the tombstone cannot keep them restorable
    /// however long it is held — and a reference to a payload that no longer
    /// exists is the index claiming content it cannot produce, which `verify`
    /// is right to call damage. Those references are released here instead.
    /// See [decision 0024](../../docs/decisions/0024-the-file-is-the-payload-store.md).
    pub fn delete_file(&mut self, logical_path: &str) -> Result<()> {
        if self
            .db
            .file_by_path(logical_path)?
            .is_none_or(|f| f.deleted_at.is_some())
        {
            return Err(Error::NotFound { path: logical_path.to_string() });
        }
        self.tombstone(logical_path, Stamp::Local)?;
        self.release_unbacked(logical_path)
    }

    /// Drop a tombstone's references to payloads nothing holds any more.
    ///
    /// Only the ones with no payload: a chunk the chunk store really has is
    /// still restorable and keeps its reference for the retention window,
    /// which is what a replica and any content imported from outside the
    /// folder rely on.
    fn release_unbacked(&mut self, logical_path: &str) -> Result<()> {
        if self.tree.is_none() {
            return Ok(());
        }

        let Some(row) = self.db.file_by_path(logical_path)? else { return Ok(()) };
        let orphaned: Vec<blake3::Hash> = self
            .db
            .chunk_hashes_for(row.id)?
            .into_iter()
            .filter(|hash| !self.cas.contains(hash))
            .collect();

        for hash in orphaned {
            // The delete trigger releases the reference, which starts the
            // chunk's collection clock.
            self.db.conn().execute(
                "DELETE FROM file_chunks WHERE file_id = ?1 AND chunk_hash = ?2",
                rusqlite::params![row.id, hash.as_bytes().as_slice()],
            )?;
        }
        Ok(())
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
             ON CONFLICT (path) WHERE scope IS NULL DO UPDATE SET
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
        // By id, not by way of the path: a path is no longer a unique handle,
        // and this question is about whether the bytes are here rather than
        // about which namespace holds them.
        let Some(id) = self.db.any_file_with_content(content)? else {
            return Ok(None);
        };
        Ok(Some(self.db.chunk_hashes_for(id)?))
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
    /// Whether this device can produce a chunk's bytes right now.
    ///
    /// Three places it can come from, and all three must be consulted. The
    /// index has to know it, and then either the chunk store holds the payload
    /// or the folder does. Asking only the chunk store would call almost every
    /// chunk absent on a device that syncs a folder, and the caller that most
    /// often asks is the one deciding whether to pull content over the
    /// network — so getting this wrong re-transfers files that are already
    /// here rather than failing visibly.
    pub fn has_chunk(&self, hash: &blake3::Hash) -> Result<bool> {
        if !self.db.has_chunk(hash)? {
            return Ok(false);
        }
        if self.cas.contains(hash) {
            return Ok(true);
        }
        Ok(matches!(self.chunk_from_tree(hash), Ok(Some(_))))
    }

    /// Every path one device is entitled to know about.
    ///
    /// The shared area plus that device's own vault, and never anybody else's.
    /// See [`db::Audience`], which distinguishes the three cases this depends
    /// on getting right.
    pub fn tree_for(&self, audience: db::Audience<'_>) -> Result<Vec<FileVersion>> {
        self.db.versions_for(audience)
    }

    /// Whether an audience may fetch this chunk's bytes.
    pub fn chunk_visible_to(
        &self,
        hash: &blake3::Hash,
        audience: db::Audience<'_>,
    ) -> Result<bool> {
        self.db.chunk_visible_to(hash, audience)
    }

    /// Whether an audience may resolve this content hash.
    pub fn content_visible_to(
        &self,
        content: &blake3::Hash,
        audience: db::Audience<'_>,
    ) -> Result<bool> {
        self.db.content_visible_to(content, audience)
    }

    /// Put a path in a device's private vault, or back in the shared area.
    pub fn set_scope(&self, logical_path: &str, vault: Option<&DeviceId>) -> Result<bool> {
        self.db.set_scope(logical_path, vault)
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

    /// What this store is costing on disk, in bytes.
    ///
    /// Both halves, because under single-copy storage neither is the whole
    /// picture: the files the folder is holding, plus the chunk payloads that
    /// the folder cannot supply. A cap has to bound the sum — bounding the
    /// chunk store alone would bound almost nothing.
    pub fn usage(&self) -> Result<Usage> {
        Ok(Usage {
            // Zero without a folder, and that is not a special case so much as
            // the plain meaning: a store with no folder holds nothing in one.
            // Its bytes are all in the chunk store and are counted there, so
            // adding the file sizes as well would report a replica using twice
            // the disk it does.
            files: match self.tree {
                Some(_) => self.db.materialised_bytes()?,
                None => 0,
            },
            chunks: self.db.size_totals()?.1,
        })
    }

    /// Drop a file's bytes, keeping everything the index knows about it.
    ///
    /// The file leaves the folder and the row stays, marked as not held here.
    /// Afterwards the path still syncs, still appears in listings, and still
    /// has a content hash and a chunk list — it simply has no local content
    /// until something fetches it back.
    ///
    /// Refuses when no other device is known to hold the content. That check is
    /// the difference between eviction and data loss, and it is made here
    /// rather than in the caller so that no caller can skip it.
    pub fn evict(&mut self, logical_path: &str) -> Result<u64> {
        let Some(tree) = self.tree.clone() else {
            return Err(Error::CannotEvict {
                path: logical_path.to_string(),
                why: "this device has no folder, so it is the only holder",
            });
        };

        let Some(row) = self.db.file_by_path(logical_path)? else {
            return Err(Error::NotFound { path: logical_path.to_string() });
        };
        if row.deleted_at.is_some() {
            return Err(Error::NotFound { path: logical_path.to_string() });
        }

        if self.db.replica_count(&row.content_hash)? == 0 {
            return Err(Error::CannotEvict {
                path: logical_path.to_string(),
                why: "no other device is known to hold this content",
            });
        }

        // Mark first, remove second. The other order leaves a window in which
        // the file is gone from the folder and the index still calls it held:
        // a scan landing there would read that as the user deleting it and
        // propagate a tombstone. Marked-but-present is the harmless direction —
        // it costs one needless fetch at worst.
        self.db.set_materialised(logical_path, false)?;

        let full = tree.join(logical_path);
        let freed = std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(&full) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                self.db.set_materialised(logical_path, true)?;
                return Err(Error::Io { path: full, source: e });
            }
        }

        Ok(freed)
    }

    /// Whether this device is holding a live path's bytes.
    pub fn is_materialised(&self, logical_path: &str) -> Result<Option<bool>> {
        self.db.is_materialised(logical_path)
    }

    /// Live paths whose bytes this device has dropped to stay under its cap.
    pub fn evicted(&self) -> Result<Vec<String>> {
        self.db.evicted_paths()
    }

    /// Live files this device made that no other device is known to hold.
    /// See [`Db::undelivered`].
    pub fn undelivered(&self) -> Result<Vec<(String, u64)>> {
        self.db.undelivered()
    }

    /// Note that another device has taken delivery of this content.
    pub fn note_replica(&self, content: &blake3::Hash, device: &DeviceId) -> Result<()> {
        self.db.note_replica(content, device)
    }

    /// Files whose bytes could be dropped, coldest first. See [`Db::evictable`].
    pub fn evictable(&self) -> Result<Vec<(String, u64, blake3::Hash)>> {
        self.db.evictable()
    }

    /// Drop payloads the tree can supply, and report what that freed.
    ///
    /// The single-copy rule applies when a file is indexed. A store written
    /// before the rule existed — or one whose tree was attached later — holds
    /// a second encrypted copy of content the user's own folder already has,
    /// and nothing re-indexes an unchanged file, so that copy would otherwise
    /// stay forever. This is the one-off pass that removes it.
    ///
    /// Safe to interrupt: each chunk is verified out of the tree *before* its
    /// payload is deleted, so a chunk is only ever dropped once the bytes are
    /// known to be readable somewhere else. A storage-only replica has no tree
    /// and reclaims nothing, which is correct — it is the only holder.
    pub fn reclaim(&mut self) -> Result<GcStats> {
        let mut stats = GcStats::default();
        if self.tree.is_none() {
            return Ok(stats);
        }

        for hash in self.cas.iter_hashes()? {
            if !matches!(self.chunk_from_tree(&hash), Ok(Some(_))) {
                continue;
            }
            let freed = self.cas.stored_size(&hash).unwrap_or(0);
            self.cas.remove(&hash)?;
            self.db.conn().execute(
                "UPDATE chunks SET stored_size = 0 WHERE hash = ?1",
                rusqlite::params![hash.as_bytes().as_slice()],
            )?;
            stats.chunks_removed += 1;
            stats.bytes_reclaimed += freed;
        }

        Ok(stats)
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

        let mut stmt = self.db.conn().prepare("SELECT hash, refcount FROM chunks")?;
        let indexed: Vec<(blake3::Hash, i64)> = stmt
            .query_map([], |r| {
                let raw: Vec<u8> = r.get(0)?;
                Ok((db::to_hash(&raw), r.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let indexed_set: HashSet<_> = indexed.iter().map(|(hash, _)| *hash).collect();

        for (hash, references) in &indexed {
            // A chunk nothing references is waiting to be collected. Its
            // payload being gone is not data loss — no file depends on it —
            // and under single-copy storage this is the ordinary state of a
            // superseded version, whose bytes left when the file was
            // overwritten. Reporting these would mean `verify` calling a
            // device damaged for the normal act of editing a file.
            if *references == 0 {
                continue;
            }

            if !self.cas.contains(hash) {
                // Not in the chunk store is not the same as missing. A tree
                // supplies the payloads for content it materialises, and
                // reporting every such chunk as lost would make `verify`
                // useless on exactly the devices people run it on.
                match self.chunk_from_tree(hash) {
                    Ok(Some(_)) => {}
                    _ => report.missing.push(*hash),
                }
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

/// What a store is costing on disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Live files this device is holding in the folder.
    pub files: u64,
    /// Chunk payloads, which under single-copy storage are only the ones the
    /// folder cannot supply: remote content not yet materialised, superseded
    /// versions, and anything indexed from outside the folder.
    pub chunks: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.files + self.chunks
    }
}
