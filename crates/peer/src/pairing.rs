//! Deciding which device to trust.
//!
//! Everything else in this crate assumes you already know the fingerprint you
//! expect. Pairing is where that knowledge comes from, and it is the piece
//! without which pinned identity means nothing: a fingerprint only protects you
//! if you learned the right one.
//!
//! ```text
//!   inviter  invite()      generate a one-time code, listen for it
//!            ──── QR code or read aloud ────►         out of band
//!   joiner   accept()      connect with the invite's fingerprint pinned,
//!                          present the token, exchange identities
//! ```
//!
//! # Where the security actually comes from
//!
//! The invite carries the inviter's **full fingerprint**, not an abbreviation.
//! Transferring it out of band — a QR code on a screen, a code read over the
//! phone — is what authenticates the inviter, because an attacker in the
//! network path cannot change what is printed on a screen.
//!
//! That makes the one-time token a narrower thing than it first appears. It
//! does not authenticate anybody. It exists so the inviter can tell a device
//! that saw the invite from one that merely found the port open, and so an
//! invite stops working once it has been used.
//!
//! The joiner is authenticated by TLS: it must present a certificate and sign
//! the handshake, so the fingerprint recorded for it is proven rather than
//! claimed. Neither side takes the other's word for anything that matters.

use crate::base32;
use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
use crate::tls;
use crate::wire::{Request, Response, MAX_MESSAGE};
use qurb_storage::Store;
use qurb_sync::DeviceId;
use std::net::SocketAddr;
use std::time::Duration;
use std::sync::{Arc, Mutex};

/// How long an invite is good for.
///
/// Short on purpose. An invite is something a person is looking at right now;
/// one left lying around is a port that accepts strangers.
pub const INVITE_LIFETIME_SECS: i64 = 300;

const PREFIX: &str = "qurb1-";
/// A guest invite: another person's device visiting a computer (decision
/// 0060). Its own prefix, so that a build that knows nothing of guests refuses
/// it as not a code, rather than pairing as one of this person's devices.
const GUEST_PREFIX: &str = "qurbg1-";
const TOKEN_LEN: usize = 16;

/// What crosses the out-of-band channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// The inviter's identity. The whole point of the exercise.
    pub fingerprint: Fingerprint,
    pub address: SocketAddr,
    /// One-time, so an invite cannot be used twice or by someone who found the
    /// port rather than the code.
    pub token: [u8; TOKEN_LEN],
    pub expires_at: i64,
    /// For another person's device, visiting as a guest (decision 0060),
    /// rather than for one of this person's own.
    pub guest: bool,
}

impl Invite {
    pub fn new(fingerprint: Fingerprint, address: SocketAddr, now: i64) -> Self {
        use rand::RngCore;
        let mut token = [0u8; TOKEN_LEN];
        rand::rngs::OsRng.fill_bytes(&mut token);
        Self { fingerprint, address, token, expires_at: now + INVITE_LIFETIME_SECS, guest: false }
    }

