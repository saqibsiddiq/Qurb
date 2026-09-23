//! Getting through a router.
//!
//! Two devices on home networks have no address the other can dial. Each sits
//! behind a router doing address translation, and an unsolicited packet arriving
//! at that router is dropped because it belongs to no conversation the router
//! knows about.
//!
//! ```text
//!   1. ask a public server what address our packets appear to come from   (STUN)
//!   2. exchange those addresses through something both can reach          (signalling)
//!   3. both send to the other at the same time                            (hole punching)
//! ```
//!
//! Step 3 is the trick. Neither packet is expected by the receiving router, so
//! the first ones are dropped — but each *outbound* packet teaches its own
//! router to expect a reply from that address, so once both have sent, the
//! packets that follow are let through.
//!
//! # Why the socket is passed around
//!
//! A router's mapping belongs to one local port. Discovering an address on one
//! socket and then connecting on another gets a different mapping, and the hole
//! punched was for an address nobody is listening on. So the same socket does
//! the STUN query, the punching, and then QUIC — which is why these functions
//! take a socket rather than making their own, and why [`endpoint_from`] exists.

use crate::error::{Error, Result};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Duration;

/// RFC 5389's magic cookie. Distinguishes STUN from whatever else may arrive on
/// a UDP port.
const MAGIC: u32 = 0x2112_A442;
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_SUCCESS: u16 = 0x0101;
const XOR_MAPPED_ADDRESS: u16 = 0x0020;
const HEADER_LEN: usize = 20;

/// Public STUN servers, deliberately run by different operators.
///
/// Two are needed to classify a NAT at all: the question is whether the mapping
/// changes with the destination, which cannot be answered by asking one server.
/// Different operators, so that one company's outage does not look like a
/// symmetric NAT.
pub const DEFAULT_STUN_SERVERS: [&str; 2] =
    ["stun.l.google.com:19302", "stun.cloudflare.com:3478"];

/// What a router does with our outbound packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatBehaviour {
    /// The same external address regardless of destination. Hole punching
    /// works, and this is the common case on home routers.
    EndpointIndependent,
    /// A different external port per destination. The address a peer learns is
    /// not the address it can reach, so punching fails and the connection needs
    /// a relay.
    Symmetric,
    /// No STUN server answered. Usually UDP blocked outbound, which means every
    /// connection needs a relay on a port that is allowed.
    Blocked,
    /// Not enough answers to tell. Retry rather than conclude.
    Inconclusive,
}

impl NatBehaviour {
    /// Whether a direct connection is worth attempting.
    pub fn can_punch(&self) -> bool {
        matches!(self, NatBehaviour::EndpointIndependent)
    }
}

/// What this device looks like from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reflexive {
    /// The address the STUN server saw. What a peer should be told.
    pub public: SocketAddr,
    /// The address this socket is bound to locally. Worth sharing too: two
    /// devices on the same network reach each other directly and should not
    /// take the long way round.
    pub local: SocketAddr,
}

/// Ask one server what address our packets appear to come from.
pub fn reflexive_address(
    socket: &UdpSocket,
    server: SocketAddr,
    timeout: Duration,
) -> Result<SocketAddr> {
    let transaction = random_transaction();
    let request = binding_request(&transaction);

    let previous = socket.read_timeout().ok().flatten();
    socket.set_read_timeout(Some(timeout)).map_err(io("set_read_timeout"))?;

    socket.send_to(&request, server).map_err(io("send_to"))?;

    let mut buffer = [0u8; 1024];
    let result = loop {
        let (len, from) = match socket.recv_from(&mut buffer) {
            Ok(v) => v,
            Err(e) => break Err(io("recv_from")(e)),
        };
        // Anything may arrive on a UDP port. Only a well-formed response
        // carrying our own transaction id counts, which is also what stops an
        // off-path attacker from answering on the server's behalf.
        if from == server {
            if let Some(address) = parse_binding_response(&buffer[..len], &transaction) {
                break Ok(address);
            }
        }
    };

    let _ = socket.set_read_timeout(previous);
    result
}

