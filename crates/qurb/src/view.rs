//! What an interface asks a device, when it is not asking what is happening
//! right now.
//!
//! The engine's surface is `reconcile`, `plan_against`, `apply_plan` — the
//! verbs of a sync. An interface needs nouns: what devices, what files, what is
//! available where, what happened, what is still on its way. Those questions
//! are all answerable from the index, and this is where they are asked, so that
//! no front end has to learn SQL or invent its own idea of "available".
//!
//! # Pull, not push
//!
//! Everything here is a *query*. Being told when something changes is a
//! different mechanism with a different shape — the daemon publishes its live
//! state on [`crate::status`], a `watch` channel, and an interface holds that
//! open while polling this for detail.
//!
//! Two mechanisms rather than one because they answer differently. "Syncing,
//! three devices, 40% through" changes many times a second and is only ever
//! wanted as the latest value. "What files are in this folder" changes rarely
//! and is wanted in full, in order, a page at a time. A single channel
//! carrying both would either flood the interface or make it wait.
//!
//! # Read-only, and safe while the daemon runs
//!
//! Nothing here writes. The index is SQLite in WAL mode, so these queries run
//! against a live daemon without blocking it or being blocked — which is what
//! makes it possible for a window to show a folder while it is being synced.
//! It is also what `qurb status` has always relied on.

use crate::status::State;
use qurb_storage::db::{Activity, Listed};
use qurb_storage::Store;
use qurb_sync::DeviceId;

/// A paired device, as far as the index knows.
///
/// Durable facts only. Whether a device is reachable *now* is not one of them —
/// that belongs to the daemon's live state, because it is true for as long as a
/// connection is open and false a moment later. An interface joins the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    /// Chosen by that device, so display-only — never used to decide anything.
    pub name: String,
    /// Short form of the pinned certificate fingerprint: what pairing shows,
    /// and what a person can compare between two screens.
    pub fingerprint: String,
    pub paired_at: i64,
    pub last_seen: Option<i64>,
    /// Who it is to this device: one of the same person's, a guest of this
    /// computer, or a computer this device visits (decision 0060).
    pub relation: qurb_storage::db::Relation,
    /// For a guest: how much this computer keeps for its person, sealed so
    /// that nothing here can open it.
    pub kept: Option<u64>,
    /// "phone", "computer" or "replica", as the device said (decision 0053):
    /// when pairing, or for one paired before that, at its first sync since.
    /// None until then.
    pub kind: Option<String>,
    /// For a phone whose vault this device keeps: how many files of its
    /// WhatsApp backup are kept here, and how many bytes (decision 0062).
    /// None when there is none.
    pub whatsapp: Option<(u64, u64)>,
    /// Everything this device keeps for it: files and bytes. None when none.
    pub held: Option<(u64, u64)>,
    /// The device this one was set up again as: the same person's, the same
    /// name and kind, paired after this one was last seen. Shown as that one,
    /// not beside it (the owner's item 6, 2026-10-14: the S23 was listed
    /// twice after its data was cleared and it was paired again).
    pub replaced_by: Option<DeviceId>,
    /// The device this one replaced, the other way round.
    pub earlier: Option<DeviceId>,
}

/// Mark each device that was set up again as the newer one it became.
///
/// The same person's, with the same name and kind, the newer paired after the
/// older was last seen: clearing a phone's data and pairing it again makes a
/// new identity, and the old one stays paired, with whatever was kept for it.
/// Neither is forgotten; the newer stands for both, and says what the older
/// left here. A device still seen after the newer was paired is a second
/// device, not the same one again.
fn set_up_again(devices: &mut [Device]) {
    for newer in 0..devices.len() {
        for older in 0..devices.len() {
            let (n, o) = (&devices[newer], &devices[older]);
            let same = newer != older
                && n.relation == qurb_storage::db::Relation::Own
                && o.relation == qurb_storage::db::Relation::Own
                && n.name == o.name
                && n.kind.is_some()
                && n.kind == o.kind
                && o.paired_at < n.paired_at
                && o.last_seen.is_none_or(|seen| seen <= n.paired_at);
            // The latest earlier one, if it was set up again more than once.
            let later_than_known = devices[newer]
                .earlier
                .and_then(|known| devices.iter().find(|d| d.id == known))
                .is_none_or(|known| known.paired_at < devices[older].paired_at);
            if same && later_than_known {
                let (older_id, newer_id) = (devices[older].id, devices[newer].id);
                devices[newer].earlier = Some(older_id);
                devices[older].replaced_by = Some(newer_id);
            }
        }
    }
}

