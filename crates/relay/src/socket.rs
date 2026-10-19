//! A relay connection, pretending to be a UDP socket.
//!
//! This is what makes the relay safe. Rather than forwarding application
//! messages — which would mean the relay handling data it could tamper with, and
//! us inventing a second encryption layer to stop it — the relay carries
//! **datagrams**, and an ordinary QUIC session runs inside.
//!
//! Everything above is unchanged: the same pinned certificates, the same
//! handshake, the same end-to-end encryption. The relay sees ciphertext
//! addressed to an identifier it cannot connect to a person, and can do nothing
//! with it but pass it on or drop it. Dropping is a denial of service, which is
//! true of any router.
//!
//! # The synthetic address
//!
//! QUIC addresses peers by `SocketAddr`, and a relayed peer has none that means
//! anything. So the socket hands each peer an address from the documentation
//! range — never routed, so one appearing in a log is unambiguously relayed —
//! and translates between that and the peer's relay identifier.
//!
//! The mapping is two-way and allocated on demand, because a device does not
//! know in advance who will call it. A socket opened only to dial one peer would
//! be unable to listen, and a device that cannot be reached over the relay is
//! half a fallback.
//!
//! # More than one name
//!
//! A device registers under its own identifier, and may [`register`] more:
//! one for each meeting with another person's device (decision 0061), whose
//! name for it is not its own. Each synthetic address then stands for a pair
//! -- the peer, and which of this device's names the two use -- so a reply
//! goes out under the name it was called by.
//!
//! # Staying on
//!
//! A relay restarts -- for a deploy, a reboot, a crash -- and the rendezvous
//! service carrying one does too (decision 0068). Until 2026-10-18 a socket
//! whose connection went stayed gone, and told QUIC so, which ends a whole
//! endpoint: a laptop whose relay restarted, or which started before it at
//! login, could not be reached through it again until it was itself
//! restarted. A socket that knows how to dial ([`RelaySocket::redialling`],
//! and [`connect`](RelaySocket::connect)) dials again instead, waiting longer
//! each time up to a minute, and registers every name it holds on the new
//! connection. Meanwhile what QUIC sends is dropped, as a router with no route
//! drops it, and a session through it either outlives the gap or times out
//! as over any lost path. [`RelaySocket::link`] says which it is now, so a
//! device does not dial through a relay that is not there.
//!
//! [`register`]: RelaySocket::register

use crate::error::{Error, Result};
use crate::frame::{Frame, RelayId, MAX_FRAME};
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};

/// The address a relayed peer appears to have.
///
/// From the documentation range reserved by RFC 5737, which is guaranteed never
/// to be routed. If this ever shows up in a log it is unambiguously a relayed
/// peer rather than somewhere a packet was really sent.
const LOCAL_ADDRESS: &str = "192.0.2.255:9";

/// How many relayed peers one socket can address at once.
///
/// The documentation range gives 254 usable hosts, and a personal sync system
/// talks to a handful of devices. Reaching this means something is wrong rather
/// than popular.
const MAX_PEERS: u32 = 254;

/// The first wait before dialling a relay again, doubled each time it fails.
const RETRY_FIRST: Duration = Duration::from_secs(1);

/// The longest wait between attempts: a relay back up is found within this.
const RETRY_MOST: Duration = Duration::from_secs(60);

/// How long one attempt may take. TCP alone gives up after two minutes.
const DIAL_TIMEOUT: Duration = Duration::from_secs(15);

/// A connection that held this long was not a relay failing at once, so the
/// waits start again from the shortest when it goes.
const STEADY: Duration = Duration::from_secs(30);

/// A conversation on the relay: the peer's identifier, and which of this
/// device's the two use.
type Pair = (RelayId, RelayId);

/// A byte stream to a relay: TCP, or a WebSocket a rendezvous service carries.
pub type RelayStream = Pin<Box<dyn Duplex>>;

/// What a [`RelayStream`] is made of.
pub trait Duplex: AsyncRead + AsyncWrite + Send + 'static {}
impl<T: AsyncRead + AsyncWrite + Send + 'static> Duplex for T {}

/// How a socket opens its connection to the relay, as often as it must.
pub type Dial = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = io::Result<RelayStream>> + Send>> + Send + Sync>;

/// Whether a socket is on its relay now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// The first connection is still being made.
    Trying,
    /// Connected, and registered under every name the socket holds.
    Up,
    /// Not connected, and trying again if the socket can; why, in words.
    Down(String),
}

