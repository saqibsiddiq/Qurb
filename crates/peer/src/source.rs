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
/// And deliveries taken before, sent again. `offered` is the peer's tree. A
/// send of content this device has already taken is skipped, rightly, since
/// a delivery is taken once -- but the sender is then never told, and keeps it
/// waiting for good, announcing news on every pass. That happened on
/// 2026-10-05: a phone cleared and set up again sent a 1.7 GB video the laptop
/// had taken from it earlier that day, and two days later still held it as
/// undelivered. Told now, as it would have been on delivery.
///
/// Best effort throughout. This corrects a number on someone's screen; it must
/// never be the reason a sync reports failure.
pub async fn report_holdings(
    client: &PeerClient,
    store: &Store,
    peer: &qurb_sync::DeviceId,
    offered: &[qurb_sync::FileVersion],
    limit: usize,
) -> usize {
    let mut holdings = match store.db().unreported_to(peer, limit) {
        Ok(holdings) => holdings,
        Err(e) => {
            tracing::debug!(error = %e, "could not work out what to report");
            return 0;
        }
    };
    for version in offered.iter().filter(|v| v.area == qurb_sync::Area::Sent) {
        if holdings.len() >= limit {
            break;
        }
        let Some(hash) = version.content.hash() else { continue };
        let content = blake3::Hash::from(*hash);
        if !holdings.contains(&content)
            && matches!(store.vault_knows(&content), Ok(true))
            && matches!(store.db().was_reported(peer, &content), Ok(false))
        {
            holdings.push(content);
        }
    }

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

/// Ask a peer whether it holds what it is recorded as holding: content of
/// files freed here, a few per pass and each once (decision 0055).
///
/// A copy is recorded on the word of the device that made the file, or of a
/// report, and nothing took it back when that device freed its own. So a
/// phone listed 7 files as on the laptop that the laptop had freed long
/// before, and that no device had any more. A copy denied is marked as one
/// this device cannot ask for, and the file is shown as on no device. A copy
/// marked so -- by removing the device, which pairing it again did not undo
/// -- and confirmed counts again.
///
/// Best effort, like [`report_holdings`]: it corrects what a screen says, and
/// never fails a sync. Returns how many were asked.
pub async fn check_holders(
    client: &PeerClient,
    store: &Store,
    peer: &qurb_sync::DeviceId,
    limit: usize,
) -> usize {
    let claims = match store.db().unconfirmed_holdings(peer, limit) {
        Ok(claims) => claims,
        Err(e) => {
            tracing::debug!(error = %e, "could not work out what to ask");
            return 0;
        }
    };
    let mut asked = 0;
    for content in claims {
        let recorded = match client.manifest(*content.as_bytes()).await {
            Ok(Some(_)) => store.db().note_held(peer, &content),
            Ok(None) => store.db().note_not_held(peer, &content),
            // The connection is probably gone; the rest wait for next time.
            Err(_) => break,
        };
        if let Err(e) = recorded {
            tracing::debug!(error = %e, "could not record what the peer said");
            break;
        }
        asked += 1;
    }
    asked
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
    /// The device at the other end, where the caller knows it.
    peer: Option<qurb_sync::DeviceId>,
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
        Self { client, local, runtime: tokio::runtime::Handle::current(), peer: None }
    }

    /// Name the device at the other end, so that its saying it does not hold
    /// something is recorded against it (decision 0055).
    pub fn for_peer(mut self, device: Option<qurb_sync::DeviceId>) -> Self {
        self.peer = device;
        self
    }
}

impl ContentSource for NetworkSource<'_> {
    fn fetch(&mut self, hash: &[u8; 32], size: u64) -> qurb_engine::Result<Vec<u8>> {
        let result = tokio::task::block_in_place(|| {
            self.runtime
                .block_on(self.client.fetch_content(self.local, *hash, size))
        });

        result.map_err(for_the_engine)
    }

    /// Record against the device at the other end that it does not hold this,
    /// when the caller said which device that is.
    fn not_held(&mut self, content: &[u8; 32]) {
        if let Some(peer) = &self.peer {
            if let Err(e) = self.local.db().note_not_held(peer, &blake3::Hash::from(*content)) {
                tracing::debug!(error = %e, "could not record that the peer does not hold it");
            }
        }
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

        result.map_err(for_the_engine)
    }

    /// Carries on from what an earlier, interrupted attempt left in
    /// `partial`, rather than starting again.
    fn resume_into(
        &mut self,
        hash: &[u8; 32],
        _size: u64,
        partial: &std::path::Path,
        progress: &mut dyn qurb_engine::Progress,
    ) -> qurb_engine::Result<u64> {
        self.resume(hash, partial, progress).map_err(for_the_engine)
    }
}

impl NetworkSource<'_> {
    /// The resuming form of [`fetch_into`](ContentSource::fetch_into); see
    /// [`PeerClient::resume_point`].
    fn resume(
        &mut self,
        hash: &[u8; 32],
        partial: &std::path::Path,
        progress: &mut dyn qurb_engine::Progress,
    ) -> crate::error::Result<u64> {
        tokio::task::block_in_place(|| {
            self.runtime.block_on(async {
                let resume = self.client.resume_point(*hash, partial).await?;
                if resume.kept() > 0 {
                    tracing::info!(kept = resume.kept(), "carrying on from an earlier attempt");
                    progress.advanced(resume.kept());
                }
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(partial)
                    .map_err(|e| crate::error::Error::Io { path: partial.to_path_buf(), source: e })?;
                let mut sink = Counted { out: &mut file, progress };
                self.client.fetch_rest_into(self.local, *hash, resume, &mut sink).await
            })
        })
    }
}

/// What a failed fetch tells the engine.
///
/// "It does not hold that" is passed on as such, and the engine records the
/// file as being elsewhere. Passed on as a failure, it was asked again on
/// every sync and failed every time: a phone listed *Didn't finish* for 18
/// files nobody had any more (2026-10-07). Everything else stays a failure,
/// to be tried again.
fn for_the_engine(e: crate::error::Error) -> qurb_engine::Error {
    match e {
        crate::error::Error::NotHeld { hash } => qurb_engine::Error::ContentUnavailable { hash },
        e => qurb_engine::Error::Source { detail: e.to_string() },
    }
}

/// Reports what passes through to `out`.
struct Counted<'a> {
    out: &'a mut dyn std::io::Write,
    progress: &'a mut dyn qurb_engine::Progress,
}

impl std::io::Write for Counted<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.out.write(buf)?;
        self.progress.advanced(n as u64);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
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

