//! Talking to another device.
//!
//! [`qurb_sync`] decides what two devices should do about their differences and
//! [`qurb_engine`] carries it out locally. This crate is how the two devices
//! reach each other in the first place.
//!
//! ```text
//!   PeerServer   serves a store, read-only, to peers it recognises
//!   PeerClient   asks: what do you have, what is this file made of, send me a chunk
//!   NetworkSource  plugs the client into the engine's ContentSource seam
//! ```
//!
//! # Transfer is incremental
//!
//! Fetching a file asks for its *manifest* — the list of chunks it is made
//! of — and then only for the chunks this device does not already hold. A large
//! file with a small edit shares nearly all its chunks with the copy already
//! here, so nearly nothing crosses the wire. This is the point of the content-
//! defined chunking in [`qurb_storage`], finally realised over a network.
//!
//! # Identity
//!
//! Both ends present a self-signed certificate and check the other against a
//! fingerprint given in advance, and both verify the handshake signature, so a
//! peer must hold the matching private key rather than merely replay a public
//! certificate.
//!
//! **Pairing is not built.** Deciding which fingerprint to expect — the QR-code
//! exchange in the architecture — is Phase 3. Until then the caller supplies it,
//! which is why [`PeerClient::connect`] demands a fingerprint rather than
//! offering a way to skip one.
//!
//! See ../../docs/CODEBASE.md for where this sits in the system.

pub mod base32;
pub mod client;
pub mod connect;
pub mod error;
pub mod identity;
pub mod local;
pub mod nat;
pub mod pairing;
pub mod server;
pub mod source;
pub mod tls;
pub mod wire;

pub use client::PeerClient;
pub use connect::{Connector, Finding};
pub use error::{Error, Result};
pub use identity::{Fingerprint, Identity};
pub use local::Beacon;
pub use nat::{NatBehaviour, Reflexive};
pub use pairing::{accept, Invite, Paired, PairingHost};
pub use server::{trusted_fingerprints, Generation, PeerServer, ServerStats};
pub use source::{report_holdings, NetworkSource};
pub use wire::{Request, Response};
