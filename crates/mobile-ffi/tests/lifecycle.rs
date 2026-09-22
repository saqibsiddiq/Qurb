//! The FFI exercised the way a phone would use it.
//!
//! These are plain Rust calls to the exported functions. They do not test the
//! generated Kotlin or Swift, which needs a device or emulator — see
//! `docs/phases/phase-5-mobile.md` for what that leaves unverified. What they
//! do test is everything on this side of the boundary: that the sequence an app
//! actually performs works, and that the memory promise holds.

use qurb_mobile::{create, is_set_up, restore, Qurb, QurbError};
use qurb_storage::{ChunkKey, Store};

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// First launch: no vault, set one up, get words, open it.
#[test]
fn first_launch() {
    let dir = scratch();
    let root = dir.path().display().to_string();

    assert!(!is_set_up(root.clone()));

    let setup = create(root.clone()).unwrap();
    assert_eq!(setup.recovery_phrase.split_whitespace().count(), 24);
    assert!(is_set_up(root.clone()));

    let qurb = Qurb::open(root.clone(), None).unwrap();
    assert_eq!(qurb.list().unwrap(), vec![]);
    assert_eq!(qurb.root(), root);
}

/// Setting up twice would overwrite the only copy of a key that may be
/// protecting files this device cannot re-fetch.
#[test]
fn creating_twice_is_refused() {
    let dir = scratch();
    let root = dir.path().display().to_string();

    create(root.clone()).unwrap();
    assert!(matches!(create(root), Err(QurbError::Other { .. })));
}

/// Opening a directory that was never set up must say so specifically, because
/// it is the one error the app can act on by showing the setup screen.
#[test]
fn opening_an_unprepared_directory_says_so() {
    let dir = scratch();
    match Qurb::open(dir.path().display().to_string(), None) {
        Err(QurbError::NotSetUp { .. }) => {}
        Err(other) => panic!("wrong error: {other:?}"),
        Ok(_) => panic!("opened a directory that was never set up"),
    }
}

/// The second phone. The same words must produce a device that can read what
/// the first one encrypted.
#[test]
fn a_second_device_restores_from_the_phrase() {
    let first_dir = scratch();
    let second_dir = scratch();
    let first = first_dir.path().display().to_string();
    let second = second_dir.path().display().to_string();

    let setup = create(first.clone()).unwrap();

    std::fs::write(first_dir.path().join("notes.txt"), b"hello from device one").unwrap();
    let a = Qurb::open(first.clone(), None).unwrap();
    a.scan().unwrap();

    restore(second.clone(), setup.recovery_phrase.clone()).unwrap();
    let b = Qurb::open(second, None).unwrap();

    // The index and the content both have to arrive, and since single-copy
    // storage they are two different things: a device that materialises a file
    // keeps the payload in the file itself, not as chunks. So standing in for
    // the network means moving the folder as well as the store -- copying
    // `.qurb` alone would move an index pointing at bytes that never left.
    let out = second_dir.path().join("fetched.txt");
    copy_store(first_dir.path(), second_dir.path());
    std::fs::copy(first_dir.path().join("notes.txt"), second_dir.path().join("notes.txt"))
        .unwrap();
    let b = {
        drop(b);
        Qurb::open(second_dir.path().display().to_string(), None).unwrap()
    };
    let n = b.export("notes.txt".into(), out.display().to_string()).unwrap();

    assert_eq!(n, 21);
    assert_eq!(std::fs::read(&out).unwrap(), b"hello from device one");

    // And the part that copying cannot show, because a materialised file is
    // read without ever using the key: that the phrase really did restore the
    // same chunk key. Checked where encrypted chunks actually live -- a store
    // with no folder behind it, which is what a storage-only replica is.
    // Written with the first device's key, read with the second's.
    let elsewhere = scratch();
    let plain = elsewhere.path().join("secret.txt");
    std::fs::write(&plain, b"only the key opens this").unwrap();

    let vault_path = elsewhere.path().join("chunks-only");
    let mut wrote = Store::open(&vault_path, chunk_key_of(first_dir.path())).unwrap();
    wrote.put_file("secret.txt", &plain).unwrap();
    drop(wrote);

    let read = Store::open(&vault_path, chunk_key_of(second_dir.path())).unwrap();
    assert_eq!(
        read.read_file("secret.txt").unwrap(),
        b"only the key opens this",
        "the restored phrase did not produce the same chunk key"
    );
}

/// The chunk key a device derives from its vault.
fn chunk_key_of(root: &std::path::Path) -> ChunkKey {
    let master = qurb_keys::Vault::at(&root.join(".qurb")).unlock(None).unwrap();
    ChunkKey::from_bytes(master.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes())
}

/// A wrong phrase must be rejected before it produces a key, not after. BIP-39
/// has a checksum precisely so this is detectable.
#[test]
fn a_wrong_phrase_is_refused() {
    let dir = scratch();
    let err = restore(dir.path().display().to_string(), "not even close".into()).unwrap_err();
    assert!(matches!(err, QurbError::BadPhrase { .. }), "{err:?}");
}

