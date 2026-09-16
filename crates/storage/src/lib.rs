//! Local storage for qurb.
//!
//! Files are split into content-defined chunks, each chunk is compressed,
//! encrypted, and stored under a path derived from the hash of its *plaintext*.
//! A SQLite index records which chunks make up which file, and how many files
//! reference each chunk.
//!
//! ```text
//!   put_file
//!       │
//!       ├─ chunker  split at content-defined boundaries, hash each piece
//!       ├─ db       already have this hash? then no payload is written
//!       ├─ format   zstd, then XChaCha20-Poly1305
//!       ├─ cas      write to chunks/<2 hex>/<full hex>
//!       └─ db       record path -> ordered chunk list (refcounts follow)
//! ```
//!
//! Start with [`Store`]. The layers beneath it are public so that the sync
//! engine can work at whatever level it needs — chunk by chunk when talking to
//! a peer, whole files when talking to the filesystem.
//!
//! See ../../docs/CODEBASE.md for how this fits into the wider system.

pub mod cas;
pub mod chunker;
pub mod db;
pub mod error;
pub mod format;
pub mod gc;
pub mod store;

pub use chunker::{ChunkRef, Manifest, AVG_CHUNK, MAX_CHUNK, MIN_CHUNK};
pub use error::{Error, Result};
pub use format::ChunkKey;
pub use gc::GcStats;
pub use store::{PutStats, Store, VerifyReport};
