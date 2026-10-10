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
    /// Whether a file this device adds, that the index has never seen, goes
    /// into this device's own vault rather than the shared area. Set on a
    /// phone, where a person's own files are theirs until they send them
    /// somewhere ([decision 0036]); left off on a desktop.
    ///
    /// [decision 0036]: ../../docs/decisions/0036-a-phone-keeps-its-own-files.md
    new_files_private: bool,
    /// How to open a send's file that is not a path: on Android, a document
    /// the app was lent (decision 0060). `None` everywhere else.
    documents: Option<std::sync::Arc<dyn Documents>>,
}

/// Opens a send's file again when it is to be read, for a source that is not
/// a path -- on Android, a `content://` document the app may read again later
/// (decision 0060). The platform supplies it, the way it supplies the keystore
/// (decision 0021).
pub trait Documents: Send + Sync {
    /// The document, open for reading, or `None` when it cannot be opened any
    /// more: deleted, or the permission to read it taken back.
    fn open(&self, source: &str) -> Option<std::fs::File>;
    /// Nothing more will be read from it: give back the permission to.
    fn release(&self, source: &str);
}

/// A send called off because its file changed or went before the recipient
/// collected it (decision 0060).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalledOff {
    /// The name the recipient would have seen.
    pub path: String,
    pub to: DeviceId,
    /// Why, in words for the person who sent it.
    pub why: String,
}

/// How long a deleted file stays in Recently deleted before it goes for good.
///
/// Thirty days: long enough to notice something is missing after a holiday,
/// short enough that a deletion made on purpose is really gone within a month.
/// Under storage pressure the trash goes sooner -- it is the first thing freed.
pub const TRASH_RETENTION: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);

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

/// The three things about a write that the caller decides and the bytes cannot.
///
/// Grouped because they travel together through every path into the index, and
/// because seven positional arguments of which three are `Some(_)`/`None` is a
/// shape that invites silent mistakes at the call site.
struct Placement<'a> {
    stamp: Stamp<'a>,
    payloads: Payloads,
    /// Whose vault this belongs in. `None` is the shared area — everything the
    /// product had before vaults existed.
    vault: Option<&'a DeviceId>,
    /// Kept for the vault's owner rather than sent to it. See [`Store::hold_file`].
    held: bool,
    /// A new version even when the bytes are what the row already holds: a
    /// send made again is a new send (decision 0059).
    fresh: bool,
    /// The bytes stay where they are, and are read from there when asked
    /// for: a send, which keeps no copy (decision 0060).
    by_reference: bool,
}

impl<'a> Placement<'a> {
    /// A change made on this device, in the shared area.
    fn local() -> Self {
        Self {
            stamp: Stamp::Local,
            payloads: Payloads::TrustIndex,
            vault: None,
            held: false,
            fresh: false,
            by_reference: false,
        }
    }

    /// A version decided elsewhere, in the shared area.
    fn remote(version: &'a FileVersion) -> Self {
        Self {
            stamp: Stamp::Remote(version),
            payloads: Payloads::TrustIndex,
            vault: None,
            held: false,
            fresh: false,
            by_reference: false,
        }
    }

    /// The same, but into a vault rather than the shared area.
    fn in_vault(mut self, owner: Option<&'a DeviceId>) -> Self {
        self.vault = owner;
        self
    }

    /// Kept for the vault's owner, never released.
    fn held(mut self) -> Self {
        self.held = true;
        self
    }

    /// A new version, however unchanged the bytes.
    fn fresh(mut self) -> Self {
        self.fresh = true;
        self
    }

    /// Record the chunks, store none of them.
    fn by_reference(mut self) -> Self {
        self.by_reference = true;
        self
    }

    /// Write every payload, whatever the index believes. Repair only.
    fn rewriting(mut self) -> Self {
        self.payloads = Payloads::Rewrite;
        self
    }
}

/// One file of a guest's folder, as it reads once opened at the computer
/// keeping it (decision 0060).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptFile {
    pub path: String,
    pub size: u64,
    pub modified_at: i64,
    /// The sealed file it is kept as, to unseal it from.
    pub sealed: blake3::Hash,
}

/// A file this device has sent to a device before (decision 0059).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentBefore {
    /// The file picked now: a path, or a document the platform lends.
    pub source: String,
    /// The name it went under then.
    pub sent_as: String,
    /// When, in unix seconds.
    pub at: i64,
}

