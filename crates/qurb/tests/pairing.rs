//! Pairing the way an interface does it, rather than the way a command does.
//!
//! The difference is small and load-bearing. A command opens the store, pairs,
//! and exits. A window is already running a daemon on that folder — holding the
//! configured port and writing to the index — and has to pair *alongside* it.
//! These check the two things that makes necessary: a second store handle, and
//! a port of its own.

use qurb_peer::{Identity, PairingHost};
use qurb_storage::{ChunkKey, Store};
use std::path::Path;
use std::sync::{Arc, Mutex, Once};

fn isolated() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let scratch = std::env::temp_dir().join(format!("qurb-pair-tests-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).expect("scratch config directory");
        std::env::set_var("XDG_CONFIG_HOME", &scratch);
    });
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A set-up device, and a second handle on its store of the kind an interface
/// would open for pairing.
struct Device {
    _dir: tempfile::TempDir,
    identity: Identity,
    store: Arc<Mutex<Store>>,
}

impl Device {
    fn new(name: &str) -> Self {
        isolated();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(name);
        qurb_cli::setup::create(&root).unwrap();
        Self { _dir: dir, identity: Self::identity(&root), store: Self::store(&root) }
    }

    fn identity(root: &Path) -> Identity {
        Identity::load_or_create(&root.join(".qurb")).unwrap()
    }

    /// The handle a window opens for pairing: its own, alongside whatever else
    /// has the folder open.
    fn store(root: &Path) -> Arc<Mutex<Store>> {
        let key = qurb_keys::Vault::at(&root.join(".qurb")).unlock(None).unwrap();
        let chunk_key =
            ChunkKey::from_bytes(key.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes());
        Arc::new(Mutex::new(Store::open(&root.join(".qurb"), chunk_key).unwrap().in_tree(root)))
    }

    fn peers(&self) -> Vec<String> {
        self.store
            .lock()
            .unwrap()
            .db()
            .trusted_peers()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_pair_through_handles_of_their_own() {
    let host = Device::new("one");
    let guest = Device::new("two");

    // Port zero, which is what the window uses: the daemon in the same process
    // already has the configured one, and a pairing screen that could not open
    // while syncing was working would be useless every time somebody wanted it.
    let listener =
        PairingHost::open("127.0.0.1:0".parse().unwrap(), &host.identity, now()).unwrap();
    let code = listener.invite().encode();

    let host_store = Arc::clone(&host.store);
    let waiting = tokio::spawn(async move { listener.wait(host_store, "laptop", now()).await });

    let invite = qurb_peer::Invite::parse(&code).unwrap();
    let joined = qurb_peer::accept(&invite, &guest.identity, Arc::clone(&guest.store), "phone", now())
        .await
        .unwrap();
    let hosted = waiting.await.unwrap().unwrap();

    assert_eq!(joined.name, "laptop", "the guest learned the wrong name");
    assert_eq!(hosted.name, "phone", "the host learned the wrong name");

    // And both wrote it down, which is the point: the daemon's trust refresh
    // reads this and starts talking to the new device.
    assert_eq!(host.peers(), vec!["phone"]);
    assert_eq!(guest.peers(), vec!["laptop"]);
}

/// A window can open a code, throw it away, and open another. The old one must
/// stop working, or "stop showing it" is a button that only stops showing it.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_code_stops_working() {
    let host = Device::new("one");
    let guest = Device::new("two");

    let listener =
        PairingHost::open("127.0.0.1:0".parse().unwrap(), &host.identity, now()).unwrap();
    let code = listener.invite().encode();

    // What cancelling does: the task holding the host is aborted, so the host
    // is dropped and the socket closes.
    let host_store = Arc::clone(&host.store);
    let waiting = tokio::spawn(async move { listener.wait(host_store, "laptop", now()).await });
    waiting.abort();
    let _ = waiting.await;

    let invite = qurb_peer::Invite::parse(&code).unwrap();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        qurb_peer::accept(&invite, &guest.identity, Arc::clone(&guest.store), "phone", now()),
    )
    .await;

    // Either refused or never answered. Both are "that code is dead"; what
    // must not happen is a successful pairing.
    let paired = matches!(outcome, Ok(Ok(_)));
    assert!(!paired, "a cancelled code still paired");
    assert!(host.peers().is_empty(), "the host recorded a peer it had stopped waiting for");
}

/// An expired code is the ordinary ending, not a fault. It has to be
/// distinguishable from a failure, because the useful thing to say is "show a
/// new one" rather than "something went wrong".
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_code_says_so() {
    let host = Device::new("one");

    // Opened with a clock far enough in the past that the invite is already
    // dead when the wait begins.
    let listener =
        PairingHost::open("127.0.0.1:0".parse().unwrap(), &host.identity, now() - 10_000).unwrap();

    let outcome = listener.wait(Arc::clone(&host.store), "laptop", now()).await;
    assert!(
        matches!(outcome, Err(qurb_peer::Error::InviteExpired)),
        "expiry was reported as something else: {outcome:?}"
    );
}

/// The invite has to carry an address the far end can dial. A wildcard bind
/// reports `0.0.0.0`, which is true and useless.
#[tokio::test(flavor = "multi_thread")]
async fn the_code_carries_an_address_that_can_be_dialled() {
    let host = Device::new("one");
    let listener = PairingHost::open("0.0.0.0:0".parse().unwrap(), &host.identity, now()).unwrap();

    let address = listener.invite().address;
    assert!(!address.ip().is_unspecified(), "the invite says 0.0.0.0, which nobody can dial");
    assert_ne!(address.port(), 0, "the invite says port 0");
}
