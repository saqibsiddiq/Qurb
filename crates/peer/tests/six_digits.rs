//! Six digits to pair, on the same network (decision 0066).
//!
//! The device showing a code offers it on the local network; a device typing
//! the digits finds it, proves it knows them by SPAKE2, and is given the long
//! code, with which it pairs exactly as if it had scanned the QR code. These
//! run on one machine, the offers going round multicast loopback, each test
//! on a port of its own so that none hears another's.

use qurb_peer::short::find_on;
use qurb_peer::{accept, Error, Identity, Invite, Ours, PairingHost};
use qurb_storage::{ChunkKey, Store};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const NOW: i64 = 1_757_462_400;

static KEY: std::sync::LazyLock<qurb_keys::MasterKey> =
    std::sync::LazyLock::new(|| qurb_keys::MasterKey::from_bytes([42; 32]));

fn ours(name: &'static str) -> &'static Ours<'static> {
    Box::leak(Box::new(Ours { name, kind: "computer", key: &KEY }))
}

async fn approve_all(_: qurb_peer::Asking) -> bool {
    true
}

/// Every address: an offer comes from whichever address the machine sends
/// multicast from, and the listener has to answer there.
const ANY: &str = "0.0.0.0:0";

/// A port no other test offers on.
fn port() -> u16 {
    use rand::Rng;
    rand::thread_rng().gen_range(48_000..60_000)
}

struct Device {
    _dir: tempfile::TempDir,
    identity: Identity,
    store: Arc<Mutex<Store>>,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("store"), ChunkKey::from_bytes([42; 32])).unwrap();
        let identity = Identity::load_or_create(dir.path()).unwrap();
        Self { _dir: dir, identity, store: Arc::new(Mutex::new(store)) }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn six_digits_pair_two_devices() {
    let (host, joiner) = (Device::new(), Device::new());
    let port = port();
    let listener = PairingHost::open(ANY.parse().unwrap(), &host.identity, NOW).unwrap().offering_on(port);
    let digits = listener.digits().expect("digits while the code is shown");
    assert_eq!(digits.len(), 6);

    let waiting = listener.wait(Arc::clone(&host.store), ours("Desktop"), NOW, approve_all);
    let joining = async {
        let invite = find_on(port, &digits, &joiner.identity, Duration::from_secs(8)).await?;
        // The long code, and the device it names is the one that showed the
        // digits.
        assert_eq!(invite.fingerprint, host.identity.fingerprint());
        assert_eq!(&invite, &Invite { address: invite.address, ..listener.invite().clone() });
        accept(&invite, &joiner.identity, Arc::clone(&joiner.store), ours("Laptop"), NOW).await
    };
    let (host_saw, joiner_saw) = tokio::join!(waiting, joining);
    assert_eq!(host_saw.expect("host side").fingerprint, joiner.identity.fingerprint());
    assert_eq!(joiner_saw.expect("joiner side").fingerprint, host.identity.fingerprint());
}


/// Wrong digits are refused, and each wrong try is spent: after five the
/// digits stop working, the right ones included, while the code itself still
/// pairs.
#[tokio::test(flavor = "multi_thread")]
async fn wrong_digits_are_refused_and_five_withdraw_them() {
    let (host, joiner) = (Device::new(), Device::new());
    let port = port();
    let listener = PairingHost::open(ANY.parse().unwrap(), &host.identity, NOW).unwrap().offering_on(port);
    let digits = listener.digits().unwrap();
    let wrong = if digits == "000000" { "000001" } else { "000000" };

    let waiting = listener.wait(Arc::clone(&host.store), ours("Desktop"), NOW, approve_all);
    let trying = async {
        for _ in 0..qurb_peer::short::TRIES {
            let tried = find_on(port, wrong, &joiner.identity, Duration::from_secs(8)).await;
            assert!(matches!(tried, Err(Error::WrongDigits)), "{tried:?}");
        }
        assert_eq!(listener.digits(), None, "withdrawn after five");
        let tried = find_on(port, &digits, &joiner.identity, Duration::from_secs(4)).await;
        // Withdrawn, the offers stop, so the right digits find nobody.
        assert!(matches!(tried, Err(Error::NoneOffering | Error::DigitsWithdrawn)), "{tried:?}");
        // The QR code still works.
        accept(listener.invite(), &joiner.identity, Arc::clone(&joiner.store), ours("Laptop"), NOW).await
    };
    let (host_saw, joiner_saw) = tokio::join!(waiting, trying);
    assert!(host_saw.is_ok() && joiner_saw.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn with_nobody_offering_it_says_so() {
    let joiner = Device::new();
    let tried = find_on(port(), "123456", &joiner.identity, Duration::from_secs(2)).await;
    assert!(matches!(tried, Err(Error::NoneOffering)), "{tried:?}");
}
