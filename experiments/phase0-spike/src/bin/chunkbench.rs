//! Phase 0 kill-criterion harness.
//!
//! Usage:
//!   chunkbench                 -- generate a synthetic corpus and measure it
//!   chunkbench <path>          -- measure a real file or directory tree
//!
//! Reports the three numbers that decide whether Phase 1 is worth starting:
//! chunking throughput, dedup ratio, and boundary stability under an edit.

use anyhow::{bail, Result};
use rand::{Rng, SeedableRng};
use spike::{human, Cas, Manifest};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const FIXED_BLOCK: usize = 4 << 20; // Dropbox-style static 4 MiB blocks

fn main() -> Result<()> {
    let scratch = PathBuf::from(
        std::env::var("SPIKE_SCRATCH").unwrap_or_else(|_| "/tmp/spike".into()),
    );
    fs::create_dir_all(&scratch)?;

    let target = std::env::args().nth(1).map(PathBuf::from);
    let files = match &target {
        Some(p) => {
            if !p.exists() {
                bail!("{} does not exist", p.display());
            }
            let found = collect(p)?;
            if found.is_empty() {
                bail!(
                    "no non-empty files under {} -- point this at a directory with real \
                     content, or run with no argument to use the synthetic corpus",
                    p.display()
                );
            }
            found
        }
        None => vec![generate_corpus(&scratch)?],
    };

    println!("=== FastCDC / BLAKE3 spike ===");
    println!("min {} avg {} max {}\n",
        human(spike::MIN_CHUNK as u64), human(spike::AVG_CHUNK as u64), human(spike::MAX_CHUNK as u64));

    // ---- 1. Chunking throughput -------------------------------------------
    let mut total_bytes = 0u64;
    let mut total_chunks = 0usize;
    let mut manifests: Vec<(PathBuf, Manifest)> = Vec::new();

    let t0 = Instant::now();
    for f in &files {
        let m = spike::chunk_file(f)?;
        total_bytes += m.size;
        total_chunks += m.chunks.len();
        manifests.push((f.clone(), m));
    }
    let elapsed = t0.elapsed();
    let mbps = (total_bytes as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64();

    println!("[1] chunk + hash");
    println!("    files          {}", files.len());
    println!("    input          {}", human(total_bytes));
    println!("    chunks         {total_chunks}");
    println!("    mean chunk     {}", human(if total_chunks > 0 { total_bytes / total_chunks as u64 } else { 0 }));
    println!("    elapsed        {:.2}s", elapsed.as_secs_f64());
    println!("    throughput     {mbps:.0} MiB/s   {}",
        if mbps >= 150.0 { "PASS (>=150)" } else { "FAIL (<150)" });

    // ---- 2. Dedup ratio ----------------------------------------------------
    let cas_root = scratch.join("cas");
    let _ = fs::remove_dir_all(&cas_root);
    let cas = Cas::open(&cas_root)?;

    let mut written = 0u64;
    let mut deduped = 0u64;
    for (path, m) in &manifests {
        let s = cas.ingest(path, m)?;
        written += s.bytes_written;
        deduped += s.bytes_deduped;
    }
    let ratio = if total_bytes > 0 { deduped as f64 / total_bytes as f64 * 100.0 } else { 0.0 };

    println!("\n[2] dedup on ingest");
    println!("    stored         {}", human(written));
    println!("    deduplicated   {} ({ratio:.1}% of input)", human(deduped));

    // ---- 3. Round trip -----------------------------------------------------
    println!("\n[3] reassemble + verify");
    let out = scratch.join("rebuilt");
    let _ = fs::remove_dir_all(&out);
    for (i, (_, m)) in manifests.iter().enumerate() {
        cas.reassemble(m, &out.join(format!("f{i}")))?;
    }
    println!("    {} files rebuilt, whole-file BLAKE3 verified   PASS", manifests.len());

    // ---- 4. Boundary stability --------------------------------------------
    // The claim CDC exists to make: prepending a byte should invalidate one
    // chunk, not the whole file. Fixed blocks invalidate everything.
    let (probe_path, probe_manifest) = manifests
        .iter()
        .max_by_key(|(_, m)| m.size)
        .expect("file list is non-empty, checked above");

    let original = fs::read(probe_path)?;
    let mut shifted = Vec::with_capacity(original.len() + 1);
    shifted.push(0x42);
    shifted.extend_from_slice(&original);

    let shifted_path = scratch.join("shifted.bin");
    fs::write(&shifted_path, &shifted)?;
    let shifted_manifest = spike::chunk_file(&shifted_path)?;

    let before: HashSet<_> = probe_manifest.distinct_chunks();
    let after: HashSet<_> = shifted_manifest.distinct_chunks();
    let survived = before.intersection(&after).count();
    let cdc_pct = survived as f64 / before.len().max(1) as f64 * 100.0;

    let fixed_before = fixed_hashes(&original);
    let fixed_after = fixed_hashes(&shifted);
    let fixed_survived = fixed_before.intersection(&fixed_after).count();
    let fixed_pct = fixed_survived as f64 / fixed_before.len().max(1) as f64 * 100.0;

    println!("\n[4] boundary stability (1 byte prepended to {})", human(original.len() as u64));
    println!("    FastCDC        {survived}/{} chunks reusable ({cdc_pct:.1}%)", before.len());
    println!("    fixed 4MiB     {fixed_survived}/{} blocks reusable ({fixed_pct:.1}%)", fixed_before.len());
    println!("    verdict        {}",
        if cdc_pct >= 90.0 { "PASS (>=90% reuse)" } else { "FAIL -- CDC not earning its keep" });

    println!("\nCAS at {}", cas_root.display());
    Ok(())
}

fn fixed_hashes(data: &[u8]) -> HashSet<blake3::Hash> {
    data.chunks(FIXED_BLOCK).map(blake3::hash).collect()
}

fn collect(path: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(out);
    }
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let p = entry?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() && p.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                out.push(p);
            }
        }
    }
    Ok(out)
}

/// A 2 GiB corpus that mimics a real sync folder: some highly compressible
/// text, some incompressible media, and a deliberate duplicate to prove the
/// dedup path fires.
fn generate_corpus(scratch: &Path) -> Result<PathBuf> {
    let path = scratch.join("corpus.bin");
    if path.exists() && path.metadata()?.len() > 0 {
        println!("(reusing corpus at {})\n", path.display());
        return Ok(path);
    }
    println!("generating 2 GiB synthetic corpus...\n");

    let mut rng = rand::rngs::StdRng::seed_from_u64(0xC0FFEE);
    let mut buf = Vec::with_capacity(2 << 30);

    // Incompressible: stands in for photos and video.
    let mut media = vec![0u8; 512 << 20];
    rng.fill(&mut media[..]);
    buf.extend_from_slice(&media);

    // Compressible and repetitive: stands in for documents and code.
    let para = b"the quick brown fox jumps over the lazy dog while the sync engine hashes along. ";
    while buf.len() < (1 << 30) {
        buf.extend_from_slice(para);
    }

    // An exact duplicate of the media region -- dedup must catch all of this.
    buf.extend_from_slice(&media);

    // Tail of fresh randomness so the file does not end on a repeat.
    let mut tail = vec![0u8; 256 << 20];
    rng.fill(&mut tail[..]);
    buf.extend_from_slice(&tail);

    fs::write(&path, &buf)?;
    Ok(path)
}
