//! A phone pairing with another device and syncing with it, through the FFI.
//!
//! The claim this file exists to check is the one the crate README used to have
//! to disclaim: that `qurb-mobile` makes the phone a *syncing* device and not
//! just a local encrypted file store.
//!
//! Both sides are `Qurb` handles here, so "the desktop" is a stand-in. What is
//! real is everything between them: pairing out of band, a rendezvous service,
//! a QUIC connection with pinned certificates, and the engine's own planning.

use qurb_mobile::{create, join_new, restore, Qurb, Settings};
use qurb_signal::SignalServer;
use std::sync::Arc;

/// The "desktop" in these tests is a second phone engine, which tells devices
/// it is a phone; a real desktop says it is a computer. Recorded as one on the
/// phone at `phone_root`, so the phone counts its copy as a safe one to free
/// its own for (decision 0053).
fn as_a_computer(phone_root: &std::path::Path) {
    let store_dir = phone_root.join(".qurb");
    let master = qurb_keys::Vault::at(&store_dir).unlock(None).unwrap();
    let chunk_key = qurb_storage::ChunkKey::from_bytes(
        master.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes(),
    );
    let store = qurb_storage::Store::open(&store_dir, chunk_key).unwrap();
    for peer in store.db().trusted_peers().unwrap() {
        store.db().set_peer_kind(&peer.device_id, "computer").unwrap();
    }
}

/// The person at the device showing the code approves whoever asks.
struct Yes;

impl qurb_mobile::PairingApprover for Yes {
    fn approve(&self, _: qurb_mobile::PairingRequest) -> bool {
        true
    }
}

/// Keeps the memory measurement away from everything else in this file.
///
/// `receiving_a_large_file_does_not_hold_it_in_memory` reads the *process's*
/// anonymous memory, and cargo runs the tests in one binary as threads of one
/// process — so another test syncing a file at the same moment is counted as
/// the heap growing, and the measurement blames it on buffering. Seven tests
/// share this process and the reading moved by 70 MiB depending on what else
/// happened to be running.
///
/// The measurement takes the write lock; everything else takes a read lock, so
/// the rest still run concurrently with each other.
static ALONE: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Logs, when `RUST_LOG` asks for them. Off otherwise, so a passing run is quiet.
fn logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

/// A rendezvous service on a loopback port, running for the test's lifetime.
fn signalling() -> (tokio::runtime::Runtime, String) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let url = runtime.block_on(async {
        let server = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
        let url = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(async move { server.serve().await });
        url
    });

    (runtime, url)
}

fn settings(name: &str, signal: &str) -> Settings {
    Settings {
        device_name: name.to_string(),
        signal_url: signal.to_string(),
        relay: None,
        port: 0,
        // Off, for two reasons. Everything here is on loopback, so there is no
        // router to discover. And discovery contacts Google's and Cloudflare's
        // STUN servers, which a test suite has no business doing on every run:
        // it is slow, it fails offline, and it tells a third party the address
        // of every machine that runs `cargo test`.
        discover: false,
        // No push in tests: the devices here are both awake, and a test that
        // depended on Google would not be a test.
        wake_token: None,
        // The shared area, as every test here expects.
        own_files_private: false,
    }
}