/// A file's content hash as the index records it -- BLAKE3 over all of its
/// bytes in order, which is what [`chunker::chunk_bytes`] computes -- read in
/// pieces rather than held.
fn hash_reader(mut file: std::fs::File) -> std::io::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize())
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
        Ok(Self { cas, db, key, tree: None, new_files_private: false, documents: None })
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

    /// File new local files in this device's own vault. See the field.
    pub fn set_new_files_private(&mut self, private: bool) {
        self.new_files_private = private;
    }

    /// Whether a file added here goes into this device's own vault.
    pub fn new_files_private(&self) -> bool {
        self.new_files_private
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

    /// A send's chunk, read from the send's file and checked (decision 0060).
    ///
    /// `None` when there is no such send, or its file no longer holds those
    /// bytes. The send is called off at the next check, and the person told;
    /// here it is only not a source.
    fn chunk_from_source(&self, hash: &blake3::Hash) -> Result<Option<Vec<u8>>> {
        let Some((source, offset, len)) = self.db.locate_send_chunk(hash)? else { return Ok(None) };
        let Ok(mut file) = self.open_source(&source) else { return Ok(None) };
        use std::io::{Read, Seek, SeekFrom};
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return Ok(None);
        }
        let mut buffer = vec![0u8; len as usize];
        if file.read_exact(&mut buffer).is_err() || blake3::hash(&buffer) != *hash {
            // The file is not what was sent any more. Marked, so that the next
            // check calls the send off even when its size and time say
            // nothing changed.
            self.db.send_source_changed(&source)?;
            return Ok(None);
        }
        Ok(Some(buffer))
    }

    /// A chunk that is not in the chunk store, from wherever this device has
    /// it on disk: a file in the folder, or a send's file.
    fn chunk_from_disk(&self, hash: &blake3::Hash) -> Result<Option<Vec<u8>>> {
        match self.chunk_from_tree(hash)? {
            Some(plaintext) => Ok(Some(plaintext)),
            None => self.chunk_from_source(hash),
        }
    }

    /// Open a send's file: a path, or a document the platform lends.
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        if source.starts_with("content://") {
            return self
                .documents
                .as_ref()
                .and_then(|documents| documents.open(source))
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "the document cannot be opened"));
        }
        std::fs::File::open(source)
    }

    /// How to open documents that are not paths (decision 0060). Android's
    /// app sets this when it opens the engine.
    pub fn set_documents(&mut self, documents: std::sync::Arc<dyn Documents>) {
        self.documents = Some(documents);
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
            return self.put_manifest(logical_path, &manifest, &[], mtime_ns, Placement::local());
        }

        let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| Error::io(source, e))?;
        let manifest = chunker::chunk_bytes(&mmap);
        let stats = self.put_manifest(logical_path, &manifest, &mmap, mtime_ns, Placement::local())?;
        // Recorded here rather than in the engine, because the bulk pass stores
        // through worker threads with their own handles and would otherwise
        // have to remember to. A file that turned out to be identical is not
        // something that happened: re-reading an unchanged file is the hot path
        // and runs on every reconciliation.
        if !stats.unchanged {
            let _ = self.db.record(
                db::Event::Stored,
                Some(logical_path),
                Some(manifest.size),
                None,
                None,
            );
        }
        Ok(stats)
    }

    /// Store an in-memory buffer under a logical path, as a change made here.
    pub fn put_bytes(&mut self, logical_path: &str, data: &[u8], mtime_ns: i64) -> Result<PutStats> {
        let manifest = chunker::chunk_bytes(data);
        self.put_manifest(logical_path, &manifest, data, mtime_ns, Placement::local())
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
                    Placement::remote(version),
                )
            }
            Content::Deleted => {
                self.tombstone(&version.path, Stamp::Remote(version), None)?;
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
        self.adopt_file_scoped(version, source, mtime_ns, None)
    }

    /// Take on a version that was sent to this device privately.
    ///
    /// Written into the folder exactly like anything else — the user asked for
    /// a file and should find a file. What differs is the index row: it is
    /// scoped to this device, so it is never advertised to any peer. A
    /// received file that entered the shared area would be pushed to every
    /// other device the next time they synced, which is the opposite of what
    /// sending something to one device means.
    pub fn adopt_file_privately(
        &mut self,
        version: &FileVersion,
        source: &Path,
        mtime_ns: i64,
    ) -> Result<PutStats> {
        let me = self.db.local_device()?;
        let stats = self.adopt_file_scoped(version, source, mtime_ns, Some(&me))?;
        self.db.note_taken(version, &version.path)?;
        Ok(stats)
    }

    fn adopt_file_scoped(
        &mut self,
        version: &FileVersion,
        source: &Path,
        mtime_ns: i64,
        vault: Option<&DeviceId>,
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
                Placement::remote(version).in_vault(vault),
            );
        }

        let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| Error::io(source, e))?;
        let manifest = chunker::chunk_bytes(&mmap);
        self.put_manifest(
            &version.path,
            &manifest,
            &mmap,
            mtime_ns,
            Placement::remote(version).in_vault(vault),
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
        placement: Placement<'_>,
    ) -> Result<PutStats> {
        let Placement { stamp, payloads, vault, held, fresh, by_reference } = placement;
        let mut stats = PutStats { chunks_total: manifest.chunks.len(), ..Default::default() };

        // A path that is already this device's own private content stays
        // private when it changes here.
        //
        // Received files live in the folder like any other, so an ordinary
        // write to one -- an edit, or a scan re-reading it -- arrives here with
        // no vault named. Letting that create a shared row would publish
        // somebody's private file to every device the moment it was touched,
        // which is what happened on a phone: the scan after a delivery indexed
        // it as shared and advertised it straight back to the sender.
        //
        // Staying private is also the right answer for a genuine edit. A file
        // that silently became public because it was opened and saved would be
        // the worse of the two mistakes.
        //
        // Local changes only. A version arriving from another device says for
        // itself which area it belongs to, and a shared one must never be
        // re-filed as private because a received file happens to share its
        // name -- that would quietly take it out of the shared area here while
        // every other device still has it there.
        //
        // And on a device that keeps its own files private, a file the index
        // has never seen goes into this device's vault. A path already in the
        // shared area stays there: an edit does not move a file between areas
        // in either direction.
        let vault = match vault {
            Some(owner) => Some(*owner),
            None if matches!(stamp, Stamp::Local) => match self.db.folder_row(logical_path)? {
                Some((_, scope)) => scope,
                None if self.new_files_private
                    && self.db.file_by_path(logical_path)?.is_none_or(|f| f.deleted_at.is_some()) =>
                {
                    Some(self.db.local_device()?)
                }
                None => None,
            },
            None => None,
        };
        let vault = vault.as_ref();

        // Short-circuit an unchanged file: the content hash already matches, so
        // the chunk list in the index is by definition still correct.
        //
        // Compared against the row this write would land on, in the area just
        // decided, and not against whatever row the path happens to have. A file
        // sent from the folder under its own name has the same path and the same
        // bytes as the sender's own copy; comparing against that made every such
        // send look like an unchanged file, and nothing was ever sent.
        if payloads == Payloads::TrustIndex && !fresh {
            if let Some(existing) = self.db.live_row_in(logical_path, vault)? {
                if existing.content_hash == manifest.file_hash {
                    // Unchanged content can still be a change in whether this
                    // device holds it: a file dropped for the storage cap and
                    // fetched back arrives exactly like this. Without saying so
                    // here it stayed marked as dropped with its bytes sitting in
                    // the folder -- fetched again on every sync, and a later
                    // deletion of it taken for the cap's own doing and never
                    // passed on.
                    let backed = match vault {
                        None => true,
                        Some(owner) => *owner == self.db.local_device()?,
                    };
                    let holding = (backed && self.supplies(logical_path)) || self.tree.is_none();
                    self.db.conn().execute(
                        "UPDATE files SET mtime_ns = ?1, updated_at = unixepoch(),
                                          materialised = ?3,
                                          wanted = CASE WHEN ?3 = 1 THEN 0 ELSE wanted END
                          WHERE id = ?2",
                        rusqlite::params![mtime_ns, existing.id, holding as i64],
                    )?;
                    // A send the owner has collected, now kept for it: the same
                    // bytes, and from here on never released.
                    if let (true, Some(owner)) = (held, vault) {
                        self.db.mark_held(logical_path, owner)?;
                    }
                    // A local write of identical bytes is not a change and must
                    // not advance the clock. A version adopted from a peer still
                    // has to record the history it arrived with, even though no
                    // data moved -- in the shared area, which is the only place
                    // history is compared. `merge_version` writes the shared
                    // row, so a private version's history would land on a
                    // namesake.
                    if let (Stamp::Remote(version), None) = (&stamp, vault) {
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

        // A vault entry held for *someone else* is never backed by a file in
        // this device's folder, even when a file of that name and content
        // happens to be sitting there. Letting it lean on that file would mean
        // deleting your own `report.pdf` quietly destroyed the copy you sent
        // somebody — which is not a say you should have over their data.
        //
        // This device's *own* vault is different: content sent here is written
        // into the folder like anything else, and is read from there.
        let backed_by_folder = match vault {
            None => true,
            Some(owner) => *owner == self.db.local_device()?,
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
        let materialised = backed_by_folder && self.supplies(logical_path);

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
            // true: it occupies nothing of its own. Nor does a send's: its
            // file supplies it, wherever that is (decision 0060).
            if materialised || by_reference {
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
                // The same for a send's, read from its file.
                if materialised || by_reference {
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

        let conflict = match vault {
            None => "ON CONFLICT (path) WHERE scope IS NULL DO UPDATE SET",
            Some(_) => "ON CONFLICT (scope, path) WHERE scope IS NOT NULL DO UPDATE SET",
        };
        let file_id: i64 = tx.query_row(
            &format!(
            "INSERT INTO files
                 (path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
                  vector, modified_by, materialised, touched_at, wanted, scope, held)
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5, NULL, ?6, ?7, ?8, unixepoch(), 0, ?9, ?10)
             {conflict}
                 held = max(held, excluded.held),
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
             RETURNING id"
            ),
            rusqlite::params![
                logical_path,
                manifest.size as i64,
                manifest.file_hash.as_bytes().as_slice(),
                mtime_ns,
                modified_at,
                vector.encode(),
                modified_by.as_bytes().as_slice(),
                holding as i64,
                vault.map(|d| d.as_bytes().to_vec()),
                held as i64,
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
        // Anything in the folder, shared or sent here: reading back a file
        // somebody sent you is the commonest reason to read one by name.
        let file = self
            .db
            .in_folder(logical_path)?
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
        // Anything in the folder, shared or sent here: reading back a file
        // somebody sent you is the commonest reason to read one by name.
        let file = self
            .db
            .in_folder(logical_path)?
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
            if let Some(plaintext) = self.chunk_from_disk(hash)? {
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
        // Either kind of file the folder holds. Deleting something sent to this
        // device is the recipient's business and has to work; before this it
        // came back as "not found" and the row stayed live for ever.
        //
        // The row found is the row tombstoned and the row released. Looking the
        // path up again at each step would find the shared namesake whenever
        // there is one, and leave the private file live with its references to
        // a payload that has just been deleted.
        let Some((row, scope)) = self.db.folder_row(logical_path)? else {
            return Err(Error::NotFound { path: logical_path.to_string() });
        };
        self.tombstone(logical_path, Stamp::Local, scope)?;
        let _ = self.db.record(db::Event::Deleted, Some(logical_path), None, None, None);
        self.release_unbacked(row.id)
    }

    /// Drop a tombstone's references to payloads nothing holds any more.
    ///
    /// Only the ones with no payload: a chunk the chunk store really has is
    /// still restorable and keeps its reference for the retention window,
    /// which is what a replica and any content imported from outside the
    /// folder rely on.
    fn release_unbacked(&mut self, file_id: i64) -> Result<()> {
        if self.tree.is_none() {
            return Ok(());
        }

        let orphaned: Vec<blake3::Hash> = self
            .db
            .chunk_hashes_for(file_id)?
            .into_iter()
            .filter(|hash| !self.cas.contains(hash))
            .collect();

        for hash in orphaned {
            // The delete trigger releases the reference, which starts the
            // chunk's collection clock.
            self.db.conn().execute(
                "DELETE FROM file_chunks WHERE file_id = ?1 AND chunk_hash = ?2",
                rusqlite::params![file_id, hash.as_bytes().as_slice()],
            )?;
        }
        Ok(())
    }

    /// Write a tombstone, creating the row if this device never held the file.
    ///
    /// `scope` says which row: `None` for the shared area, this device's id for
    /// something sent here. The caller says, rather than this guessing from the
    /// path, because a deletion that arrived from another device is always
    /// about the shared area — guessing would let a peer deleting its own
    /// `notes.txt` tombstone the private `notes.txt` somebody sent here.
    fn tombstone(
        &mut self,
        logical_path: &str,
        stamp: Stamp<'_>,
        scope: Option<DeviceId>,
    ) -> Result<()> {
        let (vector, modified_by, modified_at) = match stamp {
            Stamp::Local => {
                let (v, d) = self.db.next_local_vector(logical_path)?;
                (v, d, db::now())
            }
            Stamp::Remote(version) => {
                (version.vector.clone(), version.modified_by, version.modified_at)
            }
        };

        let conflict = match scope {
            None => "ON CONFLICT (path) WHERE scope IS NULL DO UPDATE SET",
            Some(_) => "ON CONFLICT (scope, path) WHERE scope IS NOT NULL DO UPDATE SET",
        };

        self.db.conn().execute(
            &format!(
            "INSERT INTO files
                 (path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
                  vector, modified_by, scope)
             VALUES (?1, 0, zeroblob(32), 0, unixepoch(), ?2, ?2, ?3, ?4, ?5)
             {conflict}
                 deleted_at = excluded.updated_at,
                 updated_at = excluded.updated_at,
                 vector = excluded.vector,
                 modified_by = excluded.modified_by"),
            rusqlite::params![
                logical_path,
                modified_at,
                vector.encode(),
                modified_by.as_bytes().as_slice(),
                scope.map(|d| d.as_bytes().to_vec()),
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
        // Checked before anything is written, so the caller's `None` really does
        // mean nothing happened.
        let Some(chunks) = self.readable_chunks(content)? else {
            return Ok(None);
        };

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

    /// Whether [`read_content_into`](Self::read_content_into) would produce
    /// `content` from this device alone.
    ///
    /// Asked before a destination is touched, by a caller that would otherwise
    /// have to open it to find out -- and opening it fresh would wipe what an
    /// interrupted transfer left there to be resumed.
    pub fn can_read_content(&self, content: &blake3::Hash) -> Result<bool> {
        Ok(self.readable_chunks(content)?.is_some())
    }

    /// The chunks of `content`, if this device holds every one of them by its
    /// index: in the chunk store, or in a live file in the folder. What a peer
    /// asking for the content is told.
    ///
    /// [`chunk_hashes_for_content`](Self::chunk_hashes_for_content) answers
    /// for content merely known, and a file freed here is known: its chunk
    /// list stays, its bytes do not. Answering a peer with that list sent it
    /// off to fail at the first chunk, and a phone did, on every sync, for 18
    /// files the laptop had freed and nobody had any more (2026-10-07).
    ///
    /// Not [`can_read_content`](Self::can_read_content), which reads the
    /// folder's file to be sure and would read a whole video to answer one
    /// request. A file changed under the index fails at the chunk, as it
    /// always has, until the next scan.
    pub fn held_chunks(&self, content: &blake3::Hash) -> Result<Option<Vec<blake3::Hash>>> {
        let Some(chunks) = self.chunk_hashes_for_content(content)? else {
            return Ok(None);
        };
        for chunk in &chunks {
            let held = self.cas.contains(chunk)
                || (self.tree.is_some() && self.db.chunk_in_folder(chunk)?)
                || self.db.chunk_in_send(chunk)?;
            if !held {
                return Ok(None);
            }
        }
        Ok(Some(chunks))
    }

    /// The chunks of `content`, if every one of them can be read here.
    fn readable_chunks(&self, content: &blake3::Hash) -> Result<Option<Vec<blake3::Hash>>> {
        let Some(chunks) = self.chunk_hashes_for_content(content)? else {
            return Ok(None);
        };
        for chunk in &chunks {
            if !self.has_chunk(chunk)? {
                return Ok(None);
            }
        }
        Ok(Some(chunks))
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
        Ok(matches!(self.chunk_from_disk(hash), Ok(Some(_))))
    }

    /// Send a file to another device, into its private vault, keeping no copy
    /// (decision 0060). The recipient learns of it from the tree, which shows
    /// a device its own vault, and collects it whenever it next appears.
    ///
    /// The file is read from `source` when it does. Until 2026-10-10 a send
    /// kept its own copy as chunks, so that the sender changing or deleting
    /// the file could not touch it; the owner chose instead that qurb carries
    /// files and does not keep them. A file changed or deleted before the
    /// recipient collects it is not sent, and the history says why
    /// ([`check_sends`](Self::check_sends)).
    ///
    /// The path is the one the *recipient* will see, so it is theirs to
    /// organise. Two recipients may be sent the same name without collision.
    pub fn send_to_vault(
        &mut self,
        logical_path: &str,
        source: &Path,
        recipient: &DeviceId,
    ) -> Result<PutStats> {
        let absolute = std::path::absolute(source).map_err(|e| Error::io(source, e))?;
        self.send_from(logical_path, &absolute.to_string_lossy(), false, recipient)
    }

    /// Send what `source` holds to `recipient`, keeping no copy (decision
    /// 0060): the file is read here once, to describe it, and its chunks are
    /// read from it again when the recipient collects them.
    ///
    /// `source` is an absolute path, or a document the platform lends (see
    /// [`Documents`]). `temporary` marks a copy qurb made for the purpose,
    /// deleted once collected. Changed or deleted before then, the file is
    /// not sent: [`check_sends`](Self::check_sends) calls the send off and says
    /// why.
    pub fn send_from(
        &mut self,
        logical_path: &str,
        source: &str,
        temporary: bool,
        recipient: &DeviceId,
    ) -> Result<PutStats> {
        let at = Path::new(source);
        let file = self.open_source(source).map_err(|e| Error::io(at, e))?;
        let meta = file.metadata().map_err(|e| Error::io(at, e))?;
        let mtime_ns = mtime_from(&meta);
        let manifest = chunker::chunk_reader(std::io::BufReader::with_capacity(1 << 20, file))
            .map_err(|e| Error::io(at, e))?;

        // A send, every time: the same file sent again -- the person asked
        // first, by whatever sends it -- is a new version the recipient takes,
        // and not collected until it has (decision 0059).
        self.db.sending_again(&manifest.file_hash, recipient)?;
        let stats = self.put_manifest(
            logical_path,
            &manifest,
            &[],
            mtime_ns,
            Placement::local().in_vault(Some(recipient)).fresh().by_reference(),
        )?;
        let row = self
            .db
            .live_row_in(logical_path, Some(recipient))?
            .ok_or_else(|| Error::NotFound { path: logical_path.to_string() })?;
        self.db.note_send_source(row.id, source, manifest.size, mtime_ns, temporary)?;
        let _ = self.db.record(
            db::Event::Sent,
            Some(logical_path),
            Some(manifest.size),
            Some(recipient),
            None,
        );
        Ok(stats)
    }

    /// Call off every waiting send whose file changed or went (decision
    /// 0060), and say why in the history, which is where both apps' "something
    /// failed" notification comes from.
    ///
    /// Checked by size and modification time; a file whose time moved and
    /// whose size did not is read again, and only a change in its bytes calls
    /// the send off. A document the platform lends has no time worth trusting,
    /// so only its size is compared here; one changed without changing size
    /// is caught as it is served, and called off at the next check.
    pub fn check_sends(&mut self) -> Result<Vec<CalledOff>> {
        let mut called_off = Vec::new();
        for send in self.db.send_sources()?.into_iter().filter(|s| !s.done) {
            let why = match self.open_source(&send.source).and_then(|f| f.metadata()) {
                Err(_) => Some("it was deleted before it was collected"),
                Ok(meta) if meta.len() != send.size => Some("it changed after it was sent"),
                Ok(_) if send.source.starts_with("content://") => None,
                Ok(meta) if mtime_from(&meta) == send.mtime_ns => None,
                Ok(meta) => match self.open_source(&send.source).map(hash_reader) {
                    Ok(Ok(hash)) if hash == send.content => {
                        self.db.send_source_seen(send.file_id, mtime_from(&meta))?;
                        None
                    }
                    _ => Some("it changed after it was sent"),
                },
            };
            let Some(why) = why else { continue };
            self.tombstone(&send.path, Stamp::Local, Some(send.to))?;
            let _ = self.db.record(
                db::Event::Failed,
                Some(&send.path),
                Some(send.size),
                Some(&send.to),
                Some(&format!("not sent: {why}")),
            );
            self.let_go_of_source(&send)?;
            called_off.push(CalledOff { path: send.path, to: send.to, why: why.to_string() });
        }
        Ok(called_off)
    }

    /// Stop reading the files of sends that have been collected or taken
    /// back: delete the copies qurb made, give back what the platform lent,
    /// and drop the references to chunks nothing here holds (decision 0060).
    /// Returns how many.
    pub fn tidy_sends(&mut self) -> Result<usize> {
        let done: Vec<db::SendSource> =
            self.db.send_sources()?.into_iter().filter(|s| s.done).collect();
        for send in &done {
            self.let_go_of_source(send)?;
        }
        Ok(done.len())
    }

    fn let_go_of_source(&mut self, send: &db::SendSource) -> Result<()> {
        if send.temporary {
            let _ = std::fs::remove_file(&send.source);
        }
        if send.source.starts_with("content://") {
            if let Some(documents) = &self.documents {
                documents.release(&send.source);
            }
        }
        self.db.forget_send_source(send.file_id)?;
        self.release_unbacked(send.file_id)
    }

    /// Keep another device's own file for it (decision 0036).
    ///
    /// `source` holds the bytes, somewhere outside the folder: this device
    /// never shows what it holds, so the content goes into the chunk store and
    /// nowhere else. The row sits in the owner's vault, marked held, and is
    /// never released -- that is the whole difference from a send.
    pub fn hold_file(
        &mut self,
        version: &FileVersion,
        owner: &DeviceId,
        source: &Path,
    ) -> Result<PutStats> {
        let file = std::fs::File::open(source).map_err(|e| Error::io(source, e))?;
        let meta = file.metadata().map_err(|e| Error::io(source, e))?;
        // SAFETY: as for every other write -- mapped once, chunked and stored
        // from the map. An empty file cannot be mapped and has nothing to map.
        let mapped = match meta.len() {
            0 => None,
            _ => Some(unsafe { memmap2::Mmap::map(&file) }.map_err(|e| Error::io(source, e))?),
        };
        let data: &[u8] = mapped.as_deref().unwrap_or(&[]);
        let manifest = chunker::chunk_bytes(data);
        self.put_manifest(
            &version.path,
            &manifest,
            data,
            0,
            Placement::remote(version).in_vault(Some(owner)).held(),
        )
    }

    /// The owner deleted a file this device holds for it.
    ///
    /// Only ever on the owner's say-so, carried as a tombstone. A file simply
    /// absent from the owner's list is left alone: a wiped phone must not
    /// delete its own backup. See decision 0036.
    pub fn unhold(&mut self, version: &FileVersion, owner: &DeviceId) -> Result<()> {
        if self.db.live_row_in(&version.path, Some(owner))?.is_none() {
            return Ok(());
        }
        self.tombstone(&version.path, Stamp::Remote(version), Some(*owner))
    }

    /// Take back a send the other device has not collected yet.
    ///
    /// The entry becomes a tombstone in that device's vault. A recipient never
    /// takes a tombstone as a delivery, so nothing arrives however long it has
    /// been switched off. The bytes held for it are released like any other
    /// deleted file's, after the retention window rather than at once.
    ///
    /// Refused once collected: the file is the other device's by then, and
    /// [decision 0030](../../docs/decisions/0030-sending-a-file-to-one-device.md)
    /// is that a send cannot be withdrawn from somebody who has it. A device in
    /// the middle of collecting when this runs may still finish.
    pub fn cancel_send(&mut self, logical_path: &str, recipient: &DeviceId) -> Result<()> {
        let Some(row) = self.db.live_row_in(logical_path, Some(recipient))? else {
            return Err(Error::NotFound { path: logical_path.to_string() });
        };
        if self.db.device_holds(&row.content_hash, recipient)? {
            return Err(Error::AlreadyCollected { path: logical_path.to_string() });
        }
        self.tombstone(logical_path, Stamp::Local, Some(*recipient))?;
        let _ = self.db.record(
            db::Event::Cancelled,
            Some(logical_path),
            Some(row.size),
            Some(recipient),
            None,
        );
        Ok(())
    }

    fn trash_file(&self, id: i64) -> PathBuf {
        self.root().join("trash").join(id.to_string())
    }

    /// Take a file out of the folder into the trash, instead of deleting it.
    ///
    /// `disk` is where the file is now; `entry` describes it for the list. Its
    /// size is read from the file, which is what the trash will cost. Returns
    /// the entry's id, or `None` when there was no regular file there to keep.
    pub fn move_to_trash(&mut self, disk: &Path, entry: &db::NewTrash<'_>) -> Result<Option<i64>> {
        let meta = match std::fs::symlink_metadata(disk) {
            Ok(meta) if meta.is_file() => meta,
            Ok(_) => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(disk, e)),
        };
        let dir = self.root().join("trash");
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let id = self.db.add_trash(&db::NewTrash { size: meta.len(), ..entry.clone() })?;
        let into = self.trash_file(id);
        if let Err(e) = move_file(disk, &into) {
            self.db.remove_trash(id)?;
            return Err(Error::io(disk, e));
        }
        Ok(Some(id))
    }

    /// Delete a file in the folder, as a change made here, keeping its bytes
    /// in Recently deleted: the deletion reaches every other device as usual,
    /// and this device can still put it back. `why` is for when "deleted" is
    /// not the whole story. Returns the trash entry, if there was a file on
    /// disk to keep.
    pub fn delete_to_trash(&mut self, logical_path: &str, why: Option<&str>) -> Result<Option<i64>> {
        let Some((row, scope)) = self.db.folder_row(logical_path)? else {
            return Err(Error::NotFound { path: logical_path.to_string() });
        };
        let me = self.device_id()?;
        let trashed = match self.tree.clone() {
            Some(root) => self.move_to_trash(
                &root.join(logical_path),
                &db::NewTrash {
                    path: logical_path,
                    scope: scope.as_ref(),
                    content: &row.content_hash,
                    size: 0,
                    by: Some(&me),
                    why,
                },
            )?,
            None => None,
        };
        self.delete_file(logical_path)?;
        Ok(trashed)
    }

    /// Rebuild the share tables from the rule files, if those changed since
    /// they were last built (decision 0044).
    ///
    /// A rule file that cannot be read is taken as a folder shared with
    /// nobody: closed, not open. Its bytes not being here yet is not a reason
    /// to show the folder to everyone.
    pub fn refresh_shares(&self) -> Result<()> {
        let stamp = self.db.sharing_stamp()?;
        if self.db.shares_stamp()?.as_deref() == Some(stamp.as_str()) {
            return Ok(());
        }
        let mut rules = std::collections::BTreeMap::new();
        for path in self.db.live_rule_paths()? {
            let Some(folder) = qurb_sync::sharing::rule_folder(&path) else { continue };
            let members = match self.read_file(&path) {
                Ok(bytes) => qurb_sync::sharing::parse_members(&String::from_utf8_lossy(&bytes)),
                Err(e) => {
                    tracing::warn!(path, error = %e, "a sharing rule cannot be read; its folder is closed");
                    Default::default()
                }
            };
            rules.insert(folder, members);
        }
        let rules = qurb_sync::sharing::Rules::new(rules);
        // Nested rules are refused when made here; one that arrives nested
        // anyway (made by hand, or on two devices at once) is kept -- both
        // folders closed as their rules say -- and said.
        for (folder, _) in rules.iter() {
            if let Some(other) = rules.nests(folder) {
                tracing::warn!(folder, other, "two sharing rules nest; both apply");
            }
        }
        self.db.replace_shares(&rules, &stamp)
    }

    /// The sharing rules in force: which folders are shared with which
    /// devices. A folder with no rule is shared with every device.
    pub fn sharing(&self) -> Result<qurb_sync::sharing::Rules> {
        self.refresh_shares()?;
        self.db.shares()
    }

    /// Every folder a person might share, and who it is shared with: the
    /// folders at the top of the shared area, and any folder that has a rule
    /// wherever it is. `None` is every device.
    pub fn folder_sharing(
        &self,
    ) -> Result<Vec<(String, Option<std::collections::BTreeSet<DeviceId>>)>> {
        let rules = self.sharing()?;
        let mut folders: std::collections::BTreeMap<String, Option<_>> =
            self.db.top_folders()?.into_iter().map(|f| (f, None)).collect();
        for (folder, members) in rules.iter() {
            folders.insert(folder.clone(), Some(members.clone()));
        }
        Ok(folders.into_iter().collect())
    }

    /// Share `folder` with exactly `members`, from now on, on every device.
    ///
    /// Written as a rule file that syncs like any other. Refused when it would
    /// nest inside or around another shared folder, when it names nobody, and
    /// when this device is not one the folder is shared with now -- the other
    /// devices would refuse the change, so it is refused here first.
    pub fn set_sharing(
        &mut self,
        folder: &str,
        members: &std::collections::BTreeSet<DeviceId>,
    ) -> Result<()> {
        let folder = folder.trim_matches('/');
        let rules = self.sharing()?;
        let me = self.device_id()?;
        if !qurb_sync::sharing::valid_folder(folder) {
            return Err(Error::Sharing { why: format!("{folder:?} cannot be shared on its own") });
        }
        if let Some(other) = rules.nests(folder) {
            return Err(Error::Sharing {
                why: format!("{other} is already shared on its own, and one shared folder cannot hold another"),
            });
        }
        if members.is_empty() {
            return Err(Error::Sharing { why: "a folder has to be shared with at least one device".into() });
        }
        if !rules.may_change(folder, &me) {
            return Err(Error::Sharing {
                why: format!("{folder} is not shared with this device, so this device cannot change who it is shared with"),
            });
        }
        self.write_rule(folder, Some(members))
    }

    /// Share `folder` with every device again.
    pub fn clear_sharing(&mut self, folder: &str) -> Result<()> {
        let folder = folder.trim_matches('/');
        let me = self.device_id()?;
        if !self.sharing()?.may_change(folder, &me) {
            return Err(Error::Sharing {
                why: format!("{folder} is not shared with this device, so this device cannot change who it is shared with"),
            });
        }
        self.write_rule(folder, None)
    }

    fn write_rule(
        &mut self,
        folder: &str,
        members: Option<&std::collections::BTreeSet<DeviceId>>,
    ) -> Result<()> {
        let Some(root) = self.tree.clone() else {
            return Err(Error::Sharing { why: "a device with no folder has no rules to change".into() });
        };
        let path = qurb_sync::sharing::rule_path(folder);
        let disk = root.join(&path);
        match members {
            Some(members) => {
                let dir = root.join(qurb_sync::sharing::SHARING_DIR);
                std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
                std::fs::write(&disk, qurb_sync::sharing::encode_members(members))
                    .map_err(|e| Error::io(&disk, e))?;
                // Always the shared area: a rule every device must see, even
                // made on a phone that files new things privately.
                self.put_file_in(&path, &disk, None)?;
            }
            None => {
                if self.db.folder_row(&path)?.is_some() {
                    match std::fs::remove_file(&disk) {
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                            return Err(Error::io(&disk, e))
                        }
                        _ => {}
                    }
                    self.delete_file(&path)?;
                }
            }
        }
        self.refresh_shares()
    }

    /// Every conflict waiting for somebody to say which version they want.
    ///
    /// Found by name -- a conflict copy's name says what it is a version of
    /// (see [`qurb_sync::conflict_origin`]) -- because only the device that
    /// noticed a conflict records it; every other device just receives the
    /// copy, and has to recognise it all the same.
    pub fn conflicts(&self) -> Result<Vec<Conflict>> {
        let mut found = Vec::new();
        for path in self.db.folder_paths_containing(".conflict-")? {
            let Some(name) = qurb_sync::conflict_origin(&path) else { continue };
            let Some(copy) = self.conflict_version(&path)? else { continue };
            found.push(Conflict {
                original: self.conflict_version(&name.original)?,
                original_path: name.original,
                copy,
                copy_device: name.device,
            });
        }
        Ok(found)
    }

    fn conflict_version(&self, path: &str) -> Result<Option<ConflictVersion>> {
        let Some((row, _)) = self.db.folder_row(path)? else { return Ok(None) };
        Ok(Some(ConflictVersion {
            path: row.path,
            size: row.size,
            here: self.db.is_materialised(path)?.unwrap_or(false),
            modified_by: row.modified_by,
            updated_at: row.updated_at,
        }))
    }

    /// Settle a conflict the way somebody chose: keep the version under the
    /// file's own name, keep the other one in its place, or keep both under
    /// names a person can read. `label` names the other version when both are
    /// kept -- usually the device it came from.
    ///
    /// Every choice is an ordinary change here, so it reaches the other devices
    /// the way any change does, and the choice is made once for all of them.
    /// Nothing is lost by choosing: the version not kept goes to Recently
    /// deleted (decision 0042). Returns the path of what was kept.
    pub fn settle_conflict(&mut self, copy: &str, keep: Keep, label: &str) -> Result<String> {
        let name = qurb_sync::conflict_origin(copy)
            .ok_or_else(|| Error::NotAConflict { path: copy.to_string() })?;
        let Some(root) = self.tree.clone() else {
            return Err(Error::NotFound { path: copy.to_string() });
        };
        let Some((_, copy_scope)) = self.db.folder_row(copy)? else {
            return Err(Error::NotFound { path: copy.to_string() });
        };
        let not_kept = "the version not kept when a conflict was settled";
        let needs_copy_here = |store: &Self| -> Result<()> {
            match store.db.is_materialised(copy)? == Some(true) && root.join(copy).is_file() {
                true => Ok(()),
                false => Err(Error::NotHere { path: copy.to_string() }),
            }
        };

        match keep {
            Keep::Original => {
                self.delete_to_trash(copy, Some(not_kept))?;
                Ok(name.original)
            }
            Keep::Copy => {
                needs_copy_here(self)?;
                // Into the area the name was in -- or the copy's, if the
                // name has gone.
                let area = match self.db.folder_row(&name.original)? {
                    Some((_, scope)) => {
                        self.delete_to_trash(&name.original, Some(not_kept))?;
                        scope
                    }
                    None => copy_scope,
                };
                self.rename_in_folder(&root, copy, &name.original, area)?;
                Ok(name.original)
            }
            Keep::Both => {
                needs_copy_here(self)?;
                let target = self.free_path_labelled(&root, &name.original, label)?;
                self.rename_in_folder(&root, copy, &target, copy_scope)?;
                Ok(target)
            }
        }
    }

    /// Rename or move a file in the folder, as a change made here: the new
    /// path appears and the old one is deleted on every device, and the file
    /// stays in the area it was in -- a shared file does not become private
    /// by moving on a phone that files new things privately.
    ///
    /// Needs the file's bytes here: a file known only elsewhere has nothing to
    /// move. Refuses a destination that is taken, in the index or on disk.
    pub fn rename_file(&mut self, from: &str, to: &str) -> Result<()> {
        let to = to.trim_matches('/');
        let Some(root) = self.tree.clone() else {
            return Err(Error::NotFound { path: from.to_string() });
        };
        let Some((_, scope)) = self.db.folder_row(from)? else {
            return Err(Error::NotFound { path: from.to_string() });
        };
        if !qurb_sync::is_safe_path(to) || qurb_sync::sharing::is_rule_path(to) || to.is_empty() {
            return Err(Error::Sharing { why: format!("{to:?} is not a name a file can have here") });
        }
        if self.db.folder_row(to)?.is_some() || root.join(to).exists() {
            return Err(Error::Sharing { why: format!("something is already called {to}") });
        }
        if self.db.is_materialised(from)? != Some(true) || !root.join(from).is_file() {
            return Err(Error::NotHere { path: from.to_string() });
        }
        self.rename_in_folder(&root, from, to, scope)
    }

    /// Move a file between the shared area and this device's own Private
    /// Vault (decision 0057): the same file, at the same path, in the other
    /// area. Returns whether it moved; a file already there does not.
    ///
    /// Into the vault, the shared row is retired with a tombstone, so every
    /// other device removes its copy at its next sync, as with any deletion,
    /// and only a device keeping this one's vault holds it from then on. Out
    /// of it, the private row is retired the same way, which tells a device
    /// keeping the vault to let it go, and the file becomes an ordinary shared
    /// one that reaches every device.
    ///
    /// Refused for a file whose bytes are not here: the move writes the file
    /// into the other area from the folder, and a freed file has nothing
    /// there. And for a path live in both areas, which is ambiguous.
    pub fn move_area(&mut self, logical_path: &str, private: bool) -> Result<bool> {
        let Some(root) = self.tree.clone() else {
            return Err(Error::NotFound { path: logical_path.to_string() });
        };
        let Some((_, scope)) = self.db.folder_row(logical_path)? else {
            return Err(Error::NotFound { path: logical_path.to_string() });
        };
        let me = self.device_id()?;
        let target = private.then_some(me);
        if scope == target {
            return Ok(false);
        }
        if scope.is_some_and(|owner| owner != me) {
            return Err(Error::Sharing { why: "that file is another device's, kept here for it".into() });
        }
        // The shared row is the one found when both are live, so that is the
        // only way the path can be in both.
        if scope.is_none() && self.db.own_vault_row(logical_path)?.is_some() {
            return Err(Error::Sharing {
                why: format!("Private Vault already has a file called {logical_path}"),
            });
        }
        let disk = root.join(logical_path);
        if self.db.is_materialised(logical_path)? != Some(true) || !disk.is_file() {
            return Err(Error::NotHere { path: logical_path.to_string() });
        }
        self.put_file_in(logical_path, &disk, target)?;
        let _ = self.db.record(
            db::Event::Moved,
            Some(logical_path),
            None,
            None,
            Some(if private { "into Private Vault" } else { "out of Private Vault, to every device" }),
        );
        Ok(true)
    }

    /// Move a file in the folder from one path to another, recording both
    /// sides as changes made here, and keeping it in `scope`'s area.
    fn rename_in_folder(
        &mut self,
        root: &Path,
        from: &str,
        to: &str,
        scope: Option<DeviceId>,
    ) -> Result<()> {
        let (source, target) = (root.join(from), root.join(to));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::rename(&source, &target).map_err(|e| Error::io(&source, e))?;
        self.put_file_in(to, &target, scope)?;
        self.delete_file(from)
    }

    /// Store a file from the folder as a change made here, in the area given:
    /// the shared area for `None`, this device's own vault for its own id.
    ///
    /// For qurb writing a file itself -- a sharing rule, a restore, a rename,
    /// a settled conflict -- where the file's area is already decided. A plain
    /// [`put_file`](Self::put_file) of a path the index has no live row for
    /// files it the way a *new* file goes, which on a phone is its own vault:
    /// a shared file restored or renamed there became private, and so to
    /// every other device looked deleted. Decided before the write, because
    /// moving the row afterwards collides with a shared tombstone at the path.
    fn put_file_in(&mut self, logical: &str, disk: &Path, scope: Option<DeviceId>) -> Result<PutStats> {
        let private = match scope {
            None => false,
            Some(owner) if owner == self.device_id()? => true,
            Some(_) => {
                return Err(Error::Sharing {
                    why: "another device's vault is not written through this device's folder".into(),
                })
            }
        };
        // A live row at the path in the other area is retired first: an
        // existing file keeps its area, so without this the write would land
        // beside it in the wrong one. Found on the emulator, where a sharing
        // rule written privately by an earlier build stayed private.
        if let Some((_, current)) = self.db.folder_row(logical)? {
            if current != scope {
                self.tombstone(logical, Stamp::Local, current)?;
            }
        }
        let was = std::mem::replace(&mut self.new_files_private, private);
        let stored = self.put_file(logical, disk);
        self.new_files_private = was;
        stored
    }

    /// `name (label).ext` beside `path`, numbered if that is taken too.
    fn free_path_labelled(&self, root: &Path, path: &str, label: &str) -> Result<String> {
        let (dir, file) = match path.rfind('/') {
            Some(i) => (&path[..=i], &path[i + 1..]),
            None => ("", path),
        };
        let split = file.get(1..).and_then(|rest| rest.rfind('.')).map(|i| i + 1);
        let (stem, ext) = match split {
            Some(i) => (&file[..i], &file[i..]),
            None => (file, ""),
        };
        // A device name is chosen by that device: nothing in it may make a
        // path of it.
        let label: String =
            label.chars().filter(|c| !matches!(c, '/' | '\\' | '\0') && !c.is_control()).collect();
        let label = if label.trim().is_empty() { "other version".to_string() } else { label };
        for n in 1.. {
            let candidate = match n {
                1 => format!("{dir}{stem} ({label}){ext}"),
                n => format!("{dir}{stem} ({label} {n}){ext}"),
            };
            if self.db.folder_row(&candidate)?.is_none() && !root.join(&candidate).exists() {
                return Ok(candidate);
            }
        }
        unreachable!("an unbounded search ends")
    }

    /// Record a version of a shared file without its bytes: listed, known to
    /// be on the device that made it, and fetched when somebody asks. What a
    /// folder kept only remotely does with a file that arrives (decision
    /// 0045) -- the same state freeing a file leaves behind.
    pub fn know_elsewhere(&mut self, version: &FileVersion) -> Result<()> {
        let Content::File { hash, size } = &version.content else {
            self.adopt(version, None, 0)?;
            return Ok(());
        };
        let tx = self.db.conn().unchecked_transaction()?;
        tx.execute(
            "INSERT INTO files
                 (path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
                  vector, modified_by, scope, materialised)
             VALUES (?1, ?2, ?3, ?7, unixepoch(), ?4, NULL, ?5, ?6, NULL, 0)
             ON CONFLICT (path) WHERE scope IS NULL DO UPDATE SET
                 size = excluded.size,
                 content_hash = excluded.content_hash,
                 mtime_ns = excluded.mtime_ns,
                 updated_at = excluded.updated_at,
                 deleted_at = NULL,
                 vector = excluded.vector,
                 modified_by = excluded.modified_by,
                 materialised = 0",
            rusqlite::params![
                version.path,
                *size as i64,
                hash.as_slice(),
                version.modified_at,
                version.vector.encode(),
                version.modified_by.as_bytes().as_slice(),
                // When the version was made: there is no file here to have a
                // time of its own. Written as 0 before 2026-10-07, which a
                // phone showed as "20734 days ago".
                version.modified_at.saturating_mul(1_000_000_000),
            ],
        )?;
        // The chunks of whatever content the row described before, which
        // describe nothing now; the new content's list comes with its bytes.
        tx.execute(
            "DELETE FROM file_chunks
              WHERE file_id = (SELECT id FROM files WHERE path = ?1 AND scope IS NULL)",
            rusqlite::params![version.path],
        )?;
        tx.commit()?;
        self.db.note_replica(&blake3::Hash::from(*hash), &version.modified_by)
    }

    /// Keep `folder` on this device only remotely: its files stay listed, the
    /// local copies go where another device keeps them, and what changes on
    /// other devices is no longer downloaded here until asked for.
    ///
    /// A file this device holds the only copy of stays: freeing it would be
    /// deleting it (brief §29 -- "refuse it and explain why"). Returns how many
    /// were freed, the bytes, and the paths kept for that reason.
    pub fn keep_remotely(&mut self, folder: &str) -> Result<(usize, u64, Vec<String>)> {
        let folder = folder.trim_matches('/');
        if folder.is_empty() || qurb_sync::sharing::is_rule_path(folder) {
            return Err(Error::Sharing { why: format!("{folder:?} cannot be kept remotely on its own") });
        }
        self.db.set_kept_remotely(folder, true)?;
        let (mut freed, mut bytes, mut kept) = (0, 0, Vec::new());
        for (path, here) in self.db.shared_under(folder)? {
            if !here {
                continue;
            }
            match self.free_local(&path) {
                Ok(n) => {
                    freed += 1;
                    bytes += n;
                }
                Err(Error::CannotEvict { .. }) => kept.push(path),
                Err(e) => return Err(e),
            }
        }
        Ok((freed, bytes, kept))
    }

    /// Keep `folder` on this device again: everything in it is asked for, and
    /// arrives at the next sync. Returns how many files were asked for.
    pub fn keep_locally(&mut self, folder: &str) -> Result<usize> {
        let folder = folder.trim_matches('/');
        self.db.set_kept_remotely(folder, false)?;
        let mut asked = 0;
        for (path, here) in self.db.shared_under(folder)? {
            if !here && self.db.want(&path)? {
                asked += 1;
            }
        }
        Ok(asked)
    }

    /// What is in the trash, most recently deleted first.
    ///
    /// Not the sharing rules an earlier build kept there when another device
    /// removed one: they are not anybody's files, and they leave with the
    /// rest when their time is up.
    pub fn recently_deleted(&self) -> Result<Vec<db::Trashed>> {
        let mut entries = self.db.trash()?;
        entries.retain(|e| !qurb_sync::sharing::is_rule_path(&e.path));
        Ok(entries)
    }

    /// Put a file from the trash back in the folder, as a change made here:
    /// a new version, which reaches every other device the way any change
    /// does, and so undoes the deletion everywhere and not only here.
    ///
    /// Returns where it went -- its old path, or beside it when that is taken
    /// now, because a restore must never overwrite something.
    pub fn restore_from_trash(&mut self, id: i64) -> Result<String> {
        let Some(root) = self.tree.clone() else {
            return Err(Error::NotFound { path: format!("trash/{id} (a replica has no folder)") });
        };
        let entry = self.db.trash_entry(id)?.ok_or_else(|| Error::NotFound { path: format!("trash/{id}") })?;
        if qurb_sync::sharing::is_rule_path(&entry.path) {
            return Err(Error::Sharing { why: "a sharing rule is not put back from Recently deleted".into() });
        }
        let target = self.free_path_near(&root, &entry.path)?;
        let to = root.join(&target);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        move_file(&self.trash_file(id), &to).map_err(|e| Error::io(&to, e))?;
        // Back in the area it was deleted from.
        let me = self.device_id()?;
        let area = entry.scope.filter(|owner| *owner == me);
        self.put_file_in(&target, &to, area)?;
        self.db.remove_trash(id)?;
        let _ = self.db.record(
            db::Event::Restored,
            Some(&target),
            Some(entry.size),
            None,
            Some("from Recently deleted"),
        );
        Ok(target)
    }

    /// Delete a file in the trash for good, now.
    pub fn forget_deleted(&mut self, id: i64) -> Result<()> {
        match std::fs::remove_file(self.trash_file(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(Error::io(self.trash_file(id), e))
            }
            _ => {}
        }
        self.db.remove_trash(id)?;
        Ok(())
    }

    /// Delete for good what has been in the trash longer than `retention`,
    /// or everything when `retention` is zero. Returns how many files and
    /// bytes went.
    pub fn empty_trash(&mut self, retention: std::time::Duration) -> Result<(usize, u64)> {
        let cutoff = db::now() - retention.as_secs() as i64;
        let old = match retention.is_zero() {
            true => self.db.trash()?,
            false => self.db.trash_before(cutoff)?,
        };
        let mut freed = (0, 0);
        for entry in old {
            self.forget_deleted(entry.id)?;
            freed.0 += 1;
            freed.1 += entry.size;
        }
        Ok(freed)
    }

    /// Free at least `bytes` from the trash, oldest first. Returns how much
    /// went, which is less when the trash holds less.
    pub fn empty_trash_by(&mut self, bytes: u64) -> Result<u64> {
        let mut entries = self.db.trash()?;
        entries.reverse();
        let mut freed = 0;
        for entry in entries {
            if freed >= bytes {
                break;
            }
            self.forget_deleted(entry.id)?;
            freed += entry.size;
        }
        Ok(freed)
    }

    /// `path` if nothing is there -- in the index or on disk -- or the first
    /// free `name (restored).ext`, `name (restored 2).ext` beside it.
    fn free_path_near(&self, root: &Path, path: &str) -> Result<String> {
        let taken = |candidate: &str| -> Result<bool> {
            Ok(self.db.folder_row(candidate)?.is_some() || root.join(candidate).exists())
        };
        if !taken(path)? {
            return Ok(path.to_string());
        }
        let (dir, name) = match path.rfind('/') {
            Some(i) => (&path[..=i], &path[i + 1..]),
            None => ("", path),
        };
        let split = name.get(1..).and_then(|rest| rest.rfind('.')).map(|i| i + 1);
        let (stem, ext) = match split {
            Some(i) => (&name[..i], &name[i..]),
            None => (name, ""),
        };
        for n in 1.. {
            let label = if n == 1 { "restored".to_string() } else { format!("restored {n}") };
            let candidate = format!("{dir}{stem} ({label}){ext}");
            if !taken(&candidate)? {
                return Ok(candidate);
            }
        }
        unreachable!("an unbounded search ends")
    }

    /// What removing `device` would do here, worked out before anything is
    /// done, so that the question put to a person can say it.
    pub fn removal_plan(&self, device: &DeviceId) -> Result<RemovalPlan> {
        let waiting: Vec<(String, u64)> = self
            .db
            .pending_deliveries()?
            .into_iter()
            .filter(|(_, _, to)| to == device)
            .map(|(path, size, _)| (path, size))
            .collect();
        let kept_for_it = match self.kept_scope(device)? {
            Some(scope) => self
                .db
                .vault_contents(&scope)?
                .into_iter()
                .filter(|(path, _, _)| scope != *device || !waiting.iter().any(|(w, _)| w == path))
                .map(|(path, size, _)| (path, size))
                .collect(),
            None => Vec::new(),
        };
        Ok(RemovalPlan {
            waiting,
            kept_for_it,
            only_there: self.db.only_kept_by(device)?,
            holds_ours: self.db.is_holder(device)?,
        })
    }

    /// Stop trusting `device`: it can no longer connect here or sync with
    /// this device.
    ///
    /// What it has, it keeps -- its key, and everything already on it. Nothing
    /// here reaches into another device, and removal does not pretend to.
    ///
    /// Here, sends it has not collected are cancelled, because nothing can
    /// collect them now. What this device keeps in its vault for it is kept
    /// unless `delete_kept` says otherwise: its own files may exist nowhere
    /// else, and dropping somebody's backup is a choice to be made, not a side
    /// effect. Either way the copies it was known to hold stop counting as
    /// copies (see [`Db::forget_peer`](crate::db::Db::forget_peer)).
    ///
    /// `name` goes into the history, which otherwise names a device by asking
    /// the trust table -- where this one no longer is.
    pub fn remove_device(
        &mut self,
        device: &DeviceId,
        name: &str,
        delete_kept: bool,
    ) -> Result<RemovalPlan> {
        let plan = self.removal_plan(device)?;
        for (path, _) in &plan.waiting {
            self.cancel_send(path, device)?;
        }
        if delete_kept {
            if let Some(scope) = self.kept_scope(device)? {
                for (path, _) in &plan.kept_for_it {
                    self.tombstone(path, Stamp::Local, Some(scope))?;
                }
            }
        }
        self.db.forget_peer(device)?;
        let _ = self.db.record(db::Event::Removed, None, None, Some(device), Some(name));
        Ok(plan)
    }

    /// Where what this device keeps for `device` is filed: its own vault, or
    /// for a guest its person's folder (decision 0060) -- `None` when another
    /// device of that person is still paired, whose folder it is too.
    fn kept_scope(&self, device: &DeviceId) -> Result<Option<DeviceId>> {
        match self.db.person_of(device)? {
            None => Ok(Some(*device)),
            Some(person) => {
                let shared = self.db.devices_of_person(&person)?.iter().any(|d| d != device);
                Ok((!shared).then_some(person))
            }
        }
    }

    /// What [`release_held_payloads`](Self::release_held_payloads) would free
    /// now, in bytes on disk.
    pub fn releasable_held_bytes(&self) -> Result<u64> {
        let mut total = 0;
        for hash in self.db.releasable_held_chunks()? {
            if self.cas.contains(&hash) {
                total += self.cas.stored_size(&hash).unwrap_or(0);
            }
        }
        Ok(total)
    }

    /// Drop payloads this device is holding only on somebody else's behalf.
    ///
    /// Vault content is kept after the recipient has taken it, so a send is
    /// never silently the only copy — but it is the first thing to go when
    /// disk runs short, before any of this device's own files. Somebody else's
    /// safety net should yield before your own work does.
    ///
    /// Only content another device is recorded as holding, and only chunks no
    /// other file still needs from the chunk store. A chunk shared with a file
    /// that has nowhere else to read it is left alone, however tempting its
    /// size — which is the same rule the storage cap follows and the reason
    /// this is safe to run unattended.
    ///
    /// Returns what it freed.
    pub fn release_held_payloads(&mut self) -> Result<GcStats> {
        let mut stats = GcStats::default();

        for hash in self.db.releasable_held_chunks()? {
            if !self.cas.contains(&hash) {
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

    /// Every path one device is entitled to know about.
    ///
    /// The shared area plus that device's own vault, and never anybody else's.
    /// See [`db::Audience`], which distinguishes the three cases this depends
    /// on getting right.
    pub fn tree_for(&self, audience: db::Audience<'_>) -> Result<Vec<FileVersion>> {
        // Sharing rules as the rule files say now, before answering anybody:
        // a rule that arrived since the last answer must already hold.
        self.refresh_shares()?;
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

    /// Every path this device knows about, vaults included, tombstones
    /// included. This device's own complete view.
    pub fn tree(&self) -> Result<Vec<FileVersion>> {
        self.db.all_versions()
    }

    /// The shared area only, all of it: this device's own side of a plan.
    ///
    /// Not what any peer is shown -- that is [`tree_for`](Self::tree_for),
    /// which leaves out folders shared without the peer. This device sees
    /// its own copy of those; planning without them would have it fetch them
    /// again on every sync, not knowing it had them.
    pub fn shared_tree(&self) -> Result<Vec<FileVersion>> {
        self.db.shared_versions()
    }

    /// This device's identity.
    /// Record what kind of device a peer is, and -- on a phone, the first time
    /// it learns of a computer -- let that computer keep this phone's own
    /// vault (decision 0053). Returns whether it was made a holder.
    ///
    /// Once only, and only when the phone has no holder: a person who chooses
    /// otherwise afterwards is not overruled. Before this, a phone's Private
    /// Vault had no copy anywhere unless somebody went and asked for one, and
    /// on 2026-10-05 a phone's cleared data took its vault with it.
    pub fn learn_kind(&self, device: &DeviceId, kind: &str) -> Result<bool> {
        self.db.set_peer_kind(device, kind)?;
        let phone = self.db.local_kind()?.as_deref() == Some("phone");
        // Never another person's computer by default: a guest's files go to a
        // computer it visits only when it chooses (decision 0060).
        let own = !self.db.relation_of(device)?.is_some_and(|r| r.is_other_person());
        if phone && own && kind == "computer" && !self.db.holders_defaulted()? && self.db.holders()?.is_empty() {
            self.db.add_holder(device)?;
            self.db.set_holders_defaulted()?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn device_id(&self) -> Result<DeviceId> {
        self.db.local_device()
    }

    // -- a vault kept, sealed, by another person's computer (decision 0060) --

    /// Who this device's person is to the computer with this fingerprint,
    /// visiting it as a guest: derived from this person's key for that
    /// computer alone, so one computer cannot link it to another, and every
    /// device holding the key says the same.
    pub fn person_for(&self, host: &[u8; 32]) -> [u8; 32] {
        self.key.derive(b"qurb/guest-person/v1", host)
    }

    /// Seal what `holder` has not been shown yet of this device's vault: each
    /// file whose bytes are here and whose current version has no sealed view
    /// for it. Stops once `budget` bytes have been read, the rest left for
    /// the next time it asks. Returns how many were sealed.
    pub fn prepare_sealed(&self, holder: &DeviceId, budget: u64) -> Result<usize> {
        let key = crate::sealed::FolderKey::for_host(&self.key, holder);
        let mut read = 0u64;
        let mut sealed = 0;
        for (row, _) in self.db.own_vault_rows()? {
            if row.deleted_at.is_some() || read >= budget {
                continue;
            }
            if self.db.sealed_view(holder, &row.path)?.is_some_and(|(content, _, _)| content == row.content_hash) {
                continue;
            }
            let chunks = self.db.chunks_with_sizes(row.id)?;
            // A file listed from a keeper and never fetched has no chunk list
            // here, and nothing to seal it from.
            if chunks.is_empty() && row.size > 0 {
                continue;
            }
            let meta = crate::sealed::Meta {
                path: row.path.clone(),
                size: row.size,
                content: row.content_hash,
                modified_at: row.updated_at,
                chunks: chunks.iter().map(|(_, size)| *size as u32).collect(),
            };
            if crate::sealed::seal_name(&key, &row.path).is_none() {
                continue;
            }
            let header = crate::sealed::seal_header(&key, &meta);
            let mut whole = blake3::Hasher::new();
            whole.update(&header);
            let mut size = header.len() as u64;
            let mut parts = vec![(*blake3::hash(&header).as_bytes(), db::SealedPart::Header(header))];
            let mut readable = true;
            for (chunk, _) in chunks {
                let Ok(plain) = self.read_chunk(&chunk) else {
                    readable = false;
                    break;
                };
                read += plain.len() as u64;
                let piece = crate::sealed::seal_chunk(&key, &plain);
                whole.update(&piece);
                size += piece.len() as u64;
                parts.push((*blake3::hash(&piece).as_bytes(), db::SealedPart::Chunk(chunk)));
            }
            // Its bytes are not here -- freed, and already kept somewhere --
            // so there is nothing to seal it from. The view made while they
            // were is the one that stands.
            if !readable {
                continue;
            }
            self.db.put_sealed_view(holder, &row.path, &row.content_hash, &whole.finalize(), size, &parts)?;
            sealed += 1;
        }
        Ok(sealed)
    }

    /// This device's vault as `holder` is shown it: sealed names, sealed
    /// files, each version as it is here (decision 0060). A file not sealed
    /// yet is left out until it is; a deletion is shown by the sealed name
    /// alone, which needs no view, so a file deleted here is deleted there.
    pub fn sealed_tree_for(&self, holder: &DeviceId) -> Result<Vec<FileVersion>> {
        let key = crate::sealed::FolderKey::for_host(&self.key, holder);
        let mut out = Vec::new();
        for (row, mut version) in self.db.own_vault_rows()? {
            let Some(name) = crate::sealed::seal_name(&key, &row.path) else { continue };
            version.path = name;
            version.area = qurb_sync::Area::Hold;
            if row.deleted_at.is_none() {
                match self.db.sealed_view(holder, &row.path)? {
                    Some((content, sealed, size)) if content == row.content_hash => {
                        version.content = Content::File { hash: *sealed.as_bytes(), size };
                    }
                    _ => continue,
                }
            }
            out.push(version);
        }
        Ok(out)
    }

    /// The chunk list of a sealed file shown to `holder`.
    pub fn sealed_manifest(&self, holder: &DeviceId, sealed: &blake3::Hash) -> Result<Option<Vec<[u8; 32]>>> {
        self.db.sealed_manifest(holder, sealed)
    }

    /// One sealed chunk shown to `holder`: the header as it was sealed, or a
    /// plain chunk read here and sealed again, the same way. `None` when it is
    /// not one, or its plain chunk cannot be read any more.
    pub fn sealed_chunk(&self, holder: &DeviceId, id: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        let chunk = match self.db.sealed_part(holder, id)? {
            None => return Ok(None),
            Some(db::SealedPart::Header(header)) => return Ok(Some(header)),
            Some(db::SealedPart::Chunk(chunk)) => chunk,
        };
        let Ok(plain) = self.read_chunk(&chunk) else { return Ok(None) };
        let key = crate::sealed::FolderKey::for_host(&self.key, holder);
        let piece = crate::sealed::seal_chunk(&key, &plain);
        Ok((blake3::hash(&piece).as_bytes() == id).then_some(piece))
    }

    /// Let go of this device's copy of each file of its vault that a computer
    /// of another person keeps (decision 0060). The file stays listed and is
    /// fetched back when opened. Refused, as any freeing is, unless the copy
    /// there counts as a safe one. Returns how many.
    pub fn free_kept_by_hosts(&mut self) -> Result<usize> {
        let mut freed = 0;
        for path in self.db.kept_by_hosts()? {
            if self.evict_because(&path, "kept on a computer this device visits; fetched when opened").is_ok() {
                freed += 1;
            }
        }
        Ok(freed)
    }

    /// Record a file of this device's vault that a computer of another person
    /// keeps and this device does not know of -- a phone set up again with
    /// the same key -- listed, kept there, and fetched when asked for
    /// (decision 0060). `version` is the file as its sealed header says: real
    /// path, size, content hash, time. Returns whether it was new.
    pub fn know_kept(&mut self, version: &FileVersion, holder: &DeviceId) -> Result<bool> {
        let Content::File { hash, size } = &version.content else { return Ok(false) };
        if self.db.own_vault_row(&version.path)?.is_some() {
            return Ok(false);
        }
        let me = self.db.local_device()?;
        self.db.conn().execute(
            "INSERT INTO files
                 (path, size, content_hash, mtime_ns, created_at, updated_at, deleted_at,
                  vector, modified_by, scope, materialised)
             VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5, NULL, ?6, ?7, ?8, 0)",
            rusqlite::params![
                version.path,
                *size as i64,
                hash.as_slice(),
                version.modified_at.saturating_mul(1_000_000_000),
                version.modified_at,
                version.vector.encode(),
                me.as_bytes().as_slice(),
                me.as_bytes().as_slice(),
            ],
        )?;
        self.db.note_replica(&blake3::Hash::from(*hash), holder)?;
        Ok(true)
    }

    /// Put back a file of this device's vault, fetched from a computer that
    /// kept it sealed and unsealed into `staging` (decision 0060). Checked
    /// against the content the index has for it before anything moves.
    pub fn restore_kept(&mut self, path: &str, staging: &Path) -> Result<()> {
        let Some(tree) = self.tree.clone() else {
            return Err(Error::NotFound { path: path.to_string() });
        };
        let Some(row) = self.db.own_vault_row(path)? else {
            return Err(Error::NotFound { path: path.to_string() });
        };
        let file = std::fs::File::open(staging).map_err(|e| Error::io(staging, e))?;
        if hash_reader(file).map_err(|e| Error::io(staging, e))? != row.content_hash {
            return Err(Error::ChunkCorrupt { hash: row.content_hash.to_hex().to_string() });
        }
        let to = tree.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        move_file(staging, &to).map_err(|e| Error::io(&to, e))?;
        // Recorded under the version it already had, so the computer keeping
        // it is not sent it again as a change; and written out in full, so a
        // file known only from that computer's list gains its chunk list.
        let me = self.db.local_device()?;
        let version = self
            .db
            .own_vault_rows()?
            .into_iter()
            .find(|(r, _)| r.path == path)
            .map(|(_, v)| v)
            .ok_or_else(|| Error::NotFound { path: path.to_string() })?;
        let file = std::fs::File::open(&to).map_err(|e| Error::io(&to, e))?;
        let meta = file.metadata().map_err(|e| Error::io(&to, e))?;
        // SAFETY: as for every other write -- mapped once, chunked and
        // recorded from the map. An empty file cannot be mapped.
        let mapped = match meta.len() {
            0 => None,
            _ => Some(unsafe { memmap2::Mmap::map(&file) }.map_err(|e| Error::io(&to, e))?),
        };
        let data: &[u8] = mapped.as_deref().unwrap_or(&[]);
        let manifest = chunker::chunk_bytes(data);
        self.put_manifest(
            path,
            &manifest,
            data,
            mtime_from(&meta),
            Placement::remote(&version).in_vault(Some(&me)).rewriting(),
        )?;
        self.db.note_kept_opened(path)?;
        let _ = self.db.record(db::Event::Restored, Some(path), Some(row.size), None, Some("from a computer that keeps it"));
        Ok(())
    }

    /// A guest's folder kept here, opened with the key its phone sent
    /// (decision 0060, step 5): each file's real path, size and time, read
    /// from its sealed header. Files that do not open with `key` are left out.
    pub fn open_kept(&self, person: &DeviceId, key: &crate::sealed::FolderKey) -> Result<Vec<KeptFile>> {
        let mut out = Vec::new();
        for (name, sealed, _) in self.db.kept_entries(person)? {
            let Some(path) = crate::sealed::open_name(key, &name) else { continue };
            let Some(chunks) = self.chunk_hashes_for_content(&sealed)? else { continue };
            let mut reader = crate::sealed::Unsealer::new(key.clone());
            for chunk in chunks {
                let Ok(piece) = self.read_chunk(&chunk) else { break };
                if reader.push(&piece).is_err() || reader.meta().is_some() {
                    break;
                }
            }
            if let Some(meta) = reader.meta() {
                if meta.path == path {
                    out.push(KeptFile { path, size: meta.size, modified_at: meta.modified_at, sealed });
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// The sealed file `path` of a guest's folder is kept as here, found by
    /// its sealed name: no header is read.
    pub fn kept_file(&self, person: &DeviceId, key: &crate::sealed::FolderKey, path: &str) -> Result<Option<blake3::Hash>> {
        let Some(name) = crate::sealed::seal_name(key, path) else { return Ok(None) };
        Ok(self.db.kept_entries(person)?.into_iter().find(|(n, _, _)| *n == name).map(|(_, sealed, _)| sealed))
    }

    /// Unseal the file `path` of a guest's folder kept here into `to`,
    /// checked against its header -- the path, and the content -- before it is
    /// called done (decision 0060, step 5).
    pub fn unseal_kept(
        &self,
        key: &crate::sealed::FolderKey,
        sealed: &blake3::Hash,
        path: &str,
        to: &Path,
    ) -> Result<()> {
        use std::io::Write;
        let chunks = self
            .chunk_hashes_for_content(sealed)?
            .ok_or_else(|| Error::NotFound { path: sealed.to_hex().to_string() })?;
        let mut reader = crate::sealed::Unsealer::new(key.clone());
        let mut out = std::fs::File::create(to).map_err(|e| Error::io(to, e))?;
        let mut whole = blake3::Hasher::new();
        for chunk in chunks {
            let piece = self.read_chunk(&chunk)?;
            let opened = reader
                .push(&piece)
                .map_err(|_| Error::ChunkCorrupt { hash: sealed.to_hex().to_string() })?;
            for plain in opened {
                whole.update(&plain);
                out.write_all(&plain).map_err(|e| Error::io(to, e))?;
            }
        }
        let expected = reader.meta().filter(|m| m.path == path).map(|m| m.content);
        if !reader.finished() || expected != Some(whole.finalize()) {
            let _ = std::fs::remove_file(to);
            return Err(Error::ChunkCorrupt { hash: sealed.to_hex().to_string() });
        }
        Ok(())
    }

    /// `holder` says it keeps a sealed file it was shown: record that it
    /// keeps the plain one, so this device can free its own copy and fetch it
    /// back from there (decision 0060). Returns whether it was one.
    pub fn note_kept_sealed(&self, holder: &DeviceId, sealed: &blake3::Hash) -> Result<bool> {
        match self.db.unsealed(holder, sealed)? {
            Some((_, content)) => {
                self.db.note_replica(&content, holder)?;
                Ok(true)
            }
            None => Ok(false),
        }
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
            Placement::remote(version).rewriting(),
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
            trash: self.db.trash_bytes()?,
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
        self.evict_because(
            logical_path,
            "dropped to stay under the storage limit; `qurb fetch` brings it back",
        )
    }

    /// The same, because the person asked: *Free local space* (brief §14).
    /// Refused on exactly the same terms.
    pub fn free_local(&mut self, logical_path: &str) -> Result<u64> {
        self.evict_because(logical_path, "local copy freed; another device keeps it")
    }

    fn evict_because(&mut self, logical_path: &str, why: &str) -> Result<u64> {
        let Some(tree) = self.tree.clone() else {
            return Err(Error::CannotEvict {
                path: logical_path.to_string(),
                why: "this device has no folder, so it is the only holder",
            });
        };

        // The shared area or this device's own vault: both are in the folder,
        // and on a phone that keeps its own files private the second is most
        // of what there is to free.
        let Some(row) = self.db.in_folder(logical_path)? else {
            return Err(Error::NotFound { path: logical_path.to_string() });
        };

        if self.db.safe_copies_elsewhere(&row.content_hash)? == 0 {
            return Err(Error::CannotEvict {
                path: logical_path.to_string(),
                why: match self.db.replica_count(&row.content_hash)? {
                    0 => "no other device is known to hold this content",
                    // Decision 0053: one tap in a phone's settings erases it.
                    _ => "only a phone holds another copy, and a phone's copy is not a safe one",
                },
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

        let _ = self.db.record(
            db::Event::Evicted,
            Some(logical_path),
            Some(freed),
            None,
            Some(why),
        );
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

    /// What would be gone if this device were wiped: see [`Db::only_here`](db::Db::only_here).
    pub fn only_here(&self) -> Result<Vec<(String, u64, bool)>> {
        self.db.only_here()
    }

    /// Note that another device has taken delivery of this content, into the
    /// shared area where this device could ask for it back.
    pub fn note_replica(&self, content: &blake3::Hash, device: &DeviceId) -> Result<()> {
        self.db.note_replica(content, device)
    }

    /// Note that another device has taken this content into its private vault.
    ///
    /// Enough to stop holding it for them; never enough to drop something of
    /// this device's own. See [`Db::note_replica_in_vault`](crate::db::Db::note_replica_in_vault).
    pub fn note_replica_in_vault(&self, content: &blake3::Hash, device: &DeviceId) -> Result<()> {
        self.db.note_replica_in_vault(content, device)
    }

    /// Devices with content waiting in their vault here. See
    /// [`Db::awaiting_collection`](crate::db::Db::awaiting_collection).
    pub fn awaiting_collection(&self) -> Result<Vec<DeviceId>> {
        self.db.awaiting_collection()
    }

    /// Files sent to another device that it has not collected yet. See
    /// [`Db::pending_deliveries`](crate::db::Db::pending_deliveries).
    pub fn pending_deliveries(&self) -> Result<Vec<(String, u64, DeviceId)>> {
        self.db.pending_deliveries()
    }

    /// Whether this device has already taken this send. See
    /// [`Db::delivery_taken`](crate::db::Db::delivery_taken).
    pub fn delivery_taken(&self, version: &FileVersion) -> Result<bool> {
        self.db.delivery_taken(version)
    }

    /// Record a send taken, wherever it was filed. See
    /// [`Db::note_taken`](crate::db::Db::note_taken).
    pub fn note_taken(&self, version: &FileVersion, filed_as: &str) -> Result<()> {
        self.db.note_taken(version, filed_as)
    }

    /// Which of `files` this device has sent to `device` before, with the name
    /// each went under and when (decision 0059): what to ask about before
    /// sending them again.
    ///
    /// Only a file the size of something already sent there is read, since a
    /// file of any other size cannot hold the same bytes. One that cannot be
    /// read is left out here; sending it says why.
    pub fn sent_before(&self, sources: &[String], device: &DeviceId) -> Result<Vec<SentBefore>> {
        let sizes = self.db.sizes_sent_to(device)?;
        if sizes.is_empty() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for source in sources {
            let Ok(file) = self.open_source(source) else { continue };
            let Ok(meta) = file.metadata() else { continue };
            if !meta.is_file() || !sizes.contains(&meta.len()) {
                continue;
            }
            let Ok(content) = hash_reader(file) else { continue };
            if let Some((sent_as, at)) = self.db.sent_before(&content, device)? {
                found.push(SentBefore { source: source.clone(), sent_as, at });
            }
        }
        Ok(found)
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
                // useless on exactly the devices people run it on. A send's
                // file supplies its chunks the same way (decision 0060).
                match self.chunk_from_disk(hash) {
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
/// Two versions of one file made on two devices without either seeing the
/// other, both kept (decision 0005). See [`Store::conflicts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The path the file has, and the version under it -- `None` if that has
    /// since been deleted or renamed.
    pub original_path: String,
    pub original: Option<ConflictVersion>,
    /// The other version, under its conflict name.
    pub copy: ConflictVersion,
    /// The short id of the device that made the other version, from its name.
    pub copy_device: String,
}

/// One side of a [`Conflict`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictVersion {
    pub path: String,
    pub size: u64,
    /// Whether its bytes are on this device.
    pub here: bool,
    /// The device that made this version, where known.
    pub modified_by: Option<DeviceId>,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Which version of a conflict to keep. See [`Store::settle_conflict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// The one under the file's own name.
    Original,
    /// The other one, which takes the file's name.
    Copy,
    /// Both, the other under a readable name.
    Both,
}

/// What removing a device does here. See [`Store::removal_plan`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemovalPlan {
    /// Sends it has not collected, with their sizes: cancelled.
    pub waiting: Vec<(String, u64)>,
    /// What this device keeps in its vault for it -- its own files, and sends
    /// it has collected: kept, unless removal is told to delete them.
    pub kept_for_it: Vec<(String, u64)>,
    /// Files freed here whose only other known copy is on it. Once it is
    /// removed they cannot be fetched back.
    pub only_there: Vec<String>,
    /// Whether it keeps this device's own vault.
    pub holds_ours: bool,
}

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
    /// Recently deleted files, kept for a while in case they are wanted back.
    pub trash: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.files + self.chunks + self.trash
    }
}

/// Move a file, across filesystems if it has to.
///
/// A rename where it can be -- instant, and atomic -- and a copy then a delete
/// where it cannot: on a phone the store directory and the folder need not be
/// on the same filesystem.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)
        }
        Err(e) => Err(e),
    }
}
