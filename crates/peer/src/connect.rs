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
}

/// What the signalling task is asked to do.
enum Command {
    Introduce { to: MemberId, reply: oneshot::Sender<Result<Endpoints>> },
}

struct RelayPath {
    socket: Arc<qurb_relay::RelaySocket>,
    endpoint: quinn::Endpoint,
}

impl Connector {
    /// Bind, discover, and get ready to dial or be dialled.
    ///
    /// `allowed` is the guest list for incoming connections, which in practice
    /// comes from the trust store. `discover` controls whether to ask STUN —
    /// tests on one machine have nothing to discover and should not reach for
    /// the network to find that out.
    pub async fn start(
        bind: SocketAddr,
        identity: Identity,
        master: MasterKey,
        allowed: &tls::TrustList,
        signal_url: impl Into<String>,
        discover: bool,
        relay: Option<SocketAddr>,
    ) -> Result<Self> {
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
        let client = SignalClient::connect(
            &signal_url,
            GroupId::derive(&master),
            MemberId::derive(&master, identity.fingerprint().as_bytes()),
            endpoints.clone(),
        )
        .await
        .map_err(|e| Error::Signalling { detail: e.to_string() })?;

        let (signal, commands) = mpsc::unbounded_channel();
        tokio::spawn(run_signalling(
            client,
            commands,
            endpoints.clone(),
            endpoint.clone(),
            identity.clone(),
            arrivals_tx,
        ));

        Ok(Self { endpoint, identity, master, endpoints, relay, signal, arrivals })
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

/// The one task that owns the signalling connection.
///
/// It answers requests to connect, hands each `Punch` to whoever asked for it,
/// and punches on this device's behalf when the request came from someone else.
/// Multiplexing here rather than opening a connection per call is what keeps a
/// device reachable while it is busy reaching somebody.
async fn run_signalling(
    mut client: SignalClient,
    mut commands: mpsc::UnboundedReceiver<Command>,
    endpoints: Endpoints,
    endpoint: quinn::Endpoint,
    identity: Identity,
    arrivals_tx: broadcast::Sender<MemberId>,
) {
    let mut waiting: HashMap<MemberId, oneshot::Sender<Result<Endpoints>>> = HashMap::new();

    loop {
        tokio::select! {
            command = commands.recv() => match command {
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
