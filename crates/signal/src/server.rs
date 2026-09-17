//! The rendezvous service.
//!
//! Holds a websocket open per device, remembers where each says it is, and
//! introduces two that want to reach each other — telling both to punch at the
//! same moment.
//!
//! # What it deliberately cannot do
//!
//! It never sees a filename, a chunk, or a key, and it does not learn whose
//! devices these are: identifiers are derived from a master key it does not
//! hold. It matches opaque values and forwards addresses.
//!
//! It does necessarily learn that some set of addresses belong together, and it
//! sees IP addresses. That is inherent to being a rendezvous point rather than a
//! shortcoming of this implementation, and it is why the data plane never goes
//! near it.
//!
//! # Websocket, not QUIC
//!
//! The data plane is QUIC over UDP. This is not, deliberately. If UDP is blocked
//! — which is exactly the situation where a device most needs coordinating — a
//! UDP control channel cannot even tell it that it needs a relay. The channel
//! that arranges the fallback has to work where the fallback is needed.

use crate::error::Result;
use crate::message::{Endpoints, FromClient, FromServer, Presence};
use crate::rendezvous::{GroupId, MemberId};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

type Outbox = mpsc::UnboundedSender<FromServer>;

/// Limits, so that anyone who finds the port cannot exhaust the machine.
///
/// None of these protect against a determined attacker with many addresses —
/// that needs infrastructure this service does not have. What they do is make
/// the cheap attacks cheap to survive: one connection cannot spend the server's
/// memory, and one group cannot grow without bound.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Connections held at once, across everyone.
    pub max_connections: usize,
    /// Devices in one group. A person with more than this has a different
    /// problem.
    pub max_members_per_group: usize,
    /// Messages one connection may send per second, averaged.
    pub messages_per_second: u32,
    /// How much of a burst to tolerate before the average applies. Announcing
    /// and immediately asking for several peers is normal.
    pub burst: u32,
    /// The largest message accepted, in bytes.
    pub max_message: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_connections: 10_000,
            max_members_per_group: 64,
            messages_per_second: 20,
            burst: 60,
            max_message: 16 * 1024,
        }
    }
}

/// A token bucket, refilled continuously rather than in steps.
///
/// Stepwise refill lets a caller send a full burst at the end of one window and
/// another at the start of the next, which is twice the rate that was
/// configured.
struct RateLimit {
    tokens: f64,
    per_second: f64,
    burst: f64,
    last: std::time::Instant,
}

impl RateLimit {
    fn new(limits: &Limits) -> Self {
        Self {
            tokens: limits.burst as f64,
            per_second: limits.messages_per_second as f64,
            burst: limits.burst as f64,
            last: std::time::Instant::now(),
        }
    }

    fn allow(&mut self) -> bool {
        let now = std::time::Instant::now();
        self.tokens =
            (self.tokens + now.duration_since(self.last).as_secs_f64() * self.per_second)
                .min(self.burst);
        self.last = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Who is connected, and where they say they are.
#[derive(Default)]
struct Directory {
    groups: HashMap<GroupId, HashMap<MemberId, Member>>,
}

struct Member {
    endpoints: Endpoints,
    outbox: Outbox,
}

impl Directory {
    fn announce(
        &mut self,
        group: GroupId,
        member: MemberId,
        endpoints: Endpoints,
        outbox: Outbox,
    ) -> Vec<Presence> {
        let members = self.groups.entry(group).or_default();
        members.insert(member, Member { endpoints, outbox });

        members
            .iter()
            .filter(|(id, _)| **id != member)
            .map(|(id, m)| Presence { member: *id, endpoints: m.endpoints.clone() })
            .collect()
    }

    fn leave(&mut self, group: GroupId, member: MemberId) {
        if let Some(members) = self.groups.get_mut(&group) {
            members.remove(&member);
            // Do not leave an empty group behind. A server that accumulates one
            // entry per group it has ever seen has a slow memory leak keyed on
            // something an attacker can generate for free.
            if members.is_empty() {
                self.groups.remove(&group);
            }
        }
    }

    fn look_up(&self, group: GroupId, member: MemberId) -> Option<(Endpoints, Outbox)> {
        let m = self.groups.get(&group)?.get(&member)?;
        Some((m.endpoints.clone(), m.outbox.clone()))
    }
}

pub struct SignalServer {
    listener: TcpListener,
    directory: Arc<Mutex<Directory>>,
    limits: Limits,
    connections: Arc<std::sync::atomic::AtomicUsize>,
}

impl SignalServer {
    /// Listen on `addr`.
    ///
    /// Plain TCP. In production this belongs behind TLS termination, because
    /// rendezvous identifiers are bearer secrets and a network observer holding
    /// one can enumerate that group's addresses.
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        Self::bind_with(addr, Limits::default()).await
    }

    /// Listen with limits other than the defaults.
    pub async fn bind_with(addr: SocketAddr, limits: Limits) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self {
            listener,
            directory: Arc::new(Mutex::new(Directory::default())),
            limits,
            connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    pub fn connection_count(&self) -> usize {
        self.connections.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// How many groups currently have anyone connected.
    pub fn group_count(&self) -> usize {
        self.directory.lock().expect("directory").groups.len()
    }

    pub async fn serve(&self) {
        use std::sync::atomic::Ordering;

        while let Ok((stream, from)) = self.listener.accept().await {
            // Checked before the handshake, so a flood costs a TCP accept rather
            // than a websocket upgrade and a task.
            if self.connections.load(Ordering::Relaxed) >= self.limits.max_connections {
                tracing::warn!(%from, "refusing a connection: at the limit");
                drop(stream);
                continue;
            }

            let directory = Arc::clone(&self.directory);
            let limits = self.limits;
            let counter = Arc::clone(&self.connections);
            counter.fetch_add(1, Ordering::Relaxed);

            tokio::spawn(async move {
                if let Err(e) = serve_one(stream, directory, limits).await {
                    tracing::debug!(%from, error = %e, "signalling connection ended");
                }
                counter.fetch_sub(1, Ordering::Relaxed);
            });
        }
    }
}

async fn serve_one(
    stream: TcpStream,
    directory: Arc<Mutex<Directory>>,
    limits: Limits,
) -> Result<()> {
    // Bounded by the library, before a frame is ever assembled in memory.
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(limits.max_message))
        .max_frame_size(Some(limits.max_message));
    let websocket =
        tokio_tungstenite::accept_async_with_config(stream, Some(config)).await?;
    let (mut sink, mut source) = websocket.split();

    let (outbox, mut outgoing) = mpsc::unbounded_channel::<FromServer>();
    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            let Ok(text) = serde_json::to_string(&message) else { continue };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Set by the first Announce. A connection that never announces can do
    // nothing, which is what stops an unannounced client from using the server
    // as a directory of other people's addresses.
    let mut identity: Option<(GroupId, MemberId)> = None;
    let mut rate = RateLimit::new(&limits);

    while let Some(message) = source.next().await {
        // Deliberately not `?`. A client that vanishes -- laptop shut, network
        // dropped, process killed -- surfaces here as an error, and propagating
        // it would skip the cleanup at the end of this function. The device
        // would stay in the directory and peers would go on being handed its
        // dead address. Ungraceful disconnection is the common case, not the
        // exceptional one, so it has to leave by the same door as everything
        // else.
        let message = match message {
            Ok(message) => message,
            Err(e) => {
                tracing::debug!(error = %e, "connection ended abruptly");
                break;
            }
        };
        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => break,
            // Ping and pong are handled by the library; anything else is not
            // part of this protocol.
            _ => continue,
        };

        if !rate.allow() {
            // Dropped rather than answered. Replying would let a caller spend
            // the server's bandwidth by spending only its own.
            tracing::debug!("dropping a message: over rate");
            continue;
        }

        let request: FromClient = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                let _ = outbox.send(FromServer::Error { detail: format!("malformed: {e}") });
                continue;
            }
        };

