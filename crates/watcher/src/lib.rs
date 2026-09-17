//! Filesystem watching for qurb.
//!
//! Reports the changes a *user* made, not the events a filesystem emitted.
//! Those are very different things: one editor save produces a dozen events
//! across two paths, and a large copy produces a write event every few
//! milliseconds for as long as it runs.
//!
//! ```text
//!   native events        raw, noisy, per-syscall
//!         │
//!         ├─ ignore      drop the store's own writes, VCS metadata, scratch files
//!         ├─ debounce    collapse bursts; hold until a path goes quiet
//!         ├─ stabilise   re-stat before releasing, so nothing is read mid-write
//!         └─ emit        settled changes, or a demand to rescan
//! ```
//!
//! Three failure modes shape the design, all of them silent if unhandled:
//!
//! - **Reading a file that is still being written** stores a torn copy, and can
//!   raise SIGBUS in the storage layer's memory-mapped read. Handled by the
//!   stability re-stat before release.
//! - **Watching our own chunk store** feeds every write back as an event.
//!   Handled by [`IgnoreRules::with_store_dir`].
//! - **Kernel queue overflow** during a large operation drops events, so the
//!   stream stops describing reality. Handled by reporting
//!   [`Event::RescanRequired`] rather than continuing as if nothing happened.
//!
//! Changes are delivered **at least once**, not exactly once — see
//! [`Watcher`] for why, and what that requires of the consumer.
//!
//! See ../../docs/CODEBASE.md for where this sits in the system.

pub mod debounce;
pub mod error;
pub mod ignore;
pub mod scan;
pub mod watcher;

pub use debounce::{Change, ChangeKind, DebounceConfig, Debouncer};
pub use error::{Error, Result};
pub use ignore::IgnoreRules;
pub use scan::{
    case_collisions, decomposes_unicode, is_case_insensitive, logical_path, normalization_collisions,
    normalize, scan, ScanEntry,
};
pub use watcher::{Event, Watcher};
