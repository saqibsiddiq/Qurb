//! Finding each other with no server at all.
//!
//! Two devices on one network have a path to each other and no need of anybody
//! to introduce them. Until this existed they needed one anyway: the rendezvous
//! service was the only way a device learned where another was, so a laptop and
//! a phone on the same Wi-Fi could not sync without something on the internet
//! being reachable and up.
//!
//! That is a dependency the product should not have, and a fragile one — it was
//! removed after a phone spent three hours unable to sync because the overlay
//! network carrying the rendezvous had quietly dropped off.
//!
//! # How it works
//!
//! Every device multicasts a small beacon saying who it is and where it can be
//! reached. Every device listens. That is the whole protocol.
//!
//! # What a stranger on the same network sees
//!
//! Random bytes.
//!
//! The beacon is encrypted under a key derived from the master key, so only a
//! device that already shares the key can read one — and since the ciphertext
//! and its nonce are indistinguishable from noise, an eavesdropper on a café
//! network cannot even tell that qurb is running, let alone how many devices
//! are present or what they are called.
//!
//! This matters more than it might seem. The identifier a device announces
//! under is a bearer secret: anyone holding it can ask the rendezvous service
//! where that device is. Broadcasting it in the clear would hand it to every
//! machine on every network the device ever joins.
//!
//! # What it does not do
//!
//! It does not replace the rendezvous service. Two devices on *different*
//! networks still need somebody to introduce them, and nothing here helps with
//! that. What it removes is the requirement for one when the devices can
//! already see each other — which, for most people, is most of the time.

use crate::error::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use qurb_keys::{MasterKey, Purpose};
use qurb_signal::{Endpoints, MemberId};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Duration;

/// The multicast group beacons go to.
///
/// Administratively scoped (RFC 2365): routers do not forward this range beyond
/// the local organisation, which is exactly the reach wanted. A global-scope
/// group would be no more useful and would leak further.
pub const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 113, 7);

/// The port beacons go to.
///
/// Fixed rather than configurable. Two devices have to agree on it before they
/// have any way to agree on anything, so it is a constant of the protocol in
/// the way the group address is.
pub const PORT: u16 = 47317;

/// How often a quiet device says it is still here.
///
/// Long enough to be invisible on a network and on a battery, short enough that
/// a device joining a network is found in well under a minute. Arrival is not
/// what this interval decides — a device announces immediately on starting, and
/// again when it has news — so this only bounds how long a *missed* beacon
/// costs.
pub const INTERVAL: Duration = Duration::from_secs(20);

/// How long a sighting is worth acting on.
///
/// Three intervals, so a single dropped packet — which multicast makes ordinary
/// — does not make a device that is sitting right there look absent.
pub const FRESH_FOR: Duration = Duration::from_secs(60);

/// How far out of step a beacon's clock may be and still be believed.
///
/// Bounds replay: a beacon captured on a network and replayed later makes a
/// device look present when it is not, which costs a wasted connection attempt.
/// Two minutes is generous enough for devices that disagree about the time and
/// short enough that the recording is stale before it is useful.
const CLOCK_SLACK: i64 = 120;

/// The most addresses a beacon will carry.
///
/// A machine with many interfaces — an overlay network, a container bridge, a
/// virtual adapter per hypervisor — can have a surprising number, and a beacon
/// has to fit comfortably in one datagram.
const MAX_ADDRESSES: usize = 8;

/// A version byte, so a later format is a refusal rather than a
/// misinterpretation.
const VERSION: u8 = 1;

