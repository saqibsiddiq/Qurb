//! Sending a file into another device's vault.
//!
//! Two things have to hold at once, and they pull in opposite directions.
//!
//! The sender must *keep* the bytes: a recipient that is switched off cannot
//! collect anything, and a send that evaporated the moment the sender tidied
//! their own folder would be a promise the product did not keep.
//!
//! The sender must also not pay for it forever. Content held purely on
//! somebody else's behalf, which that somebody else has already collected, is
//! the first thing released when the disk fills — before any of this device's
//! own files.

use qurb_storage::{ChunkKey, Store};
use qurb_sync::DeviceId;

fn noisy(size: usize) -> Vec<u8> {
    let mut out = vec![0u8; size];
    let mut x: u32 = 0x9e37_79b9;
    for byte in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *byte = x as u8;
    }
    out
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    store: Store,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        std::fs::create_dir_all(&root).unwrap();
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([7; 32]))
            .unwrap()
            .in_tree(&root);
        Self { _dir: dir, root, store }
    }

    /// A file outside the synced folder, as a share sheet would hand us.
    fn loose_file(&self, name: &str, contents: &[u8]) -> std::path::PathBuf {
        let path = self.root.parent().unwrap().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
}

fn recipient() -> DeviceId {
    DeviceId::from_bytes([9; 32])
}

#[test]
fn a_file_sent_to_a_vault_is_readable_without_the_senders_folder_copy() {
    let mut fixture = Fixture::new();
    let payload = noisy(300_000);
    let source = fixture.loose_file("report.pdf", &payload);

    let stats = fixture.store.send_to_vault("report.pdf", &source, &recipient()).unwrap();
    assert!(stats.bytes_written > 0, "the payload must actually be stored");

    // Nothing was put in the sender's own folder: the recipient's path is the
    // recipient's business.
    assert!(!fixture.root.join("report.pdf").exists());

    // And the sender deleting the original changes nothing.
    std::fs::remove_file(&source).unwrap();
    let content = blake3::hash(&payload);
    let read = fixture.store.read_content(&content).unwrap().expect("still readable");
    assert_eq!(read, payload);
}

#[test]
fn a_vault_send_does_not_collide_with_the_senders_own_file_of_that_name() {
    let mut fixture = Fixture::new();
    let mine = noisy(40_000);
    let theirs = noisy(90_000);

    std::fs::write(fixture.root.join("notes.txt"), &mine).unwrap();
    fixture.store.put_file("notes.txt", &fixture.root.join("notes.txt")).unwrap();

    let source = fixture.loose_file("outgoing-notes.txt", &theirs);
    fixture.store.send_to_vault("notes.txt", &source, &recipient()).unwrap();

    // Two live rows, same path, different scopes -- and the sender's own file
    // still reads as their own.
    assert_eq!(fixture.store.db().scope_of("notes.txt").unwrap(), None);
    assert_eq!(std::fs::read(fixture.root.join("notes.txt")).unwrap(), mine);
    assert_eq!(
        fixture.store.read_content(&blake3::hash(&theirs)).unwrap().unwrap(),
        theirs
    );
}

/// The commonest send there is: a file from the synced folder, under its own
/// name, to one device. Same name *and* same bytes as the sender's own copy.
///
/// The shortcut for an unchanged file compared against whatever row the path
/// had, found the sender's own identical file, and reported "unchanged" --
/// so no vault entry was written and nothing was ever sent.
#[test]
fn sending_a_file_from_the_folder_under_its_own_name_is_recorded() {
    let mut fixture = Fixture::new();
    let payload = noisy(60_000);

    std::fs::write(fixture.root.join("notes.txt"), &payload).unwrap();
    fixture.store.put_file("notes.txt", &fixture.root.join("notes.txt")).unwrap();

    let stats =
        fixture.store.send_to_vault("notes.txt", &fixture.root.join("notes.txt"), &recipient()).unwrap();
    assert!(!stats.unchanged, "a send was mistaken for an unchanged file");
    assert_eq!(
        fixture.store.db().vault_path_for(&blake3::hash(&payload), &recipient()).unwrap(),
        Some("notes.txt".to_string()),
        "no vault entry was written"
    );
}

#[test]
fn the_same_name_can_be_sent_to_two_different_devices() {
    let mut fixture = Fixture::new();
    let first = noisy(20_000);
    let second = noisy(30_000);

    let a = fixture.loose_file("a.bin", &first);
    let b = fixture.loose_file("b.bin", &second);
    fixture.store.send_to_vault("photo.jpg", &a, &DeviceId::from_bytes([1; 32])).unwrap();
    fixture.store.send_to_vault("photo.jpg", &b, &DeviceId::from_bytes([2; 32])).unwrap();

    assert_eq!(fixture.store.read_content(&blake3::hash(&first)).unwrap().unwrap(), first);
    assert_eq!(fixture.store.read_content(&blake3::hash(&second)).unwrap().unwrap(), second);
}