    fn prefix(&self) -> &'static str {
        if self.guest { GUEST_PREFIX } else { PREFIX }
    }

    pub fn is_expired(&self, now: i64) -> bool {
        now > self.expires_at
    }

    /// Render for a QR code or for someone to read aloud.
    pub fn encode(&self) -> String {
        let mut payload = Vec::with_capacity(64);
        payload.extend_from_slice(self.fingerprint.as_bytes());
        payload.extend_from_slice(&self.token);
        payload.extend_from_slice(&self.expires_at.to_le_bytes());
        match self.address.ip() {
            std::net::IpAddr::V4(v4) => {
                payload.push(4);
                payload.extend_from_slice(&v4.octets());
            }
            std::net::IpAddr::V6(v6) => {
                payload.push(6);
                payload.extend_from_slice(&v6.octets());
            }
        }
        payload.extend_from_slice(&self.address.port().to_le_bytes());

        format!("{}{}", self.prefix(), base32::encode(&payload))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let trimmed = text.trim();
        let strip = |prefix: &str| {
            trimmed.strip_prefix(prefix).or_else(|| trimmed.strip_prefix(&prefix.to_uppercase()))
        };
        let (body, guest) = match strip(GUEST_PREFIX) {
            Some(body) => (body, true),
            None => (
                strip(PREFIX).ok_or_else(|| Error::BadInvite { detail: "not a pairing code".into() })?,
                false,
            ),
        };

        let payload = base32::decode(body)
            .ok_or_else(|| Error::BadInvite { detail: "code contains invalid characters".into() })?;

        // 32 fingerprint + 16 token + 8 expiry + 1 tag + address + 2 port
        if payload.len() < 59 {
            return Err(Error::BadInvite { detail: "code is too short".into() });
        }

        let mut fingerprint = [0u8; 32];
        fingerprint.copy_from_slice(&payload[..32]);
        let mut token = [0u8; TOKEN_LEN];
        token.copy_from_slice(&payload[32..48]);
        let mut expiry = [0u8; 8];
        expiry.copy_from_slice(&payload[48..56]);

        let (ip, rest): (std::net::IpAddr, &[u8]) = match payload[56] {
            4 if payload.len() == 63 => {
                let mut o = [0u8; 4];
                o.copy_from_slice(&payload[57..61]);
                (std::net::Ipv4Addr::from(o).into(), &payload[61..])
            }
            6 if payload.len() == 75 => {
                let mut o = [0u8; 16];
                o.copy_from_slice(&payload[57..73]);
                (std::net::Ipv6Addr::from(o).into(), &payload[73..])
            }
            _ => return Err(Error::BadInvite { detail: "malformed address".into() }),
        };
        let port = u16::from_le_bytes([rest[0], rest[1]]);

        Ok(Self {
            fingerprint: Fingerprint::from_bytes(fingerprint),
            address: SocketAddr::new(ip, port),
            token,
            expires_at: i64::from_le_bytes(expiry),
            guest,
        })
    }

    /// Grouped into fives, for someone reading it out.
    pub fn for_humans(&self) -> String {
        let encoded = self.encode();
        let body = encoded.strip_prefix(self.prefix()).unwrap_or(&encoded);
        let grouped: Vec<String> =
            body.as_bytes().chunks(5).map(|c| String::from_utf8_lossy(c).to_string()).collect();
        format!("{}{}", self.prefix(), grouped.join("-"))
    }
}

/// Listens for exactly one device to accept an invite.
pub struct PairingHost {
    endpoint: quinn::Endpoint,
    invite: Invite,
}

/// What the two sides learn about each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paired {
    pub device_id: DeviceId,
    pub fingerprint: Fingerprint,
    pub name: String,
    /// `phone`, `computer` or `replica`, as the device said (decision 0053).
    pub kind: Option<String>,
}

/// This device, as pairing presents it: its name, its kind (`phone`,
/// `computer` or `replica`) and its key, from which the check two devices
/// compare is derived -- and which a device with none is given.
pub struct Ours<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub key: &'a qurb_keys::MasterKey,
}

/// A device asking to pair, for the person at the device showing the code to
/// approve (decision 0053).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    /// What it calls itself. A claim: the number is what proves which device
    /// it is.
    pub name: String,
    pub kind: Option<String>,
    pub fingerprint: Fingerprint,
    /// Six digits the asking device shows too. Derived from both
    /// certificates and the code, so another device that used the same code
    /// shows a different number.
    pub number: String,
    /// Whether it asked for the key, as a device with none does.
    pub wants_key: bool,
    /// Whether it is another person's device asking to visit as a guest
    /// (decision 0060).
    pub guest: bool,
}

