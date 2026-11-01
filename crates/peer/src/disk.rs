//! How big the disk holding a folder is, and how much of it is free: told to
//! the owner's phone with the computer's storage ([`Request::Storage`]).
//!
//! The way to ask lives with the computer's program (`qurb_cli::disk`), which
//! depends on this crate, so the program hands it over once at start
//! ([`set`]). Not handed over -- a test, a phone -- the answer is `(0, 0)`,
//! which the phone shows as a disk it does not know the size of.
//!
//! [`Request::Storage`]: crate::wire::Request::Storage

use std::path::Path;
use std::sync::OnceLock;

static ASK: OnceLock<fn(&Path) -> (u64, u64)> = OnceLock::new();

/// How this program asks; the first one given is kept.
pub fn set(ask: fn(&Path) -> (u64, u64)) {
    let _ = ASK.set(ask);
}

/// Total and available bytes on the disk holding `dir`, or `(0, 0)`.
pub fn space(dir: &Path) -> (u64, u64) {
    ASK.get().map(|ask| ask(dir)).unwrap_or((0, 0))
}
