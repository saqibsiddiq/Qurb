//! Serving a store to peers.
//!
//! Read-only by design. A peer can ask what this device has and can ask for its
//! bytes; it cannot tell this device to change anything. Incoming versions are
//! adopted by the *local* engine after it has run them through reconciliation,
//! so nothing a peer says is applied without this side deciding it should be.

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
use crate::pairing::fingerprint_of;
use crate::tls;
use crate::wire::{Request, Response, MAX_MESSAGE};
use qurb_storage::Store;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct PeerServer {
    endpoint: quinn::Endpoint,
    stats: Arc<ServerStats>,
}

/// How far this device's state has got.
///
/// A counter rather than a flag, because a flag can be missed: a peer told
/// "something changed" has no way to tell a notification it already acted on
/// from a new one. With a counter it says what it last saw, and the answer is
/// immediate if anything has happened since.
///
/// It need not survive a restart. A peer holding a number from before will see
/// one that does not match, which is exactly the right conclusion — the device
/// it was watching has been away, and its state may well have moved.
#[derive(Debug, Default)]
pub struct Generation {
    value: std::sync::atomic::AtomicU64,
    changed: tokio::sync::Notify,
}

impl Generation {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn current(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Say that something changed, and wake everyone waiting.
    pub fn bump(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
        self.changed.notify_waiters();
    }

    /// Wait until the value differs from `since`, or until `timeout`.
    ///
    /// Any difference counts, not only an increase, so a restarted peer is told
    /// to look rather than waiting for a counter that began again to overtake
    /// one it remembers.
    async fn wait_past(&self, since: u64, timeout: Duration) -> u64 {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = self.current();
            if now != since {
                return now;
            }
            // Registered before the check below, so a change between the two
            // wakes this rather than being missed.
            let notified = self.changed.notified();
            if self.current() != since {
                return self.current();
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.current();
            }
        }
    }
}

/// How long to hold a request open before answering anyway.
///
/// Long enough that an idle pair exchanges almost nothing; short enough that a
/// connection which has quietly died is noticed rather than waited on for ever.
const CHANGES_TIMEOUT: Duration = Duration::from_secs(90);

/// What this server has done since it started.
///
/// Chunks served is the number that matters: it is the wire cost of a sync, and
/// the measure of whether content-defined chunking is actually saving anything.
#[derive(Debug, Default)]
pub struct ServerStats {
    pub trees_served: AtomicU64,
    pub manifests_served: AtomicU64,
    pub chunks_served: AtomicU64,
    pub bytes_served: AtomicU64,
}

impl ServerStats {
    pub fn chunks(&self) -> u64 {
        self.chunks_served.load(Ordering::Relaxed)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes_served.load(Ordering::Relaxed)
    }
}

impl PeerServer {
    /// Listen on `addr`, accepting only the peers in `allowed`.
    pub fn bind(addr: SocketAddr, identity: &Identity, allowed: &crate::tls::TrustList) -> Result<Self> {
        let config = tls::server_config(identity, allowed)?;
        let endpoint = quinn::Endpoint::server(config, addr)
            .map_err(|e| Error::Io { path: addr.to_string().into(), source: e })?;
        Ok(Self { endpoint, stats: Arc::new(ServerStats::default()) })
    }

    /// Counters, shareable with whoever wants to watch.
    pub fn stats(&self) -> Arc<ServerStats> {
        Arc::clone(&self.stats)
    }

    /// Listen on `addr`, accepting the devices this store has paired with.
    ///
    /// The list is read once, at bind. A device paired afterwards will not be
    /// accepted until the listener is rebuilt — acceptable while pairing is a
    /// deliberate act a person performs, and something to revisit when devices
    /// come and go on their own.
    pub fn bind_trusting(addr: SocketAddr, identity: &Identity, store: &Store) -> Result<Self> {
        let allowed = trusted_fingerprints(store)?;
        if allowed.is_empty() {
            tracing::warn!("no paired devices; this listener will refuse everyone");
        }
        Self::bind(addr, identity, &crate::tls::TrustList::new(allowed))
    }