/// Pair, then sync, then check the file arrived and its bytes are right.
///
/// The whole point, end to end. If this passes, the phone is a peer.
#[test]
fn a_phone_pairs_with_a_desktop_and_takes_its_files() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (_runtime, signal) = signalling();

    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let desktop_root = desktop_dir.path().display().to_string();
    let phone_root = phone_dir.path().display().to_string();

    // One person's two devices, so one master key.
    let setup = create(desktop_root.clone()).unwrap();
    restore(phone_root.clone(), setup.recovery_phrase.clone()).unwrap();

    let desktop = Qurb::open_with(desktop_root, None, settings("desktop", &signal)).unwrap();
    let phone = Qurb::open_with(phone_root, None, settings("phone", &signal)).unwrap();

    std::fs::write(desktop_dir.path().join("notes.txt"), b"written on the desktop").unwrap();
    desktop.scan().unwrap();

    // They have never spoken.
    assert!(desktop.peers().unwrap().is_empty());
    assert!(phone.peers().unwrap().is_empty());

    // The out-of-band step: the desktop shows a code, the phone's camera reads
    // it. Here the code is passed directly, which is the only part of this the
    // test fakes.
    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    assert!(!code.is_empty());
    assert!(!offer.spoken().is_empty(), "there must be something to read aloud");

    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    let joined = phone.join_pairing(code).unwrap();
    let hosted = waiting.join().unwrap().unwrap();

    assert_eq!(joined.name, "desktop");
    assert_eq!(hosted.name, "phone");
    assert_eq!(phone.peers().unwrap().len(), 1);
    assert_eq!(desktop.peers().unwrap().len(), 1);

    // Both must be announced and listening at the same moment: a QUIC
    // handshake's opening packets are the hole punch, so a device that only
    // listens has punched nothing. Neither knows the other's address — the
    // rendezvous service introduces them.
    //
    // The desktop loops rather than syncing once, which is what a desktop
    // daemon does. A single pass on each side is a race: whichever starts
    // first gives up on an unannounced peer and drops its connector before the
    // other is listening. That race is real on a phone too, and is why two
    // devices that are both only briefly awake may never meet — see the
    // crate README.
    let desktop = Arc::new(desktop);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = Arc::clone(&desktop);
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = serving.sync_within(5);
        }
    });

    // Retried to a deadline rather than once. A single attempt is a coin flip:
    // the desktop is between passes about as often as it is inside one, and on
    // a slow machine -- an emulator, say -- it is worse than that. An app does
    // the same thing, by asking the platform for another background window.
    let outcome = until_reached(&phone, std::time::Duration::from_secs(60));
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();

    assert_eq!(outcome.reached, 1, "the phone did not reach the desktop");
    assert_eq!(outcome.adopted, 1, "the file did not arrive");
    assert!(!outcome.timed_out);

    // And it remembers when. The phone's Devices screen said "not reached yet"
    // about a desktop it had been syncing with for days, because only the
    // desktop daemon ever wrote this down.
    let seen = phone.peers().unwrap()[0].last_seen;
    assert!(seen.is_some(), "reached the desktop and did not record it");

    // Present, listed, and byte-exact.
    let listed = phone.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, "notes.txt");

    let out = phone_dir.path().join("exported.txt");
    phone.export("notes.txt".into(), out.display().to_string()).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"written on the desktop");
}

/// A new phone joins with the code the desktop shows and comes out with the
/// desktop's key, paired, with no 24 words anywhere (decision 0052).
#[test]
fn a_new_phone_joins_with_the_code_and_needs_no_words() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (_runtime, signal) = signalling();

    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let desktop_root = desktop_dir.path().display().to_string();
    let phone_root = phone_dir.path().display().to_string();

    create(desktop_root.clone()).unwrap();
    let desktop = Qurb::open_with(desktop_root, None, settings("desktop", &signal)).unwrap();

    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    let joined = join_new(phone_root.clone(), code, "phone".into(), None).unwrap();
    let hosted = waiting.join().unwrap().unwrap();
    assert_eq!(joined.name, "desktop");
    assert_eq!(hosted.name, "phone");

    let phone = Qurb::open_with(phone_root, None, settings("phone", &signal)).unwrap();
    assert_eq!(phone.recovery_phrase(), desktop.recovery_phrase(), "one key, on both");
    assert_eq!(phone.peers().unwrap().len(), 1);
    assert_eq!(desktop.peers().unwrap().len(), 1);
}

