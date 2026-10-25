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
    let mut holdings: Vec<(blake3::Hash, Option<&qurb_sync::FileVersion>)> =
        match store.db().unreported_to(peer, limit) {
            Ok(holdings) => holdings.into_iter().map(|content| (content, None)).collect(),
            Err(e) => {
                tracing::debug!(error = %e, "could not work out what to report");
                return 0;
            }
        };
    // Sends this device took and has not told the sender about, each by
    // itself (decision 0059): told about the bytes once, a sender of the same
    // file again would wait for good.
    for version in offered.iter().filter(|v| v.area == qurb_sync::Area::Sent) {
        if holdings.len() >= limit {
            break;
        }
        let Some(hash) = version.content.hash() else { continue };
        if matches!(store.db().unacknowledged(version, peer), Ok(true)) {
            holdings.push((blake3::Hash::from(*hash), Some(version)));
        }
    }

    let mut told = 0;
    for (content, send) in holdings {
        if client.got(*content.as_bytes()).await.is_err() {
            // The connection is probably gone. Stop rather than grind through
            // the rest of the list failing.
            break;
        }
        let recorded = store
            .db()
            .note_reported(peer, &content)
            .and_then(|()| send.map_or(Ok(()), |version| store.db().acknowledge(version)));
        if let Err(e) = recorded {
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

/// Ask `peer`, which keeps this device's vault, whether it holds files of
/// that vault this device has no record of it holding (decision 0036).
///
/// The record is made from the peer's `Got` as it collects. One lost on the
/// way -- the connection dropped between fetching and telling -- left the
/// file counted here as still to go for good: 130 of the S23's WhatsApp
/// files on 2026-10-15, kept on the laptop for hours, whose 54 MB made every
/// pass on the phone a long one. A few each pass, asked at once; a yes is
/// recorded, a no left alone, being most often a file not collected yet,
/// which its own `Got` will tell. Returns how many were found held.
pub async fn confirm_kept(client: &PeerClient, store: &Store, peer: &qurb_sync::DeviceId, limit: usize) -> usize {
    let unsure = match store.db().kept_unconfirmed(peer, limit) {
        Ok(unsure) => unsure,
        Err(e) => {
            tracing::debug!(error = %e, "could not work out what to ask");
            return 0;
        }
    };
    let answers = futures_util::future::join_all(unsure.iter().map(|content| client.manifest(*content.as_bytes()))).await;
    let mut held = 0;
    for (content, answer) in unsure.iter().zip(answers) {
        if let Ok(Some(_)) = answer {
            match store.db().note_held(peer, content) {
                Ok(()) => held += 1,
                Err(e) => tracing::debug!(error = %e, "could not record what the peer said"),
            }
        }
    }
    held
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
    /// Small files the plan will ask for, in order, not fetched yet; and
    /// those fetched ahead, waiting to be asked for. See [`AHEAD`].
    ahead: std::collections::VecDeque<([u8; 32], u64)>,
    ready: std::collections::HashMap<[u8; 32], Vec<u8>>,
    /// What arrived, not yet told to the other end: told in batches.
    arrived: Vec<[u8; 32]>,
    /// The small files of this plan, whose arrival waits for a batch; any
    /// other is told at once.
    small: std::collections::HashSet<[u8; 32]>,
}

/// Files fetched at once, when a plan asks for small ones: a round trip to a
/// phone with its screen off is long, and one at a time a laptop collected 4
/// to 8 a second of a WhatsApp backup that had 36,010 (decision 0062).
const AHEAD: usize = 32;
/// Files no larger than this are fetched ahead, held in memory until asked
/// for; larger ones stream to disk as before.
const SMALL: u64 = 4 << 20;
/// At most this much held in memory at once by fetching ahead.
const AHEAD_BYTES: u64 = 64 << 20;

impl<'a> NetworkSource<'a> {
    /// # Runtime requirement
    ///
    /// Must be constructed from inside a multi-threaded Tokio runtime. Each
    /// fetch blocks the calling thread on network I/O through
    /// [`tokio::task::block_in_place`], which the current-thread scheduler
    /// cannot do — it would deadlock rather than fail, which is why this
    /// captures the handle up front and panics here instead.
    pub fn new(client: &'a PeerClient, local: &'a Store) -> Self {
        Self {
            client,
            local,
            runtime: tokio::runtime::Handle::current(),
            peer: None,
            ahead: Default::default(),
            ready: Default::default(),
            arrived: Vec::new(),
            small: Default::default(),
        }
    }

    /// Name the device at the other end, so that its saying it does not hold
    /// something is recorded against it (decision 0055).
    pub fn for_peer(mut self, device: Option<qurb_sync::DeviceId>) -> Self {
        self.peer = device;
        self
    }
}

impl ContentSource for NetworkSource<'_> {
    /// The connection has ended, from either side or by going quiet.
    fn gone(&self) -> bool {
        self.client.is_closed()
    }

    fn fetch(&mut self, hash: &[u8; 32], size: u64) -> qurb_engine::Result<Vec<u8>> {
        if let Some(bytes) = self.fetched_ahead(hash) {
            return Ok(bytes);
        }
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
    ///
    /// Put off and told [`AHEAD`] at a time, all at once, rather than a round
    /// trip after every file; the rest when the plan is done.
    /// Told in batches of small files, which a round trip each would slow;
    /// a larger one at once, so that a phone backing up videos counts them
    /// down as each arrives rather than all at the end (the owner's list of
    /// 2026-10-25: thirty of them said *29 more* for three minutes).
    fn received(&mut self, content: &[u8; 32]) {
        self.arrived.push(*content);
        if self.arrived.len() >= AHEAD || !self.small.contains(content) {
            self.tell_arrived();
        }
    }

    fn coming(&mut self, contents: &[([u8; 32], u64)]) {
        self.ahead = contents.iter().filter(|(_, size)| *size <= SMALL).copied().collect();
        self.small = self.ahead.iter().map(|(hash, _)| *hash).collect();
    }

    fn finished(&mut self) {
        self.tell_arrived();
        self.ahead.clear();
        self.ready.clear();
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
        if let Some(bytes) = self.fetched_ahead(hash) {
            out.write_all(&bytes).map_err(|e| qurb_engine::Error::Io { path: "the destination".into(), source: e })?;
            return Ok(bytes.len() as u64);
        }
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
        if let Some(bytes) = self.fetched_ahead(hash) {
            let io = |e| qurb_engine::Error::Io { path: partial.to_path_buf(), source: e };
            std::fs::write(partial, &bytes).map_err(io)?;
            progress.advanced(bytes.len() as u64);
            return Ok(bytes.len() as u64);
        }
        self.resume(hash, partial, progress).map_err(for_the_engine)
    }
}

impl NetworkSource<'_> {
    /// `hash`, if it is one of the small files the plan said it would ask
    /// for: fetched now with the next ones, [`AHEAD`] at a time, unless it
    /// already was. `None` for anything else, and for one whose fetch ahead
    /// failed, which is then fetched on its own as before -- so a failure is
    /// reported as it always was.
    fn fetched_ahead(&mut self, hash: &[u8; 32]) -> Option<Vec<u8>> {
        if let Some(bytes) = self.ready.remove(hash) {
            return Some(bytes);
        }
        let at = self.ahead.iter().position(|(h, _)| h == hash)?;
        // Those before it were asked for already, or were not needed.
        self.ahead.drain(..at);
        let mut batch = Vec::new();
        let mut bytes = 0;
        while let Some(&(next, size)) = self.ahead.front() {
            if batch.len() >= AHEAD || (bytes + size > AHEAD_BYTES && !batch.is_empty()) {
                break;
            }
            self.ahead.pop_front();
            // Content this device has already is not asked for at all.
            if next != *hash && self.local.can_read_content(&blake3::Hash::from(next)).unwrap_or(false) {
                continue;
            }
            bytes += size;
            batch.push((next, size));
        }
        self.tell_arrived();
        let (client, local) = (self.client, self.local);
        let fetched = tokio::task::block_in_place(|| {
            self.runtime.block_on(futures_util::future::join_all(batch.iter().map(|&(h, size)| async move {
                (h, client.fetch_content(local, h, size).await)
            })))
        });
        for (h, result) in fetched {
            if let Ok(bytes) = result {
                self.ready.insert(h, bytes);
            }
        }
        self.ready.remove(hash)
    }

    /// Tell the other end, all at once, what arrived since it was last told.
    fn tell_arrived(&mut self) {
        if self.arrived.is_empty() {
            return;
        }
        let arrived = std::mem::take(&mut self.arrived);
        let client = self.client;
        let outcomes = tokio::task::block_in_place(|| {
            self.runtime.block_on(futures_util::future::join_all(arrived.iter().map(|&content| client.got(content))))
        });
        if let Some(Err(e)) = outcomes.into_iter().find(|o| o.is_err()) {
            tracing::debug!(error = %e, "could not tell the peer what arrived");
        }
    }

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