/// The six digits both screens show while a device asks to pair: from the
/// fingerprint of the device showing the code, the fingerprint of the device
/// asking, and the code's token. Spaced, `482 913`, for reading.
///
/// Both ends can compute it without trusting each other: the device showing
/// the code knows its own fingerprint and has the other's from the TLS
/// handshake; the asking device knows its own and has the other's from the
/// code. A device that learned the code and used it first holds a different
/// certificate, and so shows a different number from the one in the person's
/// hand.
pub fn pairing_number(host: &Fingerprint, joiner: &Fingerprint, token: &[u8; TOKEN_LEN]) -> String {
    let mut hasher = blake3::Hasher::new_derive_key("qurb pairing number v1");
    hasher.update(host.as_bytes());
    hasher.update(joiner.as_bytes());
    hasher.update(token);
    let digest = hasher.finalize();
    let n = u32::from_le_bytes(digest.as_bytes()[..4].try_into().expect("four bytes")) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

impl Invite {
    /// The number a device joining with this code shows: see
    /// [`pairing_number`].
    pub fn number_for(&self, joiner: &Fingerprint) -> String {
        pairing_number(&self.fingerprint, joiner, &self.token)
    }
}

/// What two devices compare to know they share a key, without sending it.
fn key_check(key: &qurb_keys::MasterKey) -> [u8; 32] {
    key.derive(qurb_keys::Purpose::PairingCheck).to_bytes()
}

/// A kind a device claims, if it is one there is a use for.
fn known_kind(kind: &str) -> Option<String> {
    matches!(kind, "phone" | "computer" | "replica").then(|| kind.to_string())
}

impl PairingHost {
    /// Open a port and produce an invite for it.
    ///
    /// The listener accepts **any** certificate, unlike ordinary serving — it
    /// has to, because the device joining is by definition not yet known. What
    /// keeps that safe is that it answers nothing except a pairing request, and
    /// only one carrying the token.
    pub fn open(bind: SocketAddr, identity: &Identity, now: i64) -> Result<Self> {
        let config = tls::pairing_server_config(identity)?;
        let endpoint = quinn::Endpoint::server(config, bind)
            .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
        let bound = endpoint
            .local_addr()
            .map_err(|e| Error::Io { path: "local_addr".into(), source: e })?;

        // A wildcard bind reports `0.0.0.0`, which is true and useless: it means
        // every interface, and nobody can dial it. An invite carrying it fails
        // at the far end in a way that looks like the joining device's fault.
        let address = crate::nat::dialable(bound);

        Ok(Self { invite: Invite::new(identity.fingerprint(), address, now), endpoint })
    }

    /// The same, for another person's device to visit as a guest (decision
    /// 0060): it keeps its own key, is shown only what is sent to it, and
    /// meets this device under a secret of their own.
    pub fn open_for_guest(bind: SocketAddr, identity: &Identity, now: i64) -> Result<Self> {
        let mut host = Self::open(bind, identity, now)?;
        host.invite.guest = true;
        Ok(host)
    }

    pub fn invite(&self) -> &Invite {
        &self.invite
    }

    /// Wait for one device to pair, then stop listening.
    ///
    /// A device must present the code's token, and then be approved by the
    /// person here: `approve` is shown who is asking and the number it should
    /// be showing, and says yes or no (decision 0053). A device with no key
    /// is given this one's (decision 0052), once per code. A device with a
    /// different key is refused. A wrong token, a refusal or a decline does
    /// not burn the invite: the host goes on waiting for the right device.
    pub async fn wait<A, F>(
        &self,
        store: Arc<Mutex<Store>>,
        ours: &Ours<'_>,
        now: i64,
        approve: A,
    ) -> Result<Paired>
    where
        A: Fn(Asking) -> F,
        F: std::future::Future<Output = bool>,
    {
        let our_device = { store.lock().expect("store mutex").device_id()? };

        // How long this invite has left, from the caller's clock.
        //
        // The whole wait is bounded by it. Without this the accept loop blocks
        // for ever, so a `qurb pair` nobody answers sits there advertising a
        // code that stopped working five minutes in -- which is worse than
        // failing, because the screen still says "Waiting..." and the person
        // reading the code out has no way to know it is dead.
        let remaining = self.invite.expires_at - now;
        if remaining <= 0 {
            return Err(Error::InviteExpired);
        }
        let deadline = Duration::from_secs(remaining as u64);

        tokio::time::timeout(deadline, self.accept_one(store, ours, our_device, approve))
            .await
            .unwrap_or(Err(Error::InviteExpired))
    }

    /// The accept loop, run under the caller's deadline.
    async fn accept_one<A, F>(
        &self,
        store: Arc<Mutex<Store>>,
        ours: &Ours<'_>,
        our_device: DeviceId,
        approve: A,
    ) -> Result<Paired>
    where
        A: Fn(Asking) -> F,
        F: std::future::Future<Output = bool>,
    {
        let check = key_check(ours.key);
        // Once the key has gone to one device, no other gets it from this
        // invite, whatever it presents.
        let mut key_given = false;

        while let Some(incoming) = self.endpoint.accept().await {
            let Ok(connection) = incoming.await else { continue };

            let Some(peer_fingerprint) = fingerprint_of(&connection) else {
                tracing::debug!("a device connected without a certificate");
                continue;
            };
            let number = pairing_number(&self.invite.fingerprint, &peer_fingerprint, &self.invite.token);
            // Approved on this connection already, by its Join: its Pair,
            // which follows on the same connection, is not asked about twice.
            let mut approved = false;

            loop {
                let Ok((mut send, mut recv)) = connection.accept_bi().await else { break };
                let Ok(raw) = recv.read_to_end(MAX_MESSAGE).await else { break };

                match Request::decode(&raw) {
                    // Another person's device, visiting as a guest, on a guest
                    // invite and nothing else (decision 0060).
                    Ok(Request::Visit { token, device_id, name, kind, person }) if self.invite.guest => {
                        if !constant_time_eq(&token, &self.invite.token) {
                            tracing::warn!(peer = %peer_fingerprint.short(), "wrong pairing token");
                            answer(&mut send, &Response::NotFound, &connection).await;
                            break;
                        }
                        let asking = Asking {
                            name: sanitise(&name),
                            kind: known_kind(&kind),
                            fingerprint: peer_fingerprint,
                            number: number.clone(),
                            wants_key: false,
                            guest: true,
                        };
                        if !approve(asking).await {
                            answer(&mut send, &Response::Declined, &connection).await;
                            break;
                        }
                        let meeting = {
                            use rand::RngCore;
                            let mut secret = [0u8; 32];
                            rand::rngs::OsRng.fill_bytes(&mut secret);
                            secret
                        };
                        let guest = Paired {
                            device_id: DeviceId::from_bytes(device_id),
                            fingerprint: peer_fingerprint,
                            name: sanitise(&name),
                            kind: known_kind(&kind),
                        };
                        {
                            let store = store.lock().expect("store mutex");
                            store.db().trust_visitor(
                                &guest.device_id,
                                guest.fingerprint.as_bytes(),
                                &guest.name,
                                qurb_storage::db::Relation::Guest,
                                &meeting,
                            )?;
                            if let Some(kind) = &guest.kind {
                                store.learn_kind(&guest.device_id, kind)?;
                            }
                            store.db().set_person(&guest.device_id, &person)?;
                        }
                        let reply = Response::Welcome {
                            device_id: *our_device.as_bytes(),
                            name: ours.name.to_string(),
                            kind: ours.kind.to_string(),
                            meeting,
                        };
                        send.write_all(&reply.encode()).await?;
                        send.finish()?;
                        connection.closed().await;
                        return Ok(guest);
                    }

                    // A guest invite is for guests: it never gives this
                    // device's key, nor pairs a device as one of this
                    // person's own.
                    Ok(Request::Join { .. } | Request::Pair { .. }) if self.invite.guest => {
                        tracing::warn!(peer = %peer_fingerprint.short(), "a guest invite used to pair as one's own device");
                        answer(&mut send, &Response::NotFound, &connection).await;
                        break;
                    }

                    // A device with no key, asking for this one's.
                    Ok(Request::Join { token, name, kind }) if !approved => {
                        if !constant_time_eq(&token, &self.invite.token) {
                            tracing::warn!(peer = %peer_fingerprint.short(), "wrong pairing token");
                            answer(&mut send, &Response::NotFound, &connection).await;
                            break;
                        }
                        if key_given {
                            answer(&mut send, &Response::NotFound, &connection).await;
                            break;
                        }
                        let asking = Asking {
                            name: sanitise(&name),
                            kind: known_kind(&kind),
                            fingerprint: peer_fingerprint,
                            number: number.clone(),
                            wants_key: true,
                            guest: false,
                        };
                        if !approve(asking).await {
                            answer(&mut send, &Response::Declined, &connection).await;
                            break;
                        }
                        key_given = true;
                        approved = true;
                        let key = ours.key.for_another_device();
                        send.write_all(&Response::Key { key }.encode()).await?;
                        send.finish()?;
                        // Then its Pair, on the next stream, once it has set
                        // itself up with the key.
                    }

                    Ok(Request::Pair { token, device_id, name, kind, key_check: theirs }) => {
                        // Constant-time, because a token compared byte by byte
                        // can be guessed one byte at a time by anyone who can
                        // measure the reply.
                        if !constant_time_eq(&token, &self.invite.token) {
                            tracing::warn!(peer = %peer_fingerprint.short(), "wrong pairing token");
                            answer(&mut send, &Response::NotFound, &connection).await;
                            break;
                        }
                        if theirs != check {
                            tracing::warn!(peer = %peer_fingerprint.short(), "a device with a different key tried to pair");
                            answer(&mut send, &Response::Mismatch, &connection).await;
                            break;
                        }
                        if !approved {
                            let asking = Asking {
                                name: sanitise(&name),
                                kind: known_kind(&kind),
                                fingerprint: peer_fingerprint,
                                number: number.clone(),
                                wants_key: false,
                                guest: false,
                            };
                            if !approve(asking).await {
                                answer(&mut send, &Response::Declined, &connection).await;
                                break;
                            }
                        }

                        let peer = Paired {
                            device_id: DeviceId::from_bytes(device_id),
                            fingerprint: peer_fingerprint,
                            name: sanitise(&name),
                            kind: known_kind(&kind),
                        };
                        {
                            let store = store.lock().expect("store mutex");
                            store.db().trust_peer(&peer.device_id, peer.fingerprint.as_bytes(), &peer.name)?;
                            if let Some(kind) = &peer.kind {
                                store.learn_kind(&peer.device_id, kind)?;
                            }
                        }

                        let reply = Response::Paired {
                            device_id: *our_device.as_bytes(),
                            name: ours.name.to_string(),
                            kind: ours.kind.to_string(),
                            key_check: check,
                        };
                        send.write_all(&reply.encode()).await?;
                        send.finish()?;
                        // Give the reply time to leave before the endpoint closes under it.
                        connection.closed().await;
                        return Ok(peer);
                    }

                    _ => {
                        answer(&mut send, &Response::NotFound, &connection).await;
                        break;
                    }
                }
            }
        }

        Err(Error::PairingAbandoned)
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"paired");
    }
}