/// Receiving a large file over the network must not hold it in memory.
///
/// The local read path was measured when it was written; the *network* path was
/// not, and that turned out to matter: `NetworkSource` implemented only the
/// buffering half of `ContentSource` and silently inherited the default for the
/// streaming one. Nothing failed, because buffering is correct and merely
/// expensive — which is why this is a test rather than a comment.
#[test]
fn receiving_a_large_file_does_not_hold_it_in_memory() {
    let _alone = ALONE.write().unwrap_or_else(|e| e.into_inner());
    let (_runtime, signal) = signalling();

    let sender_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let sender_root = sender_dir.path().display().to_string();
    let phone_root = phone_dir.path().display().to_string();

    let setup = create(sender_root.clone()).unwrap();
    restore(phone_root.clone(), setup.recovery_phrase).unwrap();

    // 128 MiB: small for a phone's storage, and four times any plausible
    // FileProvider memory ceiling. The size is chosen to discriminate — a
    // buffering implementation grows the heap by 128 MiB here and cannot
    // squeeze under the bound below by luck.
    let size: usize = std::env::var("QURB_TEST_MIB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(128)
        * 1024
        * 1024;
    write_incompressible(&sender_dir.path().join("video.bin"), size);

    let sender = Qurb::open_with(sender_root, None, settings("sender", &signal)).unwrap();
    let phone = Qurb::open_with(phone_root, None, settings("phone", &signal)).unwrap();
    sender.scan().unwrap();

    let offer = sender.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    phone.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    let sender = Arc::new(sender);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = Arc::clone(&sender);
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = serving.sync_within(5);
        }
    });

    let before = anon_kib();
    let outcome = until_reached(&phone, std::time::Duration::from_secs(120));
    let after = anon_kib();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();

    assert_eq!(outcome.adopted, 1, "the file did not arrive");
    assert_eq!(
        std::fs::metadata(phone_dir.path().join("video.bin")).unwrap().len() as usize,
        size
    );

    // The bound is fixed overhead plus headroom, not a fraction of the file.
    //
    // Measured on this machine: 32 MiB file -> 29 MiB of heap, 64 -> 38,
    // 128 -> 32. Quadrupling the file leaves it flat, which is the property
    // under test. The ~30 MiB floor is two tokio runtimes, QUIC send and
    // receive buffers, SQLite page caches and an allocator that does not hand
    // pages back — all of it in *one* process, because this test runs both
    // devices here. A phone runs only the receiving half.
    //
    // Set QURB_TEST_MIB to re-measure at another size; it should not move.
    let grew = after.saturating_sub(before) / 1024;
    eprintln!("received {} MiB, heap grew {grew} MiB", size >> 20);
    assert!(
        grew < 64,
        "receiving {} MiB grew the heap by {grew} MiB -- is it buffering the file?",
        size >> 20
    );
}

/// Sync until a peer answers, or `budget` runs out.
///
/// Returns the last outcome either way, so a caller that wanted a peer reached
/// fails on its own assertion with the real numbers rather than on a timeout.
fn until_reached(qurb: &Qurb, budget: std::time::Duration) -> qurb_mobile::SyncOutcome {
    let deadline = std::time::Instant::now() + budget;
    loop {
        let outcome = qurb.sync_within(10).unwrap();
        if outcome.reached > 0 || std::time::Instant::now() >= deadline {
            return outcome;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Anonymous resident memory in KiB. Not total resident size: file-backed pages
/// are clean and droppable, and counting them would make a memory-mapped read
/// look as costly as holding the whole file in a `Vec`.
fn anon_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("RssAnon:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

/// Pseudo-random bytes, written in blocks so making the file does not itself
/// dominate the measurement. Incompressible on purpose: zeroes would compress
/// to nothing and prove nothing about size.
fn write_incompressible(path: &std::path::Path, size: usize) {
    use std::io::Write;
    let mut file = std::fs::File::create(path).unwrap();
    let mut x: u32 = 1;
    let mut block = vec![0u8; 1 << 20];
    for _ in 0..(size / (1 << 20)) {
        for byte in block.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *byte = x as u8;
        }
        file.write_all(&block).unwrap();
    }
}

/// A device with nobody to sync with must return cleanly, not hang or fail.
///
/// The commonest state a freshly installed app is in.
#[test]
fn syncing_with_no_peers_does_nothing_quickly() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    let started = std::time::Instant::now();
    let outcome = qurb.sync_within(30).unwrap();

    assert_eq!(outcome.reached, 0);
    assert_eq!(outcome.unreachable, 0);
    assert!(!outcome.timed_out);
    // It must not have contacted the rendezvous service, which is not running.
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "it went looking");
}

/// A peer that is switched off is the normal case on a phone, not a fault: it
/// must be counted and reported, never raised as an error.
#[test]
fn an_unreachable_peer_is_counted_not_raised() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let (_runtime, signal) = signalling();

    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a_root = a_dir.path().display().to_string();
    let b_root = b_dir.path().display().to_string();

    let setup = create(a_root.clone()).unwrap();
    restore(b_root.clone(), setup.recovery_phrase).unwrap();

    let a = Qurb::open_with(a_root, None, settings("a", &signal)).unwrap();
    let b = Qurb::open_with(b_root, None, settings("b", &signal)).unwrap();

    let offer = a.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    b.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    // `a` never syncs, so it never announces and cannot be found.
    drop(a);

    // The rendezvous service is running and says at once that `a` is not
    // there. This used to accept "ran out of time" as well, and the
    // difference matters -- see the next test.
    let outcome = b.sync_within(5).unwrap();
    assert_eq!(outcome.reached, 0);
    assert_eq!(outcome.adopted, 0);
    assert_eq!(outcome.unreachable, 1, "{outcome:?}");
    assert!(!outcome.timed_out, "a device that never answered was reported as time running out");
}

