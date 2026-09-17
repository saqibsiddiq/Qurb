//! Filenames that are the same name written two different ways.
//!
//! `é` is one code point (U+00E9) or two (`e` + U+0301). Linux stores whichever
//! it is given; macOS and iOS decompose on the way in. Left alone, that
//! disagreement makes sync duplicate files without limit — see
//! [`qurb_watcher::normalize`] for the mechanism.

use qurb_engine::{Engine, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::Path;

const COMPOSED: &str = "caf\u{e9}.txt"; // café, one code point
const DECOMPOSED: &str = "cafe\u{301}.txt"; // café, e + combining acute

fn open(root: &Path) -> Engine {
    std::fs::create_dir_all(root).unwrap();
    let store_dir = root.join(".qurb");
    let store = Store::open(&store_dir, ChunkKey::from_bytes([7; 32])).unwrap();
    Engine::new(root, store, IgnoreRules::new().with_store_dir(&store_dir))
}

/// The two spellings are genuinely different strings. If this ever fails, the
/// rest of the file is testing nothing.
#[test]
fn the_two_spellings_differ() {
    assert_ne!(COMPOSED, DECOMPOSED);
    assert_eq!(COMPOSED.chars().count(), 8);
    assert_eq!(DECOMPOSED.chars().count(), 9);
    assert_eq!(qurb_watcher::normalize(DECOMPOSED), COMPOSED);
}

/// A file written in the decomposed form — as a Mac's filesystem would hand it
/// back — must be indexed under the composed path, not a second one.
#[test]
fn decomposed_names_index_as_composed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(DECOMPOSED), b"hello").unwrap();

    let mut engine = open(dir.path());
    engine.reconcile().unwrap();

    let live = engine.store().db().live_paths().unwrap();
    assert_eq!(live, vec![COMPOSED.to_string()]);
}

/// The duplication loop, run end to end.
///
/// A device receives the composed name, writes it, and rescans. On a
/// decomposing filesystem the rescan sees the decomposed spelling. Before
/// normalisation that looked like "the composed file was deleted and a new
/// decomposed one appeared", which propagated back and multiplied. The decisive
/// evidence is that the second reconcile finds nothing to do.
#[test]
fn receiving_then_rescanning_is_stable() {
    let dir = tempfile::tempdir().unwrap();
    let sender_root = dir.path().join("sender");
    let receiver_root = dir.path().join("receiver");

    std::fs::create_dir_all(&sender_root).unwrap();
    std::fs::write(sender_root.join(COMPOSED), b"hello").unwrap();

    let mut sender = open(&sender_root);
    sender.reconcile().unwrap();

    let mut receiver = open(&receiver_root);
    receiver.reconcile().unwrap();

    let plan = receiver.plan_against(&sender.tree().unwrap()).unwrap();
    assert_eq!(plan.len(), 1);
    let stats = receiver.apply_plan(&plan, &mut StoreSource::new(sender.store())).unwrap();
    assert!(stats.failures.is_empty(), "{:?}", stats.failures);

    // Stand in for a decomposing filesystem: rename what was written to the
    // spelling such a filesystem would have stored. Linux will not do this on
    // its own, and the bug is only reachable when it happens.
    std::fs::rename(receiver_root.join(COMPOSED), receiver_root.join(DECOMPOSED)).unwrap();

    let rescan = receiver.reconcile().unwrap();
    assert_eq!(rescan.deleted, 0, "the rename must not look like a deletion");
    assert_eq!(rescan.stored, 0, "nor the decomposed name like a new file");
    assert_eq!(rescan.unchanged, 1);

    // And nothing to send back, which is where the loop used to start.
    let back = sender.plan_against(&receiver.tree().unwrap()).unwrap();
    assert!(back.is_empty(), "sender would have been told to change: {back:?}");
}

/// Both spellings present at once, which only Linux permits. One is indexed and
/// the other is reported, because renaming a user's file is not ours to do.
#[test]
fn both_spellings_at_once_are_reported_not_merged() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(COMPOSED), b"one").unwrap();
    std::fs::write(dir.path().join(DECOMPOSED), b"two").unwrap();

    // A filesystem that folded them would have left one file, and there is
    // nothing to test.
    if std::fs::read_dir(dir.path()).unwrap().count() < 2 {
        eprintln!("filesystem folds these spellings; skipping");
        return;
    }

    let mut engine = open(dir.path());
    let stats = engine.reconcile().unwrap();

    assert_eq!(stats.collided, 1, "the second spelling should be skipped");
    assert_eq!(stats.stored, 1);
    assert_eq!(engine.store().db().live_paths().unwrap(), vec![COMPOSED.to_string()]);

    // Deterministic, and specifically the *composed* file: it is already in the
    // form every other device will produce for this path. Leaving the choice to
    // directory-read order, as the first version of this did, meant two devices
    // could keep different files under one name.
    assert_eq!(engine.store().read_file(COMPOSED).unwrap(), b"one");
}