/// Import, list, export, remove — the whole set of things a file browser does.
#[test]
fn the_file_operations_round_trip() {
    let dir = scratch();
    let staging = scratch();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    let incoming = staging.path().join("photo.jpg");
    std::fs::write(&incoming, vec![7u8; 5000]).unwrap();
    qurb.import_file(incoming.display().to_string(), "album/photo.jpg".into()).unwrap();

    // The source is left where it was: the app hands over a temporary file the
    // system gave it and cleans up on its own schedule.
    assert!(incoming.exists());

    let listed = qurb.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, "album/photo.jpg");
    assert_eq!(listed[0].size, 5000);
    assert!(qurb.contains("album/photo.jpg".into()).unwrap());

    let out = staging.path().join("out.jpg");
    let n = qurb.export("album/photo.jpg".into(), out.display().to_string()).unwrap();
    assert_eq!(n, 5000);
    assert_eq!(std::fs::read(&out).unwrap(), vec![7u8; 5000]);

    qurb.remove("album/photo.jpg".into()).unwrap();
    assert!(!qurb.contains("album/photo.jpg".into()).unwrap());
    assert_eq!(qurb.list().unwrap(), vec![]);
    assert!(!dir.path().join("album/photo.jpg").exists());
}

/// Asking for a file that is not there is the commonest error a FileProvider
/// hits, and it must arrive as `NotFound` rather than a generic failure.
#[test]
fn a_missing_file_reports_not_found() {
    let dir = scratch();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    let err = qurb
        .export("nothing.txt".into(), dir.path().join("out").display().to_string())
        .unwrap_err();
    assert!(matches!(err, QurbError::NotFound { .. }), "{err:?}");
}

/// The memory promise, checked rather than asserted in a comment.
///
/// 64 MiB is small for a phone's storage and enormous for a FileProvider
/// extension's memory ceiling. If `export` ever goes back to buffering, this
/// grows by 64 MiB and fails.
#[test]
fn export_does_not_hold_the_file_in_memory() {
    let dir = scratch();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    let staging = scratch();
    let size = 64 * 1024 * 1024;
    let big = staging.path().join("big.bin");
    write_incompressible(&big, size);
    qurb.import_file(big.display().to_string(), "big.bin".into()).unwrap();

    let before = anon_kib();
    let out = dir.path().join("out.bin");
    let n = qurb.export("big.bin".into(), out.display().to_string()).unwrap();
    let after = anon_kib();

    assert_eq!(n as usize, size);
    let grew_mib = after.saturating_sub(before) / 1024;
    assert!(grew_mib < 16, "export grew the heap by {grew_mib} MiB, exporting {} MiB", size >> 20);
}

/// Usage reports both numbers, and deduplication is visible in the difference.
#[test]
fn usage_shows_what_deduplication_saved() {
    let dir = scratch();
    let staging = scratch();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    // The same content under three names. The library counts three copies; the
    // disk holds one.
    let source = staging.path().join("source.bin");
    write_incompressible(&source, 4 * 1024 * 1024);
    for name in ["a.bin", "b.bin", "c.bin"] {
        qurb.import_file(source.display().to_string(), name.into()).unwrap();
    }

    let usage = qurb.usage().unwrap();
    assert_eq!(usage.logical, 3 * 4 * 1024 * 1024, "three copies, counted as three");
    assert!(
        usage.on_disk < usage.logical / 2,
        "on disk {} should be near one copy, not three ({})",
        usage.on_disk,
        usage.logical
    );
}

/// Importing a file that is already inside the tree must index it, not copy it
/// onto itself. `std::fs::copy` from a path to that same path truncates it, so
/// the version of this that did not check destroyed the file it was adding.
#[test]
fn importing_a_file_already_in_place_does_not_destroy_it() {
    let dir = scratch();
    let root = dir.path().display().to_string();
    create(root.clone()).unwrap();
    let qurb = Qurb::open(root, None).unwrap();

    let inside = dir.path().join("already-here.txt");
    std::fs::write(&inside, b"do not truncate me").unwrap();

    qurb.import_file(inside.display().to_string(), "already-here.txt".into()).unwrap();

    assert_eq!(std::fs::read(&inside).unwrap(), b"do not truncate me");
    assert_eq!(qurb.list().unwrap()[0].size, 18);
}

/// Anonymous resident memory in KiB. Not total resident size: file-backed pages
/// are clean and droppable, and counting them would make a memory-mapped read
/// look as costly as holding the file in a `Vec`.
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
/// dominate the memory measurement. Incompressible on purpose: zeroes would
/// compress to nothing and prove nothing about size.
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

/// Stand in for the network: hand the second device the first one's chunks.
fn copy_store(from: &std::path::Path, to: &std::path::Path) {
    let (from, to) = (from.join(".qurb"), to.join(".qurb"));
    for entry in walk(&from) {
        let rel = entry.strip_prefix(&from).unwrap();
        // The vault stays as it is: the second device has its own, restored
        // from the phrase, and overwriting it would test nothing.
        if rel.to_string_lossy().contains("vault") {
            continue;
        }
        let dest = to.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::copy(&entry, &dest).unwrap();
        }
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