/// A relay given by name, as one on a server of your own would be.
///
/// The phone's engine took the relay only as an address and failed the whole
/// pass on a name, with "bad relay address"; the app passed no relay at all,
/// so a phone on a mobile network whose carrier defeats hole punching reached
/// nothing. The name is now looked up on each pass.
#[test]
fn a_relay_can_be_given_by_name() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (runtime, signal) = signalling();
    let relay_port = runtime.block_on(async {
        let relay = Arc::new(
            qurb_relay::RelayServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap(),
        );
        let port = relay.local_addr().unwrap().port();
        tokio::spawn(async move { relay.serve().await });
        port
    });
    let with_relay = |name: &str| Settings {
        relay: Some(format!("localhost:{relay_port}")),
        ..settings(name, &signal)
    };

    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let setup = create(desktop_dir.path().display().to_string()).unwrap();
    restore(phone_dir.path().display().to_string(), setup.recovery_phrase).unwrap();
    let desktop =
        Qurb::open_with(desktop_dir.path().display().to_string(), None, with_relay("desktop")).unwrap();
    let phone =
        Qurb::open_with(phone_dir.path().display().to_string(), None, with_relay("phone")).unwrap();
    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    phone.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    std::fs::write(desktop_dir.path().join("notes.txt"), b"via a relay with a name").unwrap();
    desktop.scan().unwrap();
    let desktop = Arc::new(desktop);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = Arc::clone(&desktop);
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = serving.sync_within(5);
        }
    });

    let outcome = until_reached(&phone, std::time::Duration::from_secs(60));
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();

    assert_eq!(outcome.reached, 1, "{outcome:?}");
    assert_eq!(outcome.adopted, 1, "{outcome:?}");
}

/// A shared file freed from the phone comes back when asked for.
///
/// The phone's sync planned with everything but the one step that turns "I
/// asked for this back" into a download -- the desktop daemon added that step
/// itself -- so asking did nothing. Found by opening a freed file from the
/// system file picker on an emulator, which asks for it and syncs.
#[test]
fn a_freed_shared_file_comes_back_when_asked_for() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (_runtime, signal) = signalling();

    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let setup = create(desktop_dir.path().display().to_string()).unwrap();
    restore(phone_dir.path().display().to_string(), setup.recovery_phrase).unwrap();
    let desktop = Qurb::open_with(
        desktop_dir.path().display().to_string(),
        None,
        settings("desktop", &signal),
    )
    .unwrap();
    let phone =
        Qurb::open_with(phone_dir.path().display().to_string(), None, settings("phone", &signal))
            .unwrap();
    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    phone.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    let bytes = b"a photograph the desktop took".to_vec();
    std::fs::write(desktop_dir.path().join("photo.jpg"), &bytes).unwrap();
    desktop.scan().unwrap();

    let desktop = Arc::new(desktop);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = Arc::clone(&desktop);
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = serving.sync_within(5);
        }
    });

    let arrived = until_reached(&phone, std::time::Duration::from_secs(60));
    assert_eq!(arrived.adopted, 1, "{arrived:?}");

    // Freed: the desktop made it, so the phone knows the desktop has it.
    as_a_computer(phone_dir.path());
    phone.free_local("photo.jpg".into()).unwrap();
    let freed = phone_dir.path().join("photo.jpg");
    assert!(!freed.exists());

    assert!(phone.fetch("photo.jpg".into()).unwrap(), "asked for a file that was not freed");
    let back = until_reached(&phone, std::time::Duration::from_secs(60));
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();

    assert_eq!(back.reached, 1, "{back:?}");
    assert_eq!(std::fs::read(&freed).ok(), Some(bytes), "asked for, reached the desktop, and not brought back");
}

