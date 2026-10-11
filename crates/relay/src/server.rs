//! The relay service.
//!
//! Holds a connection per device and forwards opaque frames between them. It
//! never learns what it carries: a full QUIC session with pinned certificates
//! runs inside, so the bytes are ciphertext the relay has no key for and cannot
//! forge.

use crate::error::Result;
use crate::frame::{Frame, RelayId, MAX_FRAME};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// What the relay has carried. The number that becomes a bill.
#[derive(Debug, Default)]
pub struct RelayStats {
    pub connections: AtomicU64,
    pub frames_forwarded: AtomicU64,
    pub bytes_forwarded: AtomicU64,
    pub frames_dropped: AtomicU64,
}

impl RelayStats {
    pub fn bytes(&self) -> u64 {
        self.bytes_forwarded.load(Ordering::Relaxed)
    }

    pub fn forwarded(&self) -> u64 {
        self.frames_forwarded.load(Ordering::Relaxed)
    }

    pub fn dropped(&self) -> u64 {
        self.frames_dropped.load(Ordering::Relaxed)
    }
}

/// How many identifiers one connection may hold: a device's own, and one for
/// each meeting with another person's device (decision 0061). Far more than a
/// household needs; a bound, so one connection cannot fill the registry.
pub const MAX_IDENTITIES: usize = 64;

/// Where packets for a registered identifier go, and whether the connection
/// behind it holds more than one -- and so has to be told which each is for.
#[derive(Clone)]
struct Outbox {
    channel: mpsc::UnboundedSender<Frame>,
    many: Arc<AtomicBool>,
}

impl Outbox {
    fn same_channel(&self, other: &Outbox) -> bool {
        self.channel.same_channel(&other.channel)
    }
}

pub struct RelayServer {
    listener: TcpListener,
    registered: Arc<Mutex<HashMap<RelayId, Outbox>>>,
    stats: Arc<RelayStats>,
}

impl RelayServer {
    /// Listen on `addr`.
    ///
    /// Plain TCP here; in production this belongs behind TLS on port 443, which
    /// is the entire reason the relay is TCP rather than UDP — it exists for
    /// networks that block everything else.
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(addr).await?,
            registered: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(RelayStats::default()),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    pub fn stats(&self) -> Arc<RelayStats> {
        Arc::clone(&self.stats)
    }

    pub fn registered_count(&self) -> usize {
        self.registered.lock().expect("registry").len()
    }

    pub async fn serve(&self) {
        while let Ok((stream, from)) = self.listener.accept().await {
            self.stats.connections.fetch_add(1, Ordering::Relaxed);
            let registered = Arc::clone(&self.registered);
            let stats = Arc::clone(&self.stats);
            tokio::spawn(async move {
                if let Err(e) = serve_one(stream, registered, stats).await {
                    tracing::debug!(%from, error = %e, "relay connection ended");
                }
            });
        }
    }
}

async fn serve_one(
    stream: TcpStream,
    registered: Arc<Mutex<HashMap<RelayId, Outbox>>>,
    stats: Arc<RelayStats>,
) -> Result<()> {
    // Relayed traffic is many small packets where latency is already bad;
    // waiting to coalesce them makes it worse.
    let _ = stream.set_nodelay(true);
    let (mut reader, mut writer) = stream.into_split();

    let (channel, mut outgoing) = mpsc::unbounded_channel::<Frame>();
    let outbox = Outbox { channel, many: Arc::new(AtomicBool::new(false)) };
    let writing = tokio::spawn(async move {
        while let Some(frame) = outgoing.recv().await {
            if writer.write_all(&frame.encode()).await.is_err() {
                break;
            }
        }
    });

    // The first is the one a plain `Forward` comes from.
    let mut identities: Vec<RelayId> = Vec::new();
    let mut length = [0u8; 4];
    let mut body = vec![0u8; MAX_FRAME];

    loop {
        if reader.read_exact(&mut length).await.is_err() {
            break;
        }
        let size = u32::from_be_bytes(length) as usize;
        if size == 0 || size > MAX_FRAME {
            // A length nobody could mean. Close rather than try to resynchronise
            // a stream we have lost our place in.
            break;
        }
        if reader.read_exact(&mut body[..size]).await.is_err() {
            break;
        }

        let Ok(frame) = Frame::decode(&body[..size]) else {
            stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        };

        match frame {
            // Another name for this connection: a device's own first, then
            // one per meeting (decision 0061).
            Frame::Register { member } => {
                if identities.contains(&member) {
                    continue;
                }
                if identities.len() >= MAX_IDENTITIES {
                    stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                registered.lock().expect("registry").insert(member, outbox.clone());
                identities.push(member);
                if identities.len() > 1 {
                    outbox.many.store(true, Ordering::Relaxed);
                }
            }

            Frame::Forward { to, payload } => {
                let Some(&from) = identities.first() else {
                    // A connection that never said who it is cannot send. Without
                    // this the relay is an open proxy for anyone who finds it.
                    stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                forward(&registered, &stats, from, to, payload);
            }

            // From one of this connection's names, and only one of its own:
            // nobody sends as an identifier they did not register.
            Frame::ForwardAs { from, to, payload } => {
                if !identities.contains(&from) {
                    stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                forward(&registered, &stats, from, to, payload);
            }

            // Only the relay sends these. A client doing so is confused or
            // probing.
            Frame::Deliver { .. } | Frame::DeliverTo { .. } => {
                stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    {
        let mut registry = registered.lock().expect("registry");
        for member in identities {
            // Only if it is still ours: the device may have reconnected
            // elsewhere and registered again, and removing it then would
            // unregister the live connection.
            if registry.get(&member).is_some_and(|o| o.same_channel(&outbox)) {
                registry.remove(&member);
            }
        }
    }
    drop(outbox);
    let _ = writing.await;
    Ok(())
}

/// Pass a packet on to `to`, saying it is from `from`: with which of its names
/// it was sent to, when the connection there holds more than one.
fn forward(
    registered: &Mutex<HashMap<RelayId, Outbox>>,
    stats: &RelayStats,
    from: RelayId,
    to: RelayId,
    payload: Vec<u8>,
) {
    let target = registered.lock().expect("registry").get(&to).cloned();
    match target {
        Some(target) => {
            let bytes = payload.len() as u64;
            let frame = if target.many.load(Ordering::Relaxed) {
                Frame::DeliverTo { from, to, payload }
            } else {
                Frame::Deliver { from, payload }
            };
            if target.channel.send(frame).is_ok() {
                stats.frames_forwarded.fetch_add(1, Ordering::Relaxed);
                stats.bytes_forwarded.fetch_add(bytes, Ordering::Relaxed);
            } else {
                stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
        // The recipient is not here. Dropped silently, as a router would: this
        // carries datagrams, and QUIC already copes with losing them.
        None => {
            stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}
