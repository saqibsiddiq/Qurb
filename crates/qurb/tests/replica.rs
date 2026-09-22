//! The daemon running as a storage-only replica.
//!
//! The engine's own tests cover what a replica *decides*. These cover the
//! wiring around it, which is where a replica goes wrong in the ways that lose
//! data: watching a directory it does not have, attaching a folder it does not
//! have, or materialising files it was never meant to write.

use qurb_cli::{store_dir, Daemon};
use qurb_engine::PinSet;
use qurb_keys::{Opened, Vault};
use qurb_peer::Identity;
use std::path::Path;

/// A set-up store with no files in it.
fn set_up(root: &Path) -> (qurb_keys::MasterKey, Identity) {
    std::fs::create_dir_all(root).unwrap();
    let store_dir = store_dir(root);
    let key = match Vault::at(&store_dir).open_or_create().unwrap() {
        Opened::Created { key, .. } => key,
        Opened::Existing(key) => key,
    };
    let identity = Identity::load_or_create(&store_dir).unwrap();
    qurb_cli::config::Config::default().save(&store_dir).unwrap();
    (key, identity)
}

/// A replica must not write files into its directory. It holds chunks; a
/// replica that materialised a library would be a copy of someone's files on a
/// machine they do not sit at, which is the opposite of the point.
#[test]
fn a_replica_materialises_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("replica");
    let (key, identity) = set_up(&root);

    let daemon = Daemon::new(
        &root,
        &store_dir(&root),
        key,
        identity,
        qurb_cli::config::Config::default(),
    )
    .holding(PinSet::everything());

    // Run briefly, then stop. It has no peers, so there is nothing to do
    // except the things that would be wrong to do.
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), daemon.run()).await;
    });

    let visible: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name != ".qurb")
        .collect();
    assert!(visible.is_empty(), "a replica wrote files into its directory: {visible:?}");
}

/// The hazard that makes a replica dangerous to get wrong.
///
/// A replica's directory is empty by design. If it ran the ordinary
/// reconciliation, the walk would find nothing, conclude that every file it
/// holds had been deleted, and propagate those tombstones to every device that
/// trusts it — destroying the library it exists to protect.
#[test]
fn a_replica_does_not_tombstone_what_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("replica");
    let (key, identity) = set_up(&root);

    // Content in the store with nothing on disk: exactly a replica's state
    // after receiving a file.
    {
        let chunk_key = qurb_storage::ChunkKey::from_bytes(
            key.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes(),
        );
        let mut store =
            qurb_storage::Store::open(&store_dir(&root), chunk_key).unwrap();
        let source = dir.path().join("elsewhere.bin");
        std::fs::write(&source, b"content a replica is holding").unwrap();
        store.put_file("kept.bin", &source).unwrap();
        assert_eq!(store.db().live_paths().unwrap(), vec!["kept.bin".to_string()]);
    }

    let daemon = Daemon::new(
        &root,
        &store_dir(&root),
        key.clone(),
        identity,
        qurb_cli::config::Config::default(),
    )
    .holding(PinSet::everything());

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), daemon.run()).await;
    });

    // Still live, and still readable.
    let chunk_key = qurb_storage::ChunkKey::from_bytes(
        key.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes(),
    );
    let store = qurb_storage::Store::open(&store_dir(&root), chunk_key).unwrap();
    assert_eq!(
        store.db().live_paths().unwrap(),
        vec!["kept.bin".to_string()],
        "the replica tombstoned content it was holding"
    );
    assert_eq!(store.read_file("kept.bin").unwrap(), b"content a replica is holding");
}

/// A replica keeps every payload, because there is no folder for the bytes to
/// live in. Single-copy storage is for devices that materialise files; a store
/// with no folder is the only holder of what it has.
#[test]
fn a_replica_keeps_its_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("replica");
    let (key, _identity) = set_up(&root);

    let chunk_key = qurb_storage::ChunkKey::from_bytes(
        key.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes(),
    );
    let mut store = qurb_storage::Store::open(&store_dir(&root), chunk_key).unwrap();

    let source = dir.path().join("payload.bin");
    // Incompressible, so the size on disk says something about whether the
    // payload was kept. Half a megabyte of one repeated byte compresses to
    // about eighty, which would make this test pass on a store that had
    // thrown the content away.
    let mut data = vec![0u8; 512 * 1024];
    let mut x: u32 = 0x9e37_79b9;
    for byte in data.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *byte = x as u8;
    }
    std::fs::write(&source, &data).unwrap();
    store.put_file("payload.bin", &source).unwrap();

    let on_disk = store.db().size_totals().unwrap().1;
    assert!(
        on_disk > data.len() as u64 / 2,
        "a replica kept only {on_disk} bytes for {} of content",
        data.len()
    );
}