/// A device that is switched off does not keep the phone from the one that is
/// on.
///
/// The phone used to try its devices one after another, in name order, each
/// with whatever was left of the window. One whose announced address leads
/// nowhere takes eight seconds to give up on, so with a six-second window the
/// working desktop sorted after it was never tried -- in every pass, since the
/// order never changes. Now every device is reached at once.
#[test]
fn a_device_that_is_off_does_not_starve_one_that_is_on() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (signal_runtime, signal) = signalling();

    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let setup = create(desktop_dir.path().display().to_string()).unwrap();
    restore(phone_dir.path().display().to_string(), setup.recovery_phrase.clone()).unwrap();
    let master = qurb_keys::MasterKey::from_phrase(
        &qurb_keys::RecoveryPhrase::parse(&setup.recovery_phrase).unwrap(),
    )
    .unwrap();

    // A laptop that is off, paired long ago, and named so it is tried first.
    let off = [0xB1; 32];
    {
        let store_dir = phone_dir.path().join(".qurb");
        let key = qurb_keys::Vault::at(&store_dir).unlock(None).unwrap();
        let chunk_key = qurb_storage::ChunkKey::from_bytes(
            key.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes(),
        );
        let store = qurb_storage::Store::open(&store_dir, chunk_key).unwrap();
        let device = qurb_sync::DeviceId::from_bytes([0xB0; 32]);
        store.db().trust_peer(&device, &off, "a laptop that is off").unwrap();
    }
    // Still announced at an address where nothing answers: the rendezvous
    // service introduces the phone to it, and the phone waits on it.
    let _ghost = signal_runtime.block_on(qurb_signal::SignalClient::connect_insecure(
        &signal,
        qurb_signal::GroupId::derive(&master),
        qurb_signal::MemberId::derive(&master, &off),
        qurb_signal::Endpoints { public: None, local: vec!["192.0.2.1:9".parse().unwrap()] },
    ));

    let desktop = Qurb::open_with(
        desktop_dir.path().display().to_string(),
        None,
        settings("desktop", &signal),
    )
    .unwrap();
    let phone =
        Qurb::open_with(phone_dir.path().display().to_string(), None, settings("phone", &signal))
            .unwrap();
    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    phone.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();
    assert_eq!(phone.peers().unwrap()[0].name, "a laptop that is off", "tried first, or this tests nothing");

    std::fs::write(desktop_dir.path().join("notes.txt"), b"written on the desktop").unwrap();
    desktop.scan().unwrap();
    let desktop = Arc::new(desktop);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = Arc::clone(&desktop);
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = serving.sync_within(5);
        }
    });

    // One pass, shorter than it takes to give up on the laptop that is off.
    let outcome = phone.sync_within(6).unwrap();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();

    assert_eq!(outcome.reached, 1, "the working desktop was never tried: {outcome:?}");
    assert_eq!(outcome.adopted, 1, "{outcome:?}");
    assert_eq!(outcome.unreachable, 1, "{outcome:?}");
    assert!(!outcome.timed_out, "{outcome:?}");
}

/// One pass that reaches the desktop is enough for the desktop to take what
/// the phone made.
///
/// Every device pulls, so a photo moves only when the desktop dials the phone
/// back and asks -- and a phone that reached the desktop in a second used to
/// end its pass before the desktop could. The test above waits for it over as
/// many passes as it takes; this one allows exactly one, which is what a phone
/// in a background window gets.
///
/// What this does not show: that the phone *waiting* for the desktop is what
/// makes it pass. It passes with the wait switched off too, because the
/// stand-in desktop here runs pass after pass and is always dialling; a real
/// daemon dials when it hears the phone, and can be busy. The case for the
/// wait is a Galaxy S23 whose desktop, busy for ten seconds, collected inside
/// the wait -- see decision 0020.
#[test]
fn one_pass_is_enough_for_the_desktop_to_collect() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (_runtime, signal) = signalling();

    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let setup = create(desktop_dir.path().display().to_string()).unwrap();
    restore(phone_dir.path().display().to_string(), setup.recovery_phrase).unwrap();
    let desktop = Qurb::open_with(
        desktop_dir.path().display().to_string(),
        None,
        settings("desktop", &signal),
    )
    .unwrap();
    let phone =
        Qurb::open_with(phone_dir.path().display().to_string(), None, settings("phone", &signal))
            .unwrap();

    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    phone.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    let source = phone_dir.path().join("outside.jpg");
    std::fs::write(&source, b"taken on the phone this morning").unwrap();
    phone.import_file(source.display().to_string(), "morning.jpg".into()).unwrap();

    // The desktop, always on.
    let desktop = Arc::new(desktop);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = Arc::clone(&desktop);
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = serving.sync_within(5);
        }
    });

    // Passes until one reaches the desktop -- finding each other is not what
    // this tests -- and then no more.
    let outcome = until_reached(&phone, std::time::Duration::from_secs(60));
    assert_eq!(outcome.reached, 1, "the phone never reached the desktop");

    // The desktop acknowledges a file when it has the bytes, a moment before
    // the file is in its folder.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !desktop.contains("morning.jpg".into()).unwrap_or(false)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();

    assert!(
        desktop.contains("morning.jpg".into()).unwrap(),
        "the pass that reached the desktop ended before the desktop could take the file"
    );
    assert!(phone.outstanding().unwrap().files.is_empty(), "the phone does not know it arrived");
}