/// Accept an invite as a device that has the key already: connect, prove we
/// saw the code, show the person the number while they approve it at the
/// other device, and exchange identities.
pub async fn accept(
    invite: &Invite,
    identity: &Identity,
    store: Arc<Mutex<Store>>,
    ours: &Ours<'_>,
    now: i64,
) -> Result<Paired> {
    if invite.is_expired(now) {
        return Err(Error::InviteExpired);
    }

    let bind: SocketAddr =
        if invite.address.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().expect("literal");
    let mut endpoint = quinn::Endpoint::client(bind)
        .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
    // The inviter's fingerprint came from the code, so this connection is
    // pinned exactly as an ordinary one would be. A machine in the path cannot
    // impersonate the device whose screen the user is looking at.
    endpoint.set_default_client_config(tls::client_config(identity, invite.fingerprint)?);

    let connection = endpoint.connect(invite.address, "qurb-device")?.await?;
    let host =
        pair_on(&connection, invite, store, ours.name, ours.kind, key_check(ours.key), now).await;
    hang_up(&connection, &endpoint).await;
    host
}

/// Close, and let the close reach the other device.
///
/// `close` only queues it. A joining program that exited straight afterwards
/// -- `qurb join` does -- took the close with it, and the device showing the
/// code, waiting to see its reply arrive, sat out the thirty-second idle
/// timeout before saying it had paired.
async fn hang_up(connection: &quinn::Connection, endpoint: &quinn::Endpoint) {
    connection.close(0u32.into(), b"paired");
    endpoint.close(0u32.into(), b"paired");
    let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
}