    /// The address actually bound, which matters when port 0 was requested.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.endpoint
            .local_addr()
            .map_err(|e| Error::Io { path: "local_addr".into(), source: e })
    }

    /// Serve until the endpoint is closed.
    ///
    /// # Runtime requirement
    ///
    /// Storage reads are synchronous and can take milliseconds, so they run
    /// inside [`tokio::task::block_in_place`], which needs the multi-threaded
    /// scheduler. The same constraint the engine has.
    /// Serve, without telling anyone when this device changes.
    ///
    /// Peers fall back to asking periodically, which works and is slower.
    pub async fn serve(&self, store: Arc<Mutex<Store>>) {
        self.serve_with(store, Generation::new()).await
    }

    /// Serve, and answer "tell me when you change" from `generation`.
    pub async fn serve_with(&self, store: Arc<Mutex<Store>>, generation: Arc<Generation>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let store = Arc::clone(&store);
            let stats = Arc::clone(&self.stats);
            let generation = Arc::clone(&generation);
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        tracing::debug!(peer = %connection.remote_address(), "peer connected");
                        serve_connection_inner(connection, store, stats, generation).await;
                    }
                    // A failed handshake is the normal outcome for an
                    // unrecognised peer, and is not worth more than a debug line.
                    Err(e) => tracing::debug!(error = %e, "handshake failed"),
                }
            });
        }
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutting down");
    }
}

/// The fingerprints of every device this store trusts.
pub fn trusted_fingerprints(store: &Store) -> Result<Vec<Fingerprint>> {
    Ok(store
        .db()
        .trusted_peers()?
        .into_iter()
        .map(|peer| Fingerprint::from_bytes(peer.fingerprint))
        .collect())
}

/// Serve one connection, for tests that build their own endpoint.
///
/// Exists because hole punching requires the endpoint to be constructed from an
/// existing socket, which [`PeerServer::bind`] cannot do.
pub async fn serve_connection_for_test(connection: quinn::Connection, store: Arc<Mutex<Store>>) {
    serve_connection_inner(connection, store, Arc::new(ServerStats::default()), Generation::new())
        .await
}

/// Serve one connection, telling peers about changes to `generation`.
pub async fn serve_connection(
    connection: quinn::Connection,
    store: Arc<Mutex<Store>>,
    generation: Arc<Generation>,
) {
    serve_connection_inner(connection, store, Arc::new(ServerStats::default()), generation).await
}

async fn serve_connection_inner(
    connection: quinn::Connection,
    store: Arc<Mutex<Store>>,
    stats: Arc<ServerStats>,
    generation: Arc<Generation>,
) {
    // One request per bidirectional stream, served concurrently. This is the
    // property QUIC was chosen for: a large chunk in flight does not hold up
    // the small requests behind it.
    // Taken once, from the certificate this connection authenticated with. A
    // request that says something about the sender -- `Got` does -- must be
    // attributed to whoever the TLS handshake proved them to be, never to
    // whatever the message claims.
    let asker = fingerprint_of(&connection);

    while let Ok((send, recv)) = connection.accept_bi().await {
        let store = Arc::clone(&store);
        let stats = Arc::clone(&stats);
        let generation = Arc::clone(&generation);
        tokio::spawn(async move {
            if let Err(e) = serve_request(send, recv, store, stats, generation, asker).await {
                tracing::debug!(error = %e, "request failed");
            }
        });
    }
}

async fn serve_request(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    store: Arc<Mutex<Store>>,
    stats: Arc<ServerStats>,
    generation: Arc<Generation>,
    asker: Option<Fingerprint>,
) -> Result<()> {
    let raw = recv.read_to_end(MAX_MESSAGE).await?;
    let request = Request::decode(&raw)?;

    // Handled here rather than in `answer`, because it is the one request that
    // waits: holding the store's lock while doing so would stop this device
    // getting on with anything.
    let response = if let Request::Changes { since } = request {
        Response::Changed { generation: generation.wait_past(since, CHANGES_TIMEOUT).await }
    } else {
        tokio::task::block_in_place(|| {
            let store = store.lock().expect("store mutex poisoned");
            answer(&store, &request, asker)
        })?
    };

    match &response {
        Response::Tree(_) => stats.trees_served.fetch_add(1, Ordering::Relaxed),
        Response::Manifest(_) => stats.manifests_served.fetch_add(1, Ordering::Relaxed),
        Response::Chunk(bytes) => {
            stats.bytes_served.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            stats.chunks_served.fetch_add(1, Ordering::Relaxed)
        }
        Response::NotFound
        | Response::Noted
        | Response::Paired { .. }
        | Response::Changed { .. } => 0,
    };

    let encoded = response.encode();
    send.write_all(&encoded).await?;
    send.finish()?;
    Ok(())
}