/// The phone's ordinary morning: its computer is switched off, so the
/// rendezvous service that runs on it is too. A background window closes while
/// the phone is still waiting to be introduced.
///
/// That is a device that did not answer, and must be reported as one. It was
/// reported as time running out, which the worker answers by asking for
/// another window sooner -- exponential backoff, for a device that will not
/// answer until someone switches it on. Measured on a Galaxy S23: the worker's
/// window was twenty seconds and the wait for an introduction is twenty
/// seconds, so every background pass came back that way.
#[test]
fn a_device_that_never_answers_is_unreachable_not_out_of_time() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let (_runtime, signal) = signalling();

    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a_root = a_dir.path().display().to_string();
    let b_root = b_dir.path().display().to_string();

    let setup = create(a_root.clone()).unwrap();
    restore(b_root.clone(), setup.recovery_phrase).unwrap();

    let a = Qurb::open_with(a_root, None, settings("a", &signal)).unwrap();
    let b = Qurb::open_with(b_root.clone(), None, settings("b", &signal)).unwrap();
    let offer = a.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    b.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();
    drop((a, b));

    // A rendezvous address that accepts the connection and never says a word:
    // what a switched-off computer's port looks like from behind a router that
    // holds the connection open, and what a slow one looks like everywhere.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", silent.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in silent.incoming().flatten() {
            held.push(stream);
        }
    });

    let b = Qurb::open_with(b_root, None, settings("b", &url)).unwrap();

    // Three seconds: the window closes while the phone is still connecting to
    // the rendezvous service. Eight: connecting gives up after five, and the
    // window closes while the phone waits for `a` -- the phone's own case.
    for window in [3, 8] {
        let started = std::time::Instant::now();
        let outcome = b.sync_within(window).unwrap();

        assert_eq!(outcome.reached, 0);
        assert_eq!(outcome.unreachable, 1, "{window}s: {outcome:?}");
        assert!(!outcome.timed_out, "{window}s: a device that never answered was reported as time running out");
        let limit = std::time::Duration::from_secs(window as u64 + 2);
        assert!(started.elapsed() < limit, "{window}s: overran its window: {:?}", started.elapsed());
    }
}

/// An expired or malformed code must be refused as such, because "the code is
/// wrong" and "the network is down" want completely different advice on screen.
#[test]
fn a_bad_pairing_code_is_refused_as_a_bad_code() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    match qurb.join_pairing("not-a-real-code".into()) {
        Err(qurb_mobile::QurbError::BadCode { .. }) => {}
        other => panic!("expected BadCode, got {other:?}"),
    }
}

/// Cancelling an offer stops it, and the code cannot then be used.
#[test]
fn a_cancelled_offer_cannot_be_joined() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    let offer = qurb.offer_pairing().unwrap();
    offer.cancel();

    // Waiting on a cancelled offer reports rather than blocking for ever.
    assert!(offer.wait(Arc::new(Yes)).is_err());
}

/// The phone shows a code and waits on it; the person gives up. The wait
/// already blocking must return then, not five minutes later when the code
/// expires -- the Cancel button on the phone's code screen depends on it.
#[test]
fn giving_up_on_a_code_stops_the_wait_already_blocking() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    let offer = qurb.offer_pairing().unwrap();
    let waiting = {
        let offer = Arc::clone(&offer);
        std::thread::spawn(move || offer.wait(Arc::new(Yes)))
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    let started = std::time::Instant::now();
    offer.cancel();
    assert!(waiting.join().unwrap().is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?}", started.elapsed());
}

/// A real pairing code, drawn: square, and a size a QR code can be.
#[test]
fn a_pairing_code_is_drawn_as_a_qr_code() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();
    let offer = qurb.offer_pairing().unwrap();

    let qr = qurb_mobile::qr_code(offer.code()).unwrap();
    assert_eq!(qr.dark.len(), (qr.width * qr.width) as usize);
    assert!(qr.width >= 21 && (qr.width - 17).is_multiple_of(4), "not a QR size: {}", qr.width);
    offer.cancel();
}

