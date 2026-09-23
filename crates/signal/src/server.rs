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
    /// Longest wake-up token accepted. Real ones are a few hundred bytes;
    /// anything much larger is somebody using the service as storage.
    pub max_wake_token: usize,
    /// How many wake-up tokens to hold before dropping the oldest. They
    /// outlive connections on purpose, so something has to bound them.
    pub max_wake_tokens: usize,
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
            max_wake_token: 4096,
            max_wake_tokens: 100_000,
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
    /// Who has been told somebody has something for them, but was not here to
    /// hear it. Delivered when they next announce.
    ///
    /// Only *that* there is work, never what: a set of member ids per absent
    /// member. Two devices that change a thousand files between them leave one
    /// entry, because the answer to "should I sync" is the same either way.
    /// That also bounds it — a group of sixty-four members cannot leave more
    /// than sixty-four ids waiting for any one of them.
    waiting: HashMap<(GroupId, MemberId), std::collections::HashSet<MemberId>>,
    /// How to wake each member that has said it can be.
    ///
    /// Outlives both the connection and the group, and that is not an
    /// oversight. A token is only ever useful for a device that is *not*
    /// here — and the case that matters most is everybody being away at once:
    /// a laptop shut overnight beside a sleeping phone empties the group, and
    /// dropping the token then would mean the laptop could never wake the
    /// phone when it came back.
    ///
    /// So it is bounded by count instead, oldest first. Devices re-register on
    /// every connection, so an eviction costs at most one missed wake-up and
    /// heals itself.
    wake: HashMap<(GroupId, MemberId), (crate::wake::WakeToken, std::time::Instant)>,
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
        // Anything noted while this member was away, delivered now.
        //
        // The reason the note is kept at all: a device that was asleep when
        // the change happened would otherwise not learn of it until its own
        // next poll, which on a phone is a quarter of an hour away. Sent
        // before this member joins the directory, so the borrow of `groups`
        // below does not have to be interrupted, and taken rather than copied,
        // because it has now been said.
        for from in self.waiting.remove(&(group, member)).unwrap_or_default() {
            let _ = outbox.send(FromServer::Waiting { from });
        }

        let members = self.groups.entry(group).or_default();
        members.insert(member, Member { endpoints: endpoints.clone(), outbox });

        let others: Vec<Presence> = members
            .iter()
            .filter(|(id, _)| **id != member)
            .map(|(id, m)| Presence { member: *id, endpoints: m.endpoints.clone() })
            .collect();

        // Tell everyone already here that this one has arrived.
        //
        // Without it the server knows the moment a device appears and tells
        // only that device. Everyone else has to discover it by asking, and a
        // peer that asks on a backoff -- which grows precisely because the
        // absent device keeps being absent -- will not be asking during the few
        // seconds a phone is awake. Measured before this existed: a laptop
        // retrying every 120s against a phone announcing for 25s never met it.
        let arrival = FromServer::Appeared {
            peer: Presence { member, endpoints },
        };
        for (id, m) in members.iter() {
            if *id != member {
                // A send failure means that member's connection has gone; the
                // reader task will clean it up. Nothing to do here.
                let _ = m.outbox.send(arrival.clone());
            }
        }

        others
    }

    /// Note that `from` has something for `to`, and say whether it was
    /// delivered now.
    ///
    /// `false` means the recipient is not connected and the note was kept. The
    /// caller is the one that knows whether there is another way to reach them
    /// — a push notification — and this is the moment to use it.
    fn note_waiting(&mut self, group: GroupId, from: MemberId, to: MemberId) -> bool {
        if let Some(m) = self.groups.get(&group).and_then(|g| g.get(&to)) {
            let _ = m.outbox.send(FromServer::Waiting { from });
            return true;
        }
        self.waiting.entry((group, to)).or_default().insert(from);
        false
    }

    /// How to wake a member, if it has said.
    fn wake_token(&self, group: GroupId, member: MemberId) -> Option<crate::wake::WakeToken> {
        self.wake.get(&(group, member)).map(|(token, _)| token.clone())
    }

    fn set_wake_token(
        &mut self,
        group: GroupId,
        member: MemberId,
        token: Option<crate::wake::WakeToken>,
        most: usize,
    ) {
        match token {
            Some(token) => {
                self.wake.insert((group, member), (token, std::time::Instant::now()));
                self.evict_wake_tokens(most);
            }
            None => {
                self.wake.remove(&(group, member));
            }
        }
    }

    /// Keep the number of tokens under `most`, dropping the oldest.
    ///
    /// Unbounded growth here is a slow leak keyed on something anyone can
    /// generate for free. Oldest-first because the newest registration is the
    /// one most likely to still be valid — a push token is reissued, and the
    /// device tells us again each time it connects.
    fn evict_wake_tokens(&mut self, most: usize) {
        if self.wake.len() <= most {
            return;
        }
        let mut by_age: Vec<((GroupId, MemberId), std::time::Instant)> =
            self.wake.iter().map(|(k, (_, at))| (*k, *at)).collect();
        by_age.sort_by_key(|(_, at)| *at);
        for (key, _) in by_age.into_iter().take(self.wake.len() - most) {
            self.wake.remove(&key);
        }
    }

    fn leave(&mut self, group: GroupId, member: MemberId) {
        if let Some(members) = self.groups.get_mut(&group) {
            members.remove(&member);
            // Do not leave an empty group behind. A server that accumulates one
            // entry per group it has ever seen has a slow memory leak keyed on
            // something an attacker can generate for free.
            if members.is_empty() {
                self.groups.remove(&group);
                // Nothing left to deliver to, and an entry per group ever seen
                // is a slow leak keyed on something anyone can generate.
                self.waiting.retain(|(g, _), _| *g != group);
                // Wake tokens deliberately survive. See the field's comment:
                // everybody being away at once is exactly when one is needed.
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
    /// How to reach devices that are not connected. Does nothing unless the
    /// deployment supplies one — see [`crate::wake`].
    waker: crate::wake::SharedWaker,
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

    /// Supply a way to wake devices that are not connected.
    ///
    /// Without one the service behaves exactly as it did before push existed:
    /// devices sync when they next look. That is the right default, and it is
    /// what a deployment with no push credentials gets.
    pub fn waking_with(mut self, waker: crate::wake::SharedWaker) -> Self {
        self.waker = waker;
        self
    }

    /// Listen with limits other than the defaults.
    pub async fn bind_with(addr: SocketAddr, limits: Limits) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self {
            listener,
            directory: Arc::new(Mutex::new(Directory::default())),
            limits,
            connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            waker: crate::wake::none(),
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
            let waker = Arc::clone(&self.waker);
            let counter = Arc::clone(&self.connections);
            counter.fetch_add(1, Ordering::Relaxed);

            tokio::spawn(async move {
                if let Err(e) = serve_one(stream, directory, limits, waker).await {
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
    waker: crate::wake::SharedWaker,
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

            FromClient::Reachable { via } => {
                let Some((group, member)) = identity else {
                    let _ = outbox.send(FromServer::Error { detail: "announce first".into() });
                    continue;
                };
                if via.as_ref().is_some_and(|t| t.len() > limits.max_wake_token) {
                    let _ = outbox.send(FromServer::Error { detail: "token too long".into() });
                    continue;
                }
                let held = via.is_some();
                directory.lock().expect("directory").set_wake_token(
                    group,
                    member,
                    via,
                    limits.max_wake_tokens,
                );
                tracing::debug!(?member, held, "a device said how to wake it");
            }

            FromClient::Waiting { to } => {
                let Some((group, from)) = identity else {
                    let _ = outbox.send(FromServer::Error { detail: "announce first".into() });
                    continue;
                };
                // Refused for a member of another group by construction: the
                // note is filed under the group this connection announced into.
                let (delivered, token) = {
                    let mut directory = directory.lock().expect("directory");
                    let delivered = directory.note_waiting(group, from, to);
                    let token = match delivered {
                        true => None,
                        false => directory.wake_token(group, to),
                    };
                    (delivered, token)
                };

                // Woken only when it could not simply be told, and only
                // because the device asking is here to sync with: that is the
                // whole condition. Waking a phone for a peer that is not there
                // spends its battery to find nobody.
                if let Some(token) = token {
                    tracing::debug!(?to, "waking a device that is not connected");
                    waker.wake(&token);
                }
                tracing::debug!(?from, ?to, delivered, "work waiting");
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

#[cfg(test)]
mod wake_tests {
    use super::*;
    use crate::wake::testing::Recorder;
    use qurb_keys::MasterKey;

    fn group_and_members() -> (GroupId, MemberId, MemberId) {
        let key = MasterKey::generate();
        (
            GroupId::derive(&key),
            MemberId::derive(&key, &[1u8; 32]),
            MemberId::derive(&key, &[2u8; 32]),
        )
    }

    fn nowhere() -> Endpoints {
        Endpoints { public: None, local: Vec::new() }
    }

    /// The rule the whole feature turns on: a device is woken when somebody
    /// has something for it and it is not here to be told.
    #[test]
    fn an_absent_device_with_a_token_is_woken() {
        let (group, one, two) = group_and_members();
        let mut directory = Directory::default();
        directory.set_wake_token(group, two, Some("token-for-two".into()), 16);

        let delivered = directory.note_waiting(group, one, two);
        assert!(!delivered, "nobody was connected, so nothing could be delivered");

        let recorder = Recorder::default();
        if let Some(token) = directory.wake_token(group, two) {
            crate::wake::Waker::wake(&recorder, &token);
        }
        assert_eq!(recorder.woken(), vec!["token-for-two".to_string()]);
    }

    /// And the rule that keeps it from being a battery drain: a device that is
    /// already connected is told, not woken.
    #[test]
    fn a_connected_device_is_told_rather_than_woken() {
        let (group, one, two) = group_and_members();
        let mut directory = Directory::default();
        directory.set_wake_token(group, two, Some("token-for-two".into()), 16);

        let (outbox, _inbox) = tokio::sync::mpsc::unbounded_channel();
        directory.announce(group, two, nowhere(), outbox);

        let delivered = directory.note_waiting(group, one, two);
        assert!(delivered, "a connected device should have been told directly");
        assert!(
            directory.wake_token(group, two).is_some(),
            "the token should survive being connected, for next time"
        );
    }

    /// A token has to outlive the connection that supplied it. It is only ever
    /// useful for a device that is *not* here.
    #[test]
    fn a_token_survives_disconnection() {
        let (group, _one, two) = group_and_members();
        let mut directory = Directory::default();

        let (outbox, _inbox) = tokio::sync::mpsc::unbounded_channel();
        directory.announce(group, two, nowhere(), outbox);
        directory.set_wake_token(group, two, Some("token-for-two".into()), 16);
        directory.leave(group, two);

        assert_eq!(
            directory.wake_token(group, two),
            Some("token-for-two".to_string()),
            "the token was thrown away with the connection"
        );
    }

    /// Withdrawing one is honoured: a signed-out device must stop being poked.
    #[test]
    fn a_withdrawn_token_is_forgotten() {
        let (group, _one, two) = group_and_members();
        let mut directory = Directory::default();
        directory.set_wake_token(group, two, Some("token-for-two".into()), 16);
        directory.set_wake_token(group, two, None, 16);
        assert_eq!(directory.wake_token(group, two), None);
    }

    /// A token outlives the whole group emptying, and that is the case that
    /// matters most: a laptop shut overnight beside a sleeping phone leaves
    /// nobody connected, and the laptop must still be able to wake the phone
    /// when it comes back.
    #[test]
    fn a_token_outlives_everybody_leaving() {
        let (group, _one, two) = group_and_members();
        let mut directory = Directory::default();

        let (outbox, _inbox) = tokio::sync::mpsc::unbounded_channel();
        directory.announce(group, two, nowhere(), outbox);
        directory.set_wake_token(group, two, Some("token-for-two".into()), 16);
        directory.leave(group, two);

        assert!(directory.groups.is_empty(), "setup: the group should be gone");
        assert_eq!(
            directory.wake_token(group, two),
            Some("token-for-two".to_string()),
            "the token went with the empty group, so nobody could ever be woken"
        );
    }

    /// Something has to bound them, since they outlive everything else.
    #[test]
    fn the_oldest_tokens_are_dropped_when_there_are_too_many() {
        let mut directory = Directory::default();
        let key = qurb_keys::MasterKey::generate();
        let group = GroupId::derive(&key);

        let members: Vec<MemberId> =
            (0u8..8).map(|n| MemberId::derive(&key, &[n; 32])).collect();
        for (n, member) in members.iter().enumerate() {
            directory.set_wake_token(group, *member, Some(format!("token-{n}")), 4);
            // Distinguishable ages; `Instant` has finer resolution than this
            // but sorting needs the order to be unambiguous.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        assert_eq!(directory.wake.len(), 4, "the cap was not applied");
        assert_eq!(directory.wake_token(group, members[0]), None, "the oldest survived");
        assert_eq!(
            directory.wake_token(group, members[7]),
            Some("token-7".to_string()),
            "the newest was dropped"
        );
    }
}
