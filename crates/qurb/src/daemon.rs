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
use qurb_engine::Engine;
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

pub struct Daemon {
    root: PathBuf,
    store_dir: PathBuf,
    master: MasterKey,
    identity: Identity,
    config: Config,
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
        }
    }

    fn chunk_key(&self) -> ChunkKey {
        ChunkKey::from_bytes(self.master.derive(Purpose::ChunkEncryption).to_bytes())
    }

    fn open_store(&self) -> Result<Store> {
        Store::open(&self.store_dir, self.chunk_key())
            .with_context(|| format!("opening the store at {}", self.store_dir.display()))
    }

    fn engine(&self) -> Result<Engine> {
        Ok(Engine::new(
            self.root.clone(),
            self.open_store()?,
            IgnoreRules::new().with_store_dir(&self.store_dir),
        ))
    }

    pub async fn run(&self) -> Result<()> {
        let mut engine = self.engine()?;

        // The trust store answers who may connect, so a device paired after
        // this point needs a restart to be let in. Stated as a limitation
        // rather than hidden: it is the same one `PeerServer::bind_trusting`
        // has, and fixing it properly means a listener that can be reconfigured.
        let trusted: Vec<Fingerprint> =
            qurb_peer::trusted_fingerprints(engine.store())?;
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
        report_collisions(&engine);

        let connector = Arc::new(
            Connector::start(
                format!("0.0.0.0:{}", self.config.port).parse()?,
                self.identity.clone(),
                self.master.clone(),
                &trusted,
                &self.config.signal,
                true,
                self.config.relay,
            )
            .await
            .context("starting the connection machinery")?,
        );
        tracing::info!(
            address = %connector.local_addr()?,
            public = ?connector.endpoints().public,
            relay = ?self.config.relay,
            "listening"
        );

        // How far this device's own state has got. Peers hold a request open
        // against it, so they hear about a change within a round trip rather
        // than whenever they next think to ask.
        let generation = qurb_peer::Generation::new();

        // Serve peers on every path we have. A device unreachable by relay is
        // unreachable by anyone whose direct attempt failed.
        let served = Arc::new(Mutex::new(self.open_store()?));
        for endpoint in [Some(connector.endpoint().clone()), connector.relay_endpoint().cloned()]
            .into_iter()
            .flatten()
        {
            let store = Arc::clone(&served);
            let generation = Arc::clone(&generation);
            tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let store = Arc::clone(&store);
                    let generation = Arc::clone(&generation);
                    tokio::spawn(async move {
                        if let Ok(connection) = incoming.await {
                            qurb_peer::server::serve_connection(connection, store, generation)
                                .await;
                        }
                    });
                }
            });
        }

        let mut watcher = Watcher::start(
            &self.root,
            IgnoreRules::new().with_store_dir(&self.store_dir),
            DebounceConfig::default(),
        )
        .context("watching the directory")?;

        let mut peers = Peers::new(trusted);
        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;

        let mut timer = tokio::time::interval(SWEEP_INTERVAL);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                event = watcher.next() => match event {
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
                        self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                    }

                    None => {
                        tracing::error!("the watcher stopped");
                        return Ok(());
                    }
                },

                _ = timer.tick() => {
                    self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                }

                // A peer said it changed. This is how news travels now; the
                // timer above is only the backstop.
                peer = peers.next_change() => {
                    tracing::debug!(peer = %peer.short(), "peer reports a change");
                    self.sync_all(&mut engine, &connector, &mut peers, &generation).await;
                }
            }
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
        for peer in peers.ready() {
            match self.sync_one(engine, connector, peers, peer).await {
                Ok(moved) => {
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
                        generation.bump();
                    }
                }
                Err(e) => {
                    // Not an error worth stopping for. A peer that is switched
                    // off is the normal case in a system built on the
                    // assumption that devices are not always on.
                    tracing::debug!(peer = %peer.short(), error = %e, "could not sync");
                    peers.failed(peer);
                }
            }
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

        let plan = engine.plan_against(&tree)?;
        if plan.is_empty() {
            return Ok(0);
        }

        let reader = self.open_store()?;
        let outcome = tokio::task::block_in_place(|| {
            let mut source = qurb_peer::NetworkSource::new(&client, &reader);
            engine.apply_plan(&plan, &mut source)
        })?;

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
}

impl Peers {
    fn new(known: Vec<Fingerprint>) -> Self {
        Self {
            known,
            connections: HashMap::new(),
            backoff: HashMap::new(),
            changes: mpsc::unbounded_channel(),
            watching: std::collections::HashSet::new(),
        }
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

    fn connected(&mut self, peer: Fingerprint, client: Arc<PeerClient>) {
        self.connections.insert(peer, client);
    }

    fn disconnected(&mut self, peer: Fingerprint) {
        self.connections.remove(&peer);
        self.stop_watching(peer);
    }

    fn succeeded(&mut self, peer: Fingerprint) {
        self.backoff.remove(&peer);
    }

    fn failed(&mut self, peer: Fingerprint) {
        self.connections.remove(&peer);
        self.stop_watching(peer);
        let wait = match self.backoff.get(&peer) {
            Some((_, previous)) => (*previous * 2).min(MAX_RETRY),
            None => FIRST_RETRY,
        };
        self.backoff.insert(peer, (tokio::time::Instant::now() + wait, wait));
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
