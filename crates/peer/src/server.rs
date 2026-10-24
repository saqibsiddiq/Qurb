//! Serving a store to peers.
//!
//! Read-only by design. A peer can ask what this device has and can ask for its
//! bytes; it cannot tell this device to change anything. Incoming versions are
//! adopted by the *local* engine after it has run them through reconciliation,
//! so nothing a peer says is applied without this side deciding it should be.

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
use crate::pairing::fingerprint_of;
use crate::tls;
use crate::wire::{Request, Response, MAX_MESSAGE};
use qurb_storage::Store;
use qurb_sync::FileVersion;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How much of this device's vault is read to seal it for a keeper of another
/// person in answering one request for the tree (decision 0060). The rest is
/// sealed when it next asks, so a large vault is shown over a few syncs rather
/// than holding up the first.
const SEALING_BUDGET: u64 = 256 << 20;

/// Told about each chunk served, and to whom.
///
/// The server's half of a transfer: what it has handed a peer is the only
/// sign on this side that a file is on its way, since a peer asks for content
/// by hash and the sender never knows which file it is part of until the
/// caller looks. Called after the bytes are read and before they are written
/// out, so it counts what was served rather than what was received -- the
/// receiver's `Got` is the evidence of that.
pub trait Served: Send + Sync {
    fn served(&self, to: &Fingerprint, chunk: &blake3::Hash, bytes: u64);

    /// A trusted device asked this one something, on a connection it opened:
    /// it is on, and reached this device itself. Told once per connection,
    /// after the trust check its first request passed.
    ///
    /// What lets a phone whose own pass could not reach a computer still know
    /// the computer is there, when the computer reached the phone -- through a
    /// relay, say. Without it the phone called a computer off that had just
    /// collected from it (2026-10-24, on an emulator).
    fn asked_by(&self, _from: &Fingerprint) {}
}

pub struct PeerServer {
    endpoint: quinn::Endpoint,
    stats: Arc<ServerStats>,
    /// The same live list the handshake checks, asked again per request.
    trust: crate::tls::TrustList,
}

/// How far this device's state has got.
///
/// A counter rather than a flag, because a flag can be missed: a peer told
/// "something changed" has no way to tell a notification it already acted on
/// from a new one. With a counter it says what it last saw, and the answer is
/// immediate if anything has happened since.
///
/// It need not survive a restart. A peer holding a number from before will see
/// one that does not match, which is exactly the right conclusion — the device
/// it was watching has been away, and its state may well have moved.
#[derive(Debug, Default)]
pub struct Generation {
    value: std::sync::atomic::AtomicU64,
    changed: tokio::sync::Notify,
}

impl Generation {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn current(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Say that something changed, and wake everyone waiting.
    pub fn bump(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
        self.changed.notify_waiters();
    }

    /// Wait until the value differs from `since`, or until `timeout`.
    ///
    /// Any difference counts, not only an increase, so a restarted peer is told
    /// to look rather than waiting for a counter that began again to overtake
    /// one it remembers.
    async fn wait_past(&self, since: u64, timeout: Duration) -> u64 {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = self.current();
            if now != since {
                return now;
            }
            // Registered before the check below, so a change between the two
            // wakes this rather than being missed.
            let notified = self.changed.notified();
            if self.current() != since {
                return self.current();
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.current();
            }
        }
    }
}

/// How long to hold a request open before answering anyway.
///
/// Long enough that an idle pair exchanges almost nothing; short enough that a
/// connection which has quietly died is noticed rather than waited on for ever.
const CHANGES_TIMEOUT: Duration = Duration::from_secs(90);

/// What this server has done since it started.
///
/// Chunks served is the number that matters: it is the wire cost of a sync, and
/// the measure of whether content-defined chunking is actually saving anything.
#[derive(Debug, Default)]
pub struct ServerStats {
    pub trees_served: AtomicU64,
    pub manifests_served: AtomicU64,
    pub chunks_served: AtomicU64,
    pub bytes_served: AtomicU64,
}

impl ServerStats {
    pub fn chunks(&self) -> u64 {
        self.chunks_served.load(Ordering::Relaxed)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes_served.load(Ordering::Relaxed)
    }
}

