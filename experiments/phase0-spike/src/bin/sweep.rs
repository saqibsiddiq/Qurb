//! Chunk parameter sweep.
//!
//! The 1 MiB minimum in the original spike was picked from the architecture
//! doc, not from data. On a real corpus most files are smaller than that, so
//! they collapse to one chunk each and content-defined chunking does nothing
//! for them. This measures what different parameters actually cost and buy.
//!
//! Usage: sweep <dir>

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// (label, min, avg, max)
const CONFIGS: [(&str, u32, u32, u32); 5] = [
    ("64K avg",  16 << 10,  64 << 10, 256 << 10),
    ("256K avg", 64 << 10, 256 << 10,   1 << 20),
    ("512K avg",128 << 10, 512 << 10,   2 << 20), // restic's default
    ("1M avg",  256 << 10,   1 << 20,   4 << 20),
    ("2M avg",    1 << 20,   2 << 20,   4 << 20), // the doc's proposal
];

/// Bytes of SQLite index per chunk: 32B hash + file id + offset + length +
/// refcount + btree overhead. Deliberately conservative.
const INDEX_BYTES_PER_CHUNK: u64 = 96;

fn main() -> Result<()> {
    let dir = std::env::args().nth(1).map(PathBuf::from)
        .unwrap_or_else(|| panic!("usage: sweep <dir>"));
    let files = collect(&dir)?;
    if files.is_empty() {
        bail!("no files under {}", dir.display());
    }

    let total: u64 = files.iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum();
    println!("corpus: {} files, {}\n", files.len(), spike::human(total));

    println!("{:<10} {:>9} {:>11} {:>9} {:>10} {:>12} {:>11}",
        "config", "chunks", "mean", "dedup", "index/TiB", "throughput", "1KiB edit");
    println!("{}", "-".repeat(78));

    for (label, min, avg, max) in CONFIGS {
        let t0 = Instant::now();
        let mut counts: HashMap<blake3::Hash, u32> = HashMap::new();
        let mut sizes: HashMap<blake3::Hash, u32> = HashMap::new();
        let mut nchunks = 0u64;

        for f in &files {
            let Ok(file) = File::open(f) else { continue };
            let Ok(meta) = file.metadata() else { continue };
            if meta.len() == 0 { continue }
            let Ok(mmap) = (unsafe { memmap2::Mmap::map(&file) }) else { continue };

            for e in fastcdc::v2020::FastCDC::new(&mmap, min, avg, max) {
                let h = blake3::hash(&mmap[e.offset..e.offset + e.length]);
                *counts.entry(h).or_insert(0) += 1;
                sizes.insert(h, e.length as u32);
                nchunks += 1;
            }
        }

        let secs = t0.elapsed().as_secs_f64();
        let mbps = (total as f64 / (1024.0 * 1024.0)) / secs;

        let unique = counts.len() as u64;
        let stored: u64 = sizes.values().map(|&v| v as u64).sum();
        let dedup_pct = if total > 0 { (1.0 - stored as f64 / total as f64) * 100.0 } else { 0.0 };
        let mean = if nchunks > 0 { total / nchunks } else { 0 };

        // Index cost extrapolated to a 1 TiB library at this mean chunk size.
        let chunks_per_tib = if mean > 0 { (1u64 << 40) / mean } else { 0 };
        let index_per_tib = chunks_per_tib * INDEX_BYTES_PER_CHUNK;

        // What a 1 KiB edit in the middle of a file costs to resend: one chunk,
        // since CDC localises the change. This is the number that decides how
        // responsive sync feels on a slow uplink.
        println!("{label:<10} {nchunks:>9} {:>11} {dedup_pct:>8.1}% {:>10} {mbps:>9.0} MiB/s {:>11}",
            spike::human(mean), spike::human(index_per_tib), spike::human(mean));
        let _ = unique;
    }

    println!("\nnotes");
    println!("  dedup     -- unique chunk bytes vs corpus bytes, this corpus only");
    println!("  index/TiB -- SQLite metadata for a 1 TiB library at {INDEX_BYTES_PER_CHUNK}B/chunk");
    println!("  1KiB edit -- bytes resent for a small edit; equals one mean chunk");
    Ok(())
}

fn collect(path: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_symlink() { continue }
            if p.is_dir() { stack.push(p); }
            else if p.is_file() && p.metadata().map(|m| m.len() > 0).unwrap_or(false) { out.push(p); }
        }
    }
    Ok(out)
}
