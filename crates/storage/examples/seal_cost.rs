//! Where the time goes when storing a chunk.
use qurb_storage::{format, ChunkKey};
use std::time::Instant;

fn main() {
    let key = ChunkKey::generate();
    let mut x: u32 = 12345;
    let incompressible: Vec<u8> = (0..(64usize << 20))
        .map(|_| { x ^= x << 13; x ^= x >> 17; x ^= x << 5; x as u8 })
        .collect();
    let compressible = vec![b'a'; 64 << 20];

    for (label, data) in [("incompressible", &incompressible), ("compressible", &compressible)] {
        for chunk in [8usize << 10, 45 << 10, 512 << 10] {
            let slices: Vec<&[u8]> = data.chunks(chunk).collect();
            let total: usize = slices.iter().map(|s| s.len()).sum();

            let t = Instant::now();
            let mut out = 0usize;
            for s in &slices {
                out += format::seal(&key, s).unwrap().len();
            }
            let secs = t.elapsed().as_secs_f64();
            println!(
                "{label:<15} {:>4} KiB chunks  seal {:>5.0} MiB/s   stored {:.0}%",
                chunk >> 10,
                total as f64 / (1024.0 * 1024.0) / secs,
                out as f64 / total as f64 * 100.0
            );
        }
    }
}