/// Two-way mapping between conversations and the addresses QUIC sees.
#[derive(Debug, Default)]
struct PeerMap {
    by_address: HashMap<SocketAddr, Pair>,
    by_pair: HashMap<Pair, SocketAddr>,
    next: u32,
}

impl PeerMap {
    fn address_for(&mut self, pair: Pair) -> Option<SocketAddr> {
        if let Some(existing) = self.by_pair.get(&pair) {
            return Some(*existing);
        }
        if self.next >= MAX_PEERS {
            return None;
        }
        self.next += 1;
        let address: SocketAddr = format!("192.0.2.{}:9", self.next)
            .parse()
            .expect("a literal address");
        self.by_address.insert(address, pair);
        self.by_pair.insert(pair, address);
        Some(address)
    }

    fn pair_for(&self, address: &SocketAddr) -> Option<Pair> {
        self.by_address.get(address).copied()
    }
}

/// A relay connection, usable as a QUIC socket.
#[derive(Debug)]
pub struct RelaySocket {
    /// The identifier this socket registered first: what a plain `Forward`
    /// is from, and what a plain `Deliver` was for.
    me: RelayId,
    outgoing: mpsc::UnboundedSender<Frame>,
    incoming: Mutex<mpsc::UnboundedReceiver<(Pair, Vec<u8>)>>,
    peers: Mutex<PeerMap>,
    /// Every name registered, to register again on a new connection.
    names: Arc<Mutex<Vec<RelayId>>>,
    link: watch::Receiver<Link>,
}

impl RelaySocket {
    /// Connect to a relay and register under `me`, dialling it again
    /// whenever the connection goes (see *Staying on*). An error if the first
    /// connection cannot be made.
    ///
    /// Carries traffic to and from any peer: dial one with
    /// [`address_for`](Self::address_for), and accept from whoever calls.
    pub async fn connect(relay: SocketAddr, me: RelayId) -> Result<Arc<Self>> {
        let dial: Dial = Arc::new(move || {
            Box::pin(async move {
                let stream = TcpStream::connect(relay).await?;
                let _ = stream.set_nodelay(true);
                Ok(Box::pin(stream) as RelayStream)
            })
        });
        let socket = Self::redialling(dial, me);
        match socket.settled(DIAL_TIMEOUT).await {
            Link::Up => Ok(socket),
            Link::Down(why) => Err(Error::Io(io::Error::new(io::ErrorKind::ConnectionRefused, why))),
            Link::Trying => Err(Error::Io(io::Error::new(io::ErrorKind::TimedOut, "the relay did not answer"))),
        }
    }

    /// The same, over a byte stream already open to a relay: TCP, or a
    /// WebSocket carried by a rendezvous service that relays as well, which
    /// is what passes through the proxies and firewalls that let only web
    /// traffic out (`qurb_signal::tunnel`). Gone with the stream: a socket
    /// that should outlive it is made by [`redialling`](Self::redialling).
    pub async fn over<S>(stream: S, me: RelayId) -> Result<Arc<Self>>
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let mut stream: RelayStream = Box::pin(stream);
        stream.write_all(&Frame::Register { member: me }.encode()).await?;
        Ok(Self::start(me, Some(stream), None))
    }

    /// A socket that opens its connection with `dial`, now and whenever it
    /// goes, and registers under `me` on each. Returned at once, before the
    /// first connection is made: [`link`](Self::link) and
    /// [`settled`](Self::settled) say how that went.
    pub fn redialling(dial: Dial, me: RelayId) -> Arc<Self> {
        Self::start(me, None, Some(dial))
    }

    fn start(me: RelayId, first: Option<RelayStream>, dial: Option<Dial>) -> Arc<Self> {
        let (outgoing, to_send) = mpsc::unbounded_channel::<Frame>();
        let (received, incoming) = mpsc::unbounded_channel::<(Pair, Vec<u8>)>();
        let names = Arc::new(Mutex::new(vec![me]));
        let (state, link) = watch::channel(match first {
            Some(_) => Link::Up,
            None => Link::Trying,
        });
        tokio::spawn(carry(Carry { me, first, dial, names: Arc::clone(&names), state, to_send, received }));
        Arc::new(Self {
            me,
            outgoing,
            incoming: Mutex::new(incoming),
            peers: Mutex::new(PeerMap::default()),
            names,
            link,
        })
    }

    /// Whether the socket is on its relay now.
    pub fn link(&self) -> Link {
        self.link.borrow().clone()
    }

    /// How the first connection went, waiting up to `within` for it.
    pub async fn settled(&self, within: Duration) -> Link {
        let mut link = self.link.clone();
        let _ = tokio::time::timeout(within, link.wait_for(|now| *now != Link::Trying)).await;
        self.link()
    }

    /// Be reachable under `also` too, on this same connection: the name a
    /// meeting with another person's device gives this one (decision 0061).
    /// Asking again for a name already held changes nothing.
    pub fn register(&self, also: RelayId) -> Result<()> {
        // Held before it is sent: a connection made after this registers it
        // whether or not this frame reached the one before.
        {
            let mut names = self.names.lock().expect("names");
            if !names.contains(&also) {
                names.push(also);
            }
        }
        self.outgoing
            .send(Frame::Register { member: also })
            .map_err(|_| Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "relay connection closed")))
    }

    /// The address to give QUIC when dialling this peer through the relay.
    pub fn address_for(&self, peer: RelayId) -> Result<SocketAddr> {
        self.address_for_as(peer, self.me)
    }

    /// The address to give QUIC when dialling this peer through the relay
    /// under `as_me`, one of this socket's registered names.
    pub fn address_for_as(&self, peer: RelayId, as_me: RelayId) -> Result<SocketAddr> {
        self.peers
            .lock()
            .expect("peer map")
            .address_for((peer, as_me))
            .ok_or(Error::TooManyPeers { max: MAX_PEERS as usize })
    }
}

