//! Writes files into a store, forever, until something kills it.
//!
//! Exists so the crash tests can kill a *real* process mid-write rather than
//! simulating what a crash might leave behind. Simulations test the states we
//! thought of; SIGKILL tests the ones we did not.
//!
//! ```bash
//! crash_writer <store-dir> [file-count]
//! ```

use qurb_storage::{ChunkKey, Store};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("usage: crash_writer <store-dir> [count]");
    let count: usize = args.get(2).map_or(10_000, |s| s.parse().unwrap());

    // A fixed key, so the test can reopen what was written.
    let mut store =
        Store::open(std::path::Path::new(dir), ChunkKey::from_bytes([77; 32])).unwrap();

    for i in 0..count {
        let size = 4_096 + (i * 7919) % 200_000;
        let mut data = Vec::with_capacity(size);
        let mut x = (i as u32) | 1;
        for _ in 0..size {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            data.push(x as u8);
        }

        store.put_bytes(&format!("file{i:05}.bin"), &data, i as i64).unwrap();

        // Announce completion *after* the write, so the test knows exactly how
        // many files were fully committed before the kill.
        println!("{i}");
        std::io::stdout().flush().ok();
    }
}
