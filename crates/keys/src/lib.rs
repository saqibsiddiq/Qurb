//! The root secret, the keys derived from it, and the recovery phrase.
//!
//! ```text
//!   MasterKey ──HKDF──► chunk encryption
//!        │              device identity
//!        │              metadata authentication
//!        │
//!        └──BIP-39──►   24 words on paper
//! ```
//!
//! # What makes this different from ordinary key handling
//!
//! The servers hold nothing. There is no reset, no support ticket, no escrow.
//! If a user loses the master key and their phrase, the data is gone — not by
//! policy but as a fact about the mathematics, because nothing else can decrypt
//! their chunks.
//!
//! Every design choice here follows from that. The phrase carries a checksum so
//! a mistyped word is caught rather than silently producing a different key and
//! an account that looks empty. [`Vault::restore`] refuses to overwrite an
//! existing key, because doing so would orphan every chunk already stored.
//! Keys are redacted in `Debug` and wiped on drop. [`Opened`] hands over a
//! phrase exactly once, at the only moment it can be produced.
//!
//! # Known gaps
//!
//! The master key is stored in a file readable only by its owner. That is not
//! the same as being protected: anyone who can read the disk can read the key.
//! The operating system keystore is the real answer and is not built. See
//! [`vault`] for the full statement.
//!
//! See ../../docs/CODEBASE.md for where this sits in the system.

pub mod error;
pub mod master;
pub mod phrase;
pub mod vault;

pub use error::{Error, Result};
pub use master::{DerivedKey, MasterKey, Purpose};
pub use phrase::RecoveryPhrase;
pub use vault::{Opened, Vault};
