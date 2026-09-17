//! The fallback path.
//!
//! Hole punching fails on some networks — a symmetric NAT at both ends, or UDP
//! blocked outright — and those devices still have to sync. A relay forwards
//! bytes between them.
//!
//! ```text
//!   device ──TCP──► relay ◄──TCP── device
//!            a QUIC session runs inside, end to end
//! ```
//!
//! # Why it can be trusted with traffic it is not trusted to read
//!
//! The relay carries **datagrams**, not messages. An ordinary QUIC session runs
//! inside — same pinned certificates, same handshake, same encryption — so the
//! relay sees ciphertext addressed to an identifier it cannot link to a person.
//! It can pass bytes on or drop them, and nothing else. Dropping is a denial of
//! service, which is true of every router between any two computers.
//!
//! The alternative would have been forwarding application messages, which means
//! the relay handling data it could tamper with, and inventing a second
//! encryption layer to stop it. That layer already exists; this reuses it.
//!
//! # Why TCP
//!
//! The data plane is QUIC over UDP because it is faster. The relay is TCP
//! because it exists for networks where UDP does not work, and it belongs on
//! port 443 behind TLS for the same reason. A fallback that needs the thing
//! being fallen back from is not a fallback.
//!
//! See ../../docs/decisions/0017-relay.md.

pub mod error;
pub mod frame;
pub mod server;
pub mod socket;

pub use error::{Error, Result};
pub use frame::{Frame, RelayId};
pub use server::{RelayServer, RelayStats};
pub use socket::{endpoint_over, RelaySocket};
