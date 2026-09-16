//! Content-defined chunking.
//!
//! Splits a byte stream at boundaries determined by the content rather than by
//! offset, so that inserting or removing bytes does not shift every subsequent
//! boundary. See ../../docs/CODEBASE.md section 2.2.

use crate::error::{Error, Result};
use std::fs::File;
use std::path::Path;

/// Chunk size bounds, in bytes.
///
/// Measured against a real corpus rather than chosen by intuition; the
/// architecture document's original 1/2/4 MiB proposal performed badly because
/// 94% of real files fall below a 1 MiB minimum and collapse to a single chunk
/// each. See ../../docs/decisions/0004-chunk-parameters.md.
pub const MIN_CHUNK: u32 = 128 << 10; // 128 KiB
pub const AVG_CHUNK: u32 = 512 << 10; // 512 KiB
pub const MAX_CHUNK: u32 = 2 << 20; //   2 MiB

/// One chunk's position within a file, and the hash of its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRef {
    pub hash: blake3::Hash,
    pub offset: u64,
    pub len: u32,
}

/// A file reduced to an ordered list of chunk references plus a whole-file hash.
///
/// The whole-file hash is what verifies a reassembled file end to end; the
/// chunk hashes alone would not catch chunks being reassembled in the wrong
/// order.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub file_hash: blake3::Hash,
    pub size: u64,
    pub chunks: Vec<ChunkRef>,
}

/// Split a file into chunks and hash each one.
///
/// The file is memory-mapped, so peak resident memory stays flat regardless of
/// file size. That property is what will make this usable inside an iOS
/// FileProvider extension, which runs under a tight memory ceiling.
///
/// # Caveat
///
/// A concurrent truncation of `path` while this runs will raise SIGBUS, which
/// Rust cannot catch. The sync engine must not chunk a file it has not first
/// stabilised; that responsibility sits above this function.
pub fn chunk_file(path: &Path) -> Result<Manifest> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let size = file.metadata().map_err(|e| Error::io(path, e))?.len();

    if size == 0 {
        return Ok(Manifest { file_hash: blake3::hash(b""), size: 0, chunks: Vec::new() });
    }

    let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| Error::io(path, e))?;
    Ok(chunk_bytes(&mmap))
}

/// Chunk an in-memory buffer. Used by tests and by callers that already hold
/// the data; prefer [`chunk_file`] for anything on disk.
pub fn chunk_bytes(data: &[u8]) -> Manifest {
    let mut chunks = Vec::new();
    let mut whole = blake3::Hasher::new();

    for entry in fastcdc::v2020::FastCDC::new(data, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK) {
        let bytes = &data[entry.offset..entry.offset + entry.length];
        chunks.push(ChunkRef {
            hash: blake3::hash(bytes),
            offset: entry.offset as u64,
            len: entry.length as u32,
        });
        whole.update(bytes);
    }

    Manifest { file_hash: whole.finalize(), size: data.len() as u64, chunks }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic, non-periodic test bytes.
    ///
    /// A linear generator like `i * K as u8` looks random but repeats every
    /// 256 bytes, and perfectly periodic input defeats content-defined
    /// chunking entirely: every cut lands at the same phase, so shifting the
    /// input by one byte changes every chunk. xorshift has a long enough
    /// period to behave like real data.
    fn pseudo_random(seed: u32, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        let mut x = seed | 1;
        for _ in 0..len {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            out.push(x as u8);
        }
        out
    }

    #[test]
    fn empty_input_produces_no_chunks() {
        let m = chunk_bytes(b"");
        assert_eq!(m.size, 0);
        assert!(m.chunks.is_empty());
    }

    #[test]
    fn chunks_cover_the_input_exactly_once() {
        let data = pseudo_random(1, 5_000_000);
        let m = chunk_bytes(&data);

        let mut expected_offset = 0u64;
        for c in &m.chunks {
            assert_eq!(c.offset, expected_offset, "chunks must be contiguous");
            expected_offset += c.len as u64;
        }
        assert_eq!(expected_offset, data.len() as u64, "chunks must cover the whole input");
    }

    #[test]
    fn insertion_leaves_distant_chunks_untouched() {
        // The property that justifies content-defined chunking at all.
        let data = pseudo_random(2, 8_000_000);
        let mut shifted = vec![0xAAu8];
        shifted.extend_from_slice(&data);

        let before: std::collections::HashSet<_> =
            chunk_bytes(&data).chunks.iter().map(|c| c.hash).collect();
        let after: std::collections::HashSet<_> =
            chunk_bytes(&shifted).chunks.iter().map(|c| c.hash).collect();

        let reused = before.intersection(&after).count();
        let pct = reused as f64 / before.len() as f64;
        assert!(pct > 0.8, "expected most chunks to survive an insertion, got {:.1}%", pct * 100.0);
    }
}
