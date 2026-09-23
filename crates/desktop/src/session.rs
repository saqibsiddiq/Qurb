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
//! # The recovery phrase lives here, briefly
//!
//! Between being created and being confirmed, the phrase is held in
//! [`Hosted::pending`] rather than in the window, so that the page can drop its
//! copy the moment it has drawn it and confirmation can be checked without the
//! words being sent back and forth. It is dropped as soon as the person says
//! they have written it down, and it is never written to disk or to a log.

use anyhow::{Context, Result};
use qurb_cli::status::{Status, Watcher};
use qurb_cli::{store_dir, Daemon};
use qurb_keys::RecoveryPhrase;
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
    /// A phrase that has been shown and not yet confirmed. See the module note.
    pending: Mutex<Option<RecoveryPhrase>>,
    /// The one pairing attempt at a time. A second code would mean two live
    /// invites to the same device, and only one of them could be the one on
    /// the screen.
    pairing: Mutex<Option<Arc<Attempt>>>,
}

impl Hosted {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: Mutex::new(root),
            running: Mutex::new(None),
            pending: Mutex::new(None),
            pairing: Mutex::new(None),
        }
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

    pub fn hold_phrase(&self, phrase: RecoveryPhrase) {
        *self.pending.lock().expect("pending") = Some(phrase);
    }

    /// Check some of the words against the phrase being held, without the
    /// window having to keep a copy to compare against.
    ///
    /// `answers` are one-based positions, as they are shown. Everything is
    /// compared lower-cased and trimmed: people retype from paper, and refusing
    /// a capital letter would be refusing a correct answer.
    pub fn phrase_matches(&self, answers: &[(usize, String)]) -> bool {
        let guard = self.pending.lock().expect("pending");
        let Some(phrase) = guard.as_ref() else { return false };
        let words = phrase.words();

        !answers.is_empty()
            && answers.iter().all(|(position, given)| {
                words
                    .get(position.wrapping_sub(1))
                    .is_some_and(|word| word.eq_ignore_ascii_case(given.trim()))
            })
    }

    /// Show the phrase being held. Only while one is.
    pub fn pending_words(&self) -> Option<Vec<String>> {
        let guard = self.pending.lock().expect("pending");
        guard.as_ref().map(|p| p.words().to_vec())
    }

    /// Forget it. Called as soon as it has served its purpose, which is the
    /// only reason it was ever held.
    pub fn forget_phrase(&self) {
        *self.pending.lock().expect("pending") = None;
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
                    .reporting_to(publisher);
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
            Some(Running { store: reader, status: watcher, key });
        Ok(())
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

        let store = Store::open(&dir, key)?.in_tree(&root);
        let identity = Identity::load_or_create(&dir)?;
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
    use qurb_keys::MasterKey;

    fn holding() -> (Hosted, Vec<String>) {
        let hosted = Hosted::new(PathBuf::from("/nowhere"));
        let phrase = MasterKey::from_bytes([7; 32]).to_phrase();
        let words = phrase.words().to_vec();
        hosted.hold_phrase(phrase);
        (hosted, words)
    }

    #[test]
    fn the_right_words_at_the_right_positions_are_accepted() {
        let (hosted, words) = holding();
        let answers =
            vec![(1, words[0].clone()), (12, words[11].clone()), (24, words[23].clone())];
        assert!(hosted.phrase_matches(&answers));
    }

    /// Retyped from paper, where nobody records the capitalisation or how much
    /// space they left. Refusing these would be refusing a correct answer.
    #[test]
    fn capitals_and_surrounding_space_do_not_matter() {
        let (hosted, words) = holding();
        let answers = vec![(1, format!("  {}  ", words[0].to_uppercase()))];
        assert!(hosted.phrase_matches(&answers));
    }

    #[test]
    fn a_right_word_at_the_wrong_position_is_refused() {
        let (hosted, words) = holding();
        // The order is part of the key, so this has to fail even though every
        // word given is one of the twenty-four.
        assert!(!hosted.phrase_matches(&[(2, words[0].clone())]));
    }

    #[test]
    fn one_wrong_answer_fails_the_whole_check() {
        let (hosted, words) = holding();
        let answers = vec![(1, words[0].clone()), (2, "rhubarb".to_string())];
        assert!(!hosted.phrase_matches(&answers));
    }

    /// Otherwise "all of nothing matched" would be a way past the step.
    #[test]
    fn answering_nothing_is_not_answering_correctly() {
        let (hosted, _) = holding();
        assert!(!hosted.phrase_matches(&[]));
    }

    #[test]
    fn a_position_outside_the_phrase_is_refused_rather_than_panicking() {
        let (hosted, words) = holding();
        assert!(!hosted.phrase_matches(&[(0, words[0].clone())]));
        assert!(!hosted.phrase_matches(&[(25, words[0].clone())]));
    }

    /// Once forgotten, nothing matches: there is no phrase to match against,
    /// and treating "no phrase" as "everything is correct" would be the worst
    /// possible reading of an empty option.
    #[test]
    fn nothing_matches_once_the_phrase_is_forgotten() {
        let (hosted, words) = holding();
        hosted.forget_phrase();
        assert!(!hosted.phrase_matches(&[(1, words[0].clone())]));
        assert!(hosted.pending_words().is_none());
    }

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
