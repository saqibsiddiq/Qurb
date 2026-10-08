//! Folders shared with chosen devices (decision 0044).
//!
//! Three devices, each serving the others only what its own rules let that
//! device see -- the tree a real connection would carry -- and each taking
//! only what its own rules let it take. The promise under test: a device a
//! folder is not shared with never receives what is in it, cannot put itself
//! back in, and keeps what it already had.

use qurb_engine::{Engine, StoreSource};
use qurb_storage::db::Audience;
use qurb_storage::{ChunkKey, Store};
use qurb_sync::{Action, DeviceId};
use qurb_watcher::IgnoreRules;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
}

impl Device {
    fn new() -> Self {
        Self::with_privacy(false)
    }

    /// A phone: files new things privately (decision 0036), which is what
    /// qurb writing a file itself must not fall into.
    fn phone() -> Self {
        Self::with_privacy(true)
    }

    fn with_privacy(private: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");
        let mut store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32])).unwrap().in_tree(&root);
        store.set_new_files_private(private);
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn id(&self) -> DeviceId {
        self.engine.store().device_id().unwrap()
    }

    fn write(&mut self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn read(&self, rel: &str) -> Option<String> {
        fs::read_to_string(self.root.join(rel)).ok()
    }

    fn share(&mut self, folder: &str, with: &[&DeviceId]) {
        let members: BTreeSet<DeviceId> = with.iter().map(|d| **d).collect();
        self.engine.store_mut().set_sharing(folder, &members).unwrap();
    }
}

/// Sync two devices as a connection would: each is shown what the other's
/// rules allow it, and plans with its own.
fn sync(a: &mut Device, b: &mut Device) {
    for _ in 0..8 {
        let (a_id, b_id) = (a.id(), b.id());
        let shown_a = b.engine.store().tree_for(Audience::Device(&a_id)).unwrap();
        let shown_b = a.engine.store().tree_for(Audience::Device(&b_id)).unwrap();
        let a_plan = a.engine.plan_with(&shown_a, Some(&b_id)).unwrap();
        let b_plan = b.engine.plan_with(&shown_b, Some(&a_id)).unwrap();
        let idle = |plan: &[Action]| plan.iter().all(|a| matches!(a, Action::Offer { .. }));
        if idle(&a_plan) && idle(&b_plan) {
            return;
        }
        let stats = a.engine.apply_plan(&a_plan, &mut StoreSource::new(b.engine.store())).unwrap();
        assert!(stats.is_clean(), "{:?}", stats.failures);
        let stats = b.engine.apply_plan(&b_plan, &mut StoreSource::new(a.engine.store())).unwrap();
        assert!(stats.is_clean(), "{:?}", stats.failures);
    }
    panic!("did not settle");
}

fn everyone(laptop: &mut Device, phone: &mut Device, tablet: &mut Device) {
    for _ in 0..2 {
        sync(laptop, phone);
        sync(laptop, tablet);
        sync(phone, tablet);
    }
}

#[test]
fn a_folder_shared_with_some_devices_never_reaches_the_others() {
    let (mut laptop, mut phone, mut tablet) = (Device::new(), Device::new(), Device::new());
    laptop.write("Family/beach.jpg", "the beach");
    laptop.write("notes.txt", "for everyone");
    let (l, p) = (laptop.id(), phone.id());
    laptop.share("Family", &[&l, &p]);
    everyone(&mut laptop, &mut phone, &mut tablet);

    assert_eq!(phone.read("Family/beach.jpg").as_deref(), Some("the beach"));
    assert_eq!(tablet.read("Family/beach.jpg"), None, "the tablet was shown a folder not shared with it");
    assert_eq!(tablet.read("notes.txt").as_deref(), Some("for everyone"));
    // It knows the rule -- which is how it knows not to ask -- and nothing more.
    assert!(tablet.engine.store().sharing().unwrap().covering("Family/x").is_some());

    // And asking for the bytes by hash is refused as well as not listed.
    let beach = blake3::hash(b"the beach");
    let t = tablet.id();
    assert!(!laptop.engine.store().db().content_visible_to(&beach, Audience::Device(&t)).unwrap());
    assert!(laptop.engine.store().db().content_visible_to(&beach, Audience::Device(&p)).unwrap());
    // A file this small is one chunk, whose hash is the content's.
    assert!(!laptop.engine.store().db().chunk_visible_to(&beach, Audience::Device(&t)).unwrap());
    assert!(!laptop.engine.store().db().chunk_visible_to(&beach, Audience::Unplaced).unwrap());
    assert!(laptop.engine.store().db().chunk_visible_to(&beach, Audience::Device(&p)).unwrap());
}

