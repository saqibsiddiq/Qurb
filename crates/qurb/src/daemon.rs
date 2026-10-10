//! The part that keeps running.
//!
//! Watches a directory, keeps the store in step with it, and syncs with every
//! paired device it can reach.
//!
//! ```text
//!   startup   reconcile the directory against the index
//!   watch     apply settled filesystem changes as they happen
//!   sync      pull from each paired device, on change and on a timer
//! ```
//!
//! # Why it syncs on a timer as well as on change
//!
//! A local change is a reason to push. It is not the only reason to pull: a peer
//! may have changed something while this device was asleep, and nothing local
//! will happen to prompt a look. The timer is what makes a device that is merely
//! *on* also up to date.

use crate::config::Config;
use anyhow::{Context, Result};
use qurb_engine::{Engine, PinSet};
use qurb_keys::{MasterKey, Purpose};
use qurb_peer::{Connector, Fingerprint, Identity, PeerClient};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::{DebounceConfig, Event, IgnoreRules, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// How often to sync regardless of whether anything appeared to change.
///
/// A device is *told* when a peer changes, so this is no longer how news
/// travels — it is the backstop for the cases being told cannot cover: a
/// notification lost with a dropped connection, a peer that was unreachable
/// when it changed, or a machine coming back from sleep.
const SWEEP_INTERVAL: Duration = Duration::from_secs(120);

/// How often to look for devices paired, and files sent, since the last check.
///
/// The backstop, now. The window that hosts the daemon says so the moment it
/// pairs or sends -- see [`Daemon::nudged_by`] -- which is when the wait is
/// felt: someone who has just scanned a pairing code is watching the screen.
/// What remains is the terminal doing either while the window runs, which
/// waits for this. It was five seconds, and each check opens the index: an
/// idle daemon woke 2.7 times a second with it, and 1.1 times after this
/// change (experiments/service-capacity, `idle-daemon.sh`).
const TRUST_INTERVAL: Duration = Duration::from_secs(30);

/// How often to notice that a connected device went away, while any is
/// connected. In memory only, and not at all while nothing is connected.
const LINKS_INTERVAL: Duration = Duration::from_secs(10);

/// How often to collect garbage and check the storage limit.
///
/// Collection takes the write lock for its deletions, so it is not something to
/// do on every change. Five minutes is often enough that a device cannot drift
/// far over its limit, and rare enough to be invisible.
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(300);

/// How long to wait before the first retry after failing to reach a peer.
///
/// Short, because the commonest failure is not a device that is off but one
/// that has not finished starting. Two daemons launched together will miss each
/// other by a second or two, and a long first backoff turns that into minutes
/// of apparently doing nothing.
const FIRST_RETRY: Duration = Duration::from_secs(5);

/// The longest gap between attempts.
///
/// A device that is genuinely switched off will be switched off for a while, and
/// retrying every few seconds achieves nothing but noise in the log.
const MAX_RETRY: Duration = Duration::from_secs(120);

/// Which folder a daemon is running on, as the filesystem knows it: the
/// device and inode of its store, which a name does not carry.
///
/// A folder moved to the Trash keeps its inode and loses its name, and the
/// daemon's open files go with it. On 2026-10-03 one went on syncing a folder
/// in the Trash for a day and a half, unseen, while the window -- opening the
/// folder by name -- made a second device where it had been. Checked on the
/// daemon's timers and before every change it applies; a folder that is not
/// at its name any more stops the daemon, saying why.
struct Here {
    dev: u64,
    ino: u64,
}

impl Here {
    fn of(dir: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(dir).with_context(|| format!("reading {}", dir.display()))?;
        Ok(Self { dev: meta.dev(), ino: meta.ino() })
    }

    /// Whether `dir` still names this folder: not gone, and not replaced by
    /// another made at the same path.
    fn still(&self, dir: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(dir).map(|m| m.dev() == self.dev && m.ino() == self.ino).unwrap_or(false)
    }
}

pub struct Daemon {
    root: PathBuf,
    store_dir: PathBuf,
    master: MasterKey,
    identity: Identity,
    config: Config,
    /// Where to publish what the daemon is doing, if anyone is displaying it.
    ///
    /// Optional because the terminal front end has no use for it: its account
    /// of the daemon is the log. An interface supplies one.
    status: Option<crate::status::Publisher>,
    /// Said to when something this daemon should look at has just happened in
    /// the same process -- a device paired, a file sent -- so it looks now
    /// rather than at the next [`TRUST_INTERVAL`].
    nudge: Option<std::sync::Arc<tokio::sync::Notify>>,
    /// Set when this daemon is a storage-only replica, and then saying what it
    /// holds. `None` is an ordinary device with a folder someone looks at.
    ///
    /// A replica has no directory to watch and nothing to materialise: it
    /// keeps chunks so that the devices people actually use do not all have to
    /// be awake at the same moment. See
    /// [decision 0006](../../docs/decisions/0006-availability-gap.md).
    pins: Option<PinSet>,
}

impl Daemon {
    pub fn new(
        root: &Path,
        store_dir: &Path,
        master: MasterKey,
        identity: Identity,
        config: Config,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            store_dir: store_dir.to_path_buf(),
            master,
            identity,
            config,
            status: None,
            nudge: None,
            pins: None,
        }
    }

    /// Look for new pairings and sends whenever `nudge` is notified, as well
    /// as on the slow timer.
    pub fn nudged_by(mut self, nudge: std::sync::Arc<tokio::sync::Notify>) -> Self {
        self.nudge = Some(nudge);
        self
    }

    /// Publish status to `publisher` as the daemon runs.
    /// Run as a storage-only replica, holding the paths `pins` selects.
    ///
    /// The difference is not a setting on the same thing: a replica has no
    /// directory to watch, materialises nothing, and keeps every chunk payload
    /// because it is the only place its copy lives. A device with a folder
    /// stores content once, in the folder; a replica has no folder to store it
    /// in.
    pub fn holding(mut self, pins: PinSet) -> Self {
        self.pins = Some(pins);
        self
    }

    /// Whether this daemon holds content without a folder behind it.
    fn is_replica(&self) -> bool {
        self.pins.is_some()
    }

    pub fn reporting_to(mut self, publisher: crate::status::Publisher) -> Self {
        self.status = Some(publisher);
        self
    }

    /// Update the published status, if anyone asked for it.
    ///
    /// Takes a closure rather than a value so a caller that is not publishing
    /// pays nothing: the closure is not run when there is no publisher, and
    /// gathering a summary means counting files and querying the store.
    /// Say that the folder has gone, where an interface will show it, and
    /// hand back the error to stop with.
    fn folder_gone(&self) -> anyhow::Error {
        let detail = format!(
            "{} was moved or deleted while qurb was syncing it, so qurb has stopped. \
             Put the folder back, or set qurb up again.",
            self.root.display()
        );
        tracing::error!("{detail}");
        self.report(|status| {
            status.problem = Some(detail.clone());
            status.settle();
        });
        anyhow::anyhow!(detail)
    }

    fn report(&self, change: impl FnOnce(&mut crate::status::Status)) {
        if let Some(publisher) = &self.status {
            publisher.send_modify(change);
        }
    }

    fn chunk_key(&self) -> ChunkKey {
        ChunkKey::from_bytes(self.master.derive(Purpose::ChunkEncryption).to_bytes())
    }

    /// Tell every trusted peer that this device has something for it.
    ///
    /// Sent through the rendezvous service, which forwards it to those that
    /// are connected and keeps it for those that are not — so a device asleep
    /// at the moment of a change hears about it when it wakes, rather than at
    /// its own next poll. On a phone that is the difference between seconds
    /// and a quarter of an hour.
    ///
    /// `except` skips the peer the news came *from*, which is how two devices
    /// avoid telling each other about the same change for ever.
    ///
    /// Carries who, never what, and is best effort throughout: a peer that
    /// never hears it syncs on its own schedule, exactly as before.
    async fn announce_news(&self, connector: &Connector, peers: &Peers, except: Option<Fingerprint>) {
        // On the local network first, because it needs nobody's permission and
        // reaches every device on this Wi-Fi at once. A device that hears it
        // syncs within a second, with no server involved.
        connector.announce_news().await;

        for peer in peers.known.iter().copied() {
            if Some(peer) == except {
                continue;
            }
            if let Err(e) = connector.tell_waiting(peer) {
                tracing::debug!(peer = %peer.short(), error = %e, "could not say we have news");
            }
        }
    }

    /// Tell devices with something waiting for them that there is.
    ///
    /// This is what turns a send into a delivery while the recipient is
    /// asleep: the same "there is news for you" signal a local change raises,
    /// which the rendezvous service turns into a push wake-up. Sent every tick
    /// while anything is outstanding, because the recipient may have been
    /// unreachable for every previous one.
    fn announce_deliveries(&self, engine: &Engine, connector: &Connector, peers: &Peers) {
        let waiting = match engine.store().awaiting_collection() {
            Ok(devices) if !devices.is_empty() => devices,
            Ok(_) => return,
            Err(e) => {
                tracing::debug!(error = %e, "could not check for waiting deliveries");
                return;
            }
        };

        for device in waiting {
            // Matched through the trust list, because `tell_waiting` addresses
            // a peer by fingerprint and a vault is scoped by device id.
            let found = engine
                .store()
                .db()
                .trusted_peers()
                .ok()
                .and_then(|list| list.into_iter().find(|p| p.device_id == device));
            let Some(peer) = found else { continue };
            let fingerprint = Fingerprint::from_bytes(peer.fingerprint);
            if !peers.known.contains(&fingerprint) {
                continue;
            }
            if let Err(e) = connector.tell_waiting(fingerprint) {
                tracing::debug!(peer = %fingerprint.short(), error = %e, "could not say there is a delivery");
            }
        }
    }

    /// Collect garbage, then bring disk use under the configured limit.
    ///
    /// In that order, and the order is the point: collection frees superseded
    /// and expired content, which costs the user nothing. Only once that has
    /// happened is it fair to start dropping local copies of files they still
    /// have.
    ///
    /// Everything here is best-effort. A device that cannot tidy up should
    /// keep syncing, which is what it is for.
    fn housekeep(&self, engine: &mut Engine) {
        match engine.housekeep(qurb_engine::RETENTION) {
            Ok(done) => {
                if done.bytes_freed() > 0 {
                    tracing::info!(
                        collected = done.collected.bytes_reclaimed,
                        reclaimed = done.reclaimed.bytes_reclaimed,
                        tombstones = done.collected.tombstones_expired,
                        "freed what nothing needs"
                    );
                }
                for off in &done.called_off {
                    tracing::info!(path = %off.path, why = %off.why, "a send was called off");
                }
            }
            Err(e) => tracing::warn!(error = %e, "housekeeping failed"),
        }

        let limit = self.limit();
        if limit == 0 {
            return;
        }

        match engine.enforce_limit(limit) {
            Ok(stats) if stats.dropped > 0 || !stats.within() => {
                tracing::info!(
                    dropped = stats.dropped,
                    freed = stats.freed,
                    used = stats.used_after,
                    limit,
                    "checked the storage limit"
                );
                if !stats.within() {
                    // Said plainly rather than buried: the device is over its
                    // limit and is keeping the excess on purpose, because
                    // everything left is content no other device is known to
                    // hold. Dropping it would be losing it.
                    tracing::warn!(
                        over_by = stats.still_over,
                        refused = stats.refused,
                        "over the storage limit, and keeping it: nothing left is safe to drop"
                    );
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "checking the storage limit failed"),
        }
    }

    fn open_store(&self) -> Result<Store> {
        Store::open(&self.store_dir, self.chunk_key())
            .map(|store| {
                // A replica has no folder, so there is no materialised file to
                // read payloads back out of: it keeps every chunk, which is
                // the whole reason it is worth having. Attaching a folder here
                // would make it believe content was available that is not.
                match self.is_replica() {
                    true => store,
                    false => store.in_tree(&self.root),
                }
            })
            .with_context(|| format!("opening the store at {}", self.store_dir.display()))
    }

    fn engine(&self) -> Result<Engine> {
        Ok(match &self.pins {
            Some(pins) => Engine::replica(self.root.clone(), self.open_store()?, pins.clone()),
            None => {
                let mut store = self.open_store()?;
                store.set_new_files_private(self.config.own_files_private);
                let ignore = IgnoreRules::new().with_store_dir(&self.store_dir);
                Engine::new(self.root.clone(), store, ignore)
            }
        })
    }

    pub async fn run(&self) -> Result<()> {
        // Before anything else, and held for the whole run. Two daemons on one
        // store contend on the write lock, answer as the same device, and both
        // enforce the same storage cap -- and none of that reports an error,
        // it just behaves oddly. See [`crate::lock`].
        let _lock = match crate::lock::Lock::take(&self.store_dir)? {
            Some(lock) => lock,
            None => anyhow::bail!(
                "another qurb is already syncing {}.\n\
                 That is either the desktop app or `qurb run` in another \
                 terminal — only one can, and they are the same daemon.",
                self.root.display()
            ),
        };

        // Which folder this is, by more than its name: see [`Here`].
        let here = Here::of(&self.store_dir)?;

        let mut engine = self.engine()?;
        // What this device tells peers it is (decision 0053).
        let _ = engine
            .store()
            .db()
            .set_local_kind(if self.is_replica() { "replica" } else { "computer" });

        // Where a file sent to this device goes. Checked before anything
        // syncs: a directory overlapping the folder would put deliveries where
        // the scan finds them and advertises them to every device, so the
        // daemon refuses to start rather than begin doing that.
        if !self.is_replica() {
            let downloads = self.config.downloads.resolve(&self.root)?;
            match &downloads {
                Some(dir) => tracing::info!(downloads = %dir.display(), "files sent here go to"),
                None => tracing::info!("files sent here are kept in the folder"),
            }
            engine.set_downloads(downloads);
        }

        // The trust store answers who may connect, so a device paired after
        // this point needs a restart to be let in. Stated as a limitation
        // rather than hidden: it is the same one `PeerServer::bind_trusting`
        // has, and fixing it properly means a listener that can be reconfigured.
        // A live set, not a snapshot. `qurb pair` is a separate process writing
        // to the trust store, so a listener holding the list it read at startup
        // refuses a device paired a minute later -- and "pair once" then means
        // "pair once, then restart the daemon", which is not what anyone reads
        // it as. Refreshed on every sweep below.
        let trust = qurb_peer::tls::TrustList::new(qurb_peer::trusted_fingerprints(
            engine.store(),
        )?);
        // A device paired by another process is accepted on its first
        // connection, not after the next sweep (see `reread_with`).
        {
            let (store_dir, chunk_key) = (self.store_dir.clone(), self.chunk_key());
            trust.reread_with(Arc::new(move || {
                let store = Store::open(&store_dir, chunk_key.clone()).ok()?;
                qurb_peer::trusted_fingerprints(&store).ok()
            }));
        }
        let trusted: Vec<Fingerprint> = qurb_peer::trusted_fingerprints(engine.store())?;
        if trusted.is_empty() {
            tracing::warn!("no paired devices; this daemon will sync with nobody");
        }

        tracing::info!(root = %self.root.display(), "reconciling");
        let initial = engine.reconcile()?;
        tracing::info!(
            stored = initial.stored,
            unchanged = initial.unchanged,
            deleted = initial.deleted,
            failures = initial.failures.len(),
            "reconciled"
        );
        // Counted here, before any networking. What this device holds is
        // knowable without reaching anything, and an interface that shows
        // "0 files" while the store has hundreds -- because the rendezvous
        // service happens to be down -- is worse than showing nothing.
        if let Ok(counted) = self.count() {
            self.report(|status| {
                status.files = counted.files;
                status.bytes_on_disk = counted.on_disk;
                status.peers = counted.peers;
                status.used = counted.used;
                status.limit = counted.limit;
                status.settle();
            });
        }

        if initial.collided > 0 {
            tracing::warn!(
                count = initial.collided,
                "files skipped: their names differ only in how the text is encoded, \
                 and this device's index cannot tell them apart"
            );
        }
        report_collisions(&engine);

        // Looked up now, and a failure costs only the fallback: devices that
        // can reach each other directly still do.
        let relay = match &self.config.relay {
            Some(text) => match qurb_peer::resolve_relay(text).await {
                Ok(address) => Some(address),
                Err(e) => {
                    tracing::warn!(error = %e, "no relay this time; direct paths only");
                    None
                }
            },
            None => None,
        };
        let connector = match Connector::start(
            format!("0.0.0.0:{}", self.config.port).parse()?,
            self.identity.clone(),
            self.master.clone(),
            &trust,
            &self.config.signal,
            qurb_peer::Finding::everything(relay),
        )
        .await
        {
            Ok(connector) => Arc::new(connector),
            Err(e) => {
                // Reported before returning, so an interface shows what
                // happened rather than the last thing that was true. Failing
                // silently here used to leave a tray saying "syncing" over a
                // daemon that had already stopped.
                let detail = format!("cannot reach the rendezvous service: {e}");
                self.report(|status| {
                    status.problem = Some(detail.clone());
                    status.settle();
                });
                return Err(anyhow::Error::new(e).context("starting the connection machinery"));
            }
        };
        tracing::info!(
            address = %connector.local_addr()?,
            // Every address this device offers peers, not just the default
            // route's. When a device elsewhere cannot connect, the first
            // question is what it was given to try.
            reachable = ?connector.endpoints().local,
            public = ?connector.endpoints().public,
            relay = ?relay,
            "listening"
        );

        // How far this device's own state has got. Peers hold a request open
        // against it, so they hear about a change within a round trip rather
        // than whenever they next think to ask.
        let generation = qurb_peer::Generation::new();

        // Serve peers on every path we have. A device unreachable by relay is
        // unreachable by anyone whose direct attempt failed.
        let served = Arc::new(Mutex::new(self.open_store()?));
        // What is being sent, for anybody displaying it. Nothing to watch for
        // when nobody is.
        let sending: Option<Arc<dyn qurb_peer::Served>> = self.status.as_ref().map(|status| {
            Arc::new(Sending { store: Arc::clone(&served), status: status.clone() })
                as Arc<dyn qurb_peer::Served>
        });
        for endpoint in [Some(connector.endpoint().clone()), connector.relay_endpoint().cloned()]
            .into_iter()
            .flatten()
        {
            let store = Arc::clone(&served);
            let generation = Arc::clone(&generation);
            let sending = sending.clone();
            let trust = trust.clone();
            tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let store = Arc::clone(&store);
                    let generation = Arc::clone(&generation);
                    let sending = sending.clone();
                    let trust = trust.clone();
                    tokio::spawn(async move {
                        let Ok(connection) = incoming.await else { return };
                        match sending {
                            Some(watcher) => {
                                qurb_peer::server::serve_connection_watched(
                                    connection, store, generation, watcher, &trust,
                                )
                                .await
                            }
                            None => {
                                qurb_peer::server::serve_connection(
                                    connection, store, generation, &trust,
                                )
                                .await
                            }
                        }
                    });
                }
            });
        }

        // A replica has no directory to watch. Starting one would be worse
        // than useless: it would report an empty tree, and every file the
        // replica holds would look like one the user had just deleted.
        let mut watcher = match self.is_replica() {
            true => None,
            false => Some(
                Watcher::start(
                    &self.root,
                    IgnoreRules::new().with_store_dir(&self.store_dir),
                    DebounceConfig::default(),
                )
                .context("watching the directory")?,
            ),
        };

        let mut peers = Peers::new(trusted);
        let mut arrivals = connector.arrivals();
        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;

        let mut timer = tokio::time::interval(SWEEP_INTERVAL);
        let mut trust_timer = tokio::time::interval(TRUST_INTERVAL);
        let mut links_timer = tokio::time::interval(LINKS_INTERVAL);
        let nudge = self.nudge.clone();
        let mut maintenance = tokio::time::interval(MAINTENANCE_INTERVAL);
        maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                // `pending()` when there is no watcher, so the arm simply
                // never fires rather than the loop needing two shapes.
                event = async {
                    match &mut watcher {
                        Some(watcher) => watcher.next().await,
                        None => std::future::pending().await,
                    }
                } => match event {
                    // Before anything is applied: a folder moved away reads to
                    // a watcher like its files going, and they must not be
                    // taken for deletions.
                    _ if !here.still(&self.store_dir) => return Err(self.folder_gone()),
                    Some(Event::Changes(changes)) => {
                        // Storage work is synchronous and can take seconds on a
                        // large file, so it must not run on the async scheduler.
                        let outcome = tokio::task::block_in_place(|| engine.apply(&changes));
                        match outcome {
                            Ok(stats) => {
                                if stats.stored > 0 || stats.deleted > 0 {
                                    tracing::info!(
                                        stored = stats.stored,
                                        deleted = stats.deleted,
                                        "local changes"
                                    );
                                    // Anyone holding a request open hears now.
                                    generation.bump();
                                    self.announce_news(&connector, &peers, None).await;
                                }
                                for failure in &stats.failures {
                                    tracing::warn!(
                                        path = %failure.path.display(),
                                        error = %failure.error,
                                        "could not store"
                                    );
                                }
                            }
                            Err(e) => tracing::error!(error = %e, "applying changes failed"),
                        }
                        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                    }

                    Some(Event::RescanRequired) => {
                        tracing::warn!("the watcher dropped events; reconciling from scratch");
                        if let Err(e) = tokio::task::block_in_place(|| engine.reconcile()) {
                            tracing::error!(error = %e, "reconciling failed");
                        }
                        generation.bump();
                        self.announce_news(&connector, &peers, None).await;
                        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                    }

                    None => {
                        tracing::error!("the watcher stopped");
                        return Ok(());
                    }
                },

                _ = timer.tick() => {
                    if !here.still(&self.store_dir) {
                        return Err(self.folder_gone());
                    }
                    self.refresh_trust(&trust, &mut peers);
                    self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                }

                // Free what nothing references, then stay under the limit.
                _ = maintenance.tick() => {
                    tokio::task::block_in_place(|| self.housekeep(&mut engine));
                }

                // Something happened in this process that the checks below
                // would find: look now. `pending()` with nobody to nudge, as
                // for the watcher.
                _ = async {
                    match &nudge {
                        Some(nudge) => nudge.notified().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if self.refresh_trust(&trust, &mut peers) {
                        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                    }
                    self.announce_deliveries(&engine, &connector, &peers);
                }

                // Which devices are connected, kept current between sync
                // passes -- which may be minutes apart -- so a device that
                // went away stops being shown as connected within the
                // half-minute it takes the connection to time out. Only while
                // something is connected: an arm switched off is not polled,
                // so an idle laptop whose phone is away is not woken for it.
                _ = links_timer.tick(), if peers.any_connected() => {
                    peers.forget_closed();
                    let links = peers.links();
                    self.report(|status| status.links = links);
                }

                // A device paired, or a file sent, from another process.
                _ = trust_timer.tick() => {
                    if !here.still(&self.store_dir) {
                        return Err(self.folder_gone());
                    }
                    if self.refresh_trust(&trust, &mut peers) {
                        // Newly paired, so try it immediately: the person who
                        // just scanned the code is waiting to see their files.
                        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                    }
                    // A file sent to somebody's vault never touches the watched
                    // folder, so no change event will ever mention it and the
                    // announcement above would not fire. Asked here instead:
                    // one indexed query every few seconds, against the
                    // alternative of a send sitting unnoticed until the next
                    // five-minute maintenance tick.
                    self.announce_deliveries(&engine, &connector, &peers);
                }

                // A peer said it changed. This is how news travels now; the
                // timer above is only the backstop.
                peer = peers.next_change() => {
                    // A watcher started before its device was removed may
                    // report once more before its connection closes.
                    if !peers.knows(&peer) {
                        continue;
                    }
                    tracing::debug!(peer = %peer.short(), "peer reports a change");
                    self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                }

                // A peer has just become reachable.
                //
                // This is what makes syncing with a phone work. A phone is
                // announced only while it is awake -- twenty-odd seconds in a
                // background window -- and a daemon that discovers it by
                // retrying on a backoff is not asking during those seconds.
                // Worse, the backoff grows *because* the phone keeps being
                // absent, so the two drift further apart the longer it goes on.
                // Measured before this existed: a laptop retrying every 120s
                // never once caught a phone announcing for 25s.
                //
                // The backoff is cleared as well as the sync triggered: the
                // device is demonstrably there, so the reason for waiting has
                // gone.
                //
                // Every arrival already queued is taken with it, and one pass
                // serves them all. A phone announces itself again and again
                // while it waits to be collected from, and those queue up
                // behind a long sync: after one that took 2½ minutes the laptop
                // synced thirteen times in half a second (2026-10-05), each
                // fetching the phone's tree to find nothing new.
                Ok(member) = arrivals.recv() => {
                    let mut arrived = vec![member];
                    while let Ok(more) = arrivals.try_recv() {
                        arrived.push(more);
                    }
                    let arrived: std::collections::BTreeSet<Fingerprint> = arrived
                        .into_iter()
                        .filter_map(|member| peers.member(&self.master, member))
                        .collect();
                    for peer in &arrived {
                        tracing::info!(peer = %peer.short(), "a peer is reachable and has news; syncing now");
                        peers.ready_now(*peer);
                    }
                    if !arrived.is_empty() {
                        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                    }
                }
            }
        }
    }

    /// Pick up devices paired since the last look.
    ///
    /// Cheap: the trust store is a handful of rows describing one person's own
    /// devices, and this runs once a sweep rather than per connection.
    ///
    /// Returns whether anything was newly trusted, so the caller can act on it.
    fn refresh_trust(&self, trust: &qurb_peer::tls::TrustList, peers: &mut Peers) -> bool {
        let Ok(store) = self.open_store() else { return false };
        let Ok(current) = qurb_peer::trusted_fingerprints(&store) else { return false };

        let added: Vec<Fingerprint> =
            current.iter().copied().filter(|f| !peers.knows(f)).collect();
        let removed: Vec<Fingerprint> =
            peers.known.iter().copied().filter(|f| !current.contains(f)).collect();

        // Removals too, not only additions: a device removed from the trust
        // store and still in `known` would go on being dialled and synced with
        // by this device, however firmly the listener refused it.
        if !added.is_empty() || !removed.is_empty() {
            for peer in &added {
                tracing::info!(peer = %peer.short(), "a new device was paired");
            }
            for peer in &removed {
                tracing::info!(peer = %peer.short(), "a device was removed");
            }
            peers.learn(&current);
            self.report(|status| {
                status.peers = current.len();
                status.settle();
            });
        }
        // Replaced every time, not only when something was added: a device
        // forgotten in the trust store must stop being accepted here too.
        let gained = !added.is_empty();
        trust.replace(current);
        gained
    }

    /// Count what the store holds, for a status summary.
    fn count(&self) -> Result<Counted> {
        let store = self.open_store()?;
        Ok(Counted {
            files: store.db().live_paths()?.len(),
            on_disk: store.db().size_totals()?.1,
            peers: store.db().trusted_peers()?.len(),
            used: store.usage()?.total(),
            limit: self.limit(),
        })
    }

    /// The storage allowance, read fresh.
    ///
    /// From the file rather than from the copy loaded at startup, so that
    /// changing it — with `qurb config`, or by moving the slider in the
    /// window — takes effect on a daemon that is already running. A setting
    /// that needs a restart to apply is a setting people will think is broken.
    fn limit(&self) -> u64 {
        Config::load(&self.store_dir).map(|c| c.limit).unwrap_or(self.config.limit)
    }

    /// Where deliveries go, and whether this device's own new files are
    /// private, read fresh, for the same reason as [`limit`]: both are
    /// switches in the window's Settings.
    ///
    /// A downloads setting that fails to resolve -- a hand-edited file naming
    /// somewhere inside the folder -- leaves the engine where it was and says
    /// so, rather than starting to write deliveries somewhere that would sync
    /// them.
    ///
    /// [`limit`]: Self::limit
    fn refresh_downloads(&self, engine: &mut Engine) {
        if self.is_replica() {
            return;
        }
        let Ok(config) = Config::load(&self.store_dir) else { return };
        if engine.store().new_files_private() != config.own_files_private {
            tracing::info!(private = config.own_files_private, "files added here are now");
            engine.store_mut().set_new_files_private(config.own_files_private);
        }
        match config.downloads.resolve(&self.root) {
            Ok(dir) if dir.as_deref() != engine.downloads() => {
                match &dir {
                    Some(dir) => {
                        tracing::info!(downloads = %dir.display(), "files sent here now go to")
                    }
                    None => tracing::info!("files sent here are now kept in the folder"),
                }
                engine.set_downloads(dir);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "not changing where files sent here go"),
        }
    }

    /// Pull from every peer we can reach.
    async fn sync_all(
        &self,
        engine: &mut Engine,
        connector: &Connector,
        peers: &mut Peers,
        generation: &Arc<qurb_peer::Generation>,
    ) {
        self.refresh_downloads(engine);
        let mut reached = 0usize;
        // Set by whichever peer brought something new, so the others can be
        // told once at the end rather than per peer.
        let mut news_from: Option<Fingerprint> = None;
        let attempted = peers.ready();
        if !attempted.is_empty() {
            self.report(|status| status.state = crate::status::State::Working);
        }

        for peer in attempted {
            match self.sync_one(engine, connector, peers, peer).await {
                Ok(moved) => {
                    reached += 1;
                    peers.succeeded(peer);
                    // So `qurb status` can say when a device was last reachable,
                    // which is usually the first question when something has not
                    // arrived.
                    if let Ok(store) = self.open_store() {
                        let _ = store.db().mark_peer_seen(peer.as_bytes());
                    }
                    if moved > 0 {
                        tracing::info!(peer = %peer.short(), files = moved, "synced");
                        // What arrived from one peer is news for the others.
                        // Not for the peer it came from, which already knows —
                        // telling it back would have the two of them nudging
                        // each other about the same change indefinitely.
                        generation.bump();
                        news_from = Some(peer);
                    }
                }
                Err(e) => {
                    // Not an error worth stopping for. A peer that is switched
                    // off is the normal case in a system built on the
                    // assumption that devices are not always on -- so that is
                    // only worth a debug line. One the rendezvous service has
                    // just said is there is different: it is on, and the path
                    // to it failed, which is the thing to know when a sync
                    // that should have happened did not. Found on 2026-09-28,
                    // when a phone on mobile data was announced and never
                    // reached, and the log could not say why.
                    if peers.failed(peer) {
                        tracing::info!(
                            peer = %peer.short(),
                            error = %e,
                            "the device is there and could not be reached"
                        );
                    } else {
                        tracing::debug!(peer = %peer.short(), error = %e, "could not sync");
                    }
                }
            }
        }

        if news_from.is_some() {
            self.announce_news(connector, peers, news_from).await;
        }

        // How each device is reached, from the connections actually held
        // rather than from anything remembered about them, so a device that
        // dropped off is not shown as connected.
        let links = peers.links();
        self.report(|status| status.links = links);

        // Counted per pass rather than accumulated, because the question an
        // interface answers is "can I reach my devices *now*".
        if let Ok(counted) = self.count() {
            self.report(|status| {
                status.files = counted.files;
                status.bytes_on_disk = counted.on_disk;
                status.peers = counted.peers;
                status.peers_reachable = reached;
                status.used = counted.used;
                status.limit = counted.limit;
                status.settle();
            });
        }
    }

    async fn sync_one(
        &self,
        engine: &mut Engine,
        connector: &Connector,
        peers: &mut Peers,
        peer: Fingerprint,
    ) -> Result<usize> {
        let client = match peers.connection(peer) {
            Some(client) => client,
            None => {
                let client = Arc::new(connector.reach(peer).await?);
                peers.connected(peer, Arc::clone(&client));
                peers.watch(peer, Arc::clone(&client));
                client
            }
        };

        let tree = match client.tree().await {
            Ok(tree) => tree,
            Err(e) => {
                // The connection has gone. Drop it so the next attempt makes a
                // fresh one rather than retrying down a dead pipe for ever.
                peers.disconnected(peer);
                return Err(e.into());
            }
        };

        // Which device this is, so that files it asks this one to keep for it
        // are filed in its vault (decision 0036).
        let peer_device = engine
            .store()
            .db()
            .peer_by_fingerprint(peer.as_bytes())
            .ok()
            .flatten()
            .map(|p| p.device_id);
        // What kind of device it is, if it has not said: paired before devices
        // said so when pairing (decision 0053). Asked once; an older build
        // that does not answer is asked again next time, which costs one
        // round trip.
        if let Some(device) = &peer_device {
            if matches!(engine.store().db().peer_kind(device), Ok(None)) {
                if let Ok(Some(kind)) = client.about().await {
                    let _ = engine.store().learn_kind(device, &kind);
                }
            }
        }

        // Files somebody asked to have back are in the plan too; see
        // `plan_with`.
        let plan = engine.plan_with(&tree, peer_device.as_ref())?;

        // Before the early return below, not after it. Two devices that agree
        // about everything have an empty plan every time, and those are
        // exactly the devices with holdings to report: content that arrived
        // before there was any way to say so is, by definition, content
        // neither side needs to transfer again.
        let reader = self.open_store()?;
        let known = engine.store().db().peer_by_fingerprint(peer.as_bytes()).ok().flatten();
        // What a transfer from this peer is shown as coming from.
        let from = known.as_ref().map(|k| k.name.clone()).unwrap_or_else(|| peer.short());
        if let Some(known) = known {
            let told = qurb_peer::report_holdings(&client, &reader, &known.device_id, &tree, 64).await;
            if told > 0 {
                tracing::debug!(peer = %peer.short(), told, "reported holdings");
            }
            // And the other way: what it is recorded as holding for files
            // freed here, asked a few at a time (decision 0055).
            let asked = qurb_peer::check_holders(&client, &reader, &known.device_id, 16).await;
            if asked > 0 {
                tracing::debug!(peer = %peer.short(), asked, "asked what it holds");
            }
        }

        if plan.is_empty() {
            return Ok(0);
        }

        // The paths before applying, because a plan is consumed by it. These
        // are what a "recently synced" list is made of -- files arriving from
        // another device, which is the part a person did not do themselves and
        // therefore the part worth telling them about.
        //
        // Never what is kept for another device: those are its files, and
        // their names have no business on this device's screen.
        let arriving: Vec<String> = plan
            .iter()
            .filter(|action| !matches!(action, qurb_sync::Action::Hold { .. }))
            .map(|action| action.path().to_string())
            .collect();

        let outcome = tokio::task::block_in_place(|| {
            let mut source = qurb_peer::NetworkSource::new(&client, &reader).for_peer(peer_device);
            let mut progress = Reporting::new(self.status.as_ref(), from);
            engine.apply_plan_reporting(&plan, &mut source, &mut progress)
        })?;

        if outcome.adopted > 0 || outcome.conflicts > 0 {
            self.report(|status| {
                for path in arriving {
                    status.remember(path, true);
                }
                status.last_sync = Some(std::time::SystemTime::now());
            });
        }

        for failure in &outcome.failures {
            tracing::warn!(
                path = %failure.path.display(),
                error = %failure.error,
                "could not apply"
            );
        }

        Ok(outcome.adopted + outcome.conflicts + outcome.resurrected)
    }
}

