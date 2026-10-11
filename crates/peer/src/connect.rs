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
    /// Everything started in the background, stopped when this is dropped.
    ///
    /// A daemon keeps one connector for hours; a phone builds one per sync
    /// pass and drops it at the end. Before this was kept, the phone's passes
    /// each left a reconnect loop, a beacon sender and a beacon listener
    /// running for the life of the process -- and the reconnect loop held the
    /// QUIC endpoint, so its socket stayed open too.
    background: Vec<tokio::task::AbortHandle>,
    /// Where the rendezvous service is, for meetings started later.
    signal_url: String,
    /// Devices of other people this one meets under a secret of their own
    /// (decision 0060): a guest of this computer, or a computer this device
    /// visits. Each has its own rendezvous session.
    meetings: std::sync::Mutex<std::collections::HashMap<Fingerprint, Meeting>>,
}

/// A rendezvous session with one device of another person.
struct Meeting {
    secret: [u8; 32],
    signal: mpsc::UnboundedSender<Command>,
    running: tokio::task::AbortHandle,
}

impl Drop for Connector {
    fn drop(&mut self) {
        for running in &self.background {
            running.abort();
        }
        if let Ok(meetings) = self.meetings.lock() {
            for meeting in meetings.values() {
                meeting.running.abort();
            }
        }
        if let Some(beacons) = &self.beacons {
            beacons.stop();
        }
    }
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

/// Whether `text` has the shape of a relay's address: a host and a port.
///
/// The host may be a name or an address -- `relay.example.com:9001`,
/// `203.0.113.5:9001`, `[2001:db8::1]:9001` -- because a relay on a server of
/// one's own is usually known by name, and names outlive addresses. Checked
/// without the network, so a setting can be refused as it is typed; whether
/// the name resolves is a question for [`resolve_relay`], asked each time a
/// device starts syncing.
pub fn relay_address_ok(text: &str) -> std::result::Result<(), String> {
    let text = text.trim();
    let Some((host, port)) = text.rsplit_once(':') else {
        return Err(format!("{text} has no port — a relay is host:port, like relay.example.com:9001"));
    };
    if port.parse::<u16>().map_or(true, |p| p == 0) {
        return Err(format!("{port} is not a port"));
    }
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(format!("{text} has no host before the port"));
    }
    Ok(())
}