/// Answer, and let the answer leave before the connection is dropped.
///
/// Dropped straight after writing, the connection closed under the reply, and
/// the device asking saw "connection lost" rather than a refusal. Waited for
/// only briefly: a device that will not hang up must not hold the pairing
/// listener open for the next one.
async fn answer(send: &mut quinn::SendStream, response: &Response, connection: &quinn::Connection) {
    let _ = send.write_all(&response.encode()).await;
    let _ = send.finish();
    let _ = tokio::time::timeout(Duration::from_secs(2), connection.closed()).await;
}

/// Join as a device with no key: connect to the device showing the code, get
/// its key once the person there approves, set this device up with it through
/// `set_up`, and pair.
///
/// `identity` must already exist, since the connection presents it, and it is
/// the certificate the other device then trusts; [`Invite::number_for`] it is
/// the number to show while the other device asks. `set_up` installs the key
/// and returns the store it opened; it is the caller's because how a key is
/// kept differs by platform. Nothing is set up unless the key arrives from the
/// device whose fingerprint the code carries.
pub async fn join<F>(
    invite: &Invite,
    identity: &Identity,
    our_name: &str,
    our_kind: &str,
    now: i64,
    set_up: F,
) -> Result<Paired>
where
    F: FnOnce(qurb_keys::MasterKey) -> std::result::Result<Arc<Mutex<Store>>, String>,
{
    if invite.is_expired(now) {
        return Err(Error::InviteExpired);
    }

    let bind: SocketAddr =
        if invite.address.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().expect("literal");
    let mut endpoint = quinn::Endpoint::client(bind)
        .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
    endpoint.set_default_client_config(tls::client_config(identity, invite.fingerprint)?);
    let connection = endpoint.connect(invite.address, "qurb-device")?.await?;

    let (mut send, mut recv) = connection.open_bi().await?;
    let ask = Request::Join { token: invite.token, name: our_name.to_string(), kind: our_kind.to_string() };
    send.write_all(&ask.encode()).await?;
    send.finish()?;
    let waiting = std::time::Instant::now();
    let raw = recv
        .read_to_end(MAX_MESSAGE)
        .await
        .map_err(|e| while_waiting(e.into(), invite, now, waiting))?;
    let key = match Response::decode(&raw)? {
        Response::Key { key } => qurb_keys::MasterKey::from_bytes(key),
        Response::Declined => return Err(Error::Declined),
        Response::NotFound => return Err(Error::NoKeyGiven),
        other => {
            return Err(Error::Protocol { detail: format!("expected a key, got {other:?}") })
        }
    };

    let check = key_check(&key);
    let store = set_up(key).map_err(|detail| Error::SetUpFailed { detail })?;
    let host = pair_on(&connection, invite, store, our_name, our_kind, check, now).await;
    hang_up(&connection, &endpoint).await;
    host
}