/// How often a transfer's progress is published, at most.
///
/// Often enough that a bar moves smoothly to the eye, rarely enough that a
/// fast local transfer -- hundreds of megabytes a second -- is not spending
/// its time telling a window about itself.
const PUBLISH_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// Shows content arriving from one peer on the status channel.
///
/// The engine takes one file at a time, so there is at most one transfer in
/// flight per observer, and `finished` removes it whether or not it arrived.
struct Reporting<'a> {
    status: Option<&'a crate::status::Publisher>,
    from: String,
    /// Bytes counted since the last publish.
    unpublished: u64,
    published_at: std::time::Instant,
}

impl<'a> Reporting<'a> {
    fn new(status: Option<&'a crate::status::Publisher>, from: String) -> Self {
        Self { status, from, unpublished: 0, published_at: std::time::Instant::now() }
    }

    fn publish(&self, change: impl FnOnce(&mut Vec<crate::status::Transfer>)) {
        if let Some(status) = self.status {
            status.send_modify(|s| change(&mut s.incoming));
        }
    }
}

impl qurb_engine::Progress for Reporting<'_> {
    fn started(&mut self, path: &str, size: u64) {
        self.unpublished = 0;
        self.published_at = std::time::Instant::now();
        let now = std::time::SystemTime::now();
        let transfer = crate::status::Transfer {
            path: path.to_string(),
            device: self.from.clone(),
            size,
            done: 0,
            started: now,
            updated: now,
        };
        self.publish(|incoming| incoming.push(transfer));
    }

    fn advanced(&mut self, bytes: u64) {
        self.unpublished += bytes;
        if self.published_at.elapsed() < PUBLISH_EVERY {
            return;
        }
        let (from, arrived) = (self.from.clone(), self.unpublished);
        self.publish(|incoming| {
            if let Some(current) = incoming.iter_mut().rev().find(|t| t.device == from) {
                current.done = (current.done + arrived).min(current.size);
                current.updated = std::time::SystemTime::now();
            }
        });
        self.unpublished = 0;
        self.published_at = std::time::Instant::now();
    }

    fn finished(&mut self, path: &str) {
        self.unpublished = 0;
        let from = self.from.clone();
        self.publish(|incoming| incoming.retain(|t| !(t.path == path && t.device == from)));
    }
}