/// Where a relay is, now: a name looked up, or an address taken as it is.
///
/// Looked up on each start rather than once, because a phone that moves from
/// Wi-Fi to a mobile network may get a different answer -- on an IPv6-only
/// network with NAT64, a synthesised address for an IPv4-only server -- and
/// because a server's address changes more often than its name.
///
/// Of the answers, the first that accepts a connection, tried in the order
/// the system's resolver prefers. Not simply the first answer: a name with an
/// IPv6 and an IPv4 address, served only on one of them, is an ordinary
/// server, and was found as `localhost` resolving to `::1` for a relay
/// listening on `127.0.0.1`. The probe is a TCP handshake that registers
/// nothing, which the relay closes. Bounded throughout, since a resolver or
/// an address that does not answer must not hold up a sync that may not need
/// the relay at all.
pub async fn resolve_relay(text: &str) -> Result<SocketAddr> {
    relay_address_ok(text).map_err(|detail| Error::Signalling { detail: format!("relay: {detail}") })?;
    let answers: Vec<SocketAddr> =
        tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host(text.trim()))
            .await
            .map_err(|_| Error::Signalling { detail: format!("relay: looking up {text} took too long") })?
            .map_err(|e| Error::Signalling { detail: format!("relay: {text}: {e}") })?
            .collect();
    for address in &answers {
        let probe = tokio::time::timeout(Duration::from_secs(3), tokio::net::TcpStream::connect(address));
        if let Ok(Ok(_)) = probe.await {
            return Ok(*address);
        }
    }
    Err(Error::Signalling {
        detail: match answers.is_empty() {
            true => format!("relay: {text} has no address"),
            false => format!("relay: nothing answers at {text}"),
        },
    })
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
        //
        // Best effort, like the rendezvous below. A relay that cannot be
        // reached costs the fallback, not the device: it used to stop a device
        // starting at all, so a server of one's own being down meant a phone
        // could not sync with the laptop beside it on the same Wi-Fi.
        let relay = match relay {
            Some(address) => {
                let me = *MemberId::derive(&master, identity.fingerprint().as_bytes()).as_bytes();
                match qurb_relay::RelaySocket::connect(address, me).await {
                    Ok(socket) => {
                        let server_config = tls::server_config(&identity, allowed)?;
                        let endpoint =
                            qurb_relay::endpoint_over(Arc::clone(&socket), Some(server_config))
                                .map_err(|e| Error::Signalling { detail: format!("relay: {e}") })?;
                        Some(RelayPath { socket, endpoint })
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, %address, "the relay cannot be reached; direct paths only");
                        None
                    }
                }
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
            url: signal_url.clone(),
            group: GroupId::derive(&master),
            member: MemberId::derive(&master, identity.fingerprint().as_bytes()),
            straight_away: false,
        };
        let mut background = vec![tokio::spawn(stay_signalled(
            client,
            reconnect,
            commands,
            endpoints.clone(),
            endpoint.clone(),
            identity.clone(),
            arrivals_tx,
        ))
        .abort_handle()];

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
                        background.push(
                            tokio::spawn(follow_beacons(
                                sightings,
                                neighbours.clone(),
                                arrivals.clone(),
                            ))
                            .abort_handle(),
                        );
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
            background,
            signal_url,
            meetings: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Meet a device of another person under the secret the two were given
    /// when one welcomed the other as a guest (decision 0060): announce in a
    /// group of their own at the rendezvous service, so that each can reach
    /// the other and tell it there is news. Once per device; asking again
    /// changes nothing.
    pub fn meet(&self, peer: Fingerprint, secret: [u8; 32]) {
        let Ok(mut meetings) = self.meetings.lock() else { return };
        if meetings.contains_key(&peer) {
            return;
        }
        let (signal, commands) = mpsc::unbounded_channel();
        let member = MemberId::for_meeting(&secret, self.identity.fingerprint().as_bytes());
        // On the relay under the meeting's name too, so a device that cannot
        // reach this one directly can still come the long way round (decision
        // 0061). Best effort, as the relay always is.
        if let Some(relay) = &self.relay {
            if let Err(e) = relay.socket.register(*member.as_bytes()) {
                tracing::warn!(error = %e, "could not be reached through the relay in a meeting");
            }
        }
        let reconnect = Reconnect {
            url: self.signal_url.clone(),
            group: GroupId::for_meeting(&secret),
            member,
            straight_away: true,
        };
        let running = tokio::spawn(stay_signalled(
            None,
            reconnect,
            commands,
            self.endpoints.clone(),
            self.endpoint.clone(),
            self.identity.clone(),
            self.arrivals.clone(),
        ))
        .abort_handle();
        meetings.insert(peer, Meeting { secret, signal, running });
    }

    /// The identifier `peer` announces under, where this device would look
    /// for it: in a meeting for another person's device, by this person's key
    /// for one of their own.
    pub fn member_of(&self, peer: &Fingerprint) -> MemberId {
        match self.meetings.lock().ok().and_then(|m| m.get(peer).map(|m| m.secret)) {
            Some(secret) => MemberId::for_meeting(&secret, peer.as_bytes()),
            None => MemberId::derive(&self.master, peer.as_bytes()),
        }
    }

    /// The rendezvous session `peer` is reached through, and what it is
    /// called there.
    fn session_for(&self, peer: &Fingerprint) -> (mpsc::UnboundedSender<Command>, MemberId, bool) {
        if let Some(meeting) = self.meetings.lock().ok().and_then(|m| {
            m.get(peer).map(|m| (m.signal.clone(), m.secret))
        }) {
            let (signal, secret) = meeting;
            return (signal, MemberId::for_meeting(&secret, peer.as_bytes()), true);
        }
        (self.signal.clone(), MemberId::derive(&self.master, peer.as_bytes()), false)
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

    /// Where a peer is on this network, waiting a moment if nobody has said
    /// yet.
    ///
    /// A device that has just started has heard nothing, and the answers to its
    /// own arrival probe take a few hundred milliseconds to come back. Failing
    /// in that window would mean a short-lived process — a phone's sync pass —
    /// never using local discovery at all, which is exactly what happened.
    ///
    /// The wait is short and only paid when the answer is not already known,
    /// which is once per connector rather than once per peer.
    async fn nearby(&self, target: &MemberId) -> Option<Endpoints> {
        if let Some(here) = self.neighbours.where_is(target) {
            return Some(here);
        }

        let beacons = self.beacons.as_ref()?;
        beacons.probe().await;

        let deadline = tokio::time::Instant::now() + LOCAL_ANSWER_WINDOW;
        while tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if let Some(here) = self.neighbours.where_is(target) {
                return Some(here);
            }
        }
        None
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
        let (signal, to, _) = self.session_for(&peer);
        signal
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
        let (signal, target, meeting) = self.session_for(&peer);
        if meeting {
            // Another person's device, met under a secret of their own: not
            // in this person's beacons, which are keyed to this person's key
            // (decision 0060). The rendezvous service carries its local
            // addresses as well as its public ones; and the relay knows it by
            // the meeting's name for it, when no address answers (0061).
            let (reply, answer) = oneshot::channel();
            signal
                .send(Command::Introduce { to: target, reply })
                .map_err(|_| Error::Signalling { detail: "signalling has stopped".into() })?;
            let endpoints = match tokio::time::timeout(RENDEZVOUS_TIMEOUT, answer).await {
                Ok(Ok(result)) => result?,
                Ok(Err(_)) => return Err(Error::Signalling { detail: "signalling has stopped".into() }),
                Err(_) => return Err(Error::PeerDidNotAnswer),
            };
            return match self.race(peer, &endpoints).await {
                Ok(client) => Ok(client),
                Err(direct) => match &self.relay {
                    Some(relay) => {
                        tracing::info!(peer = %peer.short(), "no direct path to a meeting's device; falling back to the relay");
                        self.via_relay(peer, relay).await
                    }
                    None => Err(direct),
                },
            };
        }

        // Somebody who beaconed from this network moments ago is both the
        // likeliest to answer and the cheapest to try, and reaching them
        // involves nobody else at all. Asked first, and on success the
        // rendezvous is never troubled.
        //
        // Asked *after* giving discovery a moment, because a device that has
        // only just started has an empty address book and the answers to its
        // probe are still in flight. Found on a phone, which builds a fresh
        // connector for every sync pass and so is always in that state: it
        // heard the laptop four hundred milliseconds after starting, and had
        // already given up by then.
        if let Some(here) = self.nearby(&target).await {
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
        // By the names the two use: a meeting's for another person's device,
        // this person's for one of their own (decision 0061).
        let me = self.identity.fingerprint();
        let (target, as_me) = match self.meetings.lock().ok().and_then(|m| m.get(&peer).map(|m| m.secret)) {
            Some(secret) => (MemberId::for_meeting(&secret, peer.as_bytes()), MemberId::for_meeting(&secret, me.as_bytes())),
            None => (MemberId::derive(&self.master, peer.as_bytes()), MemberId::derive(&self.master, me.as_bytes())),
        };
        let (target, as_me) = (*target.as_bytes(), *as_me.as_bytes());
        let address = relay
            .socket
            .address_for_as(target, as_me)
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
                Ok(PeerClient::from_parts(relay.endpoint.clone(), connection).through_relay())
            }
            Ok(Err(e)) => Err(Error::Connection(e)),
            Err(_) => Err(Error::Unreachable {
                peer: peer.short(),
                tried: "the relay timed out".into(),
            }),
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

        // Each attempt ends in a connection or in why not, and the reasons are
        // kept: when every one fails, which failed how is the whole diagnosis.
        type Attempted = std::result::Result<(SocketAddr, quinn::Connection), String>;
        type Attempt = std::pin::Pin<Box<dyn std::future::Future<Output = Attempted> + Send>>;
        let mut attempts: Vec<Attempt> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        for candidate in candidates {
            let config = tls::client_config(&self.identity, peer)?;
            let connecting = match self.endpoint.connect_with(config, candidate, "qurb-device") {
                Ok(connecting) => connecting,
                Err(e) => {
                    failures.push(format!("{candidate} {e}"));
                    continue;
                }
            };
            let attempt: Attempt = Box::pin(async move {
                match tokio::time::timeout(CANDIDATE_TIMEOUT, connecting).await {
                    Ok(Ok(connection)) => Ok((candidate, connection)),
                    Ok(Err(e)) => Err(format!("{candidate} {e}")),
                    Err(_) => Err(format!("{candidate} timed out")),
                }
            });
            attempts.push(attempt);
        }

        while !attempts.is_empty() {
            let (outcome, _index, rest) = futures_select(attempts).await;
            if let Err(why) = &outcome {
                failures.push(why.clone());
            }
            if let Ok((candidate, connection)) = outcome {
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
                        if let Ok((_, spare)) = attempt.await {
                            spare.close(0u32.into(), b"another path won");
                        }
                    }
                });

                return Ok(PeerClient::from_parts(self.endpoint.clone(), connection));
            }
            attempts = rest;
        }

        Err(Error::Unreachable {
            peer: peer.short(),
            tried: failures.join(", "),
        })
    }
}