/// Where a file's bytes are. Decided in the storage crate, so that the phone,
/// which does not link this one, draws the same three answers.
pub use qurb_storage::db::Availability;

/// One file, as a listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub size: u64,
    /// Unix seconds.
    pub updated_at: i64,
    pub availability: Availability,
}

/// What this device is spending, and what it is allowed to spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Storage {
    /// Files in the folder that the index knows about.
    pub files: u64,
    /// The chunk store: content with no file behind it here — evicted files,
    /// things being held for other devices, a replica's whole job.
    pub chunks: u64,
    /// The allowance. Zero means none is set.
    pub limit: u64,
    /// How many live files there are, for a listing that pages.
    pub file_count: usize,
    /// Files whose bytes this device has dropped.
    pub evicted: usize,
    /// Live files this device made that no other device is known to hold.
    pub only_here: usize,
}

impl Storage {
    /// Everything qurb costs on this disk. What a limit is measured against.
    pub fn used(&self) -> u64 {
        self.files + self.chunks
    }

    /// Whether the device is over its allowance. Always false with no limit.
    pub fn over(&self) -> bool {
        self.limit > 0 && self.used() > self.limit
    }
}

/// The answer to "which device did you mean".
///
/// Three outcomes rather than an `Option`, because the two failures need
/// different things said about them: nobody of that name is a list to show,
/// and several of that name is a request to be more specific. Collapsing them
/// into `None` would leave the caller inventing the difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipient {
    One(Box<Device>),
    /// Nothing matched. Carries every paired device, so the caller can say what
    /// the choices were rather than only that this was not one of them.
    Unknown { known: Vec<Device> },
    /// Two devices share a name. Carries them, so the caller can print the
    /// short ids that tell them apart.
    Several(Vec<Device>),
}

/// A file sent to another device that has not collected it yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub path: String,
    pub size: u64,
    pub to: DeviceId,
}

/// Read-only questions about one device's store.
///
/// Borrows rather than owns, so an interface that already has a store for
/// something else does not open a second connection to ask a question.
pub struct View<'a> {
    store: &'a Store,
    limit: u64,
}

impl<'a> View<'a> {
    /// `limit` is the configured storage allowance, which lives in the config
    /// file rather than the index — passed in because this layer does not read
    /// configuration and should not start.
    pub fn new(store: &'a Store, limit: u64) -> Self {
        Self { store, limit }
    }

    /// Every paired device.
    pub fn devices(&self) -> qurb_storage::Result<Vec<Device>> {
        let mut devices = self
            .store
            .db()
            .trusted_peers()?
            .into_iter()
            .map(|p| {
                let kept = match p.relation {
                    qurb_storage::db::Relation::Guest => {
                        let db = self.store.db();
                        let person = db.person_of(&p.device_id).ok().flatten().unwrap_or(p.device_id);
                        db.kept_for_person(&person).ok()
                    }
                    _ => None,
                };
                Device {
                    id: p.device_id,
                    name: p.name,
                    fingerprint: p.fingerprint[..4].iter().map(|b| format!("{b:02x}")).collect(),
                    paired_at: p.paired_at,
                    last_seen: p.last_seen,
                    relation: p.relation,
                    kept,
                    kind: self.store.db().peer_kind(&p.device_id).ok().flatten(),
                    whatsapp: self
                        .store
                        .db()
                        .held_outside_for(&p.device_id, &format!("{}/whatsapp", qurb_storage::db::OUTSIDE))
                        .ok()
                        .filter(|(files, _)| *files > 0),
                    held: self.store.db().held_for(&p.device_id).ok().filter(|(files, _)| *files > 0),
                    replaced_by: None,
                    earlier: None,
                }
            })
            .collect::<Vec<_>>();
        set_up_again(&mut devices);
        Ok(devices)
    }

