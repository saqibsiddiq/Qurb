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

pub struct PeerServer {
    endpoint: quinn::Endpoint,
    stats: Arc<ServerStats>,
}

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
    pub fn bind(addr: SocketAddr, identity: &Identity, allowed: &[Fingerprint]) -> Result<Self> {
        let config = tls::server_config(identity, allowed)?;
        let endpoint = quinn::Endpoint::server(config, addr)
            .map_err(|e| Error::Io { path: addr.to_string().into(), source: e })?;
        Ok(Self { endpoint, stats: Arc::new(ServerStats::default()) })
    }

    /// Counters, shareable with whoever wants to watch.
    pub fn stats(&self) -> Arc<ServerStats> {
        Arc::clone(&self.stats)
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
    pub async fn serve(&self, store: Arc<Mutex<Store>>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let store = Arc::clone(&store);
            let stats = Arc::clone(&self.stats);
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        tracing::debug!(peer = %connection.remote_address(), "peer connected");
                        serve_connection(connection, store, stats).await;
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

async fn serve_connection(
    connection: quinn::Connection,
    store: Arc<Mutex<Store>>,
    stats: Arc<ServerStats>,
) {
    // One request per bidirectional stream, served concurrently. This is the
    // property QUIC was chosen for: a large chunk in flight does not hold up
    // the small requests behind it.
    while let Ok((send, recv)) = connection.accept_bi().await {
        let store = Arc::clone(&store);
        let stats = Arc::clone(&stats);
        tokio::spawn(async move {
            if let Err(e) = serve_request(send, recv, store, stats).await {
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
) -> Result<()> {
    let raw = recv.read_to_end(MAX_MESSAGE).await?;
    let request = Request::decode(&raw)?;

    let response = tokio::task::block_in_place(|| {
        let store = store.lock().expect("store mutex poisoned");
        answer(&store, &request)
    })?;

    match &response {
        Response::Tree(_) => stats.trees_served.fetch_add(1, Ordering::Relaxed),
        Response::Manifest(_) => stats.manifests_served.fetch_add(1, Ordering::Relaxed),
        Response::Chunk(bytes) => {
            stats.bytes_served.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            stats.chunks_served.fetch_add(1, Ordering::Relaxed)
        }
        Response::NotFound => 0,
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
