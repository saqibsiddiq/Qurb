//! The on-disk chunk store.
//!
//! Deliberately dumb: it moves opaque byte strings to and from paths derived
//! from a hash. It does not know that the bytes are encrypted, and it does not
//! verify them — that belongs one layer up, in [`crate::Store`], which holds
//! the key needed to check anything.

use crate::error::{Error, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Cas {
    root: PathBuf,
}

impl Cas {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|e| Error::io(&root, e))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/<first 2 hex>/<full hex>`.
    ///
    /// The two-character fan-out caps any single directory at roughly 1/256th
    /// of the chunk population. Filesystems slow down noticeably once a
    /// directory holds millions of entries, and a large library would get
    /// there.
    fn path_for(&self, hash: &blake3::Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.root.join(&hex[..2]).join(hex.as_str())
    }

    pub fn contains(&self, hash: &blake3::Hash) -> bool {
        self.path_for(hash).exists()
    }

    /// Write a chunk unless it is already present. Returns whether it was new.
    ///
    /// Writes to a temporary name and renames into place, so a crash mid-write
    /// cannot leave a partial payload sitting at a path that claims to describe
    /// it. On every filesystem we target, rename within a directory is atomic.
    pub fn put(&self, hash: &blake3::Hash, payload: &[u8]) -> Result<bool> {
        let dest = self.path_for(hash);
        if dest.exists() {
            return Ok(false);
        }

        let dir = dest.parent().expect("chunk paths always have a parent");
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;

        let tmp = dir.join(format!(".{}.tmp", hash.to_hex()));
        {
            let mut f = fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
            f.write_all(payload).map_err(|e| Error::io(&tmp, e))?;
            // fsync before rename: without it a crash can leave the rename
            // durable but the contents not, which is the one outcome the
            // temp-and-rename dance exists to prevent.
            f.sync_all().map_err(|e| Error::io(&tmp, e))?;
        }
        fs::rename(&tmp, &dest).map_err(|e| Error::io(&dest, e))?;
        Ok(true)
    }

    pub fn get(&self, hash: &blake3::Hash) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::ChunkMissing { hash: hash.to_hex().to_string() },
            _ => Error::io(&path, e),
        })
    }

    /// Remove a chunk. Missing is not an error — garbage collection may race
    /// with another process that already removed it, and the desired end state
    /// is the same either way.
    pub fn remove(&self, hash: &blake3::Hash) -> Result<()> {
        let path = self.path_for(hash);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::io(&path, e)),
        }
    }

    pub fn stored_size(&self, hash: &blake3::Hash) -> Result<u64> {
        let path = self.path_for(hash);
        Ok(fs::metadata(&path).map_err(|e| Error::io(&path, e))?.len())
    }

    /// Every chunk hash present on disk.
    ///
    /// Used by verification to find chunks the index does not know about.
    /// Loads all hashes into memory, which is fine at the scale where anyone
    /// runs a full verification pass by hand and will need revisiting if it
    /// ever runs automatically over a very large store.
    pub fn iter_hashes(&self) -> Result<Vec<blake3::Hash>> {
        let mut out = Vec::new();
        let entries = match fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(Error::io(&self.root, e)),
        };

        for shard in entries.flatten() {
            if !shard.path().is_dir() {
                continue;
            }
            let inner = match fs::read_dir(shard.path()) {
                Ok(i) => i,
                Err(e) => return Err(Error::io(shard.path(), e)),
            };
            for entry in inner.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with('.') {
                    continue; // an interrupted write's temp file
                }
                if let Ok(hash) = parse_hash(name) {
                    out.push(hash);
                }
            }
        }
        Ok(out)
    }
}

fn parse_hash(hex: &str) -> std::result::Result<blake3::Hash, ()> {
    if hex.len() != 64 {
        return Err(());
    }
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| ())?;
    }
    Ok(blake3::Hash::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cas() -> (tempfile::TempDir, Cas) {
        let dir = tempfile::tempdir().unwrap();
        let cas = Cas::open(dir.path().join("chunks")).unwrap();
        (dir, cas)
    }

    #[test]
    fn put_get_round_trip() {
        let (_d, cas) = cas();
        let h = blake3::hash(b"payload");
        assert!(cas.put(&h, b"payload").unwrap(), "first put is new");
        assert!(!cas.put(&h, b"payload").unwrap(), "second put is a no-op");
        assert_eq!(cas.get(&h).unwrap(), b"payload");
    }

    #[test]
    fn missing_chunk_reports_missing_not_io() {
        let (_d, cas) = cas();
        let h = blake3::hash(b"never stored");
        assert!(matches!(cas.get(&h), Err(Error::ChunkMissing { .. })));
    }

    #[test]
    fn remove_is_idempotent() {
        let (_d, cas) = cas();
        let h = blake3::hash(b"x");
        cas.put(&h, b"x").unwrap();
        cas.remove(&h).unwrap();
        cas.remove(&h).unwrap();
        assert!(!cas.contains(&h));
    }

    #[test]
    fn iter_hashes_finds_everything_and_skips_temp_files() {
        let (_d, cas) = cas();
        let hashes: Vec<_> = (0..50u8).map(|i| blake3::hash(&[i])).collect();
        for (i, h) in hashes.iter().enumerate() {
            cas.put(h, &[i as u8]).unwrap();
        }
        // Simulate a write interrupted before its rename.
        let stray = cas.root().join("ab");
        fs::create_dir_all(&stray).unwrap();
        fs::write(stray.join(".partial.tmp"), b"junk").unwrap();

        let found = cas.iter_hashes().unwrap();
        assert_eq!(found.len(), hashes.len());
    }
}