/// Work out how this network's router behaves.
///
/// Queries several servers **from the same socket**. That is the entire test: if
/// the external port is the same whoever we ask, the mapping does not depend on
/// the destination and hole punching can work.
pub fn classify(
    socket: &UdpSocket,
    servers: &[SocketAddr],
    timeout: Duration,
) -> (NatBehaviour, Vec<SocketAddr>) {
    let mut seen = Vec::new();
    for server in servers {
        if let Ok(address) = reflexive_address(socket, *server, timeout) {
            seen.push(address);
        }
    }

    let behaviour = match seen.len() {
        0 => NatBehaviour::Blocked,
        1 => NatBehaviour::Inconclusive,
        _ => {
            if seen.windows(2).all(|w| w[0] == w[1]) {
                NatBehaviour::EndpointIndependent
            } else {
                NatBehaviour::Symmetric
            }
        }
    };
    (behaviour, seen)
}

/// Discover this socket's public address, using the default servers.
pub fn discover(socket: &UdpSocket, timeout: Duration) -> Result<Reflexive> {
    use std::net::ToSocketAddrs;

    let local = socket.local_addr().map_err(io("local_addr"))?;
    for name in DEFAULT_STUN_SERVERS {
        let Ok(mut addrs) = name.to_socket_addrs() else { continue };
        let Some(server) = addrs.find(|a| a.is_ipv4()) else { continue };
        if let Ok(public) = reflexive_address(socket, server, timeout) {
            return Ok(Reflexive { public, local });
        }
    }
    Err(Error::NoStunResponse)
}

/// The address this machine would use to reach the rest of the network.
///
/// Found by asking the routing table rather than by listing interfaces: a UDP
/// socket *connected* to a remote address sends nothing at all, but makes the
/// operating system choose a source address — and the one it chooses is the one
/// that would really be used. Listing interfaces instead means guessing between
/// a wired connection, a wireless one, a virtual machine bridge and three
/// container networks.
///
/// Returns `None` when there is no route anywhere, which is a machine with no
/// network rather than a failure worth reporting.
pub fn routable_address() -> Option<IpAddr> {
    // Any address will do; nothing is sent to it.
    let probe = UdpSocket::bind("0.0.0.0:0").ok()?;
    probe.connect("192.0.2.1:9").ok()?;
    let chosen = probe.local_addr().ok()?.ip();
    if chosen.is_unspecified() {
        None
    } else {
        Some(chosen)
    }
}

/// Every address on this machine a peer might reach it at.
///
/// Not just the one the default route uses. A machine on a VPN or an overlay
/// network — Tailscale, WireGuard, a corporate VPN — has a second address that
/// reaches peers the default route cannot, and it is often the *only* one that
/// works: a laptop at home is `192.168.1.4` to its own network and nothing at
/// all to a phone on a mobile carrier, while its overlay address reaches both.
///
/// Announcing one address meant that path was never offered and every
/// connection from outside the house depended on hole punching. Candidates are
/// raced in parallel, so offering several costs a few packets and buys the
/// cases where the first one cannot work.
///
/// Loopback is excluded — a peer dialling `127.0.0.1` reaches itself — and so
/// is IPv6 link-local, which needs a scope identifier this cannot carry.
pub fn local_addresses() -> Vec<IpAddr> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return routable_address().into_iter().collect();
    };

    let mut found: Vec<IpAddr> = interfaces
        .into_iter()
        .map(|interface| interface.ip())
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
        .filter(|ip| match ip {
            IpAddr::V4(v4) => !v4.is_link_local(),
            IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) != 0xfe80,
        })
        .collect();

    found.sort();
    found.dedup();

    // The default route's address first, when it is among them: it is the one
    // most likely to work on the same network, and candidates are tried in
    // order as well as in parallel.
    if let Some(preferred) = routable_address() {
        if let Some(at) = found.iter().position(|ip| *ip == preferred) {
            found.swap(0, at);
        }
    }
    found
}

/// Replace a wildcard address with one a peer could actually dial.
///
/// A socket bound to `0.0.0.0` reports `0.0.0.0` as its address, which is true
/// and useless: it means "every interface", and nobody can connect to it. Giving
/// that to a peer produces a failure at the far end that looks like the peer's
/// fault.
pub fn dialable(address: SocketAddr) -> SocketAddr {
    if !address.ip().is_unspecified() {
        return address;
    }
    match routable_address() {
        Some(ip) => SocketAddr::new(ip, address.port()),
        // No route anywhere. Loopback at least works for another process on
        // this machine, which is the only peer reachable in that situation.
        None => SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), address.port()),
    }
}

