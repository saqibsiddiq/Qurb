//! A person's folder, mounted at `~/qurb/<person>` while it is open, so the
//! computer's own file manager shows it like any folder (the owner's review
//! of 2026-11-03, F-2: "maintaining a separate file system in the qurb
//! application is not necessary").
//!
//! Decision 0067's rule still holds: nothing readable is written to disk.
//! The owner's files are read from the store a chunk at a time as the file
//! manager or an app asks, as `stream.rs` plays a video; a guest's, sealed
//! whole, are unsealed on first read into memory-backed space
//! (`$XDG_RUNTIME_DIR`), which is what opening one did before. The moment
//! the folder locks -- *Lock*, ten minutes unused, the computer asleep -- it
//! is unmounted, and the placeholder is back.
//!
//! **Read-only.** Files are opened, copied out and previewed; nothing can be
//! put in or changed through the mount yet.
//!
//! Linux only, through the system's `fusermount3`. Elsewhere [`mount`] says
//! it cannot, and the window opens files one at a time as before.

use std::path::{Path, PathBuf};

/// What a mounted folder shows.
pub enum Contents {
    /// One of the owner's phones: its folder, as `qurb_storage::person`
    /// lists it, shown at the top -- ~/qurb holds a folder for each phone
    /// (the owner, 2026-11-10). This computer's own vault is not shown.
    Phone(qurb_storage::person::DeviceFolder),
    /// A guest's, sealed under their key.
    Guest,
}

#[cfg(target_os = "linux")]
pub use linux::{any, clear_stale, mount, unmount};

#[cfg(not(target_os = "linux"))]
pub fn mount(_: &Path, _: qurb_storage::ChunkKey, _: qurb_sync::DeviceId, _: PathBuf, _: Contents, _: PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("folders open in the file manager on Linux only, for now")
}

#[cfg(not(target_os = "linux"))]
pub fn unmount(_: &qurb_sync::DeviceId) {}

