//! How much memory it costs to receive a large file from a peer.
//!
//! [`crates/storage`'s example] measures reading from the store. This measures
//! the path a real sync takes: plan against a peer's tree, fetch the content,
//! write it, adopt the version. That path is what an iOS FileProvider extension
//! would run inside, under a ceiling in the tens of megabytes, so the number
//! that matters is peak heap against file size — it should be flat.
//!
//! Two modes, for comparison. `stream` is what the engine does now. `buffered`
//! reconstructs what it did before — fetch the whole file, write it, hand the
//! same buffer to `adopt` — so the cost of the old shape stays measurable
//! rather than becoming a claim in a document nobody can check.
//!
//! Each runs in its own process: peak is a high-water mark, so whichever mode
//! costs more would otherwise set the number for both.
//!
//! ```bash
//! cargo run --release -p qurb-engine --example peak_memory -- stream 1024
//! cargo run --release -p qurb-engine --example peak_memory -- buffered 1024
//! ```
//!
//! [`crates/storage`'s example]: ../../storage/examples/peak_memory.rs

use qurb_engine::{ContentSource, Engine, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_sync::{Action, Content};
use qurb_watcher::IgnoreRules;
use std::io::Write;
use std::path::Path;

/// Heap and other anonymous memory, in KiB.
///
/// Not total resident size: a memory-limited platform counts *dirty* pages, and
/// pages backed by a file on disk are clean and droppable. Counting both alike
/// makes a memory-mapped read look exactly as costly as holding the whole file
/// in a `Vec`, which is the opposite of true.
fn anon_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|l| l.starts_with("RssAnon:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "stream".into());
    let mib: usize = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(256);
    let size = mib * 1024 * 1024;
    if mode != "stream" && mode != "buffered" {
        eprintln!("usage: peak_memory <stream|buffered> [mib]");
        std::process::exit(2);
    }

    let dir = tempfile::tempdir()?;
    let sender_root = dir.path().join("sender");
    let receiver_root = dir.path().join("receiver");

    // Written in pieces, so creating the test file does not itself dominate the
    // high-water mark we are about to read.
    std::fs::create_dir_all(&sender_root)?;
    {
        let mut file = std::fs::File::create(sender_root.join("large.bin"))?;
        let mut x: u32 = 1;
        let mut block = vec![0u8; 1 << 20];
        for _ in 0..mib {
            for byte in block.iter_mut() {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *byte = x as u8;
            }
            file.write_all(&block)?;
        }
    }

    let sender = open(&sender_root)?;
    let mut receiver = open(&receiver_root)?;

    let plan = receiver.plan_against(&sender.tree()?)?;
    assert_eq!(plan.len(), 1, "expected exactly one file to adopt");

    let landed = receiver_root.join("large.bin");
    let before = anon_kib();

    // Held until after the measurement in `buffered` mode. Dropping it first
    // returns the memory to the allocator and makes a 1 GiB buffer look free.
    let held = match mode.as_str() {
        "stream" => {
            let stats = receiver.apply_plan(&plan, &mut StoreSource::new(sender.store()))?;
            assert!(stats.failures.is_empty(), "{:?}", stats.failures);
            assert_eq!(stats.adopted, 1);
            None
        }
        _ => {
            let Action::Adopt { remote } = &plan[0] else { panic!("expected an adopt") };
            let Content::File { hash, size } = &remote.content else { panic!("expected a file") };

            let bytes = StoreSource::new(sender.store()).fetch(hash, *size)?;
            std::fs::write(&landed, &bytes)?;
            receiver.store_mut().adopt(remote, Some(&bytes), remote.modified_at)?;
            Some(bytes)
        }
    };
    let peak = anon_kib();

    // The point of the exercise: the file is there, and it is right.
    assert_eq!(std::fs::metadata(&landed)?.len() as usize, size);
    assert!(
        !receiver_root.join(".large.bin.incoming").exists(),
        "staging file left behind"
    );

    println!(
        "{mode:<9} file {mib:>5} MiB   heap {:>5} MiB   (before: {} MiB, grew by {} MiB)",
        peak / 1024,
        before / 1024,
        peak.saturating_sub(before) / 1024,
    );
    drop(held);
    Ok(())
}

fn open(root: &Path) -> Result<Engine, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root)?;
    let store_dir = root.join(".qurb");
    let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32]))?;
    let ignore = IgnoreRules::new().with_store_dir(&store_dir);
    let mut engine = Engine::new(root, store, ignore);
    engine.reconcile()?;
    Ok(engine)
}