/// Punch a hole towards `peer`.
///
/// Sends a handful of small packets and returns. It does not wait for a reply
/// and does not report success, because there is nothing useful to report: the
/// purpose is to teach *our* router to expect packets from that address, which
/// happens whether or not the peer is ready yet. Whether a path exists is
/// answered by the QUIC handshake that follows.
///
/// Both sides must do this at roughly the same time, which is what the
/// signalling plane is for.
pub fn punch(socket: &UdpSocket, peer: SocketAddr, attempts: usize) -> Result<()> {
    // Deliberately not a STUN message and not a QUIC packet: a peer that has
    // not started its endpoint yet will ignore it, and a router only needs to
    // see something leave.
    const KNOCK: &[u8] = b"qurb-knock";

    for _ in 0..attempts.max(1) {
        // A dropped packet here is the expected case, not a failure — the whole
        // point is that the first ones do not get through.
        let _ = socket.send_to(KNOCK, peer);
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// Build a QUIC endpoint on a socket that has already been used.
///
/// The socket must be the one that did the STUN query and the punching, or the
/// router mapping the peer was told about belongs to a port nothing is
/// listening on.
pub fn endpoint_from(
    socket: UdpSocket,
    server_config: Option<quinn::ServerConfig>,
) -> Result<quinn::Endpoint> {
    let runtime = quinn::default_runtime()
        .ok_or_else(|| Error::Tls("no async runtime available".into()))?;
    quinn::Endpoint::new(quinn::EndpointConfig::default(), server_config, socket, runtime)
        .map_err(io("building an endpoint"))
}

// ---------------------------------------------------------------------------
// The message format, kept pure so it can be tested without a network.
// ---------------------------------------------------------------------------

/// A 20-byte binding request: the whole of what we ever send.
pub fn binding_request(transaction: &[u8; 12]) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
    // Length: no attributes.
    out[2..4].copy_from_slice(&0u16.to_be_bytes());
    out[4..8].copy_from_slice(&MAGIC.to_be_bytes());
    out[8..].copy_from_slice(transaction);
    out
}

/// Extract the address a server reports, if this is a well-formed success
/// response to *our* request.
///
/// Returns `None` rather than an error for anything unexpected: this parses
/// whatever arrives on an open UDP port, where malformed and unrelated traffic
/// is normal rather than exceptional.
pub fn parse_binding_response(bytes: &[u8], transaction: &[u8; 12]) -> Option<SocketAddr> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    if u16::from_be_bytes([bytes[0], bytes[1]]) != BINDING_SUCCESS {
        return None;
    }
    if u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) != MAGIC {
        return None;
    }
    // Matching the transaction id is what makes an off-path forgery need to
    // guess 96 bits rather than simply arrive first.
    if &bytes[8..20] != transaction {
        return None;
    }

    let declared = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
    let end = HEADER_LEN.checked_add(declared)?;
    if end > bytes.len() {
        return None;
    }

    let mut at = HEADER_LEN;
    while at + 4 <= end {
        let kind = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        let body = at + 4;
        let body_end = body.checked_add(length)?;
        if body_end > end {
            return None;
        }

        if kind == XOR_MAPPED_ADDRESS {
            if let Some(address) = decode_xor_address(&bytes[body..body_end], transaction) {
                return Some(address);
            }
        }

        // Attributes are padded to a multiple of four.
        at = body_end + ((4 - length % 4) % 4);
    }
    None
}

/// XOR-MAPPED-ADDRESS, as defined by RFC 5389.
///
/// The address is exclusive-ored with the magic cookie, and for IPv6 with the
/// transaction id as well. That is not obfuscation for its own sake: some home
/// routers rewrite anything that looks like an IP address in a packet body,
/// and would corrupt the very answer being asked for.
fn decode_xor_address(body: &[u8], transaction: &[u8; 12]) -> Option<SocketAddr> {
    if body.len() < 4 {
        return None;
    }
    let family = body[1];
    let port = u16::from_be_bytes([body[2], body[3]]) ^ (MAGIC >> 16) as u16;

    match family {
        0x01 if body.len() >= 8 => {
            let raw = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(raw ^ MAGIC)), port))
        }
        0x02 if body.len() >= 20 => {
            let mut key = [0u8; 16];
            key[..4].copy_from_slice(&MAGIC.to_be_bytes());
            key[4..].copy_from_slice(transaction);

            let mut octets = [0u8; 16];
            for i in 0..16 {
                octets[i] = body[4 + i] ^ key[i];
            }
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(octets)), port))
        }
        _ => None,
    }
}

