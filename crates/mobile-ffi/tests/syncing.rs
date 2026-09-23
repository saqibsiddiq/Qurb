//! A phone pairing with another device and syncing with it, through the FFI.
//!
//! The claim this file exists to check is the one the crate README used to have
//! to disclaim: that `qurb-mobile` makes the phone a *syncing* device and not
//! just a local encrypted file store.
//!
//! Both sides are `Qurb` handles here, so "the desktop" is a stand-in. What is
//! real is everything between them: pairing out of band, a rendezvous service,
//! a QUIC connection with pinned certificates, and the engine's own planning.

use qurb_mobile::{create, restore, Qurb, Settings};
use qurb_signal::SignalServer;
use std::sync::Arc;

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

    let waiting = std::thread::spawn(move || offer.wait());
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

    // Present, listed, and byte-exact.
    let listed = phone.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, "notes.txt");

    let out = phone_dir.path().join("exported.txt");
    phone.export("notes.txt".into(), out.display().to_string()).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"written on the desktop");
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
    let waiting = std::thread::spawn(move || offer.wait());
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
    let waiting = std::thread::spawn(move || offer.wait());
    b.join_pairing(code).unwrap();
    waiting.join().unwrap().unwrap();

    // `a` never syncs, so it never announces and cannot be found.
    drop(a);

    let outcome = b.sync_within(5).unwrap();
    assert_eq!(outcome.reached, 0);
    assert_eq!(outcome.adopted, 0);
    // Either it gave up on the peer or it ran out of time. Both are fine; what
    // must not happen is an error reaching the caller.
    assert!(outcome.unreachable == 1 || outcome.timed_out, "{outcome:?}");
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
    assert!(offer.wait().is_err());
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
    let waiting = std::thread::spawn(move || offer.wait());
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
