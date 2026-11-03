//! Public Nostr relays as the meeting point (decision 0082, amended
//! 2026-11-02): the rendezvous without a server of anybody's own.
//!
//! The owner's project is open source and earns him nothing, so nothing it
//! needs may cost him money; and devices must not depend on his laptop. What
//! the rendezvous service does for two devices -- "I want to reach you, here
//! is where I am"; "yes, here is where I am"; both punch -- needs no server of
//! its own, only somewhere both can post a few hundred bytes and hear the
//! other within a second. Public Nostr relays are that: free, run by
//! volunteers and companies, no account, hundreds of them.
//!
//! What a relay sees: an ephemeral event (kind [`KIND`], which relays pass on
//! and do not keep) signed by a key made for this session alone, tagged with
//! a topic that changes every day, carrying bytes it cannot read. The topic
//! and the key that seals the content are derived from the group, which only
//! the group's devices hold; so a relay learns neither who is talking nor what
//! is said, and cannot link one day's traffic to the next by its tag.
//!
//! Several relays at once, because any one may be down, slow, or refuse us:
//! everything is posted to all of them and heard from all of them, each
//! message told once. One relay answering is enough. A relay only matters for
//! the first second: once the devices meet they talk directly, and the relays
//! play no further part.
//!
//! Spoken to the rest of qurb as the rendezvous is -- [`FromClient`] in,
//! [`FromServer`] out -- so the connector neither knows nor cares which
//! carried a meeting. Waking a device that is not listening is not offered:
//! decision 0082 has no instant wake-up.

use crate::message::{Endpoints, FromClient, FromServer, Presence};
use crate::rendezvous::{GroupId, MemberId};
use crate::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

/// The public relays used when none are named: those that passed an
/// ephemeral event back in every round of the test from the owner's laptop,
/// begun 2026-11-02 (docs/decisions/0082). Six, so that any one or two being
/// down changes nothing.
pub const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.primal.net",
    "wss://relay.nostr.net",
    "wss://relay.damus.io",
    "wss://offchain.pub",
    "wss://nostr.oxtr.dev",
    "wss://nostr-pub.wellorder.net",
];

/// The address that selects this carrier: `nostr` alone for the relays above,
/// or `nostr:wss://a,wss://b` for chosen ones.
pub const SCHEME: &str = "nostr";

/// An ephemeral kind (20000–29999): relays forward it and do not store it.
pub const KIND: u32 = 25050;

/// A message older than this when it arrives is ignored: a replay, or one
/// that waited somewhere it should not have.
const FRESH_FOR: u64 = 90;

/// Each relay's own connection attempt is bounded by this.
const RELAY_OPEN: Duration = Duration::from_secs(6);

/// Messages told once: the last this many, by their inner nonce.
const REMEMBERED: usize = 512;

/// Whether `url` names this carrier.
pub fn is_nostr(url: &str) -> bool {
    let url = url.trim();
    url == SCHEME || url.starts_with("nostr:")
}

/// The relays `url` names, or the defaults.
pub fn relays_of(url: &str) -> Vec<String> {
    let named: Vec<String> = url
        .trim()
        .strip_prefix("nostr:")
        .unwrap_or("")
        .split(',')
        .map(|r| r.trim().to_string())
        .filter(|r| r.starts_with("wss://") || r.starts_with("ws://127.0.0.1") || r.starts_with("ws://localhost"))
        .collect();
    if named.is_empty() {
        DEFAULT_RELAYS.iter().map(|r| r.to_string()).collect()
    } else {
        named
    }
}

/// What is said, sealed inside each event's content.
#[derive(Serialize, Deserialize)]
struct Said {
    /// What kind of message: "announce", "connect", "accept", "waiting".
    k: String,
    from: MemberId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<MemberId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ep: Option<Endpoints>,
    /// Unix seconds when said.
    at: u64,
    /// Random, so each message is told once whichever relays carry it.
    n: [u8; 16],
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The group's sealing key and its topic for a day, derived from the group:
/// what only its devices hold.
struct Keys {
    seal: [u8; 32],
    group: [u8; 32],
}

impl Keys {
    fn of(group: &GroupId) -> Self {
        Self { seal: blake3::derive_key("qurb nostr seal v1", group.as_bytes()), group: *group.as_bytes() }
    }

    /// The tag a day's messages carry, so relays can route them without the
    /// group being visible, and today's cannot be linked to yesterday's.
    fn topic(&self, day: u64) -> String {
        let mut input = self.group.to_vec();
        input.extend_from_slice(&day.to_be_bytes());
        hex(&blake3::derive_key("qurb nostr topic v1", &input)[..16])
    }