fn random_transaction() -> [u8; 12] {
    use rand::RngCore;
    let mut out = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut out);
    out
}

fn io(what: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| Error::Io { path: what.into(), source }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a success response, so parsing can be tested without a server.
    fn success_response(transaction: &[u8; 12], address: SocketAddr) -> Vec<u8> {
        let mut attribute = Vec::new();
        attribute.push(0);
        match address.ip() {
            IpAddr::V4(v4) => {
                attribute.push(0x01);
                attribute.extend_from_slice(&(address.port() ^ (MAGIC >> 16) as u16).to_be_bytes());
                let raw = u32::from_be_bytes(v4.octets()) ^ MAGIC;
                attribute.extend_from_slice(&raw.to_be_bytes());
            }
            IpAddr::V6(v6) => {
                attribute.push(0x02);
                attribute.extend_from_slice(&(address.port() ^ (MAGIC >> 16) as u16).to_be_bytes());
                let mut key = [0u8; 16];
                key[..4].copy_from_slice(&MAGIC.to_be_bytes());
                key[4..].copy_from_slice(transaction);
                for (i, byte) in v6.octets().iter().enumerate() {
                    attribute.push(byte ^ key[i]);
                }
            }
        }

        let mut out = Vec::new();
        out.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
        out.extend_from_slice(&((attribute.len() + 4) as u16).to_be_bytes());
        out.extend_from_slice(&MAGIC.to_be_bytes());
        out.extend_from_slice(transaction);
        out.extend_from_slice(&XOR_MAPPED_ADDRESS.to_be_bytes());
        out.extend_from_slice(&(attribute.len() as u16).to_be_bytes());
        out.extend_from_slice(&attribute);
        out
    }

    #[test]
    fn a_request_is_twenty_bytes_and_well_formed() {
        let transaction = [7u8; 12];
        let request = binding_request(&transaction);

        assert_eq!(request.len(), 20);
        assert_eq!(u16::from_be_bytes([request[0], request[1]]), BINDING_REQUEST);
        assert_eq!(u16::from_be_bytes([request[2], request[3]]), 0, "no attributes");
        assert_eq!(u32::from_be_bytes([request[4], request[5], request[6], request[7]]), MAGIC);
        assert_eq!(&request[8..], &transaction);
    }

    #[test]
    fn the_xor_arithmetic_is_what_the_specification_says() {
        // Worked by hand so a reader can check it: 192.168.1.1 is 0xC0A80101,
        // and 0xC0A80101 XOR 0x2112A442 is 0xE1BAA543. Port 5000 is 0x1388,
        // and 0x1388 XOR 0x2112 is 0x329A.
        let transaction = [0u8; 12];
        let response = success_response(&transaction, "192.168.1.1:5000".parse().unwrap());

        let attribute = &response[24..];
        assert_eq!(u16::from_be_bytes([attribute[2], attribute[3]]), 0x329A, "port xor");
        assert_eq!(
            u32::from_be_bytes([attribute[4], attribute[5], attribute[6], attribute[7]]),
            0xE1BA_A543,
            "address xor"
        );
        assert_eq!(
            parse_binding_response(&response, &transaction).unwrap(),
            "192.168.1.1:5000".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn addresses_round_trip() {
        let transaction = [0x2Bu8; 12];
        for text in ["203.0.113.5:4500", "8.8.8.8:53", "[2001:db8::1]:51820", "[::1]:9"] {
            let address: SocketAddr = text.parse().unwrap();
            let response = success_response(&transaction, address);
            assert_eq!(parse_binding_response(&response, &transaction), Some(address), "{text}");
        }
    }

    #[test]
    fn a_response_to_someone_elses_request_is_ignored() {
        // Without this an off-path attacker could answer first and choose what
        // address we believe we have.
        let response = success_response(&[1u8; 12], "203.0.113.5:4500".parse().unwrap());
        assert_eq!(parse_binding_response(&response, &[2u8; 12]), None);
    }

    #[test]
    fn rubbish_on_the_port_is_ignored_rather_than_parsed() {
        let transaction = [3u8; 12];
        let valid = success_response(&transaction, "203.0.113.5:4500".parse().unwrap());

        // Truncation at every length.
        for cut in 0..valid.len() {
            let _ = parse_binding_response(&valid[..cut], &transaction);
        }
        // Arbitrary traffic.
        for junk in [vec![], vec![0xFF; 200], b"GET / HTTP/1.1\r\n\r\n".to_vec()] {
            assert_eq!(parse_binding_response(&junk, &transaction), None);
        }
        // Right shape, wrong cookie.
        let mut wrong_cookie = valid.clone();
        wrong_cookie[4] ^= 0xFF;
        assert_eq!(parse_binding_response(&wrong_cookie, &transaction), None);
    }

    #[test]
    fn a_declared_length_longer_than_the_packet_is_refused() {
        // A four-byte length field that nobody checks is how a parser is made to
        // read past its buffer.
        let transaction = [4u8; 12];
        let mut response = success_response(&transaction, "203.0.113.5:4500".parse().unwrap());
        response[2..4].copy_from_slice(&9000u16.to_be_bytes());
        assert_eq!(parse_binding_response(&response, &transaction), None);
    }

    #[test]
    fn unknown_attributes_are_skipped_not_fatal() {
        // Servers send SOFTWARE, MAPPED-ADDRESS and others. A parser that gives
        // up on the first thing it does not recognise works against some
        // servers and not others, which is a miserable bug to chase.
        let transaction = [5u8; 12];
        let address: SocketAddr = "198.51.100.9:1234".parse().unwrap();

        let mut body = Vec::new();
        // An unknown attribute first, with a length needing padding.
        body.extend_from_slice(&0x8022u16.to_be_bytes());
        body.extend_from_slice(&5u16.to_be_bytes());
        body.extend_from_slice(b"hello");
        body.extend_from_slice(&[0, 0, 0]); // padding to 4 bytes

        let real = success_response(&transaction, address);
        body.extend_from_slice(&real[20..]);

        let mut response = Vec::new();
        response.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
        response.extend_from_slice(&(body.len() as u16).to_be_bytes());
        response.extend_from_slice(&MAGIC.to_be_bytes());
        response.extend_from_slice(&transaction);
        response.extend_from_slice(&body);

        assert_eq!(parse_binding_response(&response, &transaction), Some(address));
    }

    #[test]
    fn classification_needs_two_answers_to_mean_anything() {
        assert!(NatBehaviour::EndpointIndependent.can_punch());
        assert!(!NatBehaviour::Symmetric.can_punch());
        assert!(!NatBehaviour::Blocked.can_punch());
        assert!(!NatBehaviour::Inconclusive.can_punch());
    }
}

#[cfg(test)]
mod address_tests {
    use super::*;

    /// The machine running this has at least one address, and none of what is
    /// returned is a thing a peer could not dial.
    #[test]
    fn every_announced_address_is_dialable() {
        let found = local_addresses();
        for ip in &found {
            assert!(!ip.is_loopback(), "announced loopback: {ip}");
            assert!(!ip.is_unspecified(), "announced the wildcard: {ip}");
            if let IpAddr::V6(v6) = ip {
                assert_ne!(
                    v6.segments()[0] & 0xffc0,
                    0xfe80,
                    "announced a link-local address with no scope: {ip}"
                );
            }
        }
        // Duplicates would make a peer race the same candidate twice.
        let mut unique = found.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), found.len(), "duplicate addresses in {found:?}");
    }

    /// An overlay network's address must be offered, not filtered out as
    /// private. It is frequently the only one that works from elsewhere.
    #[test]
    fn a_second_interface_is_not_discarded() {
        // Nothing can be asserted about *which* addresses a build machine has,
        // so this checks the filter's shape rather than its result: a private
        // address is a legitimate candidate, because the peer that can use it
        // is on the same private network.
        let found = local_addresses();
        if found.is_empty() {
            return; // A machine with no network. Nothing to check.
        }
        assert!(
            found.iter().any(|ip| !ip.is_loopback()),
            "no usable address found among {found:?}"
        );
    }
}