/// What one device says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beacon {
    /// The same identifier the rendezvous service knows this device by, so a
    /// peer that can recognise one can recognise the other.
    pub member: MemberId,
    pub endpoints: Endpoints,
    /// Unix seconds, from the sender's clock.
    pub sent_at: i64,
    /// "I have something you have not seen." The local-network equivalent of
    /// telling the rendezvous service there is news waiting: it turns a change
    /// on one device into a sync on the other within a second, rather than at
    /// the next scheduled pass.
    pub news: bool,
    /// "I have just arrived — is anybody there?"
    ///
    /// Answered immediately by everyone who hears it, which is what makes
    /// discovery take a moment rather than up to a full interval. Without it a
    /// device that runs only for the length of a sync pass — which is what a
    /// phone does — would start with an empty address book and usually finish
    /// before the next scheduled beacon arrived. Found on hardware, where it
    /// failed every time.
    ///
    /// A reply is never itself a probe, so an arrival costs one round of
    /// answers and not a storm.
    pub probe: bool,
}

impl Beacon {
    /// Encrypt for the group.
    ///
    /// Nonce in the clear followed by the ciphertext, and nothing else: no
    /// magic number, no length prefix, no version outside the encryption. A
    /// packet is indistinguishable from random bytes to anybody without the
    /// key, which is what stops a listener on a shared network from learning
    /// that qurb is running here.
    pub fn seal(&self, master: &MasterKey) -> Result<Vec<u8>> {
        let mut nonce = [0u8; 24];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut nonce);

        let sealed = cipher(master)
            .encrypt(XNonce::from_slice(&nonce), self.encode().as_slice())
            .map_err(|_| Error::Protocol { detail: "could not seal a beacon".into() })?;

        let mut packet = Vec::with_capacity(24 + sealed.len());
        packet.extend_from_slice(&nonce);
        packet.extend_from_slice(&sealed);
        Ok(packet)
    }

    /// Read a packet, or say why it is not one of ours.
    ///
    /// Everything that is not a beacon from this group fails here, and it is
    /// meant to: the decryption either works, in which case the sender holds
    /// the master key, or it does not, in which case there is nothing to
    /// distinguish an unrelated packet from a corrupted one and no reason to
    /// try. Failure is cheap and silent by design — on a busy network this runs
    /// against every multicast packet that happens to use the port.
    pub fn open(master: &MasterKey, packet: &[u8], now: i64) -> Result<Self> {
        if packet.len() < 24 {
            return Err(Error::Protocol { detail: "beacon too short".into() });
        }
        let (nonce, sealed) = packet.split_at(24);

        let plain = cipher(master)
            .decrypt(XNonce::from_slice(nonce), sealed)
            .map_err(|_| Error::Protocol { detail: "not a beacon of ours".into() })?;

        let beacon = Self::decode(&plain)?;

        // A beacon from far enough outside the present is either a replay or a
        // device whose clock is wrong, and neither is worth acting on.
        if (beacon.sent_at - now).abs() > CLOCK_SLACK {
            return Err(Error::Protocol { detail: "beacon is out of its window".into() });
        }
        Ok(beacon)
    }

    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.push(VERSION);
        out.extend_from_slice(self.member.as_bytes());
        out.extend_from_slice(&self.sent_at.to_le_bytes());
        // Two flags in one byte, so that adding the second did not change the
        // shape of the packet.
        out.push((self.news as u8) | ((self.probe as u8) << 1));

        let mut addresses: Vec<SocketAddr> = self.endpoints.candidates();
        addresses.truncate(MAX_ADDRESSES);
        out.push(addresses.len() as u8);
        for address in addresses {
            match address.ip() {
                IpAddr::V4(ip) => {
                    out.push(4);
                    out.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    out.push(6);
                    out.extend_from_slice(&ip.octets());
                }
            }
            out.extend_from_slice(&address.port().to_le_bytes());
        }
        out
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut read = Reader { bytes, at: 0 };

        if read.u8()? != VERSION {
            return Err(Error::Protocol { detail: "beacon from a newer qurb".into() });
        }
        let member = MemberId::from_bytes(read.array()?);
        let sent_at = i64::from_le_bytes(read.take(8)?.try_into().expect("8 bytes"));
        let flags = read.u8()?;
        let news = flags & 1 != 0;
        let probe = flags & 2 != 0;

        let count = read.u8()? as usize;
        if count > MAX_ADDRESSES {
            return Err(Error::Protocol { detail: "beacon claims too many addresses".into() });
        }

        let mut local = Vec::with_capacity(count);
        for _ in 0..count {
            let ip = match read.u8()? {
                4 => {
                    let octets: [u8; 4] = read.take(4)?.try_into().expect("4 bytes");
                    IpAddr::from(octets)
                }
                6 => IpAddr::from(read.array::<16>()?),
                other => {
                    return Err(Error::Protocol {
                        detail: format!("beacon has an address of kind {other}"),
                    })
                }
            };
            let port = u16::from_le_bytes(read.take(2)?.try_into().expect("2 bytes"));
            local.push(SocketAddr::new(ip, port));
        }

        // Everything a beacon carries is somewhere the sender can be reached
        // from *this* network, so it all goes in `local`. A public address
        // learned from STUN is in there too when the sender had one; racing it
        // costs nothing and occasionally wins on a network that hairpins.
        Ok(Self { member, endpoints: Endpoints { public: None, local }, sent_at, news, probe })
    }
}

