//! Whether there is a device yet, and the daemon once there is.
//!
//! The window has to exist before the device does. That is the whole reason
//! this module is not just a field on a struct: the previous version opened the
//! key on the way up and refused to start without one, which is correct for a
//! terminal and useless for the screen whose job is to create the key.
//!
//! So the window opens in one of two situations and can move from the first to
//! the second exactly once:
//!
//! - **Unmade.** No key in the folder. Only the setting-up commands answer.
//! - **Running.** A key, a store to query, and a daemon publishing what it is
//!   doing.
//!
//! # No recovery phrase passes through here
//!
//! A new device is not asked to write its 24 words down any more (decision
//! 0052), so nothing holds them between screens. Settings can still show them,
//! derived from the key on demand and never kept.

use anyhow::{Context, Result};
use qurb_cli::status::{Status, Watcher};
use qurb_cli::{store_dir, Daemon};
use qurb_peer::Identity;
use qurb_storage::{ChunkKey, Store};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A device that exists and is being synced.
pub struct Running {
    /// The window's own handle on the index. SQLite in WAL mode allows a reader
    /// alongside the daemon's writer, which is what lets a folder be browsed
    /// while it is being synced.
    pub store: Store,
    /// The daemon's live account of itself.
    pub status: Watcher,
    /// Kept so that pairing can open a store of its own. Pairing *writes* --
    /// it records a peer -- and handing it the window's read handle would mean
    /// passing a `&Store` where an owned one is needed.
    pub key: ChunkKey,
    /// What a device joining with this one's code is given, so that it
    /// becomes another of the same person's devices with nothing typed
    /// (decision 0052). Held only while running, as the daemon holds it.
    pub master: qurb_keys::MasterKey,
}

/// How a pairing attempt is going.
#[derive(Debug, Clone)]
pub enum Pairing {
    /// The code is on the screen and nobody has used it yet.
    Waiting,
    /// A device presented the right token.
    Paired { name: String, fingerprint: String },
    /// Five minutes passed. The code is dead and a new one is needed.
    Expired,
    Failed(String),
}

/// A code on the screen, and the wait behind it.
pub struct Attempt {
    pub code: String,
    /// The same invite in a form somebody can read down a telephone.
    pub spoken: String,
    pub expires_at: i64,
    state: Mutex<Pairing>,
    /// Aborted when the attempt is called off, which closes the socket and
    /// kills the code. A cancelled invite that still worked would be worse
    /// than no cancel button.
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Attempt {
    pub fn state(&self) -> Pairing {
        self.state.lock().expect("pairing state").clone()
    }

    pub fn settle(&self, outcome: Pairing) {
        *self.state.lock().expect("pairing state") = outcome;
    }

    pub fn watch(&self, task: tauri::async_runtime::JoinHandle<()>) {
        *self.task.lock().expect("pairing task") = Some(task);
    }

    pub fn call_off(&self) {
        if let Some(task) = self.task.lock().expect("pairing task").take() {
            task.abort();
        }
    }
}

/// Everything a command needs, handed to Tauri as managed state.
pub struct Hosted {
    root: Mutex<PathBuf>,
    running: Mutex<Option<Running>>,
    /// The one pairing attempt at a time. A second code would mean two live
    /// invites to the same device, and only one of them could be the one on
    /// the screen.
    pairing: Mutex<Option<Arc<Attempt>>>,
    /// Tells the daemon to look now, when the window has just paired a device
    /// or sent a file, instead of at its next slow check.
    nudge: Arc<tokio::sync::Notify>,
}

impl Hosted {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: Mutex::new(root),
            running: Mutex::new(None),
            pairing: Mutex::new(None),
            nudge: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Have the daemon look for new pairings and sends now. Kept if the daemon
    /// is busy, so a nudge made during a sync is acted on after it.
    pub fn nudge(&self) {
        self.nudge.notify_one();
    }

    /// The same, for a task that outlives the call that started it.
    pub fn nudger(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.nudge)
    }

    pub fn root(&self) -> PathBuf {
        self.root.lock().expect("root").clone()
    }