#[test]
fn held_content_is_kept_until_the_recipient_has_it() {
    let mut fixture = Fixture::new();
    let payload = noisy(400_000);
    let source = fixture.loose_file("big.bin", &payload);
    fixture.store.send_to_vault("big.bin", &source, &recipient()).unwrap();

    // Nobody has collected it. Releasing must free nothing at all: this is the
    // only copy the recipient will ever get.
    let released = fixture.store.release_held_payloads().unwrap();
    assert_eq!(released.chunks_removed, 0);
    assert_eq!(released.bytes_reclaimed, 0);
    assert!(fixture.store.read_content(&blake3::hash(&payload)).unwrap().is_some());
}

#[test]
fn held_content_is_released_once_the_recipient_has_it() {
    let mut fixture = Fixture::new();
    let payload = noisy(400_000);
    let source = fixture.loose_file("big.bin", &payload);
    fixture.store.send_to_vault("big.bin", &source, &recipient()).unwrap();

    let before = fixture.store.usage().unwrap().chunks;
    assert!(before > 0);

    // The recipient reports it. Now the sender's copy is a courtesy, not a
    // lifeline.
    fixture.store.note_replica_in_vault(&blake3::hash(&payload), &recipient()).unwrap();

    let released = fixture.store.release_held_payloads().unwrap();
    assert!(released.chunks_removed > 0, "expected chunks to be released");
    assert!(released.bytes_reclaimed > 0);
    assert!(fixture.store.usage().unwrap().chunks < before);
}

#[test]
fn releasing_never_takes_the_last_copy_of_a_shared_chunk() {
    let mut fixture = Fixture::new();
    let payload = noisy(250_000);

    // The same bytes exist twice: sent to a device that has collected them,
    // and as a file in the shared area that this device has evicted and can
    // only get back from the chunk store. Deduplication means both point at
    // one payload.
    let source = fixture.loose_file("shared.bin", &payload);
    fixture.store.send_to_vault("theirs.bin", &source, &recipient()).unwrap();

    std::fs::write(fixture.root.join("mine.bin"), &payload).unwrap();
    fixture.store.put_file("mine.bin", &fixture.root.join("mine.bin")).unwrap();
    std::fs::remove_file(fixture.root.join("mine.bin")).unwrap();
    fixture.store.db().set_materialised("mine.bin", false).unwrap();

    // The recipient collected it -- into *their* vault, which this device
    // cannot read back. That frees the sender of holding it for them and
    // leaves the sender's own evicted file exactly as stranded as before.
    fixture.store.note_replica_in_vault(&blake3::hash(&payload), &recipient()).unwrap();

    let released = fixture.store.release_held_payloads().unwrap();
    assert_eq!(released.chunks_removed, 0, "that payload is somebody's only copy");
    assert_eq!(fixture.store.read_content(&blake3::hash(&payload)).unwrap().unwrap(), payload);
}

/// An index from before the `taken` record keeps everything it had received.
///
/// The migration fills the record from the received files already in the
/// index. Without that, upgrading would make every delivery a device had ever
/// deleted deliverable again the moment its tombstone expired.
#[test]
fn upgrading_remembers_every_delivery_already_taken() {
    let fixture = Fixture::new();
    let store_dir = fixture.root.join(".qurb");
    let mut store = fixture.store;

    let contents = noisy(20_000);
    let sender = DeviceId::from_bytes([0xCC; 32]);
    let mut vector = qurb_sync::VersionVector::new();
    vector.increment(sender);
    let version = qurb_sync::FileVersion {
        path: "tickets.pdf".into(),
        content: qurb_sync::Content::File {
            hash: *blake3::hash(&contents).as_bytes(),
            size: contents.len() as u64,
        },
        vector,
        modified_by: sender,
        modified_at: 1_790_000_000,
        area: qurb_sync::Area::Sent,
    };
    let landed = fixture.root.join("tickets.pdf");
    std::fs::write(&landed, &contents).unwrap();
    store.adopt_file_privately(&version, &landed, 0).unwrap();

    // Put the index back the way a build before the record left it: without
    // what V11 and every later migration added, since they all run again.
    // A migration added after V12 has to be undone here too.
    store
        .db()
        .conn()
        .execute_batch(
            "DELETE FROM taken;
             ALTER TABLE files DROP COLUMN held;
             DROP TABLE holders;
             PRAGMA user_version = 10;",
        )
        .unwrap();
    drop(store);

    let reopened = Store::open(&store_dir, ChunkKey::from_bytes([7; 32])).unwrap();
    let remembered: i64 = reopened
        .db()
        .conn()
        .query_row("SELECT count(*) FROM taken", [], |r| r.get(0))
        .unwrap();
    assert_eq!(remembered, 1, "the migration did not carry over what had been received");
}

