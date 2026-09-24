//! Actually reaching the other device.
//!
//! The pieces existed separately — discovery, rendezvous, a transport, a trust
//! store — and nothing sequenced them. This is the policy that does:
//!
//! ```text
//!   start    bind a socket, ask STUN where it appears from, announce
//!   reach    ask the rendezvous service for a peer, be told to punch
//!   race     try every candidate address at once, keep the first that answers
//!   relay    if none of them answered, go the long way round
//! ```
//!
//! # Why both sides connect
//!
//! A router only lets a packet in if it has recently seen one go out to that
//! address. So both routers need to send something, and only the device making
//! the call would normally do so.
//!
//! The answer is that **both sides dial**. A QUIC handshake begins with packets
//! that are themselves the hole punch, so when the rendezvous service tells two
//! devices to punch, each attempts a connection to the other. Whichever
//! handshake completes is the one that gets used; the other is dropped.
//!
//! This also explains a constraint that is otherwise puzzling: once QUIC owns
//! the socket, nothing else can send raw packets through it. So the STUN query
//! happens *before* the endpoint is built, on the same socket, and everything
//! after that is done by the handshake itself.

use crate::client::PeerClient;
use crate::error::{Error, Result};
use crate::local::{self, Beacons, Neighbours};
use crate::identity::{Fingerprint, Identity};
use crate::{nat, tls};
use qurb_keys::MasterKey;
use qurb_signal::{Endpoints, FromServer, GroupId, MemberId, SignalClient};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};

/// How long to wait for a peer to answer a request to connect.
const RENDEZVOUS_TIMEOUT: Duration = Duration::from_secs(20);

/// How long to give one candidate address before giving up on it.
///
/// Short: the candidates are raced in parallel, so this bounds the whole attempt
/// rather than each one in turn.
const CANDIDATE_TIMEOUT: Duration = Duration::from_secs(8);

/// A device's connection machinery: one socket, one endpoint, one rendezvous.
/// How much of the outside world to involve in finding peers.
///
/// Grouped because they are one decision rather than three: a test on one
/// machine wants none of it, a phone on a carrier network wants all of it, and
/// the combinations in between are what a deployment chooses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Finding {
    /// Ask a STUN server what this device looks like from outside.
    pub stun: bool,
    /// Multicast beacons on the local network, on this port. `None` turns local
    /// discovery off, which tests want and nothing else does.
    pub beacons: Option<u16>,
    /// Where to fall back to when no direct path can be made.
    pub relay: Option<SocketAddr>,
}

impl Finding {
    /// Everything: STUN, local beacons on the standard port, and a relay if
    /// one is configured. What a real device wants.
    pub fn everything(relay: Option<SocketAddr>) -> Self {
        Self { stun: true, beacons: Some(local::PORT), relay }
    }

    /// Nothing that touches a network beyond the one socket. What a test on one
    /// machine wants: no STUN lookup, and no beacons that other tests running
    /// at the same time would hear.
    pub fn nothing() -> Self {
        Self::default()
    }

    /// Local beacons on a port of the test's own choosing, and nothing else.
    pub fn beacons_on(port: u16) -> Self {
        Self { stun: false, beacons: Some(port), relay: None }
    }
}

pub struct Connector {
    endpoint: quinn::Endpoint,
    identity: Identity,
    master: MasterKey,
    endpoints: Endpoints,
    /// The way round, when there is no way through.
    ///
    /// Held open rather than dialled on demand, because a device must be
    /// *reachable* by relay as well as able to reach: a peer whose direct
    /// attempt failed will try the relay, and finding nobody there would make
    /// the fallback useless in exactly the case it exists for.
    relay: Option<RelayPath>,
    /// Commands for the one task that owns the signalling connection.
    signal: mpsc::UnboundedSender<Command>,
    /// Peers the rendezvous service says have just appeared.
    ///
    /// The point of it: a device that is only briefly awake -- a phone in a
    /// background sync window -- is announced for seconds at a time, and a peer
    /// that discovers it by polling on a backoff will almost never be asking
    /// during one. Being told means acting inside the window rather than
    /// hoping to coincide with it.
    arrivals: broadcast::Sender<MemberId>,
    /// Devices seen on this network lately, which is better evidence of
    /// reachability than anything the rendezvous service can offer.
    neighbours: Neighbours,
    /// Saying we are here, when there is a network that will carry it.
    beacons: Option<std::sync::Arc<Beacons>>,
}