/// Visit a computer as a guest, with a guest invite it showed (decision
/// 0060): this device keeps its own key, shows the number while the person
/// there approves, and is given the secret the two will meet under. It is
/// recorded here as a host -- another person's computer, shown only what is
/// sent to it.
pub async fn visit(
    invite: &Invite,
    identity: &Identity,
    store: Arc<Mutex<Store>>,
    our_name: &str,
    our_kind: &str,
    now: i64,
) -> Result<Paired> {
    if !invite.guest {
        return Err(Error::BadInvite {
            detail: "that code adds one of your own devices, not a visit as a guest".into(),
        });
    }
    if invite.is_expired(now) {
        return Err(Error::InviteExpired);
    }

    let bind: SocketAddr =
        if invite.address.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().expect("literal");
    let mut endpoint = quinn::Endpoint::client(bind)
        .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
    endpoint.set_default_client_config(tls::client_config(identity, invite.fingerprint)?);
    let connection = endpoint.connect(invite.address, "qurb-device")?.await?;

    let (our_device, person) = {
        let store = store.lock().expect("store mutex");
        (store.device_id()?, store.person_for(invite.fingerprint.as_bytes()))
    };
    let outcome = async {
        let (mut send, mut recv) = connection.open_bi().await?;
        let ask = Request::Visit {
            token: invite.token,
            device_id: *our_device.as_bytes(),
            name: our_name.to_string(),
            kind: our_kind.to_string(),
            person,
        };
        send.write_all(&ask.encode()).await?;
        send.finish()?;
        let waiting = std::time::Instant::now();
        let raw = recv
            .read_to_end(MAX_MESSAGE)
            .await
            .map_err(|e| while_waiting(e.into(), invite, now, waiting))?;
        let (host, meeting) = match Response::decode(&raw)? {
            Response::Welcome { device_id, name, kind, meeting } => (
                Paired {
                    device_id: DeviceId::from_bytes(device_id),
                    fingerprint: invite.fingerprint,
                    name: sanitise(&name),
                    kind: known_kind(&kind),
                },
                meeting,
            ),
            Response::Declined => return Err(Error::Declined),
            Response::NotFound => return Err(Error::PairingRefused),
            other => {
                return Err(Error::Protocol { detail: format!("expected a welcome, got {other:?}") })
            }
        };
        let store = store.lock().expect("store mutex");
        store.db().trust_visitor(
            &host.device_id,
            host.fingerprint.as_bytes(),
            &host.name,
            qurb_storage::db::Relation::Host,
            &meeting,
        )?;
        if let Some(kind) = &host.kind {
            store.learn_kind(&host.device_id, kind)?;
        }
        Ok(host)
    }
    .await;
    hang_up(&connection, &endpoint).await;
    outcome
}