/// The one a left-out device would try: edit the rule file to add itself.
#[test]
fn a_device_left_out_cannot_write_itself_back_in() {
    let (mut laptop, mut phone, mut tablet) = (Device::new(), Device::new(), Device::new());
    laptop.write("Family/beach.jpg", "the beach");
    let (l, p, t) = (laptop.id(), phone.id(), tablet.id());
    laptop.share("Family", &[&l, &p]);
    everyone(&mut laptop, &mut phone, &mut tablet);

    // By hand, not through set_sharing, which would refuse it outright.
    let rule = qurb_sync::sharing::rule_path("Family");
    let members: BTreeSet<DeviceId> = [l, p, t].into();
    tablet.write(&rule, &qurb_sync::sharing::encode_members(&members));
    everyone(&mut laptop, &mut phone, &mut tablet);

    assert!(!laptop.engine.store().sharing().unwrap().allows("Family/beach.jpg", &t));
    assert!(!phone.engine.store().sharing().unwrap().allows("Family/beach.jpg", &t));
    assert_eq!(tablet.read("Family/beach.jpg"), None);
}

#[test]
fn a_device_left_out_keeps_what_it_had_and_gets_nothing_new() {
    let (mut laptop, mut phone, mut tablet) = (Device::new(), Device::new(), Device::new());
    laptop.write("Family/beach.jpg", "the beach");
    everyone(&mut laptop, &mut phone, &mut tablet);
    assert_eq!(tablet.read("Family/beach.jpg").as_deref(), Some("the beach"));

    let (l, p) = (laptop.id(), phone.id());
    laptop.share("Family", &[&l, &p]);
    laptop.write("Family/beach.jpg", "the beach, cropped");
    everyone(&mut laptop, &mut phone, &mut tablet);

    assert_eq!(phone.read("Family/beach.jpg").as_deref(), Some("the beach, cropped"));
    assert_eq!(tablet.read("Family/beach.jpg").as_deref(), Some("the beach"), "it kept what it had");

    // What it changes there now goes nowhere.
    tablet.write("Family/beach.jpg", "the tablet's edit");
    tablet.write("Family/new.jpg", "added on the tablet");
    everyone(&mut laptop, &mut phone, &mut tablet);
    assert_eq!(laptop.read("Family/beach.jpg").as_deref(), Some("the beach, cropped"));
    assert_eq!(laptop.read("Family/new.jpg"), None);
    assert_eq!(phone.read("Family/new.jpg"), None);
}

#[test]
fn sharing_with_everyone_again_brings_it_back() {
    let (mut laptop, mut phone, mut tablet) = (Device::new(), Device::new(), Device::new());
    laptop.write("Family/beach.jpg", "the beach");
    let (l, p) = (laptop.id(), phone.id());
    laptop.share("Family", &[&l, &p]);
    everyone(&mut laptop, &mut phone, &mut tablet);
    assert_eq!(tablet.read("Family/beach.jpg"), None);

    // From the phone this time: any device the folder is shared with may.
    phone.engine.store_mut().clear_sharing("Family").unwrap();
    everyone(&mut laptop, &mut phone, &mut tablet);
    assert_eq!(tablet.read("Family/beach.jpg").as_deref(), Some("the beach"));
}

#[test]
fn what_cannot_be_made_is_refused_with_a_reason() {
    let (mut laptop, phone, tablet) = (Device::new(), Device::new(), Device::new());
    let (l, p, t) = (laptop.id(), phone.id(), tablet.id());
    laptop.share("work/clients", &[&l, &p]);
    let store = laptop.engine.store_mut();

    let nested = store.set_sharing("work", &[l].into()).unwrap_err().to_string();
    assert!(nested.contains("work/clients"), "{nested}");
    assert!(store.set_sharing("work/clients/acme", &[l].into()).is_err());
    assert!(store.set_sharing("Photos", &BTreeSet::new()).is_err(), "shared with nobody");
    assert!(store.set_sharing("", &[l].into()).is_err(), "the whole shared area");
    assert!(store.set_sharing(".qurb-sharing", &[l].into()).is_err());
    assert!(store.set_sharing("../outside", &[l].into()).is_err());

    // Left out of a folder, this device cannot change who it is shared with.
    store.set_sharing("Family", &[p, t].into()).unwrap();
    assert!(store.set_sharing("Family", &[l, p, t].into()).is_err());
    assert!(store.clear_sharing("Family").is_err());
}