    /// The one paired device somebody meant, or why that is not clear.
    ///
    /// Matched on the name they gave it, on the fingerprint `qurb status` and
    /// the devices screen print, and on the device id — because a person
    /// reading any of those should not have to know which one they are looking
    /// at. Case is ignored: these get retyped.
    ///
    /// One definition, used by `qurb send` and by the window, so that the two
    /// cannot disagree about which device a name refers to.
    pub fn device_named(&self, text: &str) -> qurb_storage::Result<Recipient> {
        let text = text.trim();
        let devices = self.devices()?;

        let matched: Vec<Device> = devices
            .iter()
            .filter(|d| {
                d.name.eq_ignore_ascii_case(text)
                    || d.fingerprint.eq_ignore_ascii_case(text)
                    || d.id.short().eq_ignore_ascii_case(text)
            })
            .cloned()
            .collect();

        // A device set up again shares its name with its earlier self; the
        // name means the one in use (item 6).
        let current: Vec<Device> = matched.iter().filter(|d| d.replaced_by.is_none()).cloned().collect();
        let matched = if matched.len() > 1 && current.len() == 1 { current } else { matched };

        Ok(match matched.len() {
            1 => Recipient::One(Box::new(matched.into_iter().next().expect("one"))),
            0 => Recipient::Unknown { known: devices },
            _ => Recipient::Several(matched),
        })
    }

    /// What this device is spending.
    pub fn storage(&self) -> qurb_storage::Result<Storage> {
        let usage = self.store.usage()?;
        Ok(Storage {
            files: usage.files,
            chunks: usage.chunks,
            limit: self.limit,
            file_count: self.store.db().live_count()?,
            // Counted rather than listed: the window asks often (2026-10-28).
            evicted: self.store.db().evicted_count()?,
            only_here: self.store.undelivered_count()?.0 as usize,
        })
    }