/// How long a send may sit without the other device asking for more before
/// it stops being shown. Long enough to cover a pause between chunks on a slow
/// link; short enough that a device that went away does not leave a bar frozen
/// on screen.
const SEND_STALE: std::time::Duration = std::time::Duration::from_secs(60);

/// Shows files this device is sending, from the chunks peers ask it for.
///
/// A peer asks by hash and never says which file, so each chunk served is
/// traced back to the send it belongs to. Content in the shared area is not
/// shown: it is synced rather than sent, and the receiving device is the one
/// with something to say about it.
struct Sending {
    store: Arc<Mutex<qurb_storage::Store>>,
    status: crate::status::Publisher,
}

impl qurb_peer::Served for Sending {
    fn served(&self, to: &Fingerprint, chunk: &blake3::Hash, bytes: u64) {
        // Asked of the index, which needs the lock the request that served
        // this chunk has already given up.
        let traced = tokio::task::block_in_place(|| {
            let store = self.store.lock().ok()?;
            let peer = store.db().peer_by_fingerprint(to.as_bytes()).ok()??;
            let (path, size) = store.db().sent_file_holding(chunk, &peer.device_id).ok()??;
            Some((peer.name, path, size))
        });
        let Some((device, path, size)) = traced else { return };

        let now = std::time::SystemTime::now();
        self.status.send_modify(|status| {
            // Whatever has not moved in a while is not moving: the other end
            // finished, or went away. Either way it is not worth a bar.
            status.outgoing.retain(|t| {
                now.duration_since(t.updated).unwrap_or_default() < SEND_STALE
            });
            match status.outgoing.iter_mut().find(|t| t.path == path && t.device == device) {
                Some(sending) => {
                    sending.done = (sending.done + bytes).min(sending.size);
                    sending.updated = now;
                }
                None => status.outgoing.push(crate::status::Transfer {
                    path,
                    device,
                    size,
                    done: bytes.min(size),
                    started: now,
                    updated: now,
                }),
            }
        });
    }
}