    /// Point at a different folder. Only meaningful before the daemon starts:
    /// afterwards the daemon is watching the old one and this would be a lie.
    pub fn aim_at(&self, root: PathBuf) -> Result<()> {
        if self.is_running() {
            anyhow::bail!("this device is already set up");
        }
        *self.root.lock().expect("root") = root;
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.lock().expect("session").is_some()
    }

    /// Do something with the store, or say why there is none.
    pub fn with_store<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        let guard = self.running.lock().expect("session");
        let running = guard.as_ref().context("this device is not set up yet")?;
        f(&running.store)
    }

    /// The same, for the few things that write.
    ///
    /// Sending is the only one: it chunks a file and records it, which the
    /// window does directly rather than asking the daemon to, because the
    /// daemon is busy serving peers and a large file must not stop it. WAL mode
    /// is what makes a second writer safe.
    pub fn with_store_mut<T>(&self, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        let mut guard = self.running.lock().expect("session");
        let running = guard.as_mut().context("this device is not set up yet")?;
        f(&mut running.store)
    }

    /// The daemon's latest published state, if it is running.
    pub fn status(&self) -> Option<Status> {
        let guard = self.running.lock().expect("session");
        guard.as_ref().map(|r| r.status.borrow().clone())
    }

    /// The configured storage allowance, freshly read.
    ///
    /// From the file rather than from a cached value, because `qurb config` can
    /// change it while the window is open and a stale number would make the
    /// storage screen quietly wrong.
    pub fn limit(&self) -> u64 {
        qurb_cli::Config::load(&store_dir(&self.root())).map(|c| c.limit).unwrap_or(0)
    }

    /// Open the key, start the daemon, and begin answering the rest of the
    /// commands.
    ///
    /// Idempotent in the harmless direction: asked twice, the second does
    /// nothing rather than starting a second daemon on one folder — which the
    /// advisory lock would refuse anyway, but later and with a worse message.
    pub fn start(&self, passphrase: impl FnOnce() -> Result<String>) -> Result<()> {
        if self.is_running() {
            return Ok(());
        }

        let root = self.root();
        let (master, identity, store, config) = qurb_cli::open_with(&root, passphrase)
            .with_context(|| format!("opening {}", root.display()))?;
        let key = store.chunk_key();
        let held = master.clone();
        drop(store);

        let (publisher, watcher) = qurb_cli::status::channel(Status::starting(
            root.clone(),
            identity.fingerprint().short(),
        ));

        // The daemon owns a tokio runtime on its own threads; the window owns
        // the main thread, because every windowing system requires its event
        // loop there.
        let daemon_root = root.clone();
        let daemon_store_dir = store_dir(&root);
        let nudge = self.nudger();
        std::thread::Builder::new()
            .name("qurb-daemon".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build()
                {
                    Ok(runtime) => runtime,
                    Err(e) => {
                        tracing::error!(error = %e, "could not start the runtime");
                        return;
                    }
                };
                let daemon = Daemon::new(&daemon_root, &daemon_store_dir, master, identity, config)
                    .reporting_to(publisher)
                    .nudged_by(nudge);
                if let Err(e) = runtime.block_on(daemon.run()) {
                    tracing::error!(error = %e, "the daemon stopped");
                }
            })
            .context("starting the daemon thread")?;

        // Attached to the folder, so that a file the daemon materialised reads
        // as materialised here too: a reader without the tree would call every
        // synced file "not here".
        let reader = Store::open(&store_dir(&root), key.clone())
            .context("opening the index for the window")?
            .in_tree(&root);

        *self.running.lock().expect("session") =
            Some(Running { store: reader, status: watcher, key, master: held });
        Ok(())
    }

    /// The key a device joining with this one's code is given.
    pub fn master(&self) -> Result<qurb_keys::MasterKey> {
        let guard = self.running.lock().expect("session");
        Ok(guard.as_ref().context("this device is not set up yet")?.master.clone())
    }

    /// Everything pairing needs: a store of its own, an identity, and a name.
    ///
    /// A separate handle rather than the window's, because pairing records a
    /// peer and therefore writes. WAL mode makes a second writer safe; the
    /// daemon is already one.
    pub fn for_pairing(&self) -> Result<(Arc<std::sync::Mutex<Store>>, Identity, String)> {
        let root = self.root();
        let dir = store_dir(&root);
        let key = {
            let guard = self.running.lock().expect("session");
            guard.as_ref().context("this device is not set up yet")?.key.clone()
        };

        // Checked, not assumed: a folder moved away while the window was open
        // is not set up any more, and opening it by path made a new store and
        // a new identity in its place, which a phone then paired with.
        anyhow::ensure!(
            qurb_cli::is_set_up(&root),
            "{} is not set up any more -- it may have been moved or deleted",
            root.display()
        );
        let store = Store::open(&dir, key)?.in_tree(&root);
        let identity = Identity::load(&dir)?;
        let name = qurb_cli::Config::load(&dir).map(|c| c.name).unwrap_or_default();
        Ok((Arc::new(std::sync::Mutex::new(store)), identity, name))
    }

    /// Begin an attempt, replacing and cancelling any already running.
    pub fn begin_pairing(&self, code: String, spoken: String, expires_at: i64) -> Arc<Attempt> {
        let attempt = Arc::new(Attempt {
            code,
            spoken,
            expires_at,
            state: Mutex::new(Pairing::Waiting),
            task: Mutex::new(None),
        });
        let mut slot = self.pairing.lock().expect("pairing");
        if let Some(previous) = slot.replace(Arc::clone(&attempt)) {
            previous.call_off();
        }
        attempt
    }

    pub fn attempt(&self) -> Option<Arc<Attempt>> {
        self.pairing.lock().expect("pairing").clone()
    }

    /// Stop showing a code and stop answering it.
    pub fn end_pairing(&self) {
        if let Some(attempt) = self.pairing.lock().expect("pairing").take() {
            attempt.call_off();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    fn attempt(hosted: &Hosted) -> Arc<Attempt> {
        hosted.begin_pairing("qurb1-code".into(), "kilo seven".into(), 0)
    }

    #[test]
    fn an_attempt_starts_out_waiting() {
        let hosted = Hosted::new(PathBuf::from("/nowhere"));
        assert!(matches!(attempt(&hosted).state(), Pairing::Waiting));
        assert!(hosted.attempt().is_some());
    }

    /// Two live codes for one device would mean only one of them is the one on
    /// the screen, and no way for somebody holding the other to know.
    #[test]
    fn a_second_code_replaces_the_first() {
        let hosted = Hosted::new(PathBuf::from("/nowhere"));
        let first = attempt(&hosted);
        let second = hosted.begin_pairing("qurb1-other".into(), "romeo two".into(), 0);

        assert_eq!(hosted.attempt().unwrap().code, second.code);
        assert_ne!(first.code, second.code);
    }

    #[test]
    fn ending_an_attempt_leaves_nothing_to_report() {
        let hosted = Hosted::new(PathBuf::from("/nowhere"));
        attempt(&hosted);
        hosted.end_pairing();
        assert!(hosted.attempt().is_none());
    }

    #[test]
    fn an_outcome_is_what_gets_reported() {
        let hosted = Hosted::new(PathBuf::from("/nowhere"));
        let live = attempt(&hosted);
        live.settle(Pairing::Paired { name: "phone".into(), fingerprint: "a1b2c3d4".into() });

        match hosted.attempt().unwrap().state() {
            Pairing::Paired { name, fingerprint } => {
                assert_eq!(name, "phone");
                assert_eq!(fingerprint, "a1b2c3d4");
            }
            other => panic!("expected a pairing, got {other:?}"),
        }
    }

    /// Pairing needs a store, and there is none until the device exists.
    #[test]
    fn pairing_before_there_is_a_device_is_refused() {
        let hosted = Hosted::new(PathBuf::from("/nowhere"));
        assert!(hosted.for_pairing().is_err());
    }

    #[test]
    fn a_folder_cannot_be_changed_once_the_daemon_is_watching_one() {
        let hosted = Hosted::new(PathBuf::from("/one"));
        assert!(hosted.aim_at(PathBuf::from("/two")).is_ok());
        assert_eq!(hosted.root(), PathBuf::from("/two"));
    }
}