/// The rules are bookkeeping, not files anybody put there.
#[test]
fn the_rules_are_not_listed_as_files() {
    let mut laptop = Device::new();
    let l = laptop.id();
    laptop.write("Family/beach.jpg", "the beach");
    laptop.share("Family", &[&l]);
    let db = laptop.engine.store().db();
    let listed: Vec<String> = db.listing(None, 100, 0).unwrap().into_iter().map(|f| f.path).collect();
    assert_eq!(listed, vec!["Family/beach.jpg"]);
    assert!(db.search("sharing", 10).unwrap().is_empty());
    assert!(db.evictable().unwrap().iter().all(|(p, _, _)| !p.starts_with(".qurb-sharing")));
}

/// A rule another device removed is gone, not kept: in Recently deleted it
/// would be listed as a file, and restoring it would take a device back out
/// of the folder. Seen on the laptop when the S23 shared a folder with every
/// device again.
#[test]
fn a_rule_removed_elsewhere_is_not_kept_in_recently_deleted() {
    let (mut laptop, mut phone, mut tablet) = (Device::new(), Device::phone(), Device::new());
    laptop.write("Family/beach.jpg", "the beach");
    everyone(&mut laptop, &mut phone, &mut tablet);
    let (l, p) = (laptop.id(), phone.id());
    phone.share("Family", &[&l, &p]);
    everyone(&mut laptop, &mut phone, &mut tablet);
    let rule = qurb_sync::sharing::rule_path("Family");
    assert!(laptop.read(&rule).is_some());

    phone.engine.store_mut().clear_sharing("Family").unwrap();
    everyone(&mut laptop, &mut phone, &mut tablet);

    assert_eq!(laptop.read(&rule), None, "the rule left the folder");
    assert!(laptop.engine.store().recently_deleted().unwrap().is_empty());
    assert!(laptop.engine.store().db().trash().unwrap().is_empty(), "and was not kept");
    assert_eq!(tablet.read("Family/beach.jpg").as_deref(), Some("the beach"));
}

/// One kept by an earlier build is not listed, and cannot be put back.
#[test]
fn a_rule_kept_by_an_earlier_build_is_not_listed_or_restored() {
    let mut laptop = Device::new();
    let l = laptop.id();
    laptop.write("Family/beach.jpg", "the beach");
    laptop.share("Family", &[&l]);
    let rule = qurb_sync::sharing::rule_path("Family");
    let disk = laptop.root.join(&rule);
    let store = laptop.engine.store_mut();
    let hash = store.db().folder_row(&rule).unwrap().unwrap().0.content_hash;
    let entry = qurb_storage::db::NewTrash {
        path: &rule, scope: None, content: &hash, size: 0, by: None, why: None,
    };
    let id = store.move_to_trash(&disk, &entry).unwrap().unwrap();

    assert!(store.recently_deleted().unwrap().is_empty());
    assert!(store.restore_from_trash(id).is_err());
    assert_eq!(store.empty_trash(std::time::Duration::ZERO).unwrap().0, 1, "it still expires");
}

/// Made on a phone, which files new things privately: the rule file is still
/// the shared area's, or no other device would ever see it. Found on the
/// emulator, where a phone's Save changed nothing anywhere.
#[test]
fn a_rule_made_on_a_phone_reaches_the_other_devices() {
    let (mut laptop, mut phone, mut tablet) = (Device::new(), Device::phone(), Device::new());
    laptop.write("Family/beach.jpg", "the beach");
    everyone(&mut laptop, &mut phone, &mut tablet);

    let (l, p) = (laptop.id(), phone.id());
    phone.share("Family", &[&l, &p]);
    laptop.write("Family/new.jpg", "after the rule");
    everyone(&mut laptop, &mut phone, &mut tablet);

    let t = tablet.id();
    assert!(!laptop.engine.store().sharing().unwrap().allows("Family/new.jpg", &t));
    assert_eq!(tablet.read("Family/new.jpg"), None);
    assert_eq!(phone.read("Family/new.jpg").as_deref(), Some("after the rule"));
}

/// A rule file an earlier build filed privately on a phone is put right the
/// next time the rule is saved, rather than staying invisible for ever.
#[test]
fn a_rule_left_private_by_an_earlier_build_is_put_right() {
    let (laptop, mut phone) = (Device::new(), Device::phone());
    let (l, p) = (laptop.id(), phone.id());
    // As the earlier build left it: the rule file in the phone's own vault.
    let rule = qurb_sync::sharing::rule_path("Trips");
    phone.write(&rule, &qurb_sync::sharing::encode_members(&[l, p].into()));
    assert!(phone.engine.store().db().folder_row(&rule).unwrap().unwrap().1.is_some());

    phone.share("Trips", &[&p]);
    assert!(phone.engine.store().db().folder_row(&rule).unwrap().unwrap().1.is_none(), "still private");
    assert!(!phone.engine.store().sharing().unwrap().allows("Trips/x.jpg", &l));
}