/// Which peers exist, which are connected, and which are not worth trying yet.
struct Peers {
    known: Vec<Fingerprint>,
    connections: HashMap<Fingerprint, Arc<PeerClient>>,
    /// When to try again, and how long to wait after the next failure.
    ///
    /// Doubling rather than a fixed delay, because the two cases look identical
    /// at the moment of failure and want opposite treatment: a peer still
    /// starting up should be retried in seconds, and one that is switched off
    /// should be left alone.
    backoff: HashMap<Fingerprint, (tokio::time::Instant, Duration)>,
    /// Peers reporting that they have changed.
    ///
    /// One task per connected peer holds a request open and sends down this
    /// channel when it is answered. The daemon waits on the receiving end, so a
    /// change on any peer wakes it within a round trip.
    changes: (mpsc::UnboundedSender<Fingerprint>, mpsc::UnboundedReceiver<Fingerprint>),
    /// Which peers already have a watcher, so one is not started twice.
    watching: std::collections::HashSet<Fingerprint>,
    /// Peers the rendezvous service has just said are there, until the next
    /// attempt on each. Failing to reach one of these is worth saying at the
    /// default level; failing to reach a device that is simply off, the normal
    /// case, is not.
    announced: std::collections::HashSet<Fingerprint>,
}

impl Peers {
    fn new(known: Vec<Fingerprint>) -> Self {
        Self {
            known,
            connections: HashMap::new(),
            backoff: HashMap::new(),
            changes: mpsc::unbounded_channel(),
            watching: std::collections::HashSet::new(),
            announced: std::collections::HashSet::new(),
        }
    }