/// What it takes to open the signalling connection again.
struct Reconnect {
    url: String,
    group: GroupId,
    member: MemberId,
    /// Connect at once the first time, rather than after the backoff: a
    /// meeting started later has had no attempt yet (decision 0060).
    straight_away: bool,
}

/// How long to wait before trying the rendezvous service again.
///
/// Doubling from a second to a minute. Short at first because the common cause
/// is a restart that takes seconds, and capped because a service that is down
/// for an hour should not be asked sixty times a minute — nor left unasked for
/// an hour once it returns.
/// How long to wait for an answer to a local probe before giving up on the
/// network and asking the rendezvous service.
///
/// A beacon and its reply cross one network segment, so an answer that is
/// coming arrives in single-digit milliseconds. A second is generous enough to
/// survive a lossy first packet and short enough that a device which really is
/// elsewhere is not kept waiting for one.
const LOCAL_ANSWER_WINDOW: Duration = Duration::from_millis(1000);

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
    let mut straight_away = reconnect.straight_away;

    loop {
        let connected = match client.take() {
            Some(connected) => connected,
            None => {
                if !std::mem::take(&mut straight_away) {
                    tokio::time::sleep(wait).await;
                    wait = (wait * 2).min(RECONNECT_CEILING);
                }
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

    #[test]
    fn a_relay_address_is_a_host_and_a_port() {
        for good in ["relay.example.com:9001", "203.0.113.5:9001", "[2001:db8::1]:9001", " r.example:443 "] {
            assert!(relay_address_ok(good).is_ok(), "{good} refused");
        }
        for bad in ["relay.example.com", "203.0.113.5", ":9001", "relay.example.com:0", "relay.example.com:99999", "a b:9001", ""] {
            assert!(relay_address_ok(bad).is_err(), "{bad} accepted");
        }
    }

    #[tokio::test]
    async fn a_relay_name_is_looked_up_to_an_address_that_answers() {
        // Listening on IPv4 only: `localhost` also resolves to `::1`, which
        // here answers nothing, and the lookup has to get past it.
        let listening = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listening.local_addr().unwrap().port();

        let by_address = resolve_relay(&format!("127.0.0.1:{port}")).await.unwrap();
        assert_eq!(by_address, format!("127.0.0.1:{port}").parse().unwrap());
        let by_name = resolve_relay(&format!("localhost:{port}")).await.unwrap();
        assert_eq!(by_name, by_address, "took an address that does not answer");

        drop(listening);
        assert!(resolve_relay(&format!("127.0.0.1:{port}")).await.is_err(), "nothing answers now");
        // `.invalid` is reserved never to resolve.
        assert!(resolve_relay("nowhere.invalid:9001").await.is_err());
        assert!(resolve_relay("no-port.example.com").await.is_err());
    }

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
