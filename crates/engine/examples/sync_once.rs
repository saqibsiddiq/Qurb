//! Reconcile a directory into a store once, and report what it cost.
//!
//! ```text
//! cargo run --release --example sync_once -- <directory> <store>
//! ```
//!
//! Run it twice on the same pair. The second run should store nothing and
//! finish in a fraction of the time — that difference is the size-and-mtime
//! fast path, which is what makes startup on a large library bearable.
//!
//! Development tool. The key is generated fresh each run, so a store written by
//! one invocation cannot be read by the next; that is fine for measuring
//! reconciliation and useless for anything else.

use qurb_engine::Engine;
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(store_dir)) = (args.next(), args.next()) else {
        eprintln!("usage: sync_once <directory> <store>");
        std::process::exit(2);
    };
    let root = PathBuf::from(root);
    let store_dir = PathBuf::from(store_dir);

    let key = match std::fs::read(store_dir.join("dev-key")) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            ChunkKey::from_bytes(k)
        }
        _ => ChunkKey::generate(),
    };

    let store = Store::open(&store_dir, key)?;
    let ignore = IgnoreRules::new().with_store_dir(&store_dir);
    let mut engine = Engine::new(&root, store, ignore);

    println!("reconciling {} -> {}", root.display(), store_dir.display());
    let started = Instant::now();
    let stats = engine.reconcile()?;
    let elapsed = started.elapsed();

    let (plain, stored) = engine.store().db().size_totals()?;
    println!("\n  stored      {}", stats.stored);
    println!("  unchanged   {}", stats.unchanged);
    println!("  deleted     {}", stats.deleted);
    println!("  failed      {}", stats.failures.len());
    println!("  elapsed     {:.2}s", elapsed.as_secs_f64());
    println!("\n  chunks      {}", engine.store().db().chunk_count()?);
    println!("  plaintext   {}", human(plain));
    println!("  on disk     {} ({:.1}% of plaintext)", human(stored),
        if plain > 0 { stored as f64 / plain as f64 * 100.0 } else { 0.0 });

    for failure in stats.failures.iter().take(10) {
        println!("  ! {}: {}", failure.path.display(), failure.error);
    }
    if stats.failures.len() > 10 {
        println!("  ... and {} more", stats.failures.len() - 10);
    }

    Ok(())
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", UNITS[i])
}