    /// Which trusted peer a rendezvous identifier belongs to.
    ///
    /// The identifier is blinded — derived from the master key and the peer's
    /// fingerprint — so the rendezvous service cannot link it to a device, and
    /// neither can anyone without the key. Recovering the fingerprint means
    /// re-deriving the identifier for each peer we trust and looking for a
    /// match. That is a handful of hashes against a list of a person's own
    /// devices, not a search.
    fn member(&self, master: &MasterKey, id: qurb_signal::MemberId) -> Option<Fingerprint> {
        self.known
            .iter()
            .copied()
            .find(|f| *qurb_signal::MemberId::derive(master, f.as_bytes()).as_bytes() == *id.as_bytes())
    }

    /// Whether this peer is already known.
    fn knows(&self, peer: &Fingerprint) -> bool {
        self.known.contains(peer)
    }

    /// Adopt a new set of trusted peers.
    ///
    /// Connections to peers no longer trusted are dropped rather than left
    /// open: forgetting a device should stop it syncing now, not at whatever
    /// point the connection happens to fail.
    fn learn(&mut self, current: &[Fingerprint]) {
        self.known = current.to_vec();
        // Closed, not only dropped: the peer's watcher holds a reference of its
        // own, so dropping this one would leave the connection open.
        self.connections.retain(|peer, client| {
            let trusted = current.contains(peer);
            if !trusted {
                client.disconnect("no longer trusted");
            }
            trusted
        });
        self.watching.retain(|peer| current.contains(peer));
        self.backoff.retain(|peer, _| current.contains(peer));
        self.announced.retain(|peer| current.contains(peer));
    }