impl PeerServer {
    /// Listen on `addr`, accepting only the peers in `allowed`.
    pub fn bind(addr: SocketAddr, identity: &Identity, allowed: &crate::tls::TrustList) -> Result<Self> {
        let config = tls::server_config(identity, allowed)?;
        let endpoint = quinn::Endpoint::server(config, addr)
            .map_err(|e| Error::Io { path: addr.to_string().into(), source: e })?;
        Ok(Self { endpoint, stats: Arc::new(ServerStats::default()), trust: allowed.clone() })
    }

    /// Counters, shareable with whoever wants to watch.
    pub fn stats(&self) -> Arc<ServerStats> {
        Arc::clone(&self.stats)
    }

    /// Listen on `addr`, accepting the devices this store has paired with.
    ///
    /// The list is read once, at bind. A device paired afterwards will not be
    /// accepted until the listener is rebuilt — acceptable while pairing is a
    /// deliberate act a person performs, and something to revisit when devices
    /// come and go on their own.
    pub fn bind_trusting(addr: SocketAddr, identity: &Identity, store: &Store) -> Result<Self> {
        let allowed = trusted_fingerprints(store)?;
        if allowed.is_empty() {
            tracing::warn!("no paired devices; this listener will refuse everyone");
        }
        Self::bind(addr, identity, &crate::tls::TrustList::new(allowed))
    }

    /// The address actually bound, which matters when port 0 was requested.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.endpoint
            .local_addr()
            .map_err(|e| Error::Io { path: "local_addr".into(), source: e })
    }

    /// Serve until the endpoint is closed.
    ///
    /// # Runtime requirement
    ///
    /// Storage reads are synchronous and can take milliseconds, so they run
    /// inside [`tokio::task::block_in_place`], which needs the multi-threaded
    /// scheduler. The same constraint the engine has.
    /// Serve, without telling anyone when this device changes.
    ///
    /// Peers fall back to asking periodically, which works and is slower.
    pub async fn serve(&self, store: Arc<Mutex<Store>>) {
        self.serve_with(store, Generation::new()).await
    }

    /// Serve, and answer "tell me when you change" from `generation`.
    pub async fn serve_with(&self, store: Arc<Mutex<Store>>, generation: Arc<Generation>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let store = Arc::clone(&store);
            let stats = Arc::clone(&self.stats);
            let generation = Arc::clone(&generation);
            let trust = self.trust.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        tracing::debug!(peer = %connection.remote_address(), "peer connected");
                        serve_connection_inner(
                            connection,
                            store,
                            stats,
                            generation,
                            None,
                            Some(trust),
                        )
                        .await;
                    }
                    // A failed handshake is the normal outcome for an
                    // unrecognised peer, and is not worth more than a debug line.
                    Err(e) => tracing::debug!(error = %e, "handshake failed"),
                }
            });
        }
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutting down");
    }
}

/// The fingerprints of every device this store trusts.
pub fn trusted_fingerprints(store: &Store) -> Result<Vec<Fingerprint>> {
    Ok(store
        .db()
        .trusted_peers()?
        .into_iter()
        .map(|peer| Fingerprint::from_bytes(peer.fingerprint))
        .collect())
}

/// Serve one connection, for tests that build their own endpoint.
///
/// Exists because hole punching requires the endpoint to be constructed from an
/// existing socket, which [`PeerServer::bind`] cannot do.
pub async fn serve_connection_for_test(connection: quinn::Connection, store: Arc<Mutex<Store>>) {
    serve_connection_inner(
        connection,
        store,
        Arc::new(ServerStats::default()),
        Generation::new(),
        None,
        None,
    )
    .await
}

/// Serve one connection, telling peers about changes to `generation`, for as
/// long as the device at the other end is in `trust`.
pub async fn serve_connection(
    connection: quinn::Connection,
    store: Arc<Mutex<Store>>,
    generation: Arc<Generation>,
    trust: &crate::tls::TrustList,
) {
    serve_connection_inner(
        connection,
        store,
        Arc::new(ServerStats::default()),
        generation,
        None,
        Some(trust.clone()),
    )
    .await
}

/// The same, telling `watcher` about every chunk served.
pub async fn serve_connection_watched(
    connection: quinn::Connection,
    store: Arc<Mutex<Store>>,
    generation: Arc<Generation>,
    watcher: Arc<dyn Served>,
    trust: &crate::tls::TrustList,
) {
    serve_connection_inner(
        connection,
        store,
        Arc::new(ServerStats::default()),
        generation,
        Some(watcher),
        Some(trust.clone()),
    )
    .await
}