fn cipher(master: &MasterKey) -> XChaCha20Poly1305 {
    let key = master.derive(Purpose::LocalDiscovery);
    XChaCha20Poly1305::new(key.as_bytes().into())
}

/// A cursor that refuses to read past the end, so a malformed beacon is an
/// error rather than a panic. The same shape as the one in [`crate::wire`], and
/// for the same reason: this parses packets from anybody who can reach the
/// port.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(n).ok_or_else(|| Error::Protocol {
            detail: "beacon length overflows".into(),
        })?;
        if end > self.bytes.len() {
            return Err(Error::Protocol { detail: "beacon ends early".into() });
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("N bytes"))
    }
}

/// Open a socket that can both send to the group and hear it.
///
/// Bound to the wildcard address rather than to one interface: a laptop moving
/// between Wi-Fi and a dock changes which interface matters, and rebinding on
/// every change is a state machine nobody needs. The group is joined on every
/// address the machine has for the same reason.
///
/// `SO_REUSEADDR` because two qurb processes on one machine — a daemon and a
/// test, or two accounts — must both be able to listen. Multicast loopback is
/// left on so they can also hear each other, which is what makes this testable
/// without two machines.
pub fn open_socket(port: u16) -> Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| Error::Io { path: "multicast socket".into(), source: e })?;
    socket
        .set_reuse_address(true)
        .map_err(|e| Error::Io { path: "reuse address".into(), source: e })?;
    // Linux needs this as well for two sockets to share a port; without it the
    // second bind succeeds and receives nothing, which is the worst of both.
    #[cfg(target_os = "linux")]
    socket
        .set_reuse_port(true)
        .map_err(|e| Error::Io { path: "reuse port".into(), source: e })?;

    let bind = SocketAddr::from(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
    socket
        .bind(&bind.into())
        .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;

    let socket: UdpSocket = socket.into();
    socket
        .set_multicast_loop_v4(true)
        .map_err(|e| Error::Io { path: "multicast loop".into(), source: e })?;

    // Joined on every interface, because a laptop may be on Wi-Fi and a docked
    // ethernet at once and the peers are not always on the one the default
    // route uses. A failure on one interface is not a failure overall: a
    // virtual adapter that cannot carry multicast is ordinary, and the others
    // still work.
    let mut joined = 0;
    for ip in interfaces() {
        if socket.join_multicast_v4(&GROUP, &ip).is_ok() {
            joined += 1;
        }
    }
    // The unspecified address lets the operating system choose, which is the
    // only thing left to try and is what works inside a container with one
    // interface it did not enumerate.
    if joined == 0 {
        socket
            .join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)
            .map_err(|e| Error::Io { path: "join multicast".into(), source: e })?;
    }

    Ok(socket)
}

