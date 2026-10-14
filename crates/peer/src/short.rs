//! Six digits to pair (decision 0066).
//!
//! The long code carries the showing device's whole fingerprint, which is
//! what makes pairing safe (decision 0014), and is about a hundred characters
//! to type. Six digits cannot carry it. So they are a password instead, for
//! fetching the long code from the device showing them:
//!
//! ```text
//!   showing   multicasts an offer: "a code, its listener on this port"
//!   typing    connects, certificate unpinned, and runs SPAKE2 with the
//!             digits, both certificates bound in as the identities
//!   showing   its half, and a confirmation
//!   typing    checks it; its confirmation
//!   showing   the long code, sealed under the key
//!   typing    pairs with the long code as if it had scanned it
//! ```
//!
//! SPAKE2 lets a party without the digits test one guess per exchange, and
//! nothing offline, so five tries per code leave five chances in a million.
//! A device that guessed still has to be approved, by the number that only
//! the phone in the person's hand shows (decision 0053).

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
use crate::local;
use crate::wire::{Request, Response, MAX_MESSAGE, SPAKE_LEN};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use spake2::{Ed25519Group, Identity as SpakeIdentity, Password, Spake2};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// The port offers go to, on the beacons' group. Its own, so that neither
/// kind of packet is ever taken for the other.
pub const OFFER_PORT: u16 = 47318;

/// How many exchanges the digits allow before they stop working for this
/// code. A right one is given back.
pub const TRIES: u32 = 5;

/// How long a device typing digits listens for offers.
pub const LOOK_FOR: Duration = Duration::from_secs(10);

/// How often an offer is repeated: multicast is lossy, and a device typing
/// the digits is listening now.
const OFFER_EVERY: Duration = Duration::from_secs(1);

/// After a wrong try, how long to wait for another device offering before
/// saying the digits were wrong. Two people pairing on one network at once
/// is possible; waiting the whole ten seconds every time a digit is mistyped
/// is not worth it.
const AFTER_WRONG: Duration = Duration::from_secs(2);

const OFFER: &[u8; 8] = b"qurbofr1";
const SEEK: &[u8; 8] = b"qurbsek1";

/// Six digits, every value equally likely.
pub fn new_digits() -> String {
    use rand::RngCore;
    // The largest multiple of a million a u32 holds, so that taking the
    // remainder favours no value.
    const LIMIT: u32 = u32::MAX - u32::MAX % 1_000_000;
    loop {
        let n = rand::rngs::OsRng.next_u32();
        if n < LIMIT {
            return format!("{:06}", n % 1_000_000);
        }
    }
}

/// Six digits, if that is what was typed: spaces and dashes between them are
/// allowed, as people write them.
pub fn as_digits(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(|c| !matches!(c, ' ' | '-' | '\u{a0}')).collect();
    (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_digit())).then_some(digits)
}

/// Spaced for reading: `482 913`.
pub fn spaced(digits: &str) -> String {
    if digits.len() == 6 {
        format!("{} {}", &digits[..3], &digits[3..])
    } else {
        digits.to_string()
    }
}

/// The digits a device showing a code offers, and how many tries are left.
pub(crate) struct Offered {
    digits: String,
    tries: AtomicU32,
}

impl Offered {
    pub(crate) fn new() -> Self {
        Self { digits: new_digits(), tries: AtomicU32::new(TRIES) }
    }

    /// The digits, while they are still taken.
    pub(crate) fn digits(&self) -> Option<&str> {
        (self.tries.load(Ordering::SeqCst) > 0).then_some(self.digits.as_str())
    }

    /// Spend a try, if one is left.
    fn spend(&self) -> bool {
        self.tries.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok()
    }

    /// Give a try back: the digits were right.
    fn refund(&self) {
        self.tries.fetch_add(1, Ordering::SeqCst);
    }
}

/// One exchange, from the side of the device showing the code. Held across
/// the two requests of one connection.
#[derive(Default)]
pub(crate) struct Exchange {
    key: Option<[u8; 32]>,
}

