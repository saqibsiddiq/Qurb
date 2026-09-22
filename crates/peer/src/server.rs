//! Serving a store to peers.
//!
//! Read-only by design. A peer can ask what this device has and can ask for its
//! bytes; it cannot tell this device to change anything. Incoming versions are
//! adopted by the *local* engine after it has run them through reconciliation,
//! so nothing a peer says is applied without this side deciding it should be.

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
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
    while let Ok((send, recv)) = connection.accept_bi().await {
        let store = Arc::clone(&store);
        let stats = Arc::clone(&stats);
        let generation = Arc::clone(&generation);
        tokio::spawn(async move {
            if let Err(e) = serve_request(send, recv, store, stats, generation).await {
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
            answer(&store, &request)
        })?
    };

    match &response {
        Response::Tree(_) => stats.trees_served.fetch_add(1, Ordering::Relaxed),
        Response::Manifest(_) => stats.manifests_served.fetch_add(1, Ordering::Relaxed),
        Response::Chunk(bytes) => {
            stats.bytes_served.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            stats.chunks_served.fetch_add(1, Ordering::Relaxed)
        }
        Response::NotFound | Response::Paired { .. } | Response::Changed { .. } => 0,
    };

    let encoded = response.encode();
    send.write_all(&encoded).await?;
    send.finish()?;
    Ok(())
}

fn answer(store: &Store, request: &Request) -> Result<Response> {
    Ok(match request {
        Request::Tree => Response::Tree(store.tree()?),

        Request::Manifest { content } => {
            match store.chunk_hashes_for_content(&blake3::Hash::from(*content))? {
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

        Request::Chunk { hash } => {
            let hash = blake3::Hash::from(*hash);
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
