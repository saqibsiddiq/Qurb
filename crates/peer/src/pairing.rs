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
}

impl Invite {
    pub fn new(fingerprint: Fingerprint, address: SocketAddr, now: i64) -> Self {
        use rand::RngCore;
        let mut token = [0u8; TOKEN_LEN];
        rand::rngs::OsRng.fill_bytes(&mut token);
        Self { fingerprint, address, token, expires_at: now + INVITE_LIFETIME_SECS }
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

        format!("{PREFIX}{}", base32::encode(&payload))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let trimmed = text.trim();
        let body = trimmed
            .strip_prefix(PREFIX)
            .or_else(|| trimmed.strip_prefix(&PREFIX.to_uppercase()))
            .ok_or_else(|| Error::BadInvite { detail: "not a pairing code".into() })?;

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
        })
    }

    /// Grouped into fives, for someone reading it out.
    pub fn for_humans(&self) -> String {
        let encoded = self.encode();
        let body = encoded.strip_prefix(PREFIX).unwrap_or(&encoded);
        let grouped: Vec<String> =
            body.as_bytes().chunks(5).map(|c| String::from_utf8_lossy(c).to_string()).collect();
        format!("{PREFIX}{}", grouped.join("-"))
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

    pub fn invite(&self) -> &Invite {
        &self.invite
    }

    /// Wait for one device to pair, then stop listening.
    ///
    /// Returns once a device presents the right token. A device presenting the
    /// wrong one is refused and the host keeps waiting, so a wrong guess does
    /// not burn the invite — but it also does not get a second chance at the
    /// same connection.
    pub async fn wait(
        &self,
        store: Arc<Mutex<Store>>,
        our_name: &str,
        now: i64,
    ) -> Result<Paired> {
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

        tokio::time::timeout(deadline, self.accept_one(store, our_name, our_device))
            .await
            .unwrap_or(Err(Error::InviteExpired))
    }

    /// The accept loop, run under the caller's deadline.
    async fn accept_one(
        &self,
        store: Arc<Mutex<Store>>,
        our_name: &str,
        our_device: DeviceId,
    ) -> Result<Paired> {
        while let Some(incoming) = self.endpoint.accept().await {
            let Ok(connection) = incoming.await else { continue };

            let Some(peer_fingerprint) = fingerprint_of(&connection) else {
                tracing::debug!("a device connected without a certificate");
                continue;
            };

            let Ok((mut send, mut recv)) = connection.accept_bi().await else { continue };
            let Ok(raw) = recv.read_to_end(MAX_MESSAGE).await else { continue };

            let Ok(Request::Pair { token, device_id, name }) = Request::decode(&raw) else {
                let _ = send.write_all(&Response::NotFound.encode()).await;
                let _ = send.finish();
                continue;
            };

            // No expiry check here: it used to compare against a `now` captured
            // before the wait began, which meant it could never fire however
            // long the wait lasted. The deadline in `wait` is the real one.
            //
            // Constant-time, because a token compared byte by byte can be
            // guessed one byte at a time by anyone who can measure the reply.
            if !constant_time_eq(&token, &self.invite.token) {
                tracing::warn!(peer = %peer_fingerprint.short(), "wrong pairing token");
                let _ = send.write_all(&Response::NotFound.encode()).await;
                let _ = send.finish();
                continue;
            }

            let peer = Paired {
                device_id: DeviceId::from_bytes(device_id),
                fingerprint: peer_fingerprint,
                name: sanitise(&name),
            };

            {
                let store = store.lock().expect("store mutex");
                store.db().trust_peer(
                    &peer.device_id,
                    peer.fingerprint.as_bytes(),
                    &peer.name,
                )?;
            }

            let reply = Response::Paired {
                device_id: *our_device.as_bytes(),
                name: our_name.to_string(),
            };
            send.write_all(&reply.encode()).await?;
            send.finish()?;
            // Give the reply time to leave before the endpoint closes under it.
            connection.closed().await;

            return Ok(peer);
        }

        Err(Error::PairingAbandoned)
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"paired");
    }
}

/// Accept an invite: connect, prove we saw the code, exchange identities.
pub async fn accept(
    invite: &Invite,
    identity: &Identity,
    store: Arc<Mutex<Store>>,
    our_name: &str,
    now: i64,
) -> Result<Paired> {
    if invite.is_expired(now) {
        return Err(Error::InviteExpired);
    }

    let our_device = { store.lock().expect("store mutex").device_id()? };

    let bind: SocketAddr =
        if invite.address.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().expect("literal");
    let mut endpoint = quinn::Endpoint::client(bind)
        .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
    // The inviter's fingerprint came from the code, so this connection is
    // pinned exactly as an ordinary one would be. A machine in the path cannot
    // impersonate the device whose screen the user is looking at.
    endpoint.set_default_client_config(tls::client_config(identity, invite.fingerprint)?);

    let connection = endpoint.connect(invite.address, "qurb-device")?.await?;
    let (mut send, mut recv) = connection.open_bi().await?;

    let request = Request::Pair {
        token: invite.token,
        device_id: *our_device.as_bytes(),
        name: our_name.to_string(),
    };
    send.write_all(&request.encode()).await?;
    send.finish()?;

    let raw = recv.read_to_end(MAX_MESSAGE).await?;
    let host = match Response::decode(&raw)? {
        Response::Paired { device_id, name } => Paired {
            device_id: DeviceId::from_bytes(device_id),
            fingerprint: invite.fingerprint,
            name: sanitise(&name),
        },
        Response::NotFound => return Err(Error::PairingRefused),
        other => {
            return Err(Error::Protocol {
                detail: format!("expected a pairing reply, got {other:?}"),
            })
        }
    };

    {
        let store = store.lock().expect("store mutex");
        store.db().trust_peer(&host.device_id, host.fingerprint.as_bytes(), &host.name)?;
    }

    connection.close(0u32.into(), b"paired");
    endpoint.close(0u32.into(), b"paired");
    Ok(host)
}

/// The fingerprint of whatever certificate the peer presented.
///
/// Taken from the connection rather than from anything the peer said about
/// itself, which is the difference between an identity and a claim.
fn fingerprint_of(connection: &quinn::Connection) -> Option<Fingerprint> {
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