/// Always writable: the outbound channel is unbounded, so a send never needs to
/// wait. Back pressure is QUIC's congestion control, which is the right place
/// for it.
#[derive(Debug)]
struct AlwaysWritable;

impl quinn::UdpPoller for AlwaysWritable {
    fn poll_writable(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl quinn::AsyncUdpSocket for RelaySocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn quinn::UdpPoller>> {
        Box::pin(AlwaysWritable)
    }

    fn try_send(&self, transmit: &quinn::udp::Transmit) -> io::Result<()> {
        // The address QUIC is sending to is one this socket handed out, so it
        // names a peer. Anything else is a packet for somewhere we have never
        // been, which cannot be relayed and must not be silently discarded
        // either -- QUIC deserves to know it did not go.
        let (peer, as_me) = self
            .peers
            .lock()
            .expect("peer map")
            .pair_for(&transmit.destination)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AddrNotAvailable,
                    "no relayed peer at that address",
                )
            })?;

        // From this device's own name, the frame every relay understands.
        let payload = transmit.contents.to_vec();
        let frame = if as_me == self.me {
            Frame::Forward { to: peer, payload }
        } else {
            Frame::ForwardAs { from: as_me, to: peer, payload }
        };
        self.outgoing
            .send(frame)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "relay connection closed"))
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [quinn::udp::RecvMeta],
    ) -> Poll<io::Result<usize>> {
        if bufs.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let mut incoming = self.incoming.lock().expect("receiver");
        match incoming.poll_recv(cx) {
            Poll::Ready(Some((pair, payload))) => {
                // A peer we have not seen gets an address now. This is how a
                // device that was only listening learns to answer.
                let Some(address) = self.peers.lock().expect("peer map").address_for(pair)
                else {
                    // Out of addresses. Dropping is what a router does when it
                    // cannot deliver, and QUIC copes with a lost datagram.
                    return Poll::Pending;
                };

                let len = payload.len().min(bufs[0].len());
                bufs[0][..len].copy_from_slice(&payload[..len]);
                meta[0] = quinn::udp::RecvMeta {
                    addr: address,
                    len,
                    stride: len,
                    ecn: None,
                    dst_ip: None,
                };
                Poll::Ready(Ok(1))
            }
            // The relay is gone for good: a socket over one stream, whose
            // stream ended. Reported as an error rather than as silence, so
            // QUIC fails the connection instead of waiting out its idle
            // timeout. A socket that dials again never ends this way while it
            // is held.
            Poll::Ready(None) => {
                Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "relay closed")))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(LOCAL_ADDRESS.parse().expect("a literal address"))
    }
}

/// What the task carrying a socket's connections holds.
struct Carry {
    me: RelayId,
    first: Option<RelayStream>,
    dial: Option<Dial>,
    names: Arc<Mutex<Vec<RelayId>>>,
    state: watch::Sender<Link>,
    to_send: mpsc::UnboundedReceiver<Frame>,
    received: mpsc::UnboundedSender<(Pair, Vec<u8>)>,
}

