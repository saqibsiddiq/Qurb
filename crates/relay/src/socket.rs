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

use crate::error::{Error, Result};
use crate::frame::{Frame, RelayId, MAX_FRAME};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

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

/// Two-way mapping between relay identifiers and the addresses QUIC sees.
#[derive(Debug, Default)]
struct PeerMap {
    by_address: HashMap<SocketAddr, RelayId>,
    by_id: HashMap<RelayId, SocketAddr>,
    next: u32,
}

impl PeerMap {
    fn address_for(&mut self, peer: RelayId) -> Option<SocketAddr> {
        if let Some(existing) = self.by_id.get(&peer) {
            return Some(*existing);
        }
        if self.next >= MAX_PEERS {
            return None;
        }
        self.next += 1;
        let address: SocketAddr = format!("192.0.2.{}:9", self.next)
            .parse()
            .expect("a literal address");
        self.by_address.insert(address, peer);
        self.by_id.insert(peer, address);
        Some(address)
    }

    fn id_for(&self, address: &SocketAddr) -> Option<RelayId> {
        self.by_address.get(address).copied()
    }
}

/// A relay connection, usable as a QUIC socket.
#[derive(Debug)]
pub struct RelaySocket {
    outgoing: mpsc::UnboundedSender<Frame>,
    incoming: Mutex<mpsc::UnboundedReceiver<(RelayId, Vec<u8>)>>,
    peers: Mutex<PeerMap>,
}

impl RelaySocket {
    /// Connect to a relay and register under `me`.
    ///
    /// Carries traffic to and from any peer: dial one with
    /// [`address_for`](Self::address_for), and accept from whoever calls.
    pub async fn connect(relay: SocketAddr, me: RelayId) -> Result<Arc<Self>> {
        let stream = TcpStream::connect(relay).await?;
        let _ = stream.set_nodelay(true);
        let (mut reader, mut writer) = stream.into_split();

        let (outgoing, mut to_send) = mpsc::unbounded_channel::<Frame>();
        let (received, incoming) = mpsc::unbounded_channel::<(RelayId, Vec<u8>)>();

        writer.write_all(&Frame::Register { member: me }.encode()).await?;

        tokio::spawn(async move {
            while let Some(frame) = to_send.recv().await {
                if writer.write_all(&frame.encode()).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            let mut length = [0u8; 4];
            let mut body = vec![0u8; MAX_FRAME];
            loop {
                if reader.read_exact(&mut length).await.is_err() {
                    break;
                }
                let size = u32::from_be_bytes(length) as usize;
                if size == 0 || size > MAX_FRAME {
                    break;
                }
                if reader.read_exact(&mut body[..size]).await.is_err() {
                    break;
                }
                // Anything but a delivery is not ours to act on. A relay that
                // starts sending other frame kinds is one we do not understand.
                if let Ok(Frame::Deliver { from, payload }) = Frame::decode(&body[..size]) {
                    if received.send((from, payload)).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Arc::new(Self {
            outgoing,
            incoming: Mutex::new(incoming),
            peers: Mutex::new(PeerMap::default()),
        }))
    }

    /// The address to give QUIC when dialling this peer through the relay.
    pub fn address_for(&self, peer: RelayId) -> Result<SocketAddr> {
        self.peers
            .lock()
            .expect("peer map")
            .address_for(peer)
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
        let peer = self
            .peers
            .lock()
            .expect("peer map")
            .id_for(&transmit.destination)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AddrNotAvailable,
                    "no relayed peer at that address",
                )
            })?;

        self.outgoing
            .send(Frame::Forward { to: peer, payload: transmit.contents.to_vec() })
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
            Poll::Ready(Some((from, payload))) => {
                // A peer we have not seen gets an address now. This is how a
                // device that was only listening learns to answer.
                let Some(address) = self.peers.lock().expect("peer map").address_for(from)
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
            // The relay is gone. Reported as an error rather than as silence, so
            // QUIC fails the connection instead of waiting out its idle timeout.
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
