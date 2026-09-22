//! Bridging the network to the engine.
//!
//! [`qurb_engine::ContentSource`] is synchronous, because the engine is: it
//! chunks, hashes and writes files without an async runtime in sight. The
//! network is asynchronous. This is where the two meet.

use crate::client::PeerClient;
use qurb_engine::ContentSource;
use qurb_storage::Store;

/// Tell a peer about content it made that this device is holding.
///
/// `Got` is sent when a transfer finishes, which covers everything from now on
/// and nothing from before. A device that received a file last week holds it
/// just as truly and never said so — and since a file both devices already
/// have is never transferred again, the device that made it would go on
/// counting it as delivered nowhere for the life of the file.
///
/// So holdings are reported too, a few per pass, each one only once. Returns
/// how many were reported.
///
/// Best effort throughout. This corrects a number on someone's screen; it must
/// never be the reason a sync reports failure.
pub async fn report_holdings(
    client: &PeerClient,
    store: &Store,
    peer: &qurb_sync::DeviceId,
    limit: usize,
) -> usize {
    let holdings = match store.db().unreported_to(peer, limit) {
        Ok(holdings) => holdings,
        Err(e) => {
            tracing::debug!(error = %e, "could not work out what to report");
            return 0;
        }
    };

    let mut told = 0;
    for content in holdings {
        if client.got(*content.as_bytes()).await.is_err() {
            // The connection is probably gone. Stop rather than grind through
            // the rest of the list failing.
            break;
        }
        if let Err(e) = store.db().note_reported(peer, &content) {
            tracing::debug!(error = %e, "could not record what was reported");
            break;
        }
        told += 1;
    }
    told
}

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

    /// Tell the peer we now hold it.
    ///
    /// Failures are swallowed deliberately. This is a courtesy to the other
    /// end, and a sync that succeeded must not be reported as failed because
    /// the closing remark did not get through — the file is here either way.
    /// The peer will learn of it on the next exchange.
    fn received(&mut self, content: &[u8; 32]) {
        let content = *content;
        let outcome = tokio::task::block_in_place(|| {
            self.runtime.block_on(self.client.got(content))
        });
        if let Err(e) = outcome {
            tracing::debug!(error = %e, "could not tell the peer the content arrived");
        }
    }

    /// The streaming form, which is the one that actually runs.
    ///
    /// Without this the trait's default applies, and the default buffers the
    /// whole file — so the network path, the one a phone uses to receive a
    /// large file, would hold it all in memory no matter how carefully the
    /// layers underneath stream. That is the failure
    /// [decision 0018] names as the easy one to make here, and it was made:
    /// this override was missing for a release, and nothing failed, because
    /// buffering is correct and only expensive.
    ///
    /// [decision 0018]: ../../../docs/decisions/0018-file-contents-never-cross-the-ffi.md
    fn fetch_into(
        &mut self,
        hash: &[u8; 32],
        _size: u64,
        out: &mut dyn std::io::Write,
    ) -> qurb_engine::Result<u64> {
        let result = tokio::task::block_in_place(|| {
            self.runtime.block_on(async {
                // Awaited here rather than spawned. `out` is borrowed from this
                // blocked thread, so the future is deliberately not `Send` and
                // the compiler will refuse any attempt to move it elsewhere.
                let mut sink = Adapter(out);
                self.client.fetch_content_into(self.local, *hash, &mut sink).await
            })
        });

        result.map_err(|e| qurb_engine::Error::Source { detail: e.to_string() })
    }
}

/// Lets a `&mut dyn Write` satisfy an `impl Write` parameter.
struct Adapter<'a>(&'a mut dyn std::io::Write);

impl std::io::Write for Adapter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