    /// A page of the shared area, in path order.
    ///
    /// `under` restricts it to one directory and everything beneath.
    pub fn files(
        &self,
        under: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> qurb_storage::Result<Vec<File>> {
        let rows = self.store.db().listing(under, limit, offset)?;
        self.decorate(rows)
    }

    /// Files whose path contains `text`. See
    /// [`Db::search`](qurb_storage::db::Db::search) for what that does and does
    /// not match.
    pub fn search(&self, text: &str, limit: usize) -> qurb_storage::Result<Vec<File>> {
        let rows = self.store.db().search(text, limit)?;
        self.decorate(rows)
    }

    /// What happened, newest first. `before` pages backwards by row id.
    pub fn activity(
        &self,
        limit: usize,
        before: Option<i64>,
    ) -> qurb_storage::Result<Vec<Activity>> {
        self.store.db().activity(limit, before)
    }

    /// The same, one line for each thing that happened to a file, as a
    /// person reads it ([`qurb_storage::db::told_once`]).
    pub fn activity_told(
        &self,
        limit: usize,
        before: Option<i64>,
    ) -> qurb_storage::Result<Vec<Activity>> {
        self.store.db().activity_told(limit, before)
    }

    /// What happened to one path. The answer to "why is this file not here?".
    pub fn history_of(&self, path: &str, limit: usize) -> qurb_storage::Result<Vec<Activity>> {
        self.store.db().activity_for(path, limit)
    }

    /// Files sent to another device that it has not collected yet.
    pub fn outgoing(&self) -> qurb_storage::Result<Vec<Outgoing>> {
        Ok(self
            .store
            .pending_deliveries()?
            .into_iter()
            .map(|(path, size, to)| Outgoing { path, size, to })
            .collect())
    }

    /// One word for how a device with this store and this live state is doing,
    /// when there is no daemon to ask.
    ///
    /// A window opened before the daemon has published anything still has to
    /// draw something, and "starting" is honest for exactly as long as that is
    /// true. Over the limit outranks it, because that is a condition of the
    /// store rather than of the process.
    pub fn resting_state(&self) -> qurb_storage::Result<State> {
        match self.storage()?.over() {
            true => Ok(State::Problem),
            false => Ok(State::Starting),
        }
    }

    /// Turn index rows into the availability an interface renders.
    ///
    /// The distinction that matters is the third one. A file that is here and
    /// nowhere else is not the same as a file that is here and also on the
    /// phone, even though both are "available", and a storage screen that
    /// offered to free the first would be offering to delete it.
    fn decorate(&self, rows: Vec<Listed>) -> qurb_storage::Result<Vec<File>> {
        rows.into_iter()
            .map(|row| {
                let elsewhere = self.store.db().replica_count(&row.content)? > 0;
                let availability = Availability::of(row.here, elsewhere);
                Ok(File {
                    path: row.path,
                    size: row.size,
                    updated_at: row.updated_at,
                    availability,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod set_up_again_tests {
    use super::*;
    use qurb_storage::db::Relation;

    fn device(seed: u8, name: &str, paired_at: i64, last_seen: Option<i64>) -> Device {
        Device {
            id: DeviceId::from_bytes([seed; 32]),
            name: name.into(),
            fingerprint: format!("{seed:02x}"),
            paired_at,
            last_seen,
            relation: Relation::Own,
            kept: None,
            kind: Some("phone".into()),
            whatsapp: None,
            held: None,
            replaced_by: None,
            earlier: None,
        }
    }

    /// The owner's S23 as the laptop had it: paired 8 Oct, last seen 12 Oct,
    /// paired again 13 Oct after its data was cleared. Listed once, as the
    /// newer, which says what the older left here.
    #[test]
    fn a_phone_set_up_again_is_one_device() {
        let mut devices = vec![
            device(1, "SM-S911B", 1_791_492_981, Some(1_791_869_229)),
            device(2, "SM-S911B", 1_791_893_368, Some(1_791_980_000)),
            device(3, "Pixel 8", 1_791_000_000, Some(1_791_980_000)),
        ];
        set_up_again(&mut devices);
        assert_eq!(devices[0].replaced_by, Some(devices[1].id));
        assert_eq!(devices[1].earlier, Some(devices[0].id));
        assert_eq!((devices[1].replaced_by, devices[2].replaced_by, devices[2].earlier), (None, None, None));
    }

    /// Two phones of the same model, both still in use, are two devices; and
    /// another person's never stands for this person's.
    #[test]
    fn two_phones_alike_are_two_devices() {
        let mut both_in_use = vec![
            device(1, "SM-S911B", 1_791_000_000, Some(1_791_980_000)),
            device(2, "SM-S911B", 1_791_500_000, Some(1_791_980_000)),
        ];
        set_up_again(&mut both_in_use);
        assert!(both_in_use.iter().all(|d| d.replaced_by.is_none()), "one hid the other");

        let mut guest = vec![device(1, "SM-S911B", 1_791_000_000, Some(1_791_100_000)), device(2, "SM-S911B", 1_791_500_000, None)];
        guest[1].relation = Relation::Guest;
        set_up_again(&mut guest);
        assert!(guest.iter().all(|d| d.replaced_by.is_none()), "a guest's phone stood for this person's");
    }
}