        match request {
            FromClient::Announce { group, member, endpoints } => {
                let peers = {
                    let mut directory = directory.lock().expect("directory");

                    // A group that can grow without bound is memory anyone can
                    // spend. Re-announcing an existing member is always allowed,
                    // since it is how a device that moved networks updates its
                    // address.
                    let known = directory
                        .groups
                        .get(&group)
                        .is_some_and(|members| members.contains_key(&member));
                    let full = directory
                        .groups
                        .get(&group)
                        .is_some_and(|members| members.len() >= limits.max_members_per_group);
                    if full && !known {
                        drop(directory);
                        let _ = outbox.send(FromServer::Error { detail: "group is full".into() });
                        continue;
                    }

                    // Re-announcing replaces the previous addresses, which is
                    // what a laptop changing networks needs.
                    if let Some((previous_group, previous_member)) = identity {
                        if (previous_group, previous_member) != (group, member) {
                            directory.leave(previous_group, previous_member);
                        }
                    }
                    directory.announce(group, member, endpoints, outbox.clone())
                };
                identity = Some((group, member));
                let _ = outbox.send(FromServer::Peers { members: peers });
            }

            FromClient::Connect { to } => {
                let Some((group, from)) = identity else {
                    let _ = outbox.send(FromServer::Error { detail: "announce first".into() });
                    continue;
                };
                let found = {
                    let directory = directory.lock().expect("directory");
                    directory
                        .look_up(group, from)
                        .map(|(mine, _)| mine)
                        .zip(directory.look_up(group, to))
                };
                match found {
                    // A member can only ever be looked up within the group the
                    // asking connection announced into, so one group cannot
                    // enumerate or reach another.
                    Some((my_endpoints, (_, their_outbox))) => {
                        let _ = their_outbox.send(FromServer::ConnectRequest {
                            from,
                            endpoints: my_endpoints,
                        });
                    }
                    None => {
                        let _ = outbox.send(FromServer::Error { detail: "peer is not here".into() });
                    }
                }
            }

            FromClient::Accept { to, endpoints } => {
                let Some((group, from)) = identity else {
                    let _ = outbox.send(FromServer::Error { detail: "announce first".into() });
                    continue;
                };
                let target = {
                    let directory = directory.lock().expect("directory");
                    directory.look_up(group, to)
                };
                if let Some((their_endpoints, their_outbox)) = target {
                    // Both told at once. This is the whole reason the channel is
                    // held open: hole punching needs both routers to see an
                    // outbound packet at roughly the same time.
                    let _ = their_outbox
                        .send(FromServer::Punch { peer: from, endpoints: endpoints.clone() });
                    let _ = outbox
                        .send(FromServer::Punch { peer: to, endpoints: their_endpoints });
                } else {
                    let _ = outbox.send(FromServer::Error { detail: "peer is not here".into() });
                }
            }
        }
    }

    if let Some((group, member)) = identity {
        directory.lock().expect("directory").leave(group, member);
    }
    drop(outbox);
    let _ = writer.await;
    Ok(())
}