#[cfg(not(target_os = "linux"))]
pub fn any() -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
pub fn clear_stale(_: &Path) {}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Contents, Path, PathBuf};
    use fuser::{
        Errno, FileAttr, FileHandle, FileType, Filesystem, INodeNo, LockOwner, MountOption, OpenFlags, ReplyAttr,
        ReplyData, ReplyDirectory, ReplyEntry, Request,
    };
    use qurb_storage::Store;
    use qurb_sync::DeviceId;
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, UNIX_EPOCH};

    /// How long the kernel may keep what it was told about a file or folder.
    const TTL: Duration = Duration::from_secs(2);

    /// How often a mounted folder looks whether it is still open.
    const WATCH: Duration = Duration::from_secs(3);

    /// The mounts there are now, by person.
    fn mounts() -> &'static Mutex<HashMap<DeviceId, fuser::BackgroundSession>> {
        static MOUNTS: OnceLock<Mutex<HashMap<DeviceId, fuser::BackgroundSession>>> = OnceLock::new();
        MOUNTS.get_or_init(Default::default)
    }

    /// Mount `person`'s folder at `at`, its placeholder, unless it is already.
    /// `opened` is where a guest's files are unsealed while they are read.
    pub fn mount(
        store_dir: &Path,
        key: qurb_storage::ChunkKey,
        person: DeviceId,
        at: PathBuf,
        contents: Contents,
        opened: PathBuf,
    ) -> anyhow::Result<()> {
        let mut all = mounts().lock().map_err(|_| anyhow::anyhow!("the mounts are in a bad state"))?;
        if all.contains_key(&person) && qurb_cli::layout::mounted(&at) {
            return Ok(());
        }
        all.remove(&person);
        let store = Store::open(store_dir, key)?;
        let fs = Folder {
            person,
            contents,
            store: Mutex::new(store),
            nodes: Mutex::new(Nodes::new()),
            opened,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
        };
        // fusermount3 mounts only where the user may write; a placeholder is
        // kept read-only so that nothing put in it is taken for kept.
        writable(&at, true);
        let mut config = fuser::Config::default();
        config.mount_options = vec![
            MountOption::RO,
            MountOption::NoDev,
            MountOption::NoSuid,
            MountOption::FSName("qurb".into()),
            MountOption::Subtype("qurb".into()),
        ];
        let session = fuser::spawn_mount(fs, &at, &config);
        writable(&at, false);
        all.insert(person, session?);
        drop(all);
        // Locked by anything -- Lock, idle, asleep -- and the mount goes.
        std::thread::spawn(move || loop {
            std::thread::sleep(WATCH);
            let gone = !mounts().lock().map(|m| m.contains_key(&person)).unwrap_or(false);
            if gone {
                return;
            }
            if !qurb_peer::openings::is_open(&person) {
                unmount(&person);
                return;
            }
        });
        tracing::info!(person = %person.short(), at = %at.display(), "a person's folder mounted");
        Ok(())
    }

    /// Whether any person's folder is mounted now.
    pub fn any() -> bool {
        mounts().lock().map(|m| !m.is_empty()).unwrap_or(true)
    }

    /// Mounts left in `dir` by a QurbSpace that stopped without unmounting:
    /// killed, crashed or replaced by an update. The kernel keeps the mount
    /// and every look at the folder fails, "Transport endpoint is not
    /// connected" (found 2026-11-10). Lazily unmounted, so the folder is a
    /// placeholder again. Not ours to touch while this process mounts it.
    pub fn clear_stale(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let gone = matches!(std::fs::metadata(&path), Err(e) if e.raw_os_error() == Some(libc::ENOTCONN));
            if !gone {
                continue;
            }
            match std::process::Command::new("fusermount3").arg("-uz").arg(&path).status() {
                Ok(s) if s.success() => tracing::info!(at = %path.display(), "a folder left mounted cleared"),
                other => tracing::warn!(at = %path.display(), result = ?other, "could not clear a folder left mounted"),
            }
        }
    }

    /// Unmount `person`'s folder, if it is mounted.
    pub fn unmount(person: &DeviceId) {
        let session = mounts().lock().ok().and_then(|mut m| m.remove(person));
        if let Some(session) = session {
            if let Err(e) = session.umount_and_join() {
                tracing::warn!(error = %e, "unmounting a person's folder");
            } else {
                tracing::info!(person = %person.short(), "a person's folder unmounted");
            }
        }
    }

    fn writable(folder: &Path, on: bool) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(folder, std::fs::Permissions::from_mode(if on { 0o755 } else { 0o555 }));
    }

    /// What the mount knows of each place it has been asked about.
    #[derive(Clone)]
    struct Node {
        /// Where it is in the folder, `""` for its top.
        path: String,
        dir: bool,
        size: u64,
        modified: i64,
        /// The owner's: its content, read by its chunks.
        content: Option<blake3::Hash>,
        /// A guest's: where it is kept in their sealed folder.
        sealed: Option<(String, blake3::Hash)>,
        /// This computer's own: where it is on the disk.
        disk: Option<PathBuf>,
    }

    struct Nodes {
        by_ino: HashMap<u64, Node>,
        by_path: HashMap<String, u64>,
        next: u64,
    }

    impl Nodes {
        fn new() -> Self {
            let mut n = Nodes { by_ino: HashMap::new(), by_path: HashMap::new(), next: 2 };
            n.by_ino.insert(1, Node { path: String::new(), dir: true, size: 0, modified: 0, content: None, sealed: None, disk: None });
            n.by_path.insert(String::new(), 1);
            n
        }

        fn add(&mut self, node: Node) -> u64 {
            if let Some(ino) = self.by_path.get(&node.path) {
                self.by_ino.insert(*ino, node);
                return *ino;
            }
            let ino = self.next;
            self.next += 1;
            self.by_path.insert(node.path.clone(), ino);
            self.by_ino.insert(ino, node);
            ino
        }
    }

    struct Folder {
        person: DeviceId,
        contents: Contents,
        /// A store of its own, so that a long copy never holds the window's.
        store: Mutex<Store>,
        nodes: Mutex<Nodes>,
        opened: PathBuf,
        uid: u32,
        gid: u32,
    }

    impl Folder {
        /// The folder's key while it is open; reading counts as using it.
        fn open(&self) -> Option<[u8; 32]> {
            qurb_peer::openings::key(&self.person)
        }

        /// What is in `dir`, each one remembered by number.
        fn list(&self, dir: &str) -> Result<Vec<(u64, Node)>, Errno> {
            let key = self.open().ok_or(Errno::EACCES)?;
            let store = self.store.lock().map_err(|_| Errno::EIO)?;
            let join = |name: &str| if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") };
            let mut found: Vec<Node> = Vec::new();
            match &self.contents {
                Contents::Phone(folder) => {
                    // The phone's folder is the top: what it holds at `dir`
                    // is what `listing` holds under its name.
                    let inner = if dir.is_empty() { folder.0.clone() } else { format!("{}/{dir}", folder.0) };
                    let (folders, files) = qurb_storage::person::listing(store.db(), std::slice::from_ref(folder), &inner)
                        .map_err(|_| Errno::EIO)?;
                    found.extend(folders.into_iter().map(|f| Node {
                        path: join(&f), dir: true, size: 0, modified: 0, content: None, sealed: None, disk: None,
                    }));
                    found.extend(files.into_iter().map(|f| Node {
                        path: join(&f.name), dir: false, size: f.size, modified: f.modified_at,
                        content: Some(f.content), sealed: None, disk: None,
                    }));
                }
                Contents::Guest => {
                    let key = qurb_storage::sealed::FolderKey::from_bytes(key);
                    let kept = store.open_kept(&self.person, &key).map_err(|_| Errno::EIO)?;
                    let mut folders = std::collections::BTreeSet::new();
                    for f in kept {
                        let rest = match dir {
                            "" => f.path.as_str(),
                            _ => match f.path.strip_prefix(dir).and_then(|r| r.strip_prefix('/')) {
                                Some(rest) => rest,
                                None => continue,
                            },
                        };
                        match rest.split_once('/') {
                            Some((folder, _)) => {
                                folders.insert(folder.to_string());
                            }
                            None => found.push(Node {
                                path: join(rest), dir: false, size: f.size, modified: f.modified_at,
                                content: None, sealed: Some((f.path.clone(), f.sealed)), disk: None,
                            }),
                        }
                    }
                    found.extend(folders.into_iter().map(|f| Node {
                        path: join(&f), dir: true, size: 0, modified: 0, content: None, sealed: None, disk: None,
                    }));
                }
            }
            drop(store);
            let mut nodes = self.nodes.lock().map_err(|_| Errno::EIO)?;
            Ok(found.into_iter().map(|n| (nodes.add(n.clone()), n)).collect())
        }

        fn attr(&self, ino: u64, node: &Node) -> FileAttr {
            let at = UNIX_EPOCH + Duration::from_secs(node.modified.max(0) as u64);
            FileAttr {
                ino: INodeNo(ino),
                size: node.size,
                blocks: node.size.div_ceil(512),
                atime: at,
                mtime: at,
                ctime: at,
                crtime: at,
                kind: if node.dir { FileType::Directory } else { FileType::RegularFile },
                perm: if node.dir { 0o555 } else { 0o444 },
                nlink: if node.dir { 2 } else { 1 },
                uid: self.uid,
                gid: self.gid,
                rdev: 0,
                flags: 0,
                blksize: 4096,
            }
        }

        fn node(&self, ino: u64) -> Option<Node> {
            self.nodes.lock().ok()?.by_ino.get(&ino).cloned()
        }

        /// `size` bytes of `node` from `offset`.
        fn read_at(&self, node: &Node, offset: u64, size: u32) -> Result<Vec<u8>, Errno> {
            let key = self.open().ok_or(Errno::EACCES)?;
            let end = (offset + u64::from(size)).min(node.size);
            if offset >= end {
                return Ok(Vec::new());
            }
            if let Some(content) = node.content {
                let store = self.store.lock().map_err(|_| Errno::EIO)?;
                let chunks = store.held_layout(&content).map_err(|_| Errno::EIO)?.ok_or(Errno::EIO)?;
                let mut out = Vec::with_capacity((end - offset) as usize);
                let mut at = 0u64;
                for (hash, length) in chunks {
                    let next = at + u64::from(length);
                    if next > offset && at < end {
                        let bytes = store.read_chunk(&hash).map_err(|_| Errno::EIO)?;
                        let from = offset.saturating_sub(at) as usize;
                        let to = ((end - at) as usize).min(bytes.len());
                        out.extend_from_slice(&bytes[from.min(to)..to]);
                    }
                    if next >= end {
                        break;
                    }
                    at = next;
                }
                return Ok(out);
            }
            if let Some(at) = &node.disk {
                use std::io::{Read, Seek};
                let mut file = std::fs::File::open(at).map_err(|_| Errno::EIO)?;
                file.seek(std::io::SeekFrom::Start(offset)).map_err(|_| Errno::EIO)?;
                let mut out = vec![0u8; (end - offset) as usize];
                let n = file.read(&mut out).map_err(|_| Errno::EIO)?;
                out.truncate(n);
                return Ok(out);
            }
            let Some((path, sealed)) = &node.sealed else { return Err(Errno::EIO) };
            // Unsealed whole on first read, into memory-backed space, as
            // opening a guest's file did before; read from there after.
            let here = self.opened.join(&sealed.to_hex().as_str()[..16]);
            if !here.is_file() {
                std::fs::create_dir_all(&self.opened).map_err(|_| Errno::EIO)?;
                let key = qurb_storage::sealed::FolderKey::from_bytes(key);
                let store = self.store.lock().map_err(|_| Errno::EIO)?;
                if store.unseal_kept(&key, sealed, path, &here).is_err() {
                    let _ = std::fs::remove_file(&here);
                    return Err(Errno::EIO);
                }
            }
            use std::io::{Read, Seek};
            let mut file = std::fs::File::open(&here).map_err(|_| Errno::EIO)?;
            file.seek(std::io::SeekFrom::Start(offset)).map_err(|_| Errno::EIO)?;
            let mut out = vec![0u8; (end - offset) as usize];
            let n = file.read(&mut out).map_err(|_| Errno::EIO)?;
            out.truncate(n);
            Ok(out)
        }
    }

    impl Filesystem for Folder {
        fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
            let Some(dir) = self.node(u64::from(parent)) else { return reply.error(Errno::ENOENT) };
            let Some(name) = name.to_str() else { return reply.error(Errno::ENOENT) };
            match self.list(&dir.path) {
                Ok(found) => match found.into_iter().find(|(_, n)| n.path.rsplit('/').next() == Some(name)) {
                    Some((ino, node)) => reply.entry(&TTL, &self.attr(ino, &node), fuser::Generation(0)),
                    None => reply.error(Errno::ENOENT),
                },
                Err(e) => reply.error(e),
            }
        }

        fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
            if self.open().is_none() {
                return reply.error(Errno::EACCES);
            }
            match self.node(u64::from(ino)) {
                Some(node) => reply.attr(&TTL, &self.attr(u64::from(ino), &node)),
                None => reply.error(Errno::ENOENT),
            }
        }

        fn read(
            &self,
            _req: &Request,
            ino: INodeNo,
            _fh: FileHandle,
            offset: u64,
            size: u32,
            _flags: OpenFlags,
            _lock_owner: Option<LockOwner>,
            reply: ReplyData,
        ) {
            let Some(node) = self.node(u64::from(ino)) else { return reply.error(Errno::ENOENT) };
            match self.read_at(&node, offset, size) {
                Ok(bytes) => reply.data(&bytes),
                Err(e) => reply.error(e),
            }
        }

        fn readdir(&self, _req: &Request, ino: INodeNo, _fh: FileHandle, offset: u64, mut reply: ReplyDirectory) {
            let Some(dir) = self.node(u64::from(ino)) else { return reply.error(Errno::ENOENT) };
            let found = match self.list(&dir.path) {
                Ok(found) => found,
                Err(e) => return reply.error(e),
            };
            let mut entries: Vec<(u64, FileType, String)> = vec![
                (u64::from(ino), FileType::Directory, ".".into()),
                (1, FileType::Directory, "..".into()),
            ];
            entries.extend(found.into_iter().map(|(n, node)| {
                let name = node.path.rsplit('/').next().unwrap_or(&node.path).to_string();
                (n, if node.dir { FileType::Directory } else { FileType::RegularFile }, name)
            }));
            for (i, (n, kind, name)) in entries.into_iter().enumerate().skip(offset as usize) {
                if reply.add(INodeNo(n), (i + 1) as u64, kind, &name) {
                    break;
                }
            }
            reply.ok();
        }
    }
}