    /// Forget any waiting period for this peer.
    ///
    /// Called when a peer is known to be reachable, which makes the reason for
    /// waiting obsolete. Without it a device that has just announced itself
    /// would still be ignored for up to two minutes.
    fn ready_now(&mut self, peer: Fingerprint) {
        self.backoff.remove(&peer);
        self.announced.insert(peer);
    }

    /// Wait until some peer reports a change.
    ///
    /// Never returns when no peer is connected, which is right: there is
    /// nothing to hear, and the sweep timer covers it.
    async fn next_change(&mut self) -> Fingerprint {
        match self.changes.1.recv().await {
            Some(peer) => peer,
            // The sender is held by this struct, so this cannot happen -- but
            // returning would busy-loop the select, so wait instead.
            None => std::future::pending().await,
        }
    }

    /// Hold a request open against a peer, reporting whenever it is answered.
    fn watch(&mut self, peer: Fingerprint, client: Arc<PeerClient>) {
        if !self.watching.insert(peer) {
            return;
        }
        let announce = self.changes.0.clone();
        tokio::spawn(async move {
            let mut seen = 0u64;
            loop {
                match client.wait_for_change(seen).await {
                    Ok(generation) => {
                        // Report even when the wait merely timed out: it costs
                        // one comparison at the other end and covers the case
                        // where a notification was lost with a connection.
                        seen = generation;
                        if announce.send(peer).is_err() {
                            return;
                        }
                    }
                    // The connection has gone. The daemon will notice when it
                    // next tries to sync, and start a new watcher then.
                    Err(_) => return,
                }
            }
        });
    }