/// Sharing while the other device is switched off, and having it arrive later
/// without anybody doing anything.
///
/// The scenario the feature exists for: someone shares a photo from their
/// phone, their computer is off, and they put the phone away. The promise is
/// that the photo is on the computer afterwards and that they were never asked
/// to do anything else about it.
///
/// Three properties, and the middle one is the load-bearing one:
///
/// 1. The share **works with nothing to sync to**. There is no network in the
///    first half of this test at all.
/// 2. The phone **knows it is outstanding** — that it holds the only copy —
///    which is what it tells the person, and what a storage cap consults.
/// 3. Nobody asks for the transfer. It happens on the next ordinary sync,
///    which on a real phone is the periodic background worker.
#[test]
fn a_share_made_while_the_desktop_is_off_arrives_when_it_returns() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (_runtime, signal) = signalling();

    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let desktop_root = desktop_dir.path().display().to_string();
    let phone_root = phone_dir.path().display().to_string();

    let setup = create(desktop_root.clone()).unwrap();
    restore(phone_root.clone(), setup.recovery_phrase.clone()).unwrap();

    let desktop = Qurb::open_with(desktop_root, None, settings("desktop", &signal)).unwrap();
    let phone = Qurb::open_with(phone_root, None, settings("phone", &signal)).unwrap();

    // Paired first, as they would have been long before today's photo.
    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    phone.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    // -- the desktop is off ---------------------------------------------------
    //
    // Nothing is listening and nothing is announced. This is the half that has
    // to work without a network, so no server is started for it.

    let source = phone_dir.path().join("outside.jpg");
    std::fs::write(&source, b"a photograph taken on the phone").unwrap();
    phone.import_file(source.display().to_string(), "holiday.jpg".into()).unwrap();

    // Saved, listed, and known to be the only copy.
    assert!(phone.contains("holiday.jpg".into()).unwrap(), "the share was not saved");
    let outstanding = phone.outstanding().unwrap();
    assert_eq!(
        outstanding.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        vec!["holiday.jpg"],
        "the phone does not know it is holding the only copy"
    );
    assert_eq!(outstanding.bytes, b"a photograph taken on the phone".len() as u64);

    // Trying to sync now reaches nobody, and loses nothing by it.
    let attempt = phone.sync_within(2).unwrap();
    assert_eq!(attempt.reached, 0, "something answered; the desktop was supposed to be off");
    assert!(phone.contains("holiday.jpg".into()).unwrap(), "a failed sync lost the file");
    assert_eq!(phone.outstanding().unwrap().files.len(), 1);

    // -- the desktop comes back -----------------------------------------------

    let desktop = Arc::new(desktop);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = Arc::clone(&desktop);
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = serving.sync_within(5);
        }
    });

    // Waited on the outcome rather than on the phone reaching the desktop,
    // because those are not the same event and this direction is the slower
    // one. Every device pulls what it wants: the phone connecting gets the
    // phone up to date, and the photo only moves when the *desktop* dials the
    // phone and pulls. So the condition is "the desktop has it", which is what
    // the person actually waits for.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut arrived = false;
    while std::time::Instant::now() < deadline {
        let _ = phone.sync_within(5);
        if desktop.contains("holiday.jpg".into()).unwrap_or(false) {
            arrived = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();

    assert!(arrived, "the photo never reached the desktop");

    // It arrived, byte for byte.
    assert!(desktop.contains("holiday.jpg".into()).unwrap(), "the photo did not arrive");
    let out = desktop_dir.path().join("checked.jpg");
    desktop.export("holiday.jpg".into(), out.display().to_string()).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"a photograph taken on the phone");

    // And the phone now knows it is no longer the only holder, which is what
    // lets it stop saying the share is waiting. The report travels on the
    // connection that carried the content, so it is in hand by the time the
    // desktop has the file.
    assert!(
        phone.outstanding().unwrap().files.is_empty(),
        "the phone still reports the photo as undelivered: {:?}",
        phone.outstanding().unwrap().files
    );
}

