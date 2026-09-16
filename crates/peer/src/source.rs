//! Bridging the network to the engine.
//!
//! [`qurb_engine::ContentSource`] is synchronous, because the engine is: it
//! chunks, hashes and writes files without an async runtime in sight. The
//! network is asynchronous. This is where the two meet.

use crate::client::PeerClient;
use qurb_engine::ContentSource;
use qurb_storage::Store;

/// Serves the engine's content requests from a connected peer.
///
/// Holds the local store as well as the connection, because most of what a
/// plan asks for is already here: chunks shared with another file, or with an
/// earlier version of this one. Those are read from disk and never requested.
pub struct NetworkSource<'a> {
    client: &'a PeerClient,
    local: &'a Store,
    runtime: tokio::runtime::Handle,
}

impl<'a> NetworkSource<'a> {
    /// # Runtime requirement
    ///
    /// Must be constructed from inside a multi-threaded Tokio runtime. Each
    /// fetch blocks the calling thread on network I/O through
    /// [`tokio::task::block_in_place`], which the current-thread scheduler
    /// cannot do — it would deadlock rather than fail, which is why this
    /// captures the handle up front and panics here instead.
    pub fn new(client: &'a PeerClient, local: &'a Store) -> Self {
        Self { client, local, runtime: tokio::runtime::Handle::current() }
    }
}

impl ContentSource for NetworkSource<'_> {
    fn fetch(&mut self, hash: &[u8; 32], size: u64) -> qurb_engine::Result<Vec<u8>> {
        let result = tokio::task::block_in_place(|| {
            self.runtime
                .block_on(self.client.fetch_content(self.local, *hash, size))
        });

        result.map_err(|e| qurb_engine::Error::Source { detail: e.to_string() })
    }
}
