//! How much memory storing and reading a file actually costs.
//!
//! An iOS FileProvider extension runs under a memory ceiling in the tens of
//! megabytes. A path that holds a whole file in memory works on a desktop and
//! is killed on a phone, so the number that matters is peak resident memory
//! against file size — it should be flat.

use qurb_storage::{ChunkKey, Store};
use std::io::Write;

/// One field from /proc/self/status, in KiB.
fn status(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|l| l.starts_with(field))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

/// Heap and other anonymous memory.
///
/// The number that matters. A memory-limited platform counts *dirty* pages
/// against a process; pages backed by a file on disk are clean and the kernel
/// can drop them at will. Measuring total resident size counts both alike and
/// makes a memory-mapped read look exactly as dangerous as a 256 MiB `Vec`,
/// which is the opposite of true.
fn anon_kib() -> u64 {
    status("RssAnon:")
}

/// Pages backed by a file — mappings, mostly. Reported for contrast rather than
/// because it is a cost.
fn file_kib() -> u64 {
    status("RssFile:")
}

/// Peak is a high-water mark, so one process cannot separate three operations —
/// whichever costs most sets the number for all of them. Each mode runs alone.
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "help".into());
    let size: usize = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(256 * 1024 * 1024);

    if mode == "help" {
        eprintln!("usage: peak_memory <store|read|stream> [bytes]");
        std::process::exit(2);
    }

    let dir = tempfile::tempdir().unwrap();

    // Written in pieces, so making the test file does not itself dominate.
    let path = dir.path().join("large.bin");
    {
        let mut file = std::fs::File::create(&path).unwrap();
        let mut x: u32 = 1;
        let mut block = vec![0u8; 1 << 20];
        for _ in 0..(size / (1 << 20)) {
            for byte in block.iter_mut() {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *byte = x as u8;
            }
            file.write_all(&block).unwrap();
        }
    }
    let mut store = Store::open(&dir.path().join("store"), ChunkKey::generate()).unwrap();
    // Always stored, since reading needs something to read. Measured only in
    // `store` mode, where it is the operation under test.
    store.put_file("large.bin", &path).unwrap();

    let before = anon_kib();

    // Held until after the measurement. Dropping it first would return the
    // memory to the allocator and make a 256 MiB buffer look free, which is how
    // the first version of this measurement managed to report that reading a
    // whole file into memory cost nothing.
    let held: Option<Vec<u8>> = match mode.as_str() {
        "store" => None,
        "read" => {
            let read = store.read_file("large.bin").unwrap();
            assert_eq!(read.len(), size);
            Some(read)
        }
        "stream" => {
            let mut sink = std::io::sink();
            let streamed = store.read_file_into("large.bin", &mut sink).unwrap();
            assert_eq!(streamed as usize, size);
            None
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    };

    println!(
        "{:<8} file {:>5} MiB   heap {:>5} MiB   mapped {:>5} MiB   (heap before: {} MiB)",
        mode,
        size / (1024 * 1024),
        anon_kib() / 1024,
        file_kib() / 1024,
        before / 1024
    );
    drop(held);
}