async fn serve_connection_inner(
    connection: quinn::Connection,
    store: Arc<Mutex<Store>>,
    stats: Arc<ServerStats>,
    generation: Arc<Generation>,
    watcher: Option<Arc<dyn Served>>,
    trust: Option<crate::tls::TrustList>,
) {
    // One request per bidirectional stream, served concurrently. This is the
    // property QUIC was chosen for: a large chunk in flight does not hold up
    // the small requests behind it.
    // Taken once, from the certificate this connection authenticated with. A
    // request that says something about the sender -- `Got` does -- must be
    // attributed to whoever the TLS handshake proved them to be, never to
    // whatever the message claims.
    let asker = fingerprint_of(&connection);
    let mut told = false;

    while let Ok((send, recv)) = connection.accept_bi().await {
        // Trust is asked again for every request, not only at the handshake.
        // A device removed while it was connected must stop being answered
        // now, not whenever its connection happens to end -- which for a
        // device that keeps one open could be never. A long wait for changes
        // already in progress still finishes, within its ninety seconds; what
        // follows it is refused.
        if let (Some(trust), Some(asker)) = (&trust, asker) {
            if !trust.contains(&asker) {
                connection.close(0u32.into(), b"no longer trusted");
                break;
            }
            if !told {
                told = true;
                if let Some(watcher) = &watcher {
                    watcher.asked_by(&asker);
                }
            }
        }
        let store = Arc::clone(&store);
        let stats = Arc::clone(&stats);
        let generation = Arc::clone(&generation);
        let watcher = watcher.clone();
        tokio::spawn(async move {
            let outcome = serve_request(send, recv, store, stats, generation, asker, watcher).await;
            if let Err(e) = outcome {
                tracing::debug!(error = %e, "request failed");
            }
        });
    }
}

async fn serve_request(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    store: Arc<Mutex<Store>>,
    stats: Arc<ServerStats>,
    generation: Arc<Generation>,
    asker: Option<Fingerprint>,
    watcher: Option<Arc<dyn Served>>,
) -> Result<()> {
    let raw = recv.read_to_end(MAX_MESSAGE).await?;
    let request = Request::decode(&raw)?;

    // Handled here rather than in `answer`, because it is the one request that
    // waits: holding the store's lock while doing so would stop this device
    // getting on with anything.
    let response = if let Request::Changes { since } = request {
        Response::Changed { generation: generation.wait_past(since, CHANGES_TIMEOUT).await }
    } else {
        let answered = tokio::task::block_in_place(|| {
            let mut store = store.lock().expect("store mutex poisoned");
            answer(&mut store, &request, asker)
        })?;
        // A change made to the shared folder for a phone: devices waiting
        // to hear of one hear now (decision 0073).
        let changed = matches!(
            request,
            Request::SharedMkdir { .. } | Request::SharedMove { .. } | Request::SharedDelete { .. } | Request::UploadDone { .. }
        ) && matches!(answered, Answer::Now(Response::Noted | Response::Placed { .. }));
        if changed {
            generation.bump();
        }
        match answered {
            Answer::Now(response) => response,
            // A chunk's bytes, read after the lock is let go, so that the
            // requests a collecting device makes at once are read at once
            // (decision 0064). The index is asked again only to mark a send
            // whose file was found changed.
            Answer::Read { hash, places, reader } => tokio::task::block_in_place(|| -> Result<Response> {
                let read = reader.read(&hash, &places);
                if !read.changed_sends.is_empty() {
                    store.lock().expect("store mutex poisoned").sends_changed(&read.changed_sends)?;
                }
                // Verified against its own hash in the reading, so a corrupt
                // chunk is reported as missing rather than served.
                match read.bytes {
                    Ok(bytes) => Ok(Response::Chunk(bytes)),
                    Err(qurb_storage::Error::ChunkMissing { .. }) => Ok(Response::NotFound),
                    Err(e) => Err(e.into()),
                }
            })?,
        }
    };

    match &response {
        Response::Tree(_) => stats.trees_served.fetch_add(1, Ordering::Relaxed),
        Response::Manifest(_) => stats.manifests_served.fetch_add(1, Ordering::Relaxed),
        Response::Chunk(bytes) => {
            if let (Some(watcher), Some(to), Request::Chunk { hash }) = (&watcher, &asker, &request) {
                watcher.served(to, &blake3::Hash::from(*hash), bytes.len() as u64);
            }
            stats.bytes_served.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            stats.chunks_served.fetch_add(1, Ordering::Relaxed)
        }
        Response::NotFound
        | Response::Noted
        | Response::Paired { .. }
        | Response::Welcome { .. }
        | Response::Asks { .. }
        | Response::Key { .. }
        | Response::About { .. }
        | Response::Who { .. }
        | Response::Mismatch
        | Response::Declined
        | Response::Changed { .. }
        | Response::Folders { .. }
        | Response::Chosen(_)
        | Response::Short { .. }
        | Response::Sealed(_)
        | Response::Withdrawn
        | Response::Kept { .. }
        | Response::Layout(_)
        | Response::Digest(_)
        | Response::TreeDelta { .. }
        | Response::Refused { .. }
        | Response::Placed { .. } => 0,
    };

    match (&request, &response) {
        // In frames, so that a list larger than one message still goes
        // (decision 0065).
        (Request::TreeStream, Response::Tree(tree)) => {
            for frame in crate::wire::tree_frames(tree) {
                send.write_all(&frame).await?;
            }
        }
        _ => send.write_all(&response.encode()).await?,
    }
    send.finish()?;
    Ok(())
}

