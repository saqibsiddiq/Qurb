//! Deciding what changed and what to do about it.
//!
//! This crate is the part of the sync engine that has to be *right*. It holds
//! no data, touches no disk, and reads no clock — it takes two views of the
//! world and says what should happen. That makes it exhaustively testable,
//! which matters, because the failure mode here is losing a user's work.
//!
//! ```text
//!   local state ─┐
//!                ├─ reconcile ─→ actions: adopt, offer, conflict, resurrect
//!  remote state ─┘
//! ```
//!
//! Three ideas, in order of importance:
//!
//! 1. **Version vectors, not timestamps.** [`clock`] tracks which changes each
//!    version has seen. Device clocks disagree; causality does not.
//! 2. **Concurrent means conflict.** When neither version has seen the other,
//!    no amount of comparing decides which the user meant. [`resolve`] keeps
//!    both rather than guessing.
//! 3. **Never silently discard an edit.** The one promise the system makes
//!    about conflicts, and the reason a concurrent edit beats a concurrent
//!    delete.
//!
//! See ../../docs/decisions/0005-conflict-resolution.md and
//! ../../docs/decisions/0009-conflict-edge-cases.md.

pub mod clock;
pub mod device;
pub mod reconcile;
pub mod resolve;
pub mod version;

pub use clock::{Causality, VersionVector};
pub use device::DeviceId;
pub use reconcile::{reconcile, Action};
pub use resolve::{conflict_path, resolve, Outcome, Resolution, Side};
pub use version::{Content, FileVersion};