impl Exchange {
    /// Answer one of the two requests, or `None` if it is not one of them.
    pub(crate) fn answer(
        &mut self,
        request: &Request,
        offered: &Offered,
        ours: Fingerprint,
        theirs: Fingerprint,
        long_code: &str,
    ) -> Option<Response> {
        match request {
            Request::Short { spake } => {
                // One exchange a connection.
                if self.key.is_some() {
                    return Some(Response::NotFound);
                }
                // Counted before anything is sent: seeing this side's half and
                // confirmation is what lets the other side test its guess.
                if !offered.spend() {
                    return Some(Response::Withdrawn);
                }
                let (state, half) = start(Side::Showing, &offered.digits, theirs, ours);
                let Ok(key) = state.finish(spake) else { return Some(Response::NotFound) };
                let key = key_of(&key);
                self.key = Some(key);
                let mut ours_half = [0u8; SPAKE_LEN];
                ours_half.copy_from_slice(&half);
                Some(Response::Short { spake: ours_half, confirm: confirmation(&key, Side::Showing) })
            }
            Request::ShortConfirm { confirm } => {
                let Some(key) = self.key.take() else { return Some(Response::NotFound) };
                if !same(confirm, &confirmation(&key, Side::Typing)) {
                    tracing::warn!(peer = %theirs.short(), "six digits tried, and wrong");
                    return Some(Response::NotFound);
                }
                offered.refund();
                Some(match seal(&key, long_code) {
                    Ok(sealed) => Response::Sealed(sealed),
                    Err(_) => Response::NotFound,
                })
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Side {
    Showing,
    Typing,
}

/// This side's start of SPAKE2. The typing device is A, the showing device
/// B; the identities are the fingerprints each saw in the TLS handshake,
/// so that two sides that saw different certificates -- a machine between
/// them -- derive different keys.
fn start(side: Side, digits: &str, typing: Fingerprint, showing: Fingerprint) -> (Spake2<Ed25519Group>, Vec<u8>) {
    let password = Password::new(format!("qurb six digits v1:{digits}").as_bytes());
    let a = SpakeIdentity::new(typing.as_bytes());
    let b = SpakeIdentity::new(showing.as_bytes());
    match side {
        Side::Typing => Spake2::<Ed25519Group>::start_a(&password, &a, &b),
        Side::Showing => Spake2::<Ed25519Group>::start_b(&password, &a, &b),
    }
}

/// SPAKE2's shared secret as a key of the size the rest uses.
fn key_of(secret: &[u8]) -> [u8; 32] {
    blake3::derive_key("qurb six digits v1 key", secret)
}

fn confirmation(key: &[u8; 32], side: Side) -> [u8; 32] {
    let who: &[u8] = match side {
        Side::Showing => b"showing",
        Side::Typing => b"typing",
    };
    *blake3::keyed_hash(key, who).as_bytes()
}

fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn cipher(key: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(&blake3::derive_key("qurb six digits v1 seal", key).into())
}

fn seal(key: &[u8; 32], long_code: &str) -> Result<Vec<u8>> {
    use rand::RngCore;
    let mut nonce = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let sealed = cipher(key)
        .encrypt(XNonce::from_slice(&nonce), long_code.as_bytes())
        .map_err(|_| Error::Protocol { detail: "could not seal the code".into() })?;
    Ok([nonce.as_slice(), &sealed].concat())
}

fn open(key: &[u8; 32], sealed: &[u8]) -> Result<String> {
    if sealed.len() < 24 {
        return Err(Error::Protocol { detail: "a sealed code too short".into() });
    }
    let (nonce, body) = sealed.split_at(24);
    let plain = cipher(key)
        .decrypt(XNonce::from_slice(nonce), body)
        .map_err(|_| Error::Protocol { detail: "the code did not open".into() })?;
    String::from_utf8(plain).map_err(|_| Error::Protocol { detail: "the code is not text".into() })
}

/// Say, on the local network, that a code is being shown and where its
/// listener is, every second and whenever a device asks, until dropped.
/// Stops saying it once the digits are no longer taken.
pub(crate) async fn offer(offered: &Offered, listener_port: u16, port: u16) {
    let socket = match local::open_socket(port).and_then(|s| {
        s.set_nonblocking(true).map_err(|e| Error::Io { path: "offer socket".into(), source: e })?;
        tokio::net::UdpSocket::from_std(s).map_err(|e| Error::Io { path: "offer socket".into(), source: e })
    }) {
        Ok(socket) => socket,
        Err(e) => {
            // The QR code and the long code still work.
            tracing::warn!(error = %e, "could not offer six digits on this network");
            return std::future::pending().await;
        }
    };
    let packet = [OFFER.as_slice(), &listener_port.to_le_bytes()].concat();
    let mut timer = tokio::time::interval(OFFER_EVERY);
    let mut buffer = [0u8; 64];
    loop {
        tokio::select! {
            _ = timer.tick() => {}
            heard = socket.recv_from(&mut buffer) => {
                // Answer a device asking at once; anything else is ignored.
                match heard {
                    Ok((n, _)) if &buffer[..n] == SEEK => {}
                    _ => continue,
                }
            }
        }
        if offered.digits().is_none() {
            return std::future::pending().await;
        }
        if let Err(e) = socket.send_to(&packet, local::destination(port)).await {
            tracing::trace!(error = %e, "could not send an offer");
        }
    }
}

/// A typed code as an invite: the long code read, or six digits found on
/// this network and their long code fetched.
///
/// What six digits fetched is remembered while it lasts, so that showing the
/// number to compare and then joining, two calls, find the device once.
pub async fn resolve(text: &str, identity: &Identity) -> Result<crate::pairing::Invite> {
    let Some(digits) = as_digits(text) else { return crate::pairing::Invite::parse(text) };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let found = FOUND.lock().expect("found").get(&digits).filter(|i| !i.is_expired(now)).cloned();
    if let Some(invite) = found {
        return Ok(invite);
    }
    let invite = find(&digits, identity).await?;
    let mut remembered = FOUND.lock().expect("found");
    remembered.retain(|_, i| !i.is_expired(now));
    remembered.insert(digits, invite.clone());
    Ok(invite)
}

static FOUND: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, crate::pairing::Invite>>> =
    std::sync::LazyLock::new(Default::default);

/// Find the device showing these digits on this network, and fetch its long
/// code. The address in it is the one that answered.
pub async fn find(digits: &str, identity: &Identity) -> Result<crate::pairing::Invite> {
    find_on(OFFER_PORT, digits, identity, LOOK_FOR).await
}

/// [`find`], on a port and for a time of the caller's choosing: for tests,
/// which must not hear each other's offers.
pub async fn find_on(
    port: u16,
    digits: &str,
    identity: &Identity,
    look: Duration,
) -> Result<crate::pairing::Invite> {
    let digits = as_digits(digits).ok_or_else(|| Error::BadInvite { detail: "not six digits".into() })?;
    let socket = local::open_socket(port)?;
    socket.set_nonblocking(true).map_err(|e| Error::Io { path: "seek socket".into(), source: e })?;
    let socket = tokio::net::UdpSocket::from_std(socket)
        .map_err(|e| Error::Io { path: "seek socket".into(), source: e })?;

    let started = tokio::time::Instant::now();
    let mut deadline = started + look;
    let mut asked = tokio::time::interval(Duration::from_millis(700));
    let mut tried = HashSet::new();
    let (mut wrong, mut withdrawn) = (false, false);
    let mut buffer = [0u8; 64];
    loop {
        let heard = tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            _ = asked.tick() => {
                let _ = socket.send_to(SEEK, local::destination(port)).await;
                continue;
            }
            heard = socket.recv_from(&mut buffer) => heard,
        };
        let Ok((n, from)) = heard else { continue };
        if n != OFFER.len() + 2 || &buffer[..OFFER.len()] != OFFER {
            continue;
        }
        let listener = SocketAddr::new(from.ip(), u16::from_le_bytes([buffer[8], buffer[9]]));
        if !tried.insert(listener) {
            continue;
        }
        match try_offer(listener, &digits, identity).await {
            Ok(invite) => return Ok(invite),
            Err(Error::WrongDigits) => {
                wrong = true;
                deadline = deadline.min(tokio::time::Instant::now() + AFTER_WRONG);
            }
            Err(Error::DigitsWithdrawn) => {
                withdrawn = true;
                deadline = deadline.min(tokio::time::Instant::now() + AFTER_WRONG);
            }
            Err(e) => tracing::debug!(at = %listener, error = %e, "an offer could not be tried"),
        }
    }
    Err(if withdrawn {
        Error::DigitsWithdrawn
    } else if wrong {
        Error::WrongDigits
    } else {
        Error::NoneOffering
    })
}

/// One exchange with one device offering a code.
async fn try_offer(listener: SocketAddr, digits: &str, identity: &Identity) -> Result<crate::pairing::Invite> {
    let bind: SocketAddr = if listener.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().expect("literal");
    let mut endpoint = quinn::Endpoint::client(bind)
        .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
    endpoint.set_default_client_config(crate::tls::finding_config(identity)?);
    let connection = tokio::time::timeout(Duration::from_secs(5), endpoint.connect(listener, "qurb-device")?)
        .await
        .map_err(|_| Error::PeerDidNotAnswer)??;
    let result = exchange(&connection, digits, identity).await;
    connection.close(0u32.into(), b"done");
    endpoint.close(0u32.into(), b"done");
    let mut invite = result?;
    invite.address = listener;
    Ok(invite)
}

async fn exchange(connection: &quinn::Connection, digits: &str, identity: &Identity) -> Result<crate::pairing::Invite> {
    let showing = crate::pairing::fingerprint_of(connection)
        .ok_or_else(|| Error::Protocol { detail: "no certificate".into() })?;
    let (state, half) = start(Side::Typing, digits, identity.fingerprint(), showing);
    let mut ours = [0u8; SPAKE_LEN];
    ours.copy_from_slice(&half);

    let key = match ask(connection, &Request::Short { spake: ours }).await? {
        Response::Short { spake, confirm } => {
            let key = key_of(&state.finish(&spake).map_err(|_| Error::WrongDigits)?);
            if !same(&confirm, &confirmation(&key, Side::Showing)) {
                return Err(Error::WrongDigits);
            }
            key
        }
        Response::Withdrawn => return Err(Error::DigitsWithdrawn),
        _ => return Err(Error::WrongDigits),
    };

    let sealed = match ask(connection, &Request::ShortConfirm { confirm: confirmation(&key, Side::Typing) }).await? {
        Response::Sealed(sealed) => sealed,
        _ => return Err(Error::WrongDigits),
    };
    let invite = crate::pairing::Invite::parse(&open(&key, &sealed)?)?;
    // The code must be the device's that took part: anything else is a code
    // some other device was made to hand over.
    if invite.fingerprint != showing {
        return Err(Error::Protocol { detail: "the code is for another device".into() });
    }
    Ok(invite)
}

async fn ask(connection: &quinn::Connection, request: &Request) -> Result<Response> {
    let (mut send, mut recv) = connection.open_bi().await?;
    send.write_all(&request.encode()).await?;
    send.finish()?;
    let raw = recv.read_to_end(MAX_MESSAGE).await?;
    Response::decode(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_six_and_typed_as_people_write_them() {
        for _ in 0..1000 {
            let d = new_digits();
            assert_eq!(d.len(), 6);
            assert!(d.bytes().all(|b| b.is_ascii_digit()));
        }
        assert_eq!(as_digits("482 913").as_deref(), Some("482913"));
        assert_eq!(as_digits("482-913").as_deref(), Some("482913"));
        assert_eq!(as_digits(" 482913 ").as_deref(), Some("482913"));
        assert_eq!(as_digits("48291"), None);
        assert_eq!(as_digits("4829134"), None);
        assert_eq!(as_digits("qurb1-abcdef"), None);
        assert_eq!(spaced("482913"), "482 913");
    }

    fn fingerprint(n: u8) -> Fingerprint {
        Fingerprint::from_bytes([n; 32])
    }

    /// Both sides of the exchange, without a network: the right digits give
    /// the long code; wrong ones give nothing and spend a try; five spent and
    /// the digits are withdrawn, a right one included.
    #[test]
    fn the_right_digits_open_the_code_and_five_wrong_ones_close_it() {
        let offered = Offered::new();
        let digits = offered.digits.clone();
        let (showing, typing) = (fingerprint(1), fingerprint(2));
        let try_with = |typed: &str| -> std::result::Result<String, &'static str> {
            let mut host = Exchange::default();
            let (state, half) = start(Side::Typing, typed, typing, showing);
            let reply = host
                .answer(&Request::Short { spake: half.try_into().unwrap() }, &offered, showing, typing, "the long code")
                .unwrap();
            let Response::Short { spake, confirm } = reply else { return Err("withdrawn") };
            let key = key_of(&state.finish(&spake).unwrap());
            if !same(&confirm, &confirmation(&key, Side::Showing)) {
                return Err("wrong");
            }
            match host
                .answer(&Request::ShortConfirm { confirm: confirmation(&key, Side::Typing) }, &offered, showing, typing, "the long code")
                .unwrap()
            {
                Response::Sealed(sealed) => Ok(open(&key, &sealed).unwrap()),
                _ => Err("refused"),
            }
        };

        assert_eq!(try_with(&digits), Ok("the long code".to_string()));
        let wrong = if digits == "000000" { "000001" } else { "000000" };
        for _ in 0..TRIES {
            assert_eq!(try_with(wrong), Err("wrong"));
        }
        assert_eq!(offered.digits(), None);
        assert_eq!(try_with(&digits), Err("withdrawn"));
    }

    /// A machine between the two presents its own certificate to each, so
    /// the fingerprints the two sides bind in differ, and with the right
    /// digits the exchange still fails.
    #[test]
    fn different_certificates_seen_make_different_keys() {
        let offered = Offered::new();
        let digits = offered.digits.clone();
        let (showing, typing, between) = (fingerprint(1), fingerprint(2), fingerprint(3));
        let mut host = Exchange::default();
        // The typing side saw `between` as the showing device's certificate.
        let (state, half) = start(Side::Typing, &digits, typing, between);
        let Some(Response::Short { spake, confirm }) =
            host.answer(&Request::Short { spake: half.try_into().unwrap() }, &offered, showing, typing, "code")
        else {
            panic!("no answer")
        };
        let key = key_of(&state.finish(&spake).unwrap());
        assert!(!same(&confirm, &confirmation(&key, Side::Showing)));
    }
}
