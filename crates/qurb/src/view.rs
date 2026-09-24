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
}

/// Where a file's bytes are, from this device's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// In the folder, openable right now.
    Here,
    /// Known about, dropped locally, and another device has it. `qurb fetch`
    /// brings it back.
    Elsewhere,
    /// In the folder, and no other device is known to hold it. Worth saying
    /// out loud: while this is true, losing this device loses the file.
    OnlyHere,
}

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
    One(Device),
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
        Ok(self
            .store
            .db()
            .trusted_peers()?
            .into_iter()
            .map(|p| Device {
                id: p.device_id,
                name: p.name,
                fingerprint: p.fingerprint[..4].iter().map(|b| format!("{b:02x}")).collect(),
                paired_at: p.paired_at,
                last_seen: p.last_seen,
            })
            .collect())
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

        Ok(match matched.len() {
            1 => Recipient::One(matched.into_iter().next().expect("one")),
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
            evicted: self.store.evicted()?.len(),
            only_here: self.store.undelivered()?.len(),
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
                let availability = match (row.here, elsewhere) {
                    (true, true) => Availability::Here,
                    (true, false) => Availability::OnlyHere,
                    (false, _) => Availability::Elsewhere,
                };
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