fn answer(store: &Store, request: &Request, asker: Option<Fingerprint>) -> Result<Response> {
    // Which device is asking, as the connection proves rather than as anything
    // claims. Every answer below is scoped to it: the shared area plus that
    // device's own vault, never anybody else's.
    //
    // `None` -- an unrecognised certificate -- is treated as a device entitled
    // to nothing rather than as a device entitled to everything. That is the
    // safe direction, and the only one: a peer that cannot be identified
    // cannot be shown a vault.
    // A peer that authenticated but whose device this store does not recognise
    // is `Unplaced`: it owns no vault here, so it is shown none, and it is not
    // refused the shared area either. The connection already proved it is
    // trusted, and being stricter would break a paired device whose
    // bookkeeping is incomplete while buying nothing — vaults stay invisible
    // to it either way.
    let owner = asker.and_then(|fingerprint| {
        store
            .db()
            .peer_by_fingerprint(fingerprint.as_bytes())
            .ok()
            .flatten()
            .map(|peer| peer.device_id)
    });
    let audience = match &owner {
        Some(device) => qurb_storage::db::Audience::Device(device),
        None => qurb_storage::db::Audience::Unplaced,
    };

    Ok(match request {
        Request::Tree => Response::Tree(store.tree_for(audience)?),

        Request::Manifest { content } => {
            let content = blake3::Hash::from(*content);
            if !store.content_visible_to(&content, audience)? {
                // Indistinguishable from content this device does not hold,
                // which is deliberate: "you may not have this" and "there is
                // no such thing" should look the same from outside, or the
                // refusal itself tells a peer what exists.
                return Ok(Response::NotFound);
            }
            match store.chunk_hashes_for_content(&content)? {
                Some(hashes) => {
                    Response::Manifest(hashes.iter().map(|h| *h.as_bytes()).collect())
                }
                None => Response::NotFound,
            }
        }

        // Pairing is served by its own listener, which accepts unknown
        // certificates. This one only ever talks to devices already trusted, so
        // a pairing request here is either a mistake or a probe.
        Request::Pair { .. } => Response::NotFound,

        // Handled before the store is locked, since it waits.
        Request::Changes { .. } => unreachable!("answered without locking the store"),

        // A peer reporting that it now holds some content. Recorded against
        // the device the connection proves it to be, not against anything the
        // message says -- otherwise one peer could claim delivery on another's
        // behalf, and this record is what a storage cap trusts before dropping
        // a local copy.
        //
        // Best effort in both directions. An unknown fingerprint is ignored
        // rather than refused, and the acknowledgement is the same either way:
        // the sender has nothing useful to do with a failure, and this device
        // failing to take a note is not the sender's problem.
        Request::Got { content } => {
            match &owner {
                Some(device) => {
                    let content = blake3::Hash::from(*content);
                    if let Err(e) = store.note_replica(&content, device) {
                        tracing::debug!(error = %e, "could not record delivery");
                    }
                }
                None => tracing::debug!("delivery reported by an unrecognised device"),
            }
            Response::Noted
        }

        Request::Chunk { hash } => {
            let hash = blake3::Hash::from(*hash);

            // The bytes, checked the same way as the manifest above. Without
            // this a device could skip the tree entirely and fetch anything it
            // could name, which is exactly what a hash is.
            if !store.chunk_visible_to(&hash, audience)? {
                return Ok(Response::NotFound);
            }

            // read_chunk verifies the payload against its own hash, so a
            // corrupt chunk is reported as missing rather than served.
            match store.read_chunk(&hash) {
                Ok(bytes) => Response::Chunk(bytes),
                Err(qurb_storage::Error::ChunkMissing { .. }) => Response::NotFound,
                Err(e) => return Err(e.into()),
            }
        }
    })
}