    fn stop_watching(&mut self, peer: Fingerprint) {
        self.watching.remove(&peer);
    }

    /// Peers worth trying now.
    fn ready(&self) -> Vec<Fingerprint> {
        let now = tokio::time::Instant::now();
        self.known
            .iter()
            .filter(|peer| self.backoff.get(peer).is_none_or(|(at, _)| *at <= now))
            .copied()
            .collect()
    }

    fn connection(&self, peer: Fingerprint) -> Option<Arc<PeerClient>> {
        self.connections.get(&peer).cloned()
    }

    /// Drop connections that have ended, so the next attempt makes a new one
    /// and nothing reports a device as connected that is not.
    fn forget_closed(&mut self) {
        let closed: Vec<Fingerprint> = self
            .connections
            .iter()
            .filter(|(_, client)| client.is_closed())
            .map(|(peer, _)| *peer)
            .collect();
        for peer in closed {
            self.disconnected(peer);
        }
    }

    /// Every connection held, and how it runs.
    fn links(&self) -> Vec<crate::status::Link> {
        let mut links: Vec<_> = self
            .connections
            .iter()
            .map(|(peer, client)| crate::status::Link {
                fingerprint: peer.short(),
                relayed: client.is_relayed(),
                address: client.remote_address().to_string(),
            })
            .collect();
        links.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
        links
    }

    fn any_connected(&self) -> bool {
        !self.connections.is_empty()
    }

    fn connected(&mut self, peer: Fingerprint, client: Arc<PeerClient>) {
        self.connections.insert(peer, client);
    }

    fn disconnected(&mut self, peer: Fingerprint) {
        self.connections.remove(&peer);
        self.stop_watching(peer);
    }

    fn succeeded(&mut self, peer: Fingerprint) {
        self.backoff.remove(&peer);
        self.announced.remove(&peer);
    }

    /// Record a failure. Returns whether the peer had just been announced as
    /// there -- in which case the failure is news rather than routine.
    fn failed(&mut self, peer: Fingerprint) -> bool {
        let expected = self.announced.remove(&peer);
        self.connections.remove(&peer);
        self.stop_watching(peer);
        let wait = match self.backoff.get(&peer) {
            Some((_, previous)) => (*previous * 2).min(MAX_RETRY),
            None => FIRST_RETRY,
        };
        self.backoff.insert(peer, (tokio::time::Instant::now() + wait, wait));
        expected
    }
}

/// Warn about paths a case-insensitive filesystem could not keep apart.
///
/// Harmless here and destructive as soon as a phone joins, which is exactly why
/// it is worth saying now rather than when it happens.
fn report_collisions(engine: &Engine) {
    match engine.case_collisions() {
        Ok(groups) => {
            for group in groups {
                tracing::warn!(
                    paths = ?group,
                    "these differ only in case; a macOS or Windows device cannot hold both"
                );
            }
        }
        Err(e) => tracing::debug!(error = %e, "could not check for case collisions"),
    }
}

/// What one pass counted, for an interface to display.
struct Counted {
    files: usize,
    on_disk: u64,
    peers: usize,
    used: u64,
    limit: u64,
}

#[cfg(test)]
mod tests {
    /// A folder is followed by what it is, not what it is called: moved
    /// away, or replaced by a new one at the same path, it is not the folder
    /// the daemon started on.
    #[test]
    fn a_folder_moved_away_or_replaced_is_not_the_same_folder() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("qurb/.qurb");
        std::fs::create_dir_all(&store).unwrap();
        let here = super::Here::of(&store).unwrap();
        assert!(here.still(&store));

        // To the Trash.
        let trash = dir.path().join("Trash");
        std::fs::create_dir_all(&trash).unwrap();
        std::fs::rename(dir.path().join("qurb"), trash.join("qurb")).unwrap();
        assert!(!here.still(&store), "a folder moved away still counted as here");