/// Every IPv4 address this machine has, loopback included.
///
/// Loopback is included on purpose: it is how two processes on one machine find
/// each other, which is both a real case — two accounts, or a daemon and a
/// second folder — and the only way this is testable without two machines.
fn interfaces() -> Vec<Ipv4Addr> {
    let mut found: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Where a beacon is sent.
pub fn destination(port: u16) -> SocketAddr {
    SocketAddr::from(SocketAddrV4::new(GROUP, port))
}

/// Devices seen on this network lately, and where they said they were.
///
/// Kept separate from the rendezvous service's answers rather than merged into
/// them, because the two are known with different confidence. A beacon means "a
/// device holding our key sent this from a network we are on, moments ago",
/// which is stronger evidence of reachability than a rendezvous record — that
/// only says where a device *claimed* to be when it last announced, possibly
/// from a network with no path to this one.
#[derive(Clone, Default)]
pub struct Neighbours {
    seen: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<MemberId, Sighting>>>,
}

#[derive(Clone)]
struct Sighting {
    endpoints: Endpoints,
    at: std::time::Instant,
}

impl Neighbours {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn note(&self, member: MemberId, endpoints: Endpoints) {
        self.seen
            .lock()
            .expect("neighbours")
            .insert(member, Sighting { endpoints, at: std::time::Instant::now() });
    }

    /// Where a device was last seen, if that was recently enough to act on.
    ///
    /// Stale entries are dropped on read rather than swept on a timer: the only
    /// thing that cares is a lookup, and a map holding a handful of devices is
    /// not worth a task of its own.
    pub fn where_is(&self, member: &MemberId) -> Option<Endpoints> {
        let mut seen = self.seen.lock().expect("neighbours");
        match seen.get(member) {
            Some(sighting) if sighting.at.elapsed() <= FRESH_FOR => Some(sighting.endpoints.clone()),
            Some(_) => {
                seen.remove(member);
                None
            }
            None => None,
        }
    }

    /// How many devices are currently visible on this network.
    pub fn count(&self) -> usize {
        let seen = self.seen.lock().expect("neighbours");
        seen.values().filter(|s| s.at.elapsed() <= FRESH_FOR).count()
    }
}

/// Say who we are, and hear who else is here.
///
/// Two loops on one socket: a sender on a timer, and a receiver that never
/// stops. Both are cheap enough to run for the life of the device, which is
/// what they do — discovery that only ran at startup would miss every device
/// that joined the network afterwards, which is most of them.
///
/// Failures are logged and shrugged off. A network that blocks multicast, an
/// interface that disappears when a laptop is undocked, a router that drops the
/// group: all of them mean "no local discovery on this network right now", and
/// all of them are survivable because the rendezvous service is still there.
pub struct Beacons {
    socket: std::sync::Arc<tokio::net::UdpSocket>,
    master: MasterKey,
    member: MemberId,
    endpoints: std::sync::Arc<std::sync::Mutex<Endpoints>>,
    port: u16,
}

impl Beacons {
    /// Start announcing and listening.
    ///
    /// Sightings of *other* devices go to the returned receiver; this device's
    /// own beacons are recognised and dropped, because multicast loopback means
    /// it hears itself.
    pub fn start(
        master: MasterKey,
        member: MemberId,
        endpoints: Endpoints,
        port: u16,
    ) -> Result<(std::sync::Arc<Self>, tokio::sync::mpsc::UnboundedReceiver<Beacon>)> {
        let socket = open_socket(port)?;
        socket
            .set_nonblocking(true)
            .map_err(|e| Error::Io { path: "nonblocking".into(), source: e })?;
        let socket = tokio::net::UdpSocket::from_std(socket)
            .map_err(|e| Error::Io { path: "async socket".into(), source: e })?;

        let beacons = std::sync::Arc::new(Self {
            socket: std::sync::Arc::new(socket),
            master,
            member,
            endpoints: std::sync::Arc::new(std::sync::Mutex::new(endpoints)),
            port,
        });

        let (sightings, inbox) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(listen(std::sync::Arc::clone(&beacons), sightings));
        tokio::spawn(announce(std::sync::Arc::clone(&beacons)));

        Ok((beacons, inbox))
    }

    /// Say we are here. The routine beacon, on a timer.
    pub async fn announce(&self) {
        self.send(false, false).await
    }

    /// Say we are here and have something. Sent the moment a file changes,
    /// which is what turns an edit on one device into a sync on the other in
    /// about a second rather than at the next scheduled pass.
    pub async fn announce_news(&self) {
        self.send(true, false).await
    }

    /// Ask who else is here, and be answered at once.
    ///
    /// Sent on arrival. The answers are what fill this device's address book
    /// immediately, rather than over the following twenty seconds.
    pub async fn probe(&self) {
        self.send(false, true).await
    }

    async fn send(&self, news: bool, probe: bool) {
        let endpoints = self.endpoints.lock().expect("endpoints").clone();
        let beacon = Beacon { member: self.member, endpoints, sent_at: now(), news, probe };

        let packet = match beacon.seal(&self.master) {
            Ok(packet) => packet,
            Err(e) => {
                tracing::debug!(error = %e, "could not seal a beacon");
                return;
            }
        };

        if let Err(e) = self.socket.send_to(&packet, destination(self.port)).await {
            // Ordinary on a network that blocks multicast, and on a laptop
            // between networks. Not worth a warning every twenty seconds.
            tracing::trace!(error = %e, "could not send a beacon");
        }
    }

    /// Change what the beacons say, after a rebind or a new address.
    pub fn now_at(&self, endpoints: Endpoints) {
        *self.endpoints.lock().expect("endpoints") = endpoints;
    }
}

async fn announce(beacons: std::sync::Arc<Beacons>) {
    // Three quick probes first. Multicast is lossy, a device joining a network
    // wants to be found now rather than in twenty seconds, and — more
    // importantly — it wants to *find* now. Each probe is answered by everyone
    // who hears it, so the address book is full within a moment of starting
    // instead of filling over the next interval.
    for _ in 0..3 {
        beacons.probe().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    let mut timer = tokio::time::interval(INTERVAL);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        timer.tick().await;
        beacons.announce().await;
    }
}

async fn listen(
    beacons: std::sync::Arc<Beacons>,
    sightings: tokio::sync::mpsc::UnboundedSender<Beacon>,
) {
    // Comfortably more than a beacon, and small enough that a flood of rubbish
    // costs nothing.
    let mut buffer = vec![0u8; 1500];

    loop {
        let read = match beacons.socket.recv_from(&mut buffer).await {
            Ok((read, _from)) => read,
            Err(e) => {
                tracing::debug!(error = %e, "the discovery socket failed");
                return;
            }
        };

        // Everything that is not one of ours fails here, silently: on a shared
        // port this runs against unrelated traffic, and a warning per packet
        // would be a log nobody could read.
        let Ok(beacon) = Beacon::open(&beacons.master, &buffer[..read], now()) else { continue };

        // Multicast loopback means we hear ourselves, which is wanted — it is
        // how two processes on one machine find each other — and means our own
        // beacon has to be recognised and dropped.
        if beacon.member == beacons.member {
            continue;
        }

        // Somebody has just arrived and is asking. Answering at once is what
        // makes their address book useful immediately; a reply is not itself a
        // probe, so this costs one round rather than a storm.
        if beacon.probe {
            beacons.announce().await;
        }

        if sightings.send(beacon).is_err() {
            // Nobody is listening any more, so neither is there any reason to.
            return;
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> MasterKey {
        MasterKey::from_bytes([byte; 32])
    }

    fn beacon() -> Beacon {
        Beacon {
            member: MemberId::from_bytes([9; 32]),
            endpoints: Endpoints {
                public: Some("203.0.113.5:41935".parse().unwrap()),
                local: vec!["192.168.1.4:41935".parse().unwrap()],
            },
            sent_at: 1_790_000_000,
            news: true,
            probe: false,
        }
    }

    #[test]
    fn a_beacon_survives_the_round_trip() {
        let master = key(1);
        let sent = beacon();
        let packet = sent.seal(&master).unwrap();
        let back = Beacon::open(&master, &packet, sent.sent_at).unwrap();

        assert_eq!(back.member, sent.member);
        assert_eq!(back.sent_at, sent.sent_at);
        assert!(back.news);
        // Everything the sender offered comes back as somewhere to try.
        assert_eq!(back.endpoints.candidates(), sent.endpoints.candidates());
    }

    /// The whole privacy claim. Somebody on the same café network holds no key,
    /// and must learn nothing at all — not the device identifier, not the
    /// addresses, not that this is qurb.
    #[test]
    fn a_stranger_on_the_network_cannot_read_one() {
        let packet = beacon().seal(&key(1)).unwrap();

        assert!(Beacon::open(&key(2), &packet, beacon().sent_at).is_err());

        // And nothing recognisable is in the packet either: no identifier, no
        // address, no marker that would say what it is.
        let member = [9u8; 32];
        assert!(!packet.windows(32).any(|w| w == member), "the identifier is in the clear");
        assert!(!packet.windows(4).any(|w| w == [192, 168, 1, 4]), "an address is in the clear");
    }

    /// A beacon recorded on one network and replayed later would make a device
    /// look present when it is not. Cheap to do and cheap to refuse.
    #[test]
    fn a_replayed_beacon_is_refused() {
        let master = key(1);
        let sent = beacon();
        let packet = sent.seal(&master).unwrap();

        assert!(Beacon::open(&master, &packet, sent.sent_at + CLOCK_SLACK + 1).is_err());
        assert!(Beacon::open(&master, &packet, sent.sent_at - CLOCK_SLACK - 1).is_err());
        // Inside the window, including a clock that is behind, is fine.
        assert!(Beacon::open(&master, &packet, sent.sent_at + CLOCK_SLACK - 1).is_ok());
    }

    /// Two beacons of the same thing must not be the same bytes, or a listener
    /// could count devices and watch them come and go without any key.
    #[test]
    fn the_same_beacon_twice_looks_different() {
        let master = key(1);
        let once = beacon().seal(&master).unwrap();
        let twice = beacon().seal(&master).unwrap();
        assert_ne!(once, twice);
    }

    /// This parses packets from anybody who can reach the port.
    #[test]
    fn rubbish_is_refused_rather_than_panicking() {
        let master = key(1);
        for packet in [vec![], vec![0u8; 23], vec![0u8; 24], vec![0xFF; 200]] {
            assert!(Beacon::open(&master, &packet, 0).is_err());
        }

        // And a valid packet with its contents mangled, which exercises the
        // decoder rather than the cipher.
        let mut packet = beacon().seal(&master).unwrap();
        let last = packet.len() - 1;
        packet[last] ^= 0xFF;
        assert!(Beacon::open(&master, &packet, beacon().sent_at).is_err());
    }

    #[test]
    fn a_beacon_carries_at_most_a_datagram_of_addresses() {
        let master = key(1);
        let many = Beacon {
            endpoints: Endpoints {
                public: None,
                local: (0..40).map(|i| SocketAddr::from(([10, 0, 0, i], 41935))).collect(),
            },
            ..beacon()
        };

        let packet = many.seal(&master).unwrap();
        assert!(packet.len() < 512, "a beacon grew to {} bytes", packet.len());

        let back = Beacon::open(&master, &packet, many.sent_at).unwrap();
        assert_eq!(back.endpoints.local.len(), MAX_ADDRESSES);
    }

    #[test]
    fn a_sighting_goes_stale() {
        let neighbours = Neighbours::new();
        let member = MemberId::from_bytes([3; 32]);
        neighbours.note(member, Endpoints { public: None, local: vec![] });

        assert!(neighbours.where_is(&member).is_some());
        assert_eq!(neighbours.count(), 1);

        // Reaching into the map rather than waiting a minute.
        neighbours.seen.lock().unwrap().get_mut(&member).unwrap().at =
            std::time::Instant::now() - FRESH_FOR - Duration::from_secs(1);

        assert!(neighbours.where_is(&member).is_none());
        assert_eq!(neighbours.count(), 0);
    }
}