/// Turn beacons into sightings the rest of the device can act on.
///
/// The same `arrivals` channel the rendezvous feeds, deliberately: to everything
/// above this, "a peer just appeared" is one event with one meaning, and where
/// the news came from is this layer's business rather than the daemon's.
async fn follow_beacons(
    mut sightings: mpsc::UnboundedReceiver<crate::local::Beacon>,
    neighbours: Neighbours,
    arrivals: broadcast::Sender<MemberId>,
) {
    while let Some(beacon) = sightings.recv().await {
        tracing::debug!(
            peer = %hex_short(beacon.member.as_bytes()),
            news = beacon.news,
            "a device is on this network"
        );
        neighbours.note(beacon.member, beacon.endpoints);
        // A send with no receivers is not a failure: nothing is listening yet,
        // or nothing cares. The sighting is recorded either way.
        let _ = arrivals.send(beacon.member);
    }
}

fn hex_short(bytes: &[u8; 32]) -> String {
    bytes[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// What the signalling task is asked to do.
enum Command {
    Introduce { to: MemberId, reply: oneshot::Sender<Result<Endpoints>> },
    /// Tell a peer, through the rendezvous service, that there is something
    /// for it. No reply: this is a courtesy, not a request.
    Waiting { to: MemberId },
    /// Say how this device can be woken while it is not connected.
    Reachable { via: Option<String> },
}

struct RelayPath {
    socket: Arc<qurb_relay::RelaySocket>,
    endpoint: quinn::Endpoint,
}

impl Connector {
    /// Bind, discover, and get ready to dial or be dialled.
    ///
    /// `allowed` is the guest list for incoming connections, which in practice
    /// comes from the trust store. [`Finding`] says how much of the outside
    /// world to involve — tests on one machine have nothing to discover and
    /// should not reach for the network to find that out.
    pub async fn start(
        bind: SocketAddr,
        identity: Identity,
        master: MasterKey,
        allowed: &tls::TrustList,
        signal_url: impl Into<String>,
        finding: Finding,
    ) -> Result<Self> {
        let Finding { stun: discover, beacons: beacon_port, relay } = finding;
        let socket = std::net::UdpSocket::bind(bind)
            .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
        let local = socket
            .local_addr()
            .map_err(|e| Error::Io { path: "local_addr".into(), source: e })?;

        // Before the endpoint exists, because afterwards QUIC owns the socket
        // and nothing else can send through it. The router mapping this creates
        // belongs to this port and is the one peers will be told about.
        let public = if discover {
            match nat::discover(&socket, Duration::from_secs(3)) {
                Ok(reflexive) => Some(reflexive.public),
                Err(e) => {
                    // Not fatal. Two devices on one network still reach each
                    // other, and a relay will cover the rest.
                    tracing::warn!(error = %e, "no public address; only local ones will be offered");
                    None
                }
            }
        } else {
            None
        };

        let server_config = tls::server_config(&identity, allowed)?;
        let endpoint = nat::endpoint_from(socket, Some(server_config))?;

        // The relay path registers under the same identifier the rendezvous
        // service uses, so a peer that knows where to look for us there knows it
        // already.
        let relay = match relay {
            Some(address) => {
                let me = *MemberId::derive(&master, identity.fingerprint().as_bytes()).as_bytes();
                let socket = qurb_relay::RelaySocket::connect(address, me)
                    .await
                    .map_err(|e| Error::Signalling { detail: format!("relay: {e}") })?;

                let server_config = tls::server_config(&identity, allowed)?;
                let endpoint = qurb_relay::endpoint_over(Arc::clone(&socket), Some(server_config))
                    .map_err(|e| Error::Signalling { detail: format!("relay: {e}") })?;
                Some(RelayPath { socket, endpoint })
            }
            None => None,
        };

        // Through `dialable`, because binding `0.0.0.0` -- which is what a
        // device behind a router should do -- makes `local_addr` report
        // `0.0.0.0:port`. That is a true statement about the socket and a
        // useless thing to hand a peer: it means "every interface here", and
        // there is no "here" on the other machine.
        //
        // The same mistake was found and fixed in pairing invites. It survived
        // this long because every test bound `127.0.0.1` explicitly, and in
        // ordinary use STUN supplies a public address that works instead. What
        // it breaks is the case with no STUN: two devices on a network with no
        // route to the internet, which is exactly when a local address is the
        // only one there is.
        // Capacity is small because a listener only needs to know that
        // *something* arrived; a lagging receiver missing an older arrival
        // loses nothing it cannot rediscover on its next sweep.
        let (arrivals, _) = broadcast::channel(16);

        // Every address this machine has, when the socket is listening on all
        // of them. On a laptop with an overlay network that is the difference
        // between a phone elsewhere having a path to it and having none.
        //
        // Only for a wildcard bind, and the distinction is not pedantic: a
        // socket bound to one address is listening on exactly that address, so
        // announcing the machine's other interfaces advertises places nothing
        // is accepting. A peer then races addresses that can only fail and
        // concludes the device is unreachable — which is what happened to a
        // test binding loopback the first time this was written.
        let candidates = match local.ip().is_unspecified() {
            false => vec![local],
            true => {
                let port = local.port();
                let found: Vec<SocketAddr> = nat::local_addresses()
                    .into_iter()
                    .map(|ip| SocketAddr::new(ip, port))
                    .collect();
                match found.is_empty() {
                    true => vec![nat::dialable(local)],
                    false => found,
                }
            }
        };
        let endpoints = Endpoints { public, local: candidates };
        let signal_url = signal_url.into();
        let signal_url: String = signal_url;

        let arrivals_tx = arrivals.clone();

        // One connection, held for the life of the device, owned by one task.
        //
        // The obvious alternative -- opening a connection each time a peer needs
        // reaching -- announces under this device's identity and then closes,
        // which the server correctly reads as the device going away. A device
        // that called out would stop being reachable the moment it finished,
        // and the failure looks like the *other* device being absent.
        // Best effort, not a precondition.
        //
        // A rendezvous that is down, or absent entirely, used to stop a device
        // starting at all -- which meant two devices on one Wi-Fi could not
        // sync without something on the internet being reachable. They can see
        // each other; nobody needs to introduce them. `stay_signalled` keeps
        // trying in the background, and everything local works meanwhile.
        let client = match SignalClient::connect(
            &signal_url,
            GroupId::derive(&master),
            MemberId::derive(&master, identity.fingerprint().as_bytes()),
            endpoints.clone(),
        )
        .await
        {
            Ok(client) => Some(client),
            Err(e) => {
                tracing::info!(
                    error = %e,
                    "no rendezvous service; devices on this network can still find each other"
                );
                None
            }
        };

        let (signal, commands) = mpsc::unbounded_channel();

        // Reconnecting, not just connecting. A rendezvous service restarts --
        // for a deploy, a reboot, a crash -- and before this a device whose
        // connection went with it stayed silent until the daemon itself was
        // restarted. It kept syncing on its timer, so nothing looked broken;
        // it simply stopped being reachable and stopped being able to say it
        // had news. Found by restarting the service during a test.
        let reconnect = Reconnect {
            url: signal_url,
            group: GroupId::derive(&master),
            member: MemberId::derive(&master, identity.fingerprint().as_bytes()),
        };
        tokio::spawn(stay_signalled(
            client,
            reconnect,
            commands,
            endpoints.clone(),
            endpoint.clone(),
            identity.clone(),
            arrivals_tx,
        ));

        // Beacons on the local network, so that two devices on one Wi-Fi need
        // nothing else at all. Best effort for the same reason as the
        // rendezvous: a network that blocks multicast is a network where this
        // does not work, not one where qurb does not start.
        let neighbours = Neighbours::new();
        let beacons = match beacon_port {
            None => None,
            Some(port) => {
                let me = MemberId::derive(&master, identity.fingerprint().as_bytes());
                match Beacons::start(master.clone(), me, endpoints.clone(), port) {
                    Ok((beacons, sightings)) => {
                        tokio::spawn(follow_beacons(
                            sightings,
                            neighbours.clone(),
                            arrivals.clone(),
                        ));
                        Some(beacons)
                    }
                    Err(e) => {
                        tracing::info!(error = %e, "no local discovery on this network");
                        None
                    }
                }
            }
        };

        Ok(Self {
            endpoint,
            identity,
            master,
            endpoints,
            relay,
            signal,
            arrivals,
            neighbours,
            beacons,
        })
    }

    /// Listen for peers announcing themselves to the rendezvous service.
    ///
    /// Each item is the rendezvous identifier of a device that has just become
    /// reachable. Translate it with [`MemberId::derive`] against the
    /// fingerprints you care about — the identifier is blinded, so the service
    /// cannot link it to a device and neither can a listener without the master
    /// key.
    ///
    /// A late subscriber misses earlier arrivals, which is deliberate: this is
    /// a nudge to try now, not a log to be replayed.
    pub fn arrivals(&self) -> broadcast::Receiver<MemberId> {
        self.arrivals.subscribe()
    }

    /// Say how this device can be woken while it is not connected.
    ///
    /// For devices that cannot hold a socket open — phones. A desktop needs
    /// nothing here: it is already connected, and the service can simply tell
    /// it. `None` withdraws a token that is no longer valid.
    pub fn reachable_via(&self, token: Option<String>) -> Result<()> {
        self.signal
            .send(Command::Reachable { via: token })
            .map_err(|_| Error::Signalling { detail: "signalling has stopped".into() })
    }

    /// Tell a peer there is something for it.
    ///
    /// Sent through the rendezvous service, which forwards it if the peer is
    /// connected and keeps it if not — so a device asleep at the moment of a
    /// change learns of it on waking rather than at its own next poll.
    ///
    /// Carries who, never what. Best effort: a peer that never hears it syncs
    /// on its own schedule, which is what happens today.
    /// Devices currently visible on this network.
    pub fn neighbours(&self) -> &Neighbours {
        &self.neighbours
    }

    /// Say at once that there is news, on every channel there is.
    ///
    /// The beacon is what makes a change on one device reach another on the
    /// same Wi-Fi in about a second with no server involved; the rendezvous
    /// message covers the peer that is somewhere else.
    pub async fn announce_news(&self) {
        if let Some(beacons) = &self.beacons {
            beacons.announce_news().await;
        }
    }

    pub fn tell_waiting(&self, peer: Fingerprint) -> Result<()> {
        let to = MemberId::derive(&self.master, peer.as_bytes());
        self.signal
            .send(Command::Waiting { to })
            .map_err(|_| Error::Signalling { detail: "signalling has stopped".into() })
    }

    /// Reach a peer through the relay without trying a direct path first.
    ///
    /// Ordinarily [`reach`](Self::reach) falls back on its own. This exists for
    /// the cases where trying direct is known to be pointless — a network that
    /// has already been classified as blocking UDP — and for testing the
    /// fallback without having to arrange a real failure.
    pub async fn reach_via_relay(&self, peer: Fingerprint) -> Result<PeerClient> {
        let relay = self.relay.as_ref().ok_or(Error::NoRelay)?;
        self.via_relay(peer, relay).await
    }

    /// The endpoint accepting relayed connections, if a relay is configured.
    ///
    /// A device has two ways in and must listen on both.
    pub fn relay_endpoint(&self) -> Option<&quinn::Endpoint> {
        self.relay.as_ref().map(|r| &r.endpoint)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.endpoint
            .local_addr()
            .map_err(|e| Error::Io { path: "local_addr".into(), source: e })
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    pub fn endpoint(&self) -> &quinn::Endpoint {
        &self.endpoint
    }

    /// Say where this device is, again.
    ///
    /// Called when the addresses change, which a laptop moving between networks
    /// does several times a day.
    pub fn reannounce(&self, endpoints: Endpoints) -> Result<()> {
        if let Some(beacons) = &self.beacons {
            beacons.now_at(endpoints.clone());
        }
        let _ = endpoints;
        // The signalling task holds the connection; re-announcing through it is
        // the next thing to add here.
        Ok(())
    }

    /// Reach a peer, given the fingerprint pairing established.
    ///
    /// Asks the rendezvous service to introduce them, waits to be told to punch,
    /// then races every address the peer offered.
    pub async fn reach(&self, peer: Fingerprint) -> Result<PeerClient> {
        let target = MemberId::derive(&self.master, peer.as_bytes());

        // Somebody who beaconed from this network moments ago is both the
        // likeliest to answer and the cheapest to try, and reaching them
        // involves nobody else at all. Asked first, and on success the
        // rendezvous is never troubled.
        if let Some(here) = self.neighbours.where_is(&target) {
            match self.race(peer, &here).await {
                Ok(client) => {
                    tracing::debug!(peer = %peer.short(), "reached on the local network");
                    return Ok(client);
                }
                // The sighting was stale, or the address moved between the
                // beacon and now. Fall through and ask properly.
                Err(e) => tracing::debug!(
                    peer = %peer.short(),
                    error = %e,
                    "the local address did not answer; asking the rendezvous"
                ),
            }
        }

        let (reply, answer) = oneshot::channel();

        self.signal
            .send(Command::Introduce { to: target, reply })
            .map_err(|_| Error::Signalling { detail: "signalling has stopped".into() })?;

        let endpoints = match tokio::time::timeout(RENDEZVOUS_TIMEOUT, answer).await {
            Ok(Ok(result)) => result?,
            Ok(Err(_)) => return Err(Error::Signalling { detail: "signalling has stopped".into() }),
            Err(_) => return Err(Error::PeerDidNotAnswer),
        };

        match self.race(peer, &endpoints).await {
            Ok(client) => Ok(client),
            Err(direct) => {
                // Every address failed. This is what the relay is for, and it is
                // the only moment at which paying for it is justified.
                let Some(relay) = &self.relay else {
                    return Err(direct);
                };
                tracing::info!(
                    peer = %peer.short(),
                    "no direct path; falling back to the relay"
                );
                self.via_relay(peer, relay).await
            }
        }
    }

    /// Reach a peer the long way round.
    async fn via_relay(&self, peer: Fingerprint, relay: &RelayPath) -> Result<PeerClient> {
        let target = *MemberId::derive(&self.master, peer.as_bytes()).as_bytes();
        let address = relay
            .socket
            .address_for(target)
            .map_err(|e| Error::Signalling { detail: format!("relay: {e}") })?;

        let config = tls::client_config(&self.identity, peer)?;
        let connecting = relay
            .endpoint
            .connect_with(config, address, "qurb-device")
            .map_err(Error::Connect)?;

        // The same pinned identity as a direct connection. The relay carries the
        // handshake without being party to it, so nothing about trust changes
        // because the path got longer.
        match tokio::time::timeout(CANDIDATE_TIMEOUT, connecting).await {
            Ok(Ok(connection)) => {
                tracing::info!(peer = %peer.short(), "connected via the relay");
                Ok(PeerClient::from_parts(relay.endpoint.clone(), connection))
            }
            Ok(Err(e)) => Err(Error::Connection(e)),
            Err(_) => Err(Error::Unreachable { peer: peer.short() }),
        }
    }

    /// Try every candidate at once and keep the first that answers.
    ///
    /// In parallel rather than in turn: a candidate that is simply unreachable
    /// fails by timing out, and trying three in sequence would mean waiting
    /// three timeouts to discover the last one worked. Local addresses are
    /// listed first, so when several succeed the cheapest path is preferred.
    pub async fn race(&self, peer: Fingerprint, endpoints: &Endpoints) -> Result<PeerClient> {
        let candidates = endpoints.candidates();
        if candidates.is_empty() {
            return Err(Error::NoCandidates);
        }

        type Attempt = std::pin::Pin<
            Box<dyn std::future::Future<Output = Option<(SocketAddr, quinn::Connection)>> + Send>,
        >;
        let mut attempts: Vec<Attempt> = Vec::new();
        for candidate in candidates {
            let config = tls::client_config(&self.identity, peer)?;
            let Ok(connecting) = self.endpoint.connect_with(config, candidate, "qurb-device")
            else {
                continue;
            };
            let attempt: Attempt = Box::pin(async move {
                match tokio::time::timeout(CANDIDATE_TIMEOUT, connecting).await {
                    Ok(Ok(connection)) => Some((candidate, connection)),
                    _ => None,
                }
            });
            attempts.push(attempt);
        }

        while !attempts.is_empty() {
            let (outcome, _index, rest) = futures_select(attempts).await;
            if let Some((candidate, connection)) = outcome {
                tracing::info!(peer = %peer.short(), %candidate, "connected");

                // Close the other paths as they land, rather than dropping
                // them and leaving the peer to time them out.
                //
                // Dropping a handshake that has not finished cancels it, which
                // is free. Dropping one that *has* finished leaves a
                // connection established at the far end, holding the send and
                // receive buffers of a transfer nobody will use — and a device
                // reachable on both a local network and an overlay offers
                // several addresses that all work, so this is the ordinary
                // case rather than a rare one. Measured: racing four addresses
                // instead of one grew a 128 MiB transfer's memory from about
                // 30 MiB to 105.
                //
                // Spawned, so the caller gets its connection now and the
                // tidying happens behind it.
                tokio::spawn(async move {
                    for attempt in rest {
                        if let Some((_, spare)) = attempt.await {
                            spare.close(0u32.into(), b"another path won");
                        }
                    }
                });

                return Ok(PeerClient::from_parts(self.endpoint.clone(), connection));
            }
            attempts = rest;
        }

        Err(Error::Unreachable { peer: peer.short() })
    }
}

/// What it takes to open the signalling connection again.
struct Reconnect {
    url: String,
    group: GroupId,
    member: MemberId,
}

/// How long to wait before trying the rendezvous service again.
///
/// Doubling from a second to a minute. Short at first because the common cause
/// is a restart that takes seconds, and capped because a service that is down
/// for an hour should not be asked sixty times a minute — nor left unasked for
/// an hour once it returns.
const RECONNECT_FLOOR: Duration = Duration::from_secs(1);
const RECONNECT_CEILING: Duration = Duration::from_secs(60);

/// Keep a signalling connection up for as long as the device runs.
///
/// Each connection is served by [`run_signalling`] until it closes; this
/// reopens it. Announcing happens on every connection, because to the service
/// a reconnected device is a device it has never heard of.
#[allow(clippy::too_many_arguments)]
async fn stay_signalled(
    first: Option<SignalClient>,
    reconnect: Reconnect,
    mut commands: mpsc::UnboundedReceiver<Command>,
    endpoints: Endpoints,
    endpoint: quinn::Endpoint,
    identity: Identity,
    arrivals_tx: broadcast::Sender<MemberId>,
) {
    let mut client = first;
    let mut wait = RECONNECT_FLOOR;

    loop {
        let connected = match client.take() {
            Some(connected) => connected,
            None => {
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(RECONNECT_CEILING);
                match SignalClient::connect(
                    &reconnect.url,
                    reconnect.group,
                    reconnect.member,
                    endpoints.clone(),
                )
                .await
                {
                    Ok(fresh) => {
                        tracing::info!("reconnected to the rendezvous service");
                        fresh
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "cannot reach the rendezvous service");
                        continue;
                    }
                }
            }
        };

        // A connection that lasted is evidence the service is healthy, so the
        // next outage starts its backoff from the floor rather than from
        // wherever the last one ended.
        wait = RECONNECT_FLOOR;

        run_signalling(
            connected,
            &mut commands,
            endpoints.clone(),
            endpoint.clone(),
            identity.clone(),
            arrivals_tx.clone(),
        )
        .await;

        // `run_signalling` returns only when the connection is gone or the
        // commands channel is closed. The second means the Connector was
        // dropped and there is nothing left to serve.
        if commands.is_closed() {
            return;
        }
        tracing::debug!("the rendezvous connection dropped; reopening");
    }
}

/// The one task that owns the signalling connection.
///
/// It answers requests to connect, hands each `Punch` to whoever asked for it,
/// and punches on this device's behalf when the request came from someone else.
/// Multiplexing here rather than opening a connection per call is what keeps a
/// device reachable while it is busy reaching somebody.
async fn run_signalling(
    mut client: SignalClient,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    endpoints: Endpoints,
    endpoint: quinn::Endpoint,
    identity: Identity,
    arrivals_tx: broadcast::Sender<MemberId>,
) {
    let mut waiting: HashMap<MemberId, oneshot::Sender<Result<Endpoints>>> = HashMap::new();

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Waiting { to }) => {
                    // Failing is not worth reporting. The peer syncs on its
                    // own schedule regardless; this only makes it sooner.
                    let _ = client.waiting_for(to);
                }

                Some(Command::Reachable { via }) => {
                    let _ = client.reachable_via(via);
                }

                Some(Command::Introduce { to, reply }) => {
                    if client.connect_to(to).is_err() {
                        let _ = reply.send(Err(Error::Signalling {
                            detail: "signalling connection lost".into(),
                        }));
                        return;
                    }
                    waiting.insert(to, reply);
                }
                None => return,
            },

            message = client.next() => match message {
                Some(FromServer::ConnectRequest { from, .. }) => {
                    // Agreeing is what releases the simultaneous punch.
                    if client.accept(from, endpoints.clone()).is_err() {
                        return;
                    }
                }

                Some(FromServer::Punch { peer, endpoints: theirs }) => {
                    match waiting.remove(&peer) {
                        // We asked for this one.
                        Some(reply) => {
                            let _ = reply.send(Ok(theirs));
                        }
                        // Somebody asked for us. Dial back purely to punch: this
                        // device's router has seen nothing go out to the caller,
                        // so without it the caller's packets arrive somewhere
                        // that has never heard of them.
                        None => {
                            for candidate in theirs.candidates() {
                                knock(&endpoint, &identity, candidate);
                            }
                        }
                    }
                }

                Some(FromServer::Error { detail }) => {
                    // Not addressed to a particular request, so fail everything
                    // outstanding rather than leaving callers waiting.
                    for (_, reply) in waiting.drain() {
                        let _ = reply.send(Err(Error::Signalling { detail: detail.clone() }));
                    }
                }

                Some(FromServer::Peers { .. }) => {}

                Some(FromServer::Appeared { peer }) => {
                    // No receivers is the ordinary case -- nothing is obliged
                    // to care -- so a send error is not a problem.
                    let _ = arrivals_tx.send(peer.member);
                }

                // Somebody has something for this device. Reported on the same
                // channel as an arrival, because it asks for the same thing:
                // sync with that peer, now. The distinction between "they just
                // appeared" and "they have news" does not change what to do.
                Some(FromServer::Waiting { from }) => {
                    tracing::debug!(?from, "a peer says it has something for us");
                    let _ = arrivals_tx.send(from);
                }

                None => {
                    for (_, reply) in waiting.drain() {
                        let _ = reply.send(Err(Error::Signalling {
                            detail: "signalling connection closed".into(),
                        }));
                    }
                    return;
                }
            },
        }
    }
}