/// Carry a socket's frames over its connection to the relay, and over the
/// next when that one goes, for as long as the socket is held.
async fn carry(mut c: Carry) {
    let mut wait = RETRY_FIRST;
    let mut first = c.first.take();
    loop {
        let stream = match first.take() {
            // Registered already, by `over`.
            Some(stream) => Ok(stream),
            None => {
                let Some(dial) = c.dial.clone() else { return };
                let names = Arc::clone(&c.names);
                let attempt = async move {
                    let mut stream = tokio::time::timeout(DIAL_TIMEOUT, dial())
                        .await
                        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the relay did not answer"))??;
                    let names = names.lock().expect("names").clone();
                    for name in names {
                        stream.write_all(&Frame::Register { member: name }.encode()).await?;
                    }
                    Ok::<_, io::Error>(stream)
                };
                match dropping(&mut c.to_send, attempt).await {
                    Some(dialled) => dialled,
                    None => return,
                }
            }
        };
        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                tracing::debug!(error = %e, wait = ?wait, "the relay cannot be reached; trying again");
                c.state.send_replace(Link::Down(e.to_string()));
                if dropping(&mut c.to_send, tokio::time::sleep(wait)).await.is_none() {
                    return;
                }
                wait = (wait * 2).min(RETRY_MOST);
                continue;
            }
        };

        c.state.send_replace(Link::Up);
        let since = Instant::now();
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reading = tokio::spawn(read_frames(reader, c.me, c.received.clone()));
        let held = loop {
            tokio::select! {
                frame = c.to_send.recv() => match frame {
                    Some(frame) => {
                        if writer.write_all(&frame.encode()).await.is_err() {
                            break true;
                        }
                    }
                    // The socket was let go of.
                    None => break false,
                },
                _ = &mut reading => break true,
            }
        };
        reading.abort();
        if !held || c.dial.is_none() {
            return;
        }
        tracing::info!(after = ?since.elapsed(), "the relay connection went; dialling it again");
        c.state.send_replace(Link::Down("the relay connection went".into()));
        if since.elapsed() >= STEADY {
            wait = RETRY_FIRST;
        }
        if dropping(&mut c.to_send, tokio::time::sleep(wait)).await.is_none() {
            return;
        }
        wait = (wait * 2).min(RETRY_MOST);
    }
}

/// Run `work`, dropping what QUIC sends meanwhile, as a router with no route
/// drops it. `None` if the socket is let go of first.
async fn dropping<F: Future>(to_send: &mut mpsc::UnboundedReceiver<Frame>, work: F) -> Option<F::Output> {
    tokio::pin!(work);
    loop {
        tokio::select! {
            done = &mut work => return Some(done),
            // Dropped; and none at all is the socket let go of.
            frame = to_send.recv() => {
                frame?;
            }
        }
    }
}

/// Hand on what the relay delivers, until the connection ends or the socket
/// is let go of.
async fn read_frames(mut reader: ReadHalf<RelayStream>, me: RelayId, received: mpsc::UnboundedSender<(Pair, Vec<u8>)>) {
    let mut length = [0u8; 4];
    let mut body = vec![0u8; MAX_FRAME];
    loop {
        if reader.read_exact(&mut length).await.is_err() {
            return;
        }
        let size = u32::from_be_bytes(length) as usize;
        if size == 0 || size > MAX_FRAME {
            return;
        }
        if reader.read_exact(&mut body[..size]).await.is_err() {
            return;
        }
        // Anything but a delivery is not ours to act on. A relay that
        // starts sending other frame kinds is one we do not understand.
        let delivered = match Frame::decode(&body[..size]) {
            Ok(Frame::Deliver { from, payload }) => ((from, me), payload),
            Ok(Frame::DeliverTo { from, to, payload }) => ((from, to), payload),
            _ => continue,
        };
        if received.send(delivered).is_err() {
            return;
        }
    }
}

/// Build a QUIC endpoint whose traffic goes through a relay.
pub fn endpoint_over(
    socket: Arc<RelaySocket>,
    server_config: Option<quinn::ServerConfig>,
) -> Result<quinn::Endpoint> {
    let runtime = quinn::default_runtime()
        .ok_or_else(|| Error::Io(io::Error::other("no async runtime available")))?;

    quinn::Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        server_config,
        socket,
        runtime,
    )
    .map_err(Error::Io)
}
