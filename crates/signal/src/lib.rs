//! Finding the other device.
//!
//! Two devices behind home routers have no address the other can dial, and hole
//! punching needs both to send at roughly the same moment. This is the service
//! that makes both possible: it remembers where each device says it is, and
//! tells two of them to punch at once.
//!
//! ```text
//!   announce   I am here, at these addresses
//!   connect    I want to reach this member of my group
//!   punch      both sides told at the same moment
//! ```
//!
//! # What it is not allowed to know
//!
//! No filenames, no chunks, no keys — nothing from the data plane passes through
//! here. It does not even learn whose devices these are: the identifiers a
//! device announces under are derived from a master key the server does not
//! hold, so it matches opaque values and forwards addresses.
//!
//! It does learn that some set of addresses belong together, and it sees IP
//! addresses. That is inherent to being a rendezvous point, not a shortcoming of
//! this implementation, and it is why the data plane never goes near it.
//!
//! See ../../docs/decisions/0016-what-signalling-learns.md.

pub mod client;
pub mod error;
pub mod message;
pub mod rendezvous;
pub mod server;
pub mod tls;
pub mod wake;

/// Waking a device through Firebase. Needs the `push` feature and credentials.
#[cfg(feature = "push")]
pub mod fcm;

pub use client::SignalClient;
pub use error::{Error, Result};
pub use message::{Endpoints, FromClient, FromServer, Presence};
pub use rendezvous::{GroupId, MemberId};
pub use server::{Limits, SignalServer};
pub use tls::Certificate;