        // And something new made where it was.
        std::fs::create_dir_all(&store).unwrap();
        assert!(!here.still(&store), "a new folder at the same path counted as the old one");
    }

    use super::*;
    use qurb_engine::Progress;

    fn channel() -> (crate::status::Publisher, crate::status::Watcher) {
        crate::status::channel(crate::status::Status::starting("/q".into(), "abcd".into()))
    }

    /// A transfer is shown while it moves and gone the moment it stops, with
    /// its bytes counted -- including those that arrived since the last time
    /// anything was published.
    #[test]
    fn a_transfer_appears_moves_and_goes() {
        let (publisher, watcher) = channel();
        let mut progress = Reporting::new(Some(&publisher), "phone".into());

        progress.started("video.mp4", 1000);
        let shown = watcher.borrow().incoming.clone();
        assert_eq!(shown.len(), 1);
        assert_eq!(
            (shown[0].path.as_str(), shown[0].device.as_str(), shown[0].size, shown[0].done),
            ("video.mp4", "phone", 1000, 0)
        );

        // Held back while inside the publishing interval...
        progress.advanced(300);
        assert_eq!(watcher.borrow().incoming[0].done, 0, "published faster than it should");

        // ...and carried into the next publish rather than lost.
        progress.published_at -= PUBLISH_EVERY;
        progress.advanced(200);
        assert_eq!(watcher.borrow().incoming[0].done, 500);

        progress.finished("video.mp4");
        assert!(watcher.borrow().incoming.is_empty(), "a finished transfer stayed on screen");
    }

    /// Two devices sending at once are two transfers, and one finishing does
    /// not take the other off the screen.
    #[test]
    fn transfers_from_two_devices_are_kept_apart() {
        let (publisher, watcher) = channel();
        let mut phone = Reporting::new(Some(&publisher), "phone".into());
        let mut tablet = Reporting::new(Some(&publisher), "tablet".into());

        phone.started("a.jpg", 10);
        tablet.started("a.jpg", 20);
        phone.finished("a.jpg");

        let shown = watcher.borrow().incoming.clone();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].device, "tablet");
    }

    /// A peer asks for chunks, never files. Each one served is traced to the
    /// send it belongs to and added up; a chunk of a shared file is not a send
    /// and shows nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_send_is_traced_from_the_chunks_served() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        std::fs::create_dir_all(&root).unwrap();
        let mut store = qurb_storage::Store::open(
            &root.join(".qurb"),
            qurb_storage::ChunkKey::from_bytes([4; 32]),
        )
        .unwrap()
        .in_tree(&root);

        let phone = qurb_sync::DeviceId::from_bytes([8; 32]);
        let phone_cert = Fingerprint::from_bytes([9; 32]);
        store.db().trust_peer(&phone, phone_cert.as_bytes(), "phone").unwrap();

        let video: Vec<u8> =
            (0..600_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
        let loose = dir.path().join("video.mp4");
        std::fs::write(&loose, &video).unwrap();
        store.send_to_vault("video.mp4", &loose, &phone).unwrap();
        let chunks = store.chunk_hashes_for_content(&blake3::hash(&video)).unwrap().unwrap();

        std::fs::write(root.join("shared.txt"), b"everybody's").unwrap();
        store.put_file("shared.txt", &root.join("shared.txt")).unwrap();
        let shared = store.db().file_by_path("shared.txt").unwrap().unwrap();
        let shared_chunk = store.db().chunk_hashes_for(shared.id).unwrap()[0];

        let (publisher, watcher) = channel();
        let sending = Sending { store: Arc::new(Mutex::new(store)), status: publisher };

        qurb_peer::Served::served(&sending, &phone_cert, &shared_chunk, 11);
        assert!(watcher.borrow().outgoing.is_empty(), "a shared file was shown as a send");

        let mut total = 0;
        for chunk in &chunks {
            let bytes = 600_000 / chunks.len() as u64;
            qurb_peer::Served::served(&sending, &phone_cert, chunk, bytes);
            total += bytes;
        }
        let shown = watcher.borrow().outgoing.clone();
        assert_eq!(shown.len(), 1);
        assert_eq!((shown[0].path.as_str(), shown[0].device.as_str()), ("video.mp4", "phone"));
        assert_eq!((shown[0].size, shown[0].done), (600_000, total));
    }

    /// A device removed from the trust store is let go of: its connection
    /// closed, not merely forgotten -- the peer's watcher holds a reference of
    /// its own, and a connection kept open by it would keep syncing.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_removed_device_is_disconnected() {
        let dir = tempfile::tempdir().unwrap();
        let (server_dir, client_dir) = (dir.path().join("server"), dir.path().join("client"));
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::create_dir_all(&client_dir).unwrap();
        let server_id = Identity::load_or_create(&server_dir).unwrap();
        let client_id = Identity::load_or_create(&client_dir).unwrap();
        let store = qurb_storage::Store::open(
            &server_dir.join("store"),
            qurb_storage::ChunkKey::from_bytes([6; 32]),
        )
        .unwrap();
        let server = qurb_peer::PeerServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            &server_id,
            &qurb_peer::tls::TrustList::new(vec![client_id.fingerprint()]),
        )
        .unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });

        let peer = server_id.fingerprint();
        let client = Arc::new(PeerClient::connect(addr, &client_id, peer).await.unwrap());
        let mut peers = Peers::new(vec![peer]);
        peers.connected(peer, Arc::clone(&client));
        peers.watch(peer, Arc::clone(&client));

        peers.learn(&[]);
        assert!(!peers.knows(&peer));
        assert!(peers.links().is_empty());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !client.is_closed() {
            assert!(std::time::Instant::now() < deadline, "the connection was left open");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// A device shown as connected is one a connection is actually held to,
    /// with the route it takes, and stops being shown the moment that
    /// connection ends -- not at the next sync pass, which may be minutes away.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_that_ends_stops_being_shown() {
        let dir = tempfile::tempdir().unwrap();
        let (server_dir, client_dir) = (dir.path().join("server"), dir.path().join("client"));
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::create_dir_all(&client_dir).unwrap();
        let server_id = Identity::load_or_create(&server_dir).unwrap();
        let client_id = Identity::load_or_create(&client_dir).unwrap();

        let store = qurb_storage::Store::open(
            &server_dir.join("store"),
            qurb_storage::ChunkKey::from_bytes([6; 32]),
        )
        .unwrap();
        let server = qurb_peer::PeerServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            &server_id,
            &qurb_peer::tls::TrustList::new(vec![client_id.fingerprint()]),
        )
        .unwrap();
        let addr = server.local_addr().unwrap();
        let server = Arc::new(server);
        let serving = Arc::clone(&server);
        tokio::spawn(async move { serving.serve(Arc::new(Mutex::new(store))).await });

        let peer = server_id.fingerprint();
        let client = PeerClient::connect(addr, &client_id, peer).await.unwrap();
        client.tree().await.unwrap();

        let mut peers = Peers::new(vec![peer]);
        peers.connected(peer, Arc::new(client));
        let links = peers.links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].fingerprint, peer.short());
        assert!(!links[0].relayed, "a direct connection was shown as relayed");

        // The other device goes away.
        server.close();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            peers.forget_closed();
            if peers.links().is_empty() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "a closed connection was still shown");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// A failure to reach a device the rendezvous service has just announced
    /// is news, once; the same device failing on a later sweep, when nobody
    /// said it was there, is routine again.
    #[test]
    fn only_an_announced_device_failing_is_news() {
        let peer = Fingerprint::from_bytes([3; 32]);
        let mut peers = Peers::new(vec![peer]);

        assert!(!peers.failed(peer), "a device nobody announced is expected to be off");
        peers.ready_now(peer);
        assert!(peers.failed(peer), "announced, then not reached: that is news");
        assert!(!peers.failed(peer), "and only the once");

        peers.ready_now(peer);
        peers.succeeded(peer);
        assert!(!peers.failed(peer), "an announcement is spent by reaching it");
    }

    /// With nobody displaying anything there is nothing to publish to, and
    /// reporting must cost nothing and fail at nothing.
    #[test]
    fn nobody_watching_is_fine() {
        let mut progress = Reporting::new(None, "phone".into());
        progress.started("x", 1);
        progress.advanced(1);
        progress.finished("x");
    }
}
