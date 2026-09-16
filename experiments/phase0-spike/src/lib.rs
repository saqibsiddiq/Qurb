//! Phase 0 spike: content-defined chunking + content-addressable store.
//!
//! Deliberately throwaway. No encryption, no compression, no database.
//! The only questions this answers: is FastCDC + BLAKE3 fast enough, does
//! dedup actually pay on real files, and does an edit at the head of a file
//! leave the tail's chunks untouched.

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// FastCDC size bounds.
///
/// The architecture document proposed 1/2/4 MiB. Measured against a real
/// corpus that turned out to be wrong: 94% of files there were smaller than
/// the 1 MiB minimum, so each collapsed to a single chunk and content-defined
/// chunking bought nothing for them. Halving the target to 512 KiB nearly
/// doubled the dedup rate (7.4% -> 13.3%) while keeping the metadata index
/// under 350 MiB per TiB of library. See `sweep` for the full comparison.
pub const MIN_CHUNK: u32 = 128 << 10; // 128 KiB
pub const AVG_CHUNK: u32 = 512 << 10; // 512 KiB
pub const MAX_CHUNK: u32 = 2 << 20; //   2 MiB

pub type Hash = blake3::Hash;

#[derive(Debug, Clone)]
pub struct ChunkRef {
    pub hash: Hash,
    pub offset: u64,
    pub len: u32,
}

/// A file reduced to an ordered list of chunk references.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub file_hash: Hash,
    pub size: u64,
    pub chunks: Vec<ChunkRef>,
}

impl Manifest {
    pub fn distinct_chunks(&self) -> HashSet<Hash> {
        self.chunks.iter().map(|c| c.hash).collect()
    }
}

/// Split a file into content-defined chunks and hash each one.
///
/// Memory-maps the input so the peak RSS stays flat regardless of file size --
/// the OS pages it in and out under us. That property is the whole reason this
/// can run inside an iOS FileProvider extension later.
pub fn chunk_file(path: &Path) -> Result<Manifest> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let size = file.metadata()?.len();

    if size == 0 {
        return Ok(Manifest { file_hash: blake3::hash(b""), size: 0, chunks: Vec::new() });
    }

    // SAFETY: we accept the usual mmap caveat -- if another process truncates
    // this file mid-read we get a SIGBUS. Fine for a spike; the real engine
    // will need to handle concurrent modification anyway.
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    let mut chunks = Vec::new();
    let mut whole = blake3::Hasher::new();

    for entry in fastcdc::v2020::FastCDC::new(&mmap, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK) {
        let bytes = &mmap[entry.offset..entry.offset + entry.length];
        chunks.push(ChunkRef {
            hash: blake3::hash(bytes),
            offset: entry.offset as u64,
            len: entry.length as u32,
        });
        whole.update(bytes);
    }

    Ok(Manifest { file_hash: whole.finalize(), size, chunks })
}

/// Content-addressable chunk store: `<root>/<first-2-hex>/<full-hex>`.
///
/// The two-character fan-out keeps any single directory to roughly 1/256th of
/// the chunk population, which is what stops directory lookups from degrading
/// on ext4 and NTFS once you have millions of chunks.
pub struct Cas {
    root: PathBuf,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WriteStats {
    pub written: u64,
    pub deduped: u64,
    pub bytes_written: u64,
    pub bytes_deduped: u64,
}

impl Cas {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path_for(&self, hash: &Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.root.join(&hex[..2]).join(hex.as_str())
    }

    pub fn contains(&self, hash: &Hash) -> bool {
        self.path_for(hash).exists()
    }

    /// Write a chunk if absent. Returns true if this was a new chunk.
    ///
    /// Writes to a temp path then renames, so a crash mid-write can never leave
    /// a truncated chunk sitting at a hash that claims to describe it.
    pub fn put(&self, hash: &Hash, bytes: &[u8]) -> Result<bool> {
        let dest = self.path_for(hash);
        if dest.exists() {
            return Ok(false);
        }
        let dir = dest.parent().unwrap();
        fs::create_dir_all(dir)?;

        let tmp = dir.join(format!(".{}.tmp", hash.to_hex()));
        {
            let mut f = File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &dest)?;
        Ok(true)
    }

    pub fn get(&self, hash: &Hash) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        let bytes = fs::read(&path).with_context(|| format!("missing chunk {}", hash.to_hex()))?;
        // Verify on read. In the real engine this is what the integrity
        // scrubber leans on to detect bit rot.
        let actual = blake3::hash(&bytes);
        if actual != *hash {
            bail!("chunk {} failed integrity check (got {})", hash.to_hex(), actual.to_hex());
        }
        Ok(bytes)
    }

    /// Ingest every chunk of a manifest, skipping ones already present.
    pub fn ingest(&self, src: &Path, manifest: &Manifest) -> Result<WriteStats> {
        let file = File::open(src)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let mut stats = WriteStats::default();

        for c in &manifest.chunks {
            let bytes = &mmap[c.offset as usize..c.offset as usize + c.len as usize];
            if self.put(&c.hash, bytes)? {
                stats.written += 1;
                stats.bytes_written += c.len as u64;
            } else {
                stats.deduped += 1;
                stats.bytes_deduped += c.len as u64;
            }
        }
        Ok(stats)
    }

    /// Rebuild a file from its manifest and verify the result end to end.
    pub fn reassemble(&self, manifest: &Manifest, dest: &Path) -> Result<()> {
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = File::create(dest)?;
        let mut whole = blake3::Hasher::new();

        for c in &manifest.chunks {
            let bytes = self.get(&c.hash)?;
            if bytes.len() != c.len as usize {
                bail!("chunk {} has wrong length", c.hash.to_hex());
            }
            whole.update(&bytes);
            out.write_all(&bytes)?;
        }
        out.sync_all()?;

        let got = whole.finalize();
        if got != manifest.file_hash {
            bail!("reassembled file hash mismatch: {} != {}", got.to_hex(), manifest.file_hash.to_hex());
        }
        Ok(())
    }
}

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", UNITS[i])
}