/// What to say when the connection drops while the person at the other
/// device decides. That device stops waiting when its code expires and closes
/// without an answer, and "connection lost" said nothing of the reason
/// (2026-10-08). Allowed two seconds either way, for two clocks.
fn while_waiting(e: Error, invite: &Invite, now: i64, since: std::time::Instant) -> Error {
    if now + since.elapsed().as_secs() as i64 + 2 >= invite.expires_at {
        Error::NotApprovedInTime
    } else {
        e
    }
}

/// The pairing exchange itself, on a connection already pinned to the
/// inviting device.
async fn pair_on(
    connection: &quinn::Connection,
    invite: &Invite,
    store: Arc<Mutex<Store>>,
    our_name: &str,
    our_kind: &str,
    check: [u8; 32],
    now: i64,
) -> Result<Paired> {
    let our_device = { store.lock().expect("store mutex").device_id()? };
    let (mut send, mut recv) = connection.open_bi().await?;
    let request = Request::Pair {
        token: invite.token,
        device_id: *our_device.as_bytes(),
        name: our_name.to_string(),
        kind: our_kind.to_string(),
        key_check: check,
    };
    send.write_all(&request.encode()).await?;
    send.finish()?;

    let waiting = std::time::Instant::now();
    let raw = recv
        .read_to_end(MAX_MESSAGE)
        .await
        .map_err(|e| while_waiting(e.into(), invite, now, waiting))?;
    let host = match Response::decode(&raw)? {
        // Checked here as well as there: a device that answers "paired" to a
        // device holding another key is not one to trust.
        Response::Paired { key_check, .. } if key_check != check => return Err(Error::DifferentKey),
        Response::Paired { device_id, name, kind, .. } => Paired {
            device_id: DeviceId::from_bytes(device_id),
            fingerprint: invite.fingerprint,
            name: sanitise(&name),
            kind: known_kind(&kind),
        },
        Response::Mismatch => return Err(Error::DifferentKey),
        Response::Declined => return Err(Error::Declined),
        Response::NotFound => return Err(Error::PairingRefused),
        other => {
            return Err(Error::Protocol {
                detail: format!("expected a pairing reply, got {other:?}"),
            })
        }
    };

    let store = store.lock().expect("store mutex");
    store.db().trust_peer(&host.device_id, host.fingerprint.as_bytes(), &host.name)?;
    if let Some(kind) = &host.kind {
        store.learn_kind(&host.device_id, kind)?;
    }
    Ok(host)
}

/// The fingerprint of whatever certificate the peer presented.
///
/// Taken from the connection rather than from anything the peer said about
/// itself, which is the difference between an identity and a claim.
pub(crate) fn fingerprint_of(connection: &quinn::Connection) -> Option<Fingerprint> {
    let identity = connection.peer_identity()?;
    let certs = identity.downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>().ok()?;
    let first = certs.first()?;
    Some(Fingerprint::from_bytes(*blake3::hash(first.as_ref()).as_bytes()))
}

fn constant_time_eq(a: &[u8; TOKEN_LEN], b: &[u8; TOKEN_LEN]) -> bool {
    let mut diff = 0u8;
    for i in 0..TOKEN_LEN {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// A peer chooses its own name, so it is untrusted text that ends up in logs
/// and interfaces. Keep it short and printable.
fn sanitise(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    if cleaned.trim().is_empty() {
        "unnamed device".to_string()
    } else {
        cleaned.trim().to_string()
    }
}