    fn topics_now(&self) -> Vec<String> {
        let day = now() / 86_400;
        // Yesterday's too, for the minutes around midnight UTC.
        vec![self.topic(day), self.topic(day.saturating_sub(1))]
    }

    fn seal(&self, said: &Said) -> String {
        let plain = serde_json::to_vec(said).unwrap_or_default();
        let nonce: [u8; 12] = rand_bytes();
        let cipher = ChaCha20Poly1305::new(&self.seal.into());
        let mut out = nonce.to_vec();
        out.extend(cipher.encrypt(Nonce::from_slice(&nonce), plain.as_slice()).unwrap_or_default());
        hex(&out)
    }

    fn open(&self, content: &str) -> Option<Said> {
        let bytes = unhex(content)?;
        if bytes.len() < 12 + 16 {
            return None;
        }
        let cipher = ChaCha20Poly1305::new(&self.seal.into());
        let plain = cipher.decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..]).ok()?;
        serde_json::from_slice(&plain).ok()
    }
}

/// A key for this session alone, to sign what is posted: relays require a
/// signature, and one key per session keeps a device from being followed
/// across sessions by it.
struct Signer256 {
    key: k256::schnorr::SigningKey,
    public: String,
}

impl Signer256 {
    fn new() -> Self {
        loop {
            let secret: [u8; 32] = rand_bytes();
            if let Ok(key) = k256::schnorr::SigningKey::from_bytes(&secret) {
                let public = hex(&key.verifying_key().to_bytes());
                return Self { key, public };
            }
        }
    }

    /// A signed event, as NIP-01 has it.
    fn event(&self, topic: &str, content: String) -> serde_json::Value {
        let created = now();
        let tags = serde_json::json!([["t", topic]]);
        let serial = serde_json::json!([0, self.public, created, KIND, tags, content]).to_string();
        let id: [u8; 32] = Sha256::digest(serial.as_bytes()).into();
        // BIP-340 over the id itself, as NIP-01 has it: what relays verify.
        let aux: [u8; 32] = rand_bytes();
        let sig = self.key.sign_raw(&id, &aux).expect("signing 32 bytes");
        serde_json::json!({
            "id": hex(&id), "pubkey": self.public, "created_at": created,
            "kind": KIND, "tags": tags, "content": content, "sig": hex(&sig.to_bytes()),
        })
    }
}