/// The most entries a list's changes are sent as, apart from the list: past
/// this, or past half of it, the list is sent whole instead.
const DELTA_MOST: usize = 20_000;

/// A change to the shared folder, made, or refused with the reason the
/// asker shows its person.
fn made(result: qurb_storage::Result<()>) -> Result<Response> {
    match result {
        Ok(()) => Ok(Response::Noted),
        Err(e) => refused(e),
    }
}

/// A storage error as a refusal a person can read, where it is one; any
/// other is this device's failure, not the asker's, and is passed on.
fn refused(e: qurb_storage::Error) -> Result<Response> {
    match e {
        qurb_storage::Error::Sharing { why } => Ok(Response::Refused { why }),
        qurb_storage::Error::NotFound { path } => Ok(Response::Refused { why: format!("nothing is called {path}") }),
        qurb_storage::Error::NotHere { path } => Ok(Response::Refused { why: format!("{path} is not on this computer yet") }),
        other => Err(other.into()),
    }
}

/// Where uploads wait, in the store's own folder and so never in the shared
/// one, until the last piece arrives.
const INCOMING: &str = "incoming";

/// How long an upload nobody finished is kept before the next one clears it.
const INCOMING_KEPT: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// Where the pieces of upload `id` from `device` wait.
fn staging(store: &Store, device: &qurb_sync::DeviceId, id: &[u8; 16]) -> std::path::PathBuf {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    store.root().join(INCOMING).join(format!("{}-{}", hex(&device.as_bytes()[..8]), hex(id)))
}

