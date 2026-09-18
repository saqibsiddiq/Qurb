//! What the daemon is doing, for something else to display.
//!
//! The daemon's own account of itself has always been its log, which is the
//! right thing for a terminal and useless to an interface: a person wants
//! "up to date, three devices, last synced two minutes ago", not a stream of
//! events to reconstruct it from.
//!
//! So the daemon keeps a small summary and publishes it on a
//! [`tokio::sync::watch`] channel. Watch rather than broadcast because a
//! display only ever wants the *current* state — a tray icon that fell behind
//! and had to catch up through a queue of stale summaries would be showing the
//! past. A late reader sees the latest value and nothing else, which is exactly
//! what an interface needs.

use std::path::PathBuf;
use std::time::SystemTime;

/// The headline: one word for how things are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Catching up with the filesystem at startup.
    Starting,
    /// Nothing outstanding, and at least one device was reached.
    UpToDate,
    /// Reading, writing or transferring right now.
    Working,
    /// Running, but no paired device has answered.
    ///
    /// Distinct from an error: a phone is asleep most of the time and a laptop
    /// is shut at night, so this is the ordinary state of a group of personal
    /// devices rather than a fault to alarm anyone about.
    Alone,
    /// Something went wrong that the user may need to act on.
    Problem,
}

impl State {
    /// A word for a menu title.
    pub fn summary(&self) -> &'static str {
        match self {
            State::Starting => "starting",
            State::UpToDate => "up to date",
            State::Working => "syncing",
            State::Alone => "no devices reachable",
            State::Problem => "needs attention",
        }
    }
}

/// A file that moved, for a "recently synced" list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    pub path: String,
    pub at: SystemTime,
    /// Whether it came from a peer rather than being stored locally.
    pub from_peer: bool,
}

/// Everything an interface needs, in one value.
#[derive(Debug, Clone)]
pub struct Status {
    pub state: State,
    pub root: PathBuf,
    /// This device's fingerprint, short form.
    pub identity: String,
    pub files: usize,
    pub bytes_on_disk: u64,
    /// Paired devices, and how many answered the last time we tried.
    pub peers: usize,
    pub peers_reachable: usize,
    /// Most recent first, capped — see [`Status::REMEMBERED`].
    pub recent: Vec<Recent>,
    /// The last thing that went wrong, if anything has.
    pub problem: Option<String>,
    pub last_sync: Option<SystemTime>,
}

impl Status {
    /// How many recent files to keep.
    ///
    /// A menu can show perhaps five without becoming a file manager, and this
    /// is held in memory for the life of the process, so there is no reason to
    /// remember more than a display will ask for.
    pub const REMEMBERED: usize = 10;

    pub fn starting(root: PathBuf, identity: String) -> Self {
        Self {
            state: State::Starting,
            root,
            identity,
            files: 0,
            bytes_on_disk: 0,
            peers: 0,
            peers_reachable: 0,
            recent: Vec::new(),
            problem: None,
            last_sync: None,
        }
    }

    /// Note that a file moved.
    pub fn remember(&mut self, path: impl Into<String>, from_peer: bool) {
        self.recent.insert(
            0,
            Recent { path: path.into(), at: SystemTime::now(), from_peer },
        );
        self.recent.truncate(Self::REMEMBERED);
    }

    /// Settle on a headline, given what is known.
    ///
    /// Ordering matters: a problem outranks everything, and being alone
    /// outranks being up to date, because "up to date" next to an icon that has
    /// not spoken to another device in a week is a lie of exactly the kind that
    /// makes people stop trusting a sync tool.
    pub fn settle(&mut self) {
        self.state = if self.problem.is_some() {
            State::Problem
        } else if self.peers == 0 || self.peers_reachable == 0 {
            State::Alone
        } else {
            State::UpToDate
        };
    }
}

/// The write half, held by the daemon.
pub type Publisher = tokio::sync::watch::Sender<Status>;

/// The read half, held by a display.
pub type Watcher = tokio::sync::watch::Receiver<Status>;

/// A channel carrying the daemon's status.
pub fn channel(initial: Status) -> (Publisher, Watcher) {
    tokio::sync::watch::channel(initial)
}