/// Removing a device from the phone: trust ends here, and if it was the
/// device keeping the phone's own files, the question says so first.
#[test]
fn a_phone_removes_a_device_that_kept_its_files() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    let (_runtime, signal) = signalling();
    let desktop_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let setup = create(desktop_dir.path().display().to_string()).unwrap();
    restore(phone_dir.path().display().to_string(), setup.recovery_phrase.clone()).unwrap();
    let desktop =
        Qurb::open_with(desktop_dir.path().display().to_string(), None, settings("desktop", &signal))
            .unwrap();
    let phone =
        Qurb::open_with(phone_dir.path().display().to_string(), None, settings("phone", &signal))
            .unwrap();

    let offer = desktop.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    phone.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    let laptop = phone.peers().unwrap().remove(0);
    phone.add_holder(laptop.fingerprint.clone()).unwrap();

    let plan = phone.removal_plan(laptop.fingerprint.clone()).unwrap();
    assert!(plan.holds_ours, "the question must say it kept this phone's files");
    assert_eq!((plan.waiting, plan.kept), (0, 0));

    phone.remove_device(laptop.fingerprint.clone(), false).unwrap();
    assert!(phone.peers().unwrap().is_empty());
    assert!(phone.holders().unwrap().is_empty());
    let last = phone.history(1, None).unwrap().remove(0);
    assert_eq!((last.kind.as_str(), last.device.as_deref()), ("removed", Some("desktop")));
}

/// Keep `device` syncing, as a phone left open would, until `done` is set.
fn keep_syncing(device: Arc<Qurb>, done: Arc<std::sync::atomic::AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !done.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = device.sync_within(5);
        }
    })
}

/// Android to Android: two phones, one joining the other with its code and
/// taking its key, then a shared file each way and a file sent from one to
/// the other, collected and confirmed. Both are phone engines; run on an
/// emulator by `scripts/android-test.sh`, this is the engine as two Android
/// devices run it, on Android's own libc and filesystem.
#[test]
fn two_phones_pair_share_both_ways_and_send() {
    let _sharing = ALONE.read().unwrap_or_else(|e| e.into_inner());
    logging();
    let (_runtime, signal) = signalling();

    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();
    let first_root = first_dir.path().display().to_string();
    let second_root = second_dir.path().display().to_string();

    create(first_root.clone()).unwrap();
    let first = Qurb::open_with(first_root, None, settings("first phone", &signal)).unwrap();
    let offer = first.offer_pairing().unwrap();
    let code = offer.code();
    let waiting = std::thread::spawn(move || offer.wait(Arc::new(Yes)));
    join_new(second_root.clone(), code, "second phone".into(), None).unwrap();
    waiting.join().unwrap().unwrap();
    let second = Arc::new(Qurb::open_with(second_root, None, settings("second phone", &signal)).unwrap());
    let first = Arc::new(first);
    assert_eq!(second.recovery_phrase(), first.recovery_phrase(), "one key, on both");

    // The first phone's file, to the second.
    std::fs::write(first_dir.path().join("from-the-first.txt"), b"made on the first phone").unwrap();
    first.scan().unwrap();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = keep_syncing(Arc::clone(&first), Arc::clone(&done));
    let outcome = until_reached(&second, std::time::Duration::from_secs(60));
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    serving.join().unwrap();
    assert_eq!(outcome.reached, 1, "the second phone did not reach the first");
    assert_eq!(
        std::fs::read(second_dir.path().join("from-the-first.txt")).unwrap(),
        b"made on the first phone"
    );

    // The second phone's file, and a file sent to the first alone.
    std::fs::write(second_dir.path().join("from-the-second.txt"), b"made on the second phone").unwrap();
    second.scan().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let sent = outside.path().join("for-the-first.txt");
    std::fs::write(&sent, b"sent to the first phone only").unwrap();
    let to_first = second.peers().unwrap()[0].fingerprint.clone();
    second.send_file(sent.display().to_string(), "for-the-first.txt".into(), to_first).unwrap();
    assert_eq!(second.waiting().unwrap().len(), 1);

    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serving = keep_syncing(Arc::clone(&second), Arc::clone(&done));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while std::time::Instant::now() < deadline
        && !(first_dir.path().join("from-the-second.txt").exists()
            && first_dir.path().join("for-the-first.txt").exists())
    {
        let _ = first.sync_within(5);
    }
    // The second phone records the delivery when the first says it has it,
    // on a connection the first made: give that a pass to land.
    let confirmed = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < confirmed && !second.waiting().unwrap().is_empty() {
        let _ = first.sync_within(5);
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    serving.join().unwrap();

    assert_eq!(
        std::fs::read(first_dir.path().join("from-the-second.txt")).unwrap(),
        b"made on the second phone"
    );
    assert_eq!(
        std::fs::read(first_dir.path().join("for-the-first.txt")).unwrap(),
        b"sent to the first phone only"
    );
    assert!(second.waiting().unwrap().is_empty(), "the send is still waiting: {:?}", second.waiting());
    // Sent to one phone, so not to anybody else: it stays out of the shared area.
    assert!(!first.list().unwrap().iter().any(|f| f.path == "for-the-first.txt" && !f.private));
}