/// One piece of an upload, written where it waits. In order only: a piece
/// not at the end of what has arrived is refused, and the asker starts again.
fn piece(store: &Store, device: &qurb_sync::DeviceId, id: &[u8; 16], offset: u64, bytes: &[u8]) -> Result<Response> {
    use std::io::Write;
    let staged = staging(store, device, id);
    let dir = store.root().join(INCOMING);
    std::fs::create_dir_all(&dir).map_err(|e| Error::Io { path: dir.clone(), source: e })?;
    if offset == 0 {
        // A new upload clears out ones abandoned a day ago and more.
        if let Ok(read) = std::fs::read_dir(&dir) {
            for entry in read.flatten() {
                let old = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > INCOMING_KEPT);
                if old {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
    let have = std::fs::metadata(&staged).map(|m| m.len()).unwrap_or(0);
    if offset != 0 && offset != have {
        return Ok(Response::Refused { why: "a piece of the file went missing; send it again".into() });
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset != 0)
        .truncate(offset == 0)
        .open(&staged)
        .map_err(|e| Error::Io { path: staged.clone(), source: e })?;
    file.write_all(bytes).map_err(|e| Error::Io { path: staged.clone(), source: e })?;
    Ok(Response::Noted)
}

/// What a request is answered with: at once, or, for a chunk, the bytes once
/// they are read without the store (decision 0064).
enum Answer {
    Now(Response),
    Read { hash: blake3::Hash, places: Vec<qurb_storage::ChunkAt>, reader: qurb_storage::ChunkReader },
}

fn answer(store: &mut Store, request: &Request, asker: Option<Fingerprint>) -> Result<Answer> {
    // Which device is asking, as the connection proves rather than as anything
    // claims. Every answer below is scoped to it: the shared area plus that
    // device's own vault, never anybody else's.
    //
    // `None` -- an unrecognised certificate -- is treated as a device entitled
    // to nothing rather than as a device entitled to everything. That is the
    // safe direction, and the only one: a peer that cannot be identified
    // cannot be shown a vault.
    // A peer that authenticated but whose device this store does not recognise
    // is `Unplaced`: it owns no vault here, so it is shown none, and it is not
    // refused the shared area either. The connection already proved it is
    // trusted, and being stricter would break a paired device whose
    // bookkeeping is incomplete while buying nothing — vaults stay invisible
    // to it either way.
    let known = asker.and_then(|fingerprint| {
        store.db().peer_by_fingerprint(fingerprint.as_bytes()).ok().flatten()
    });
    let owner = known.as_ref().map(|peer| peer.device_id);
    // Another person's device -- a guest of this computer, or a computer this
    // device visits -- is shown only what was sent to it, decided here on
    // every request from what the store says it is (decision 0060).
    let other_person = known.as_ref().is_some_and(|peer| peer.relation.is_other_person());
    let audience = match &owner {
        Some(device) if other_person => qurb_storage::db::Audience::Guest(device),
        Some(device) => qurb_storage::db::Audience::Device(device),
        None => qurb_storage::db::Audience::Unplaced,
    };

    // Who may reach this computer's shared folder live (decision 0073, item
    // 6): its guests -- not a computer this device visits, whose person is as
    // much a stranger to the folder -- and its own person's phones. Its own
    // computers sync the shared area instead. A guest is given the files'
    // bytes, which it otherwise is not; it is still never sent the list.
    let guest_here = known.as_ref().is_some_and(|peer| peer.relation == qurb_storage::db::Relation::Guest);
    let reaches_shared = match &owner {
        Some(_) if guest_here => true,
        Some(device) if !other_person => store.db().peer_kind(device)?.as_deref() == Some("phone"),
        _ => false,
    };

    // Another person's computer this device keeps its own vault on: shown it
    // sealed, so that it keeps the files without being able to read them
    // (decision 0060).
    let keeps_mine = match (&owner, other_person) {
        (Some(device), true) => store.db().is_holder(device)?,
        _ => false,
    };

    Ok(Answer::Now(match request {
        // The same list either way; a stream is only how it is sent. And its
        // fingerprint, for a device that may hold it already (`trees`).
        Request::Tree | Request::TreeStream | Request::TreeDigest => {
            let mut tree = store.tree_for(audience)?;
            if let (true, Some(holder)) = (keeps_mine, &owner) {
                store.prepare_sealed(holder, SEALING_BUDGET)?;
                tree.extend(store.sealed_tree_for(holder)?);
            }
            match request {
                Request::TreeDigest => Response::Digest(crate::wire::tree_digest(&tree)),
                _ => Response::Tree(tree),
            }
        }

        // Only what changed since the list the asker holds (decision 0072),
        // with the whole list's fingerprint for it to check what it rebuilds
        // against. Not for a computer keeping this device's vault sealed: a
        // part of its list moves no counter here, so it is sent whole. Nor
        // when so much changed that the list is as cheap.
        Request::TreeSince { since } => {
            if keeps_mine {
                Response::NotFound
            } else {
                let (now, tree, changed) = store.tree_since(audience, *since)?;
                let digest = crate::wire::tree_digest(&tree);
                let total = tree.len();
                let delta: Vec<FileVersion> = tree
                    .into_iter()
                    .filter(|version| {
                        let (class, path) = crate::wire::listed_order(version);
                        changed.contains(&(class, path.to_string()))
                    })
                    .collect();
                if delta.len() > DELTA_MOST || (delta.len() > 64 && delta.len() * 2 > total) {
                    Response::NotFound
                } else {
                    Response::TreeDelta { now, digest, changed: delta }
                }
            }
        }

        Request::Manifest { content } => {
            let content = blake3::Hash::from(*content);
            if let (true, Some(holder)) = (keeps_mine, &owner) {
                if let Some(ids) = store.sealed_manifest(holder, &content)? {
                    return Ok(Answer::Now(Response::Manifest(ids)));
                }
            }
            let reached = guest_here && owner.as_ref().map_or(Ok(false), |d| store.db().shared_content_reachable(&content, d))?;
            if !reached && !store.content_visible_to(&content, audience)? {
                // Indistinguishable from content this device does not hold,
                // which is deliberate: "you may not have this" and "there is
                // no such thing" should look the same from outside, or the
                // refusal itself tells a peer what exists.
                return Ok(Answer::Now(Response::NotFound));
            }
            // Held, not merely known: a file freed here keeps its chunk list,
            // and answering with it sends the peer off to fail at the first
            // chunk instead of being told plainly.
            match store.held_chunks(&content)? {
                Some(hashes) => {
                    Response::Manifest(hashes.iter().map(|h| *h.as_bytes()).collect())
                }
                None => Response::NotFound,
            }
        }

        // Pairing is served by its own listener, which accepts unknown
        // certificates. This one only ever talks to devices already trusted, so
        // a pairing request here is either a mistake or a probe.
        // And the key, above all, is never handed out here.
        Request::Pair { .. }
        | Request::Join { .. }
        | Request::Visit { .. }
        | Request::Who { .. }
        | Request::Short { .. }
        | Request::ShortConfirm { .. } => Response::NotFound,

        // A guest asking whether this computer wants to open its folder here,
        // and answering with the folder's key once its person approved
        // (decision 0060). Only about its own person. One of this computer's
        // own person's phones too (decision 0067): the owner's folder, filed
        // under this computer's own device.
        Request::Asks => match (&owner, other_person) {
            (Some(device), true) => {
                let person = store.db().person_of(device)?.unwrap_or(*device);
                Response::Asks { opening: crate::openings::asking(&person) }
            }
            (Some(_), false) => Response::Asks { opening: crate::openings::asking(&store.device_id()?) },
            _ => Response::NotFound,
        },
        Request::Unlock { nonce, key } => match (&owner, other_person) {
            (Some(device), true) => {
                let person = store.db().person_of(device)?.unwrap_or(*device);
                // The key has to open something sealed with it: one of the
                // folder's own names.
                let names = store.db().kept_names(&person)?;
                let fits = |key: &[u8; 32]| {
                    let key = qurb_storage::sealed::FolderKey::from_bytes(*key);
                    names.iter().any(|name| qurb_storage::sealed::open_name(&key, name).is_some())
                };
                match crate::openings::unlock(&person, nonce, *key, fits) {
                    true => Response::Noted,
                    false => Response::NotFound,
                }
            }
            // The owner's folder is not sealed: this computer holds the same
            // key, so the phone's yes is all there is to check, and the key
            // it sends is none (decision 0067).
            (Some(_), false) => match crate::openings::unlock(&store.device_id()?, nonce, [0; 32], |_| true) {
                true => Response::Noted,
                false => Response::NotFound,
            },
            _ => Response::NotFound,
        },

        // A phone of this computer's own person, browsing its folder here
        // (decision 0067, step 4): what is kept for it and for its earlier
        // selves, as the window shows them. Never another person's device,
        // and never another phone's folder.
        // A file's chunks with their sizes, for reading part of it: given to
        // whoever may have its manifest, by the same rules. Not for a sealed
        // folder, whose chunks are not the file's.
        Request::Layout { content } => {
            let content = blake3::Hash::from(*content);
            let reached = guest_here && owner.as_ref().map_or(Ok(false), |d| store.db().shared_content_reachable(&content, d))?;
            if !reached && !store.content_visible_to(&content, audience)? {
                return Ok(Answer::Now(Response::NotFound));
            }
            match store.held_layout(&content)? {
                Some(chunks) => Response::Layout(chunks.into_iter().map(|(h, size)| (*h.as_bytes(), size)).collect()),
                None => Response::NotFound,
            }
        }

        // This computer's shared folder, one folder of it, live (decision
        // 0073, item 6): to its guests and its own person's phones.
        Request::Shared { dir } => match &owner {
            Some(device) if reaches_shared => {
                let (folders, files) = store.shared_listing(device, dir)?;
                Response::Kept {
                    folders,
                    files: files
                        .into_iter()
                        .map(|f| crate::wire::KeptEntry {
                            name: f.name,
                            path: f.path,
                            owner: *f.owner.as_bytes(),
                            content: *f.content.as_bytes(),
                            size: f.size,
                            modified_at: f.modified_at,
                        })
                        .collect(),
                }
            }
            _ => Response::NotFound,
        },

        // Changes to this computer's shared folder, made here when a phone of
        // its own person or a guest asks (decision 0073, item 6): the one
        // exception to serving read-only. Anyone else is told nothing is here.
        Request::SharedMkdir { path } => match &owner {
            Some(device) if reaches_shared => {
                made(qurb_storage::shared_folder::make_folder(store, device, path))?
            }
            _ => Response::NotFound,
        },
        Request::SharedMove { from, to } => match &owner {
            Some(device) if reaches_shared => {
                made(qurb_storage::shared_folder::move_path(store, device, from, to))?
            }
            _ => Response::NotFound,
        },
        Request::SharedDelete { path } => match &owner {
            Some(device) if reaches_shared => {
                made(qurb_storage::shared_folder::delete_path(store, device, path))?
            }
            _ => Response::NotFound,
        },
        Request::Upload { id, offset, bytes } => match &owner {
            Some(device) if reaches_shared => piece(store, device, id, *offset, bytes)?,
            _ => Response::NotFound,
        },
        Request::UploadDone { id, path, content } => match &owner {
            Some(device) if reaches_shared => {
                let staged = staging(store, device, id);
                if !staged.is_file() {
                    Response::Refused { why: "nothing of that file arrived; send it again".into() }
                } else {
                    let placed = qurb_storage::shared_folder::place(store, device, &staged, path, &blake3::Hash::from(*content));
                    let _ = std::fs::remove_file(&staged);
                    match placed {
                        Ok(path) => Response::Placed { path },
                        Err(e) => refused(e)?,
                    }
                }
            }
            _ => Response::NotFound,
        },

        // A search of the shared folder, or of the asking phone's own folder
        // here, to whoever may list it (decision 0073, item 4).
        Request::Search { within, query } => {
            let found = match (within, &owner) {
                (crate::wire::Within::Shared, Some(device)) if reaches_shared => Some(
                    qurb_storage::shared_folder::search(store.db(), &store.device_id()?, device, query, crate::wire::SEARCH_MOST)?,
                ),
                (crate::wire::Within::Kept, Some(device)) if !other_person => {
                    let phones = qurb_storage::person::own_phones(store.db(), device)?;
                    Some(qurb_storage::person::search(store.db(), &phones, query, crate::wire::SEARCH_MOST)?)
                }
                _ => None,
            };
            match found {
                Some(files) => Response::Kept {
                    folders: Vec::new(),
                    files: files
                        .into_iter()
                        .map(|f| crate::wire::KeptEntry {
                            name: f.name,
                            path: f.path,
                            owner: *f.owner.as_bytes(),
                            content: *f.content.as_bytes(),
                            size: f.size,
                            modified_at: f.modified_at,
                        })
                        .collect(),
                },
                None => Response::NotFound,
            }
        }

        Request::Kept { dir } => match (&owner, other_person) {
            (Some(device), false) => {
                let phones = qurb_storage::person::own_phones(store.db(), device)?;
                let (folders, files) = qurb_storage::person::listing(store.db(), &phones, dir)?;
                Response::Kept {
                    folders,
                    files: files
                        .into_iter()
                        .map(|f| crate::wire::KeptEntry {
                            name: f.name,
                            path: f.path,
                            owner: *f.owner.as_bytes(),
                            content: *f.content.as_bytes(),
                            size: f.size,
                            modified_at: f.modified_at,
                        })
                        .collect(),
                }
            }
            _ => Response::NotFound,
        },

        // What kind of device this is, for a peer paired before devices said
        // so (decision 0053). Not found until whatever opened the store has
        // said.
        Request::About => match store.db().local_kind()? {
            Some(kind) => Response::About { kind },
            None => Response::NotFound,
        },

        // This device's folders, to a computer of the same person that keeps
        // its vault, and to nobody else: the names of someone's folders are
        // theirs (decision 0063). Anyone else is told nothing is here.
        Request::Folders => match (&owner, other_person) {
            (Some(device), false) if store.db().is_holder(device)? => {
                Response::Folders { folders: store.db().own_folders()?, choices: store.db().choices_here()? }
            }
            _ => Response::NotFound,
        },

        // What was chosen for the asking device, and only for itself: the
        // device is the one the connection proves, never one it names. Another
        // person's device is told nothing.
        Request::Chosen => match (&owner, other_person) {
            (Some(device), false) => Response::Chosen(store.db().choices_for(device)?),
            _ => Response::NotFound,
        },

        // Handled before the store is locked, since it waits.
        Request::Changes { .. } => unreachable!("answered without locking the store"),

        // A peer reporting that it now holds some content. Recorded against
        // the device the connection proves it to be, not against anything the
        // message says -- otherwise one peer could claim delivery on another's
        // behalf, and this record is what a storage cap trusts before dropping
        // a local copy.
        //
        // Best effort in both directions. An unknown fingerprint is ignored
        // rather than refused, and the acknowledgement is the same either way:
        // the sender has nothing useful to do with a failure, and this device
        // failing to take a note is not the sender's problem.
        Request::Got { content } => {
            match &owner {
                // A computer keeping this device's vault, saying it keeps a
                // sealed file it was shown: recorded against the plain one.
                Some(device) if keeps_mine && store.note_kept_sealed(device, &blake3::Hash::from(*content))? => {}
                Some(device) => {
                    let content = blake3::Hash::from(*content);
                    // A delivery out of the peer's own vault is recorded
                    // differently: the bytes exist, and this device can never
                    // ask for them back. Letting that count as an ordinary
                    // replica would let the storage cap drop a local file
                    // whose only other copy is unreachable.
                    let vault_only =
                        store.db().delivery_is_vault_only(&content, device).unwrap_or(false);
                    // The owner fetching back a file this device keeps for it.
                    // Recorded, and nothing more: the file is the owner's, and
                    // its name has no business in this device's history or
                    // notifications (decision 0036).
                    // For a guest, what is kept is kept for its person
                    // (decision 0060).
                    let person = store.db().person_of(device).ok().flatten();
                    let held = store.db().holds_for(&content, device).unwrap_or(false)
                        || person.is_some_and(|p| store.db().holds_for(&content, &p).unwrap_or(false));
                    let recorded = if held {
                        store.note_replica_in_vault(&content, device)
                    } else if vault_only || other_person {
                        // Another person's copy is one nothing here can ask
                        // for back, wherever else the bytes are (decision
                        // 0060): never an ordinary replica.
                        // The sender's side of a completed delivery, and the
                        // only moment it can be observed: after this the copy
                        // here is releasable and may simply vanish.
                        let path = store
                            .db()
                            .vault_path_for(&content, device)
                            .ok()
                            .flatten();
                        let _ = store.db().record(
                            qurb_storage::db::Event::Collected,
                            path.as_deref(),
                            None,
                            Some(device),
                            None,
                        );
                        store.note_replica_in_vault(&content, device)
                    } else {
                        store.note_replica(&content, device)
                    };
                    if let Err(e) = recorded {
                        tracing::debug!(error = %e, "could not record delivery");
                    }
                }
                None => tracing::debug!("delivery reported by an unrecognised device"),
            }
            Response::Noted
        }

        Request::Chunk { hash } => {
            if let (true, Some(holder)) = (keeps_mine, &owner) {
                if let Some(sealed) = store.sealed_chunk(holder, hash)? {
                    return Ok(Answer::Now(Response::Chunk(sealed)));
                }
            }
            let hash = blake3::Hash::from(*hash);

            // The bytes, checked the same way as the manifest above. Without
            // this a device could skip the tree entirely and fetch anything it
            // could name, which is exactly what a hash is.
            let reached = guest_here && owner.as_ref().map_or(Ok(false), |d| store.db().shared_chunk_reachable(&hash, d))?;
            if !reached && !store.chunk_visible_to(&hash, audience)? {
                return Ok(Answer::Now(Response::NotFound));
            }

            return Ok(Answer::Read { places: store.locate(&hash)?, reader: store.reader(), hash });
        }
    }))
}