/// A sender sees chunks being asked for, not files. This is how it knows which
/// send they belong to -- and that a chunk of its own shared file is not one.
#[test]
fn a_served_chunk_is_traced_to_the_send_it_belongs_to() {
    let mut fixture = Fixture::new();
    let payload = noisy(700_000);
    let source = fixture.loose_file("video.mp4", &payload);
    fixture.store.send_to_vault("video.mp4", &source, &recipient()).unwrap();

    let chunks = fixture.store.chunk_hashes_for_content(&blake3::hash(&payload)).unwrap().unwrap();
    assert_eq!(
        fixture.store.db().sent_file_holding(&chunks[0], &recipient()).unwrap(),
        Some(("video.mp4".to_string(), 700_000))
    );

    // Not for another device, and not for a shared file nobody sent.
    let someone_else = DeviceId::from_bytes([3; 32]);
    assert_eq!(fixture.store.db().sent_file_holding(&chunks[0], &someone_else).unwrap(), None);
    let mine: Vec<u8> = noisy(90_000).into_iter().rev().collect();
    std::fs::write(fixture.root.join("mine.bin"), mine).unwrap();
    fixture.store.put_file("mine.bin", &fixture.root.join("mine.bin")).unwrap();
    let theirs = fixture.store.db().file_by_path("mine.bin").unwrap().unwrap();
    let shared_chunk = fixture.store.db().chunk_hashes_for(theirs.id).unwrap()[0];
    assert_eq!(fixture.store.db().sent_file_holding(&shared_chunk, &recipient()).unwrap(), None);
}

/// What a device took before sends were told apart was recorded by its bytes.
/// That old send, offered again, is still not taken twice; the same bytes
/// sent since are (decision 0059).
#[test]
fn a_delivery_taken_before_the_upgrade_is_not_taken_twice() {
    let f = Fixture::new();
    let sender = recipient();
    let content = blake3::hash(b"taken before");
    f.store
        .db()
        .conn()
        .execute(
            "INSERT INTO taken (content_hash, sender, taken_at, filed_as) VALUES (?1, ?2, 1000, 'x')",
            rusqlite::params![content.as_bytes().as_slice(), sender.as_bytes().as_slice()],
        )
        .unwrap();
    let send = |at: i64| {
        let mut vector = qurb_sync::VersionVector::new();
        vector.increment(sender);
        let mut version =
            qurb_sync::FileVersion::file("old.txt", *content.as_bytes(), 12, vector, sender, at);
        version.area = qurb_sync::Area::Sent;
        version
    };
    assert!(f.store.delivery_taken(&send(900)).unwrap(), "the old send came back");
    assert!(!f.store.delivery_taken(&send(2000)).unwrap(), "a send made since was refused");
}

/// Sending the same file to the same device again is a new send: a new
/// version for the recipient to take, waiting until it has (decision 0059).
#[test]
fn the_same_file_sent_again_is_a_new_send() {
    let mut f = Fixture::new();
    let to = recipient();
    let outgoing = f.root.parent().unwrap().join("again.bin");
    std::fs::write(&outgoing, noisy(5_000)).unwrap();

    f.store.send_to_vault("again.bin", &outgoing, &to).unwrap();
    let first = f.store.db().all_versions().unwrap().into_iter().find(|v| v.path == "again.bin").unwrap();
    f.store.db().note_replica_in_vault(&blake3::hash(&noisy(5_000)), &to).unwrap();
    assert!(f.store.pending_deliveries().unwrap().is_empty(), "setup");

    let before = f.store.sent_before(std::slice::from_ref(&outgoing), &to).unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].sent_as, "again.bin");

    f.store.send_to_vault("again.bin", &outgoing, &to).unwrap();
    let second = f.store.db().all_versions().unwrap().into_iter().find(|v| v.path == "again.bin").unwrap();
    assert_ne!(first.vector, second.vector, "sent again, and nothing new to take");
    assert_eq!(f.store.pending_deliveries().unwrap().len(), 1, "counted as collected already");
}

/// Only a file the size of something sent there before is read at all.
#[test]
fn a_file_never_sent_there_is_not_mentioned() {
    let mut f = Fixture::new();
    let to = recipient();
    let sent = f.root.parent().unwrap().join("sent.bin");
    let other = f.root.parent().unwrap().join("other.bin");
    let same_size = f.root.parent().unwrap().join("same-size.bin");
    std::fs::write(&sent, noisy(4_000)).unwrap();
    std::fs::write(&other, noisy(3_000)).unwrap();
    std::fs::write(&same_size, vec![1u8; 4_000]).unwrap();
    f.store.send_to_vault("sent.bin", &sent, &to).unwrap();

    let found = f.store.sent_before(&[sent.clone(), other, same_size], &to).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file, sent);
    assert!(f.store.sent_before(&[sent], &DeviceId::from_bytes([3; 32])).unwrap().is_empty());
}