/// Start a handshake and abandon it. Its packets are the punch.
fn knock(endpoint: &quinn::Endpoint, identity: &Identity, candidate: SocketAddr) {
    let Ok(config) = tls::punch_config(identity, identity.fingerprint()) else { return };
    if let Ok(connecting) = endpoint.connect_with(config, candidate, "qurb-device") {
        tokio::spawn(async move {
            // It will fail: we pinned our own fingerprint, which the peer does
            // not have. Failing is fine. The packets left the building.
            let _ = tokio::time::timeout(Duration::from_secs(2), connecting).await;
        });
    }
}

/// Wait for whichever future finishes first, returning the rest.
///
/// Hand-rolled rather than pulling in a combinator library for one use: the
/// losers must be *kept* until a winner emerges, because dropping them would
/// cancel handshakes that might still be the only ones that work.
async fn futures_select<T>(
    mut futures: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>>,
) -> (T, usize, Vec<std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>>) {
    std::future::poll_fn(move |cx| {
        for i in 0..futures.len() {
            if let std::task::Poll::Ready(value) = futures[i].as_mut().poll(cx) {
                // Dropped on purpose: this one has finished.
                drop(futures.remove(i));
                return std::task::Poll::Ready((value, i, std::mem::take(&mut futures)));
            }
        }
        std::task::Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Binding every interface must not announce "every interface".
    ///
    /// `0.0.0.0` is what `local_addr` reports for a socket bound to all of
    /// them, and it is meaningless to a peer: it names every interface on *this*
    /// machine, and the peer has no way to turn that into somewhere to dial.
    ///
    /// Ordinarily STUN supplies a public address and the useless local one does
    /// no harm. This matters when there is no STUN — two devices on a network
    /// with no route to the internet — which is exactly when the local address
    /// is the only one there is.
    #[test]
    fn an_unspecified_bind_is_announced_as_something_dialable() {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind");
        let local = socket.local_addr().expect("local_addr");
        assert!(local.ip().is_unspecified(), "the test needs an unspecified bind");

        let announced = nat::dialable(local);
        assert!(!announced.ip().is_unspecified(), "0.0.0.0 was announced to peers");
        assert_eq!(announced.port(), local.port(), "the port must survive");
    }
}