/// Connect to the relays `url` names, as `member` of `group`, and speak to the
/// caller as the rendezvous service would. Returns once at least one relay
/// has taken the subscription; fails if none has within [`RELAY_OPEN`].
pub(crate) async fn open(
    url: &str,
    group: GroupId,
    member: MemberId,
    endpoints: Endpoints,
) -> Result<(mpsc::UnboundedSender<FromClient>, mpsc::UnboundedReceiver<FromServer>, Vec<tokio::task::JoinHandle<()>>)> {
    let relays = relays_of(url);
    let keys = std::sync::Arc::new(Keys::of(&group));
    let pool = Pool::get(relays, keys.topics_now());
    let mut heard = pool.heard.subscribe();
    let post = pool.post.clone();

    // At least one relay listening, or this carrier is no use now.
    let mut listening = pool.listening.subscribe();
    match tokio::time::timeout(RELAY_OPEN, listening.wait_for(|n| *n > 0)).await {
        Ok(Ok(_)) => {}
        _ => return Err(Error::NoAnswer(RELAY_OPEN)),
    }

    let (outgoing, mut to_send) = mpsc::unbounded_channel::<FromClient>();
    let (received, incoming) = mpsc::unbounded_channel::<FromServer>();
    let signer = Signer256::new();
    // As the service does on announce: who is here, which over relays is
    // nobody known yet -- the devices learn each other by asking.
    let _ = received.send(FromServer::Peers { members: Vec::new() });

    let core = tokio::spawn(async move {
        // The relays stay open while a session uses them.
        let _pool = pool;
        let mut here = endpoints;
        // Where a device that asked for us is: told back to us when we agree.
        let mut asked: HashMap<MemberId, Endpoints> = HashMap::new();
        let mut told: VecDeque<[u8; 16]> = VecDeque::new();
        let say = |k: &str, to: Option<MemberId>, ep: Option<Endpoints>| {
            let said = Said { k: k.into(), from: member, to, ep, at: now(), n: rand_bytes() };
            let topic = keys.topic(now() / 86_400);
            let event = signer.event(&topic, keys.seal(&said));
            let _ = post.send(serde_json::json!(["EVENT", event]).to_string());
        };
        say("announce", None, Some(here.clone()));
        loop {
            tokio::select! {
                message = to_send.recv() => match message {
                    Some(FromClient::Announce { endpoints, .. }) => {
                        here = endpoints;
                        say("announce", None, Some(here.clone()));
                    }
                    Some(FromClient::Connect { to }) => say("connect", Some(to), Some(here.clone())),
                    Some(FromClient::Accept { to, endpoints }) => {
                        say("accept", Some(to), Some(endpoints));
                        // The service tells both sides to punch; here the one
                        // agreeing tells itself, with where the asker said it was.
                        if let Some(theirs) = asked.remove(&to) {
                            let _ = received.send(FromServer::Punch { peer: to, endpoints: theirs });
                        }
                    }
                    Some(FromClient::Waiting { to }) => say("waiting", Some(to), None),
                    // No waking: decision 0082.
                    Some(FromClient::Reachable { .. }) => {}
                    None => return,
                },
                content = heard.recv() => {
                    let content = match content {
                        Ok(content) => content,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return,
                    };
                    let Some(said) = keys.open(&content) else { continue };
                    if said.from == member || now().saturating_sub(said.at) > FRESH_FOR || told.contains(&said.n) {
                        continue;
                    }
                    told.push_back(said.n);
                    if told.len() > REMEMBERED {
                        told.pop_front();
                    }
                    let for_me = said.to.map_or(true, |to| to == member);
                    if !for_me {
                        continue;
                    }
                    let event = match said.k.as_str() {
                        "announce" => said.ep.map(|endpoints| FromServer::Appeared { peer: Presence { member: said.from, endpoints } }),
                        "connect" => said.ep.map(|endpoints| {
                            asked.insert(said.from, endpoints.clone());
                            FromServer::ConnectRequest { from: said.from, endpoints }
                        }),
                        "accept" => said.ep.map(|endpoints| FromServer::Punch { peer: said.from, endpoints }),
                        "waiting" => Some(FromServer::Waiting { from: said.from }),
                        _ => None,
                    };
                    if let Some(event) = event {
                        if received.send(event).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });
    Ok((outgoing, incoming, vec![core]))
}

/// One set of relay connections for every session of this process: on the
/// S23 on 2026-11-02 each connection the phone made held six of its own, 85
/// opened in a few minutes. Kept while any session holds it; its relays
/// closed when the last lets go. One per set of relays and day's topics.
struct Pool {
    post: broadcast::Sender<String>,
    heard: broadcast::Sender<String>,
    /// How many of its relays are listening now.
    listening: tokio::sync::watch::Sender<usize>,
    relays: Vec<tokio::task::JoinHandle<()>>,
}

impl Pool {
    fn get(relays: Vec<String>, topics: Vec<String>) -> std::sync::Arc<Pool> {
        static POOLS: std::sync::Mutex<Vec<(Vec<String>, Vec<String>, std::sync::Weak<Pool>)>> =
            std::sync::Mutex::new(Vec::new());
        let mut pools = POOLS.lock().unwrap_or_else(|e| e.into_inner());
        pools.retain(|(_, _, pool)| pool.strong_count() > 0);
        let found = pools
            .iter()
            .filter(|(r, t, _)| *r == relays && *t == topics)
            .filter_map(|(_, _, pool)| pool.upgrade())
            // Its relays ended with the runtime that ran them: not to be shared.
            .find(|pool| pool.relays.iter().any(|relay| !relay.is_finished()));
        if let Some(pool) = found {
            return pool;
        }
        let (post, _) = broadcast::channel(64);
        let (heard, _) = broadcast::channel(256);
        let listening = tokio::sync::watch::Sender::new(0);
        let tasks = relays
            .iter()
            .map(|relay| tokio::spawn(stay_on(relay.clone(), topics.clone(), post.subscribe(), heard.clone(), listening.clone())))
            .collect();
        let pool = std::sync::Arc::new(Pool { post, heard, listening, relays: tasks });
        pools.push((relays, topics, std::sync::Arc::downgrade(&pool)));
        pool
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        for relay in &self.relays {
            relay.abort();
        }
    }
}

/// One relay, kept: connected, subscribed to the group's topics, posting what
/// is posted, passing on what it hears; and again after it goes, backing off.
/// How often a relay is pinged, and how long it may say nothing before its
/// connection is taken for dead and dialled again. A laptop woken from sleep
/// keeps sockets that look open and carry nothing: on 2026-11-02 the owner's
/// sat deaf on all six relays after waking, and the phone could not reach it.
const BEAT: Duration = Duration::from_secs(20);
const SILENT: Duration = Duration::from_secs(45);

async fn stay_on(
    relay: String,
    topics: Vec<String>,
    mut post: broadcast::Receiver<String>,
    heard: broadcast::Sender<String>,
    listening: tokio::sync::watch::Sender<usize>,
) {
    let mut wait = Duration::from_secs(2);
    loop {
        let opened = tokio::time::timeout(RELAY_OPEN, tokio_tungstenite::connect_async(relay.as_str())).await;
        let Ok(Ok((socket, _))) = opened else {
            tracing::debug!(%relay, "a nostr relay did not open");
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(Duration::from_secs(300));
            continue;
        };
        let (mut sink, mut source) = socket.split();
        let since = now().saturating_sub(30);
        let request = serde_json::json!(["REQ", "qurb", {"kinds": [KIND], "#t": topics, "since": since}]).to_string();
        if sink.send(Message::Text(request.into())).await.is_err() {
            continue;
        }
        listening.send_modify(|n| *n += 1);
        wait = Duration::from_secs(2);
        tracing::debug!(%relay, "listening on a nostr relay");
        let mut beat = tokio::time::interval(BEAT);
        beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut heard_at = tokio::time::Instant::now();
        loop {
            tokio::select! {
                _ = beat.tick() => {
                    if heard_at.elapsed() > SILENT {
                        tracing::debug!(%relay, "a nostr relay fell silent");
                        break;
                    }
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
                out = post.recv() => match out {
                    Ok(text) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        listening.send_modify(|n| *n = n.saturating_sub(1));
                        return;
                    }
                },
                incoming = source.next() => {
                    let Some(Ok(message)) = incoming else { break };
                    heard_at = tokio::time::Instant::now();
                    match message {
                        Message::Text(text) => {
                            // ["EVENT", sub, event]: the content is all we read.
                            if let Ok(serde_json::Value::Array(parts)) = serde_json::from_str::<serde_json::Value>(&text) {
                                if parts.first().and_then(|p| p.as_str()) == Some("EVENT") {
                                    if let Some(content) = parts.get(2).and_then(|e| e.get("content")).and_then(|c| c.as_str()) {
                                        // None listening is no reason to stop.
                                        let _ = heard.send(content.to_string());
                                    }
                                }
                            }
                        }
                        Message::Ping(data) => {
                            let _ = sink.send(Message::Pong(data)).await;
                        }
                        _ => {}
                    }
                }
            }
        }
        listening.send_modify(|n| *n = n.saturating_sub(1));
        tracing::debug!(%relay, "a nostr relay went; trying again");
        tokio::time::sleep(wait).await;
    }
}

fn rand_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).expect("the system's random source");
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_message_opens_only_for_its_group() {
        let a = Keys::of(&GroupId::from_bytes([1; 32]));
        let b = Keys::of(&GroupId::from_bytes([2; 32]));
        let said = Said { k: "connect".into(), from: MemberId::from_bytes([3; 32]), to: Some(MemberId::from_bytes([4; 32])),
            ep: None, at: now(), n: [5; 16] };
        let sealed = a.seal(&said);
        assert_eq!(a.open(&sealed).map(|s| s.from), Some(said.from));
        assert!(b.open(&sealed).is_none(), "another group opened it");
        assert_ne!(a.topic(1), a.topic(2), "the topic does not change by day");
        assert_ne!(a.topic(1), b.topic(1));
    }

    #[test]
    fn an_event_is_signed_as_nip01_says() {
        let signer = Signer256::new();
        let event = signer.event("abc", "hello".into());
        let serial = serde_json::json!([0, event["pubkey"], event["created_at"], KIND, event["tags"], "hello"]).to_string();
        let id: [u8; 32] = Sha256::digest(serial.as_bytes()).into();
        assert_eq!(event["id"].as_str().unwrap(), hex(&id));
        let key = k256::schnorr::VerifyingKey::from_bytes(&unhex(event["pubkey"].as_str().unwrap()).unwrap()).unwrap();
        let sig = k256::schnorr::Signature::try_from(unhex(event["sig"].as_str().unwrap()).unwrap().as_slice()).unwrap();
        key.verify_raw(&id, &sig).expect("the signature does not verify");
    }

    #[test]
    fn the_address_names_the_relays() {
        assert!(is_nostr("nostr") && is_nostr("nostr:wss://a.example"));
        assert!(!is_nostr("wss://saqib.example"));
        assert_eq!(relays_of("nostr").len(), DEFAULT_RELAYS.len());
        assert_eq!(relays_of("nostr:wss://a.example, wss://b.example"), vec!["wss://a.example", "wss://b.example"]);
    }
}
