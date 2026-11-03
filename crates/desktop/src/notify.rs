//! The few things worth interrupting somebody about.
//!
//! Almost nothing. A sync application that announced every file it moved would
//! be turned off within a day, and the ones that survive on a desktop are the
//! ones that stay quiet. So this raises a notification for exactly three kinds
//! of event and ignores the rest:
//!
//! - **somebody sent you a file** — a thing another person did, on purpose, for
//!   you, which is the one case where an interruption is the point;
//! - **a device collected what you sent it** — the completion of something you
//!   did, which is worth closing the loop on;
//! - **something failed** — because the alternative is finding out later.
//!
//! Ordinary syncing is silent. So is pairing, so is eviction, so is every file
//! that arrives because it was in a shared folder: those are the system working,
//! and the system working is not news.
//!
//! # Why in Rust rather than in the page
//!
//! A notification is most useful when nobody is looking at the window. Raising
//! it from the page would mean it only appeared while the page was open and
//! being polled, which is precisely when it is least needed.

use crate::Hosted;
use qurb_storage::db::{Activity, Event};
use std::sync::Arc;
use std::time::Duration;

/// How often to look for something worth saying.
///
/// Three seconds is far below the threshold at which a person would call a
/// notification late, and the query behind it is one indexed read.
const LOOK_EVERY: Duration = Duration::from_secs(3);

/// The most to raise at once.
///
/// A device that has been off for a week comes back to a great deal of history,
/// and turning that into forty notifications is worse than useless. Past this
/// the rest are skipped rather than queued: they are all in the Activity screen,
/// which is where a backlog belongs.
const AT_ONCE: usize = 3;

/// Watch the history and speak up about the parts that matter.
///
/// Starts from whatever has already happened, so launching the application does
/// not announce a week of it.
pub fn watch(hosted: Arc<Hosted>) {
    tauri::async_runtime::spawn(async move {
        let mut announced = latest(&hosted).unwrap_or(0);

        loop {
            tokio::time::sleep(LOOK_EVERY).await;

            let looked = hosted.with_store(|store| {
                let view = qurb_cli::View::new(store, 0);
                // The devices too, because history records who by id and a
                // notification has to say who by name.
                Ok((view.activity(50, None)?, view.devices()?))
            });
            let (rows, devices) = match looked {
                Ok(both) => both,
                // Not set up yet, or the store is momentarily unavailable.
                // Nothing to say either way.
                Err(_) => continue,
            };

            // Oldest first, so several arriving together read in the order they
            // happened rather than backwards.
            let fresh: Vec<&Activity> =
                rows.iter().filter(|r| r.id > announced).rev().collect();

            if let Some(newest) = fresh.iter().map(|r| r.id).max() {
                announced = newest;
            }

            // Turned off in Settings: history still advances past what was
            // not said, so turning them back on does not announce the backlog.
            let wanted = qurb_cli::Config::load(&qurb_cli::store_dir(&hosted.root()))
                .map(|c| c.notifications)
                .unwrap_or(true);
            if !wanted {
                continue;
            }

            let named = |id: &qurb_sync::DeviceId| {
                devices.iter().find(|d| &d.id == id).map(|d| d.name.clone())
            };

            for notice in fresh
                .iter()
                .filter_map(|row| worth_saying(row, row.device.as_ref().and_then(named)))
                .take(AT_ONCE)
            {
                let hosted = Arc::clone(&hosted);
                off_the_runtime(move || show(&notice, Some(hosted)));
            }
        }
    });
}

/// The id of the most recent thing that has happened, so a fresh start
/// announces nothing that is already in the past.
fn latest(hosted: &Hosted) -> Option<i64> {
    hosted
        .with_store(|store| Ok(qurb_cli::View::new(store, 0).activity(1, None)?))
        .ok()?
        .first()
        .map(|row| row.id)
}

/// What to say about one event, if anything.
struct Notice {
    title: String,
    body: String,
    /// Its one button, as the phone's has (the owner's review of 2026-11-03,
    /// N-1).
    action: Option<Action>,
}

/// What a notification's button does.
enum Action {
    /// Show the file in the file manager, at this folder.
    Show(std::path::PathBuf),
    /// Sync now, which tries what failed again.
    TryAgain,
}

/// `who` is the other device's name, already resolved: history records a device
/// by id, and an id is the right key and the wrong thing to put in a
/// notification.
fn worth_saying(row: &Activity, who: Option<String>) -> Option<Notice> {
    let what = row.path.clone().unwrap_or_default();

    match &row.kind {
        // A file somebody sent *to this device*, rather than one that arrived
        // because it was in a shared folder. The detail is what distinguishes
        // them, and it is set at the point of adoption: "sent to this device",
        // followed by "; saved to <path>" when it went to Downloads rather
        // than into the folder. Both strings are written in
        // `qurb_engine::peer`, and a change to either has to happen in both.
        // The phone's words (Notifications draft 1): who and what in the
        // title, the file or the reason under it, and one button.
        Event::Received
            if row.detail.as_deref().is_some_and(|d| d.starts_with("sent to this device")) =>
        {
            let saved = row
                .detail
                .as_deref()
                .and_then(|d| d.split_once("; saved to "))
                .map(|(_, path)| std::path::PathBuf::from(path));
            Some(Notice {
                title: match &who {
                    Some(name) => format!("{name} sent you a file"),
                    None => "Somebody sent you a file".to_string(),
                },
                body: what.rsplit('/').next().unwrap_or(&what).to_string(),
                action: saved.and_then(|p| p.parent().map(|f| Action::Show(f.to_path_buf()))),
            })
        }

        Event::Collected => Some(Notice {
            title: match &who {
                Some(name) => format!("Delivered to {name}"),
                None => "Delivered".to_string(),
            },
            body: what.rsplit('/').next().unwrap_or(&what).to_string(),
            action: None,
        }),

        Event::Failed => {
            let name = what.rsplit('/').next().unwrap_or(&what).to_string();
            let why = row.detail.clone().unwrap_or_default();
            // A send called off: choosing it again is sending it again, so no
            // button.
            match why.strip_prefix("not sent: ") {
                Some(reason) => Some(Notice { title: format!("Could not send {name}"), body: upper(reason), action: None }),
                None => Some(Notice {
                    title: if name.is_empty() { "QurbSpace could not finish something".to_string() } else { format!("Could not finish {name}") },
                    body: upper(&why),
                    action: Some(Action::TryAgain),
                }),
            }
        }

        _ => None,
    }
}

/// The first letter a capital, as a line on its own is written.
fn upper(words: &str) -> String {
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}


/// Run a blocking desktop call on a thread of its own, not on the watcher's.
///
/// Raising a notification is a blocking D-Bus round trip, and the watcher is a
/// Tokio task. Blocking one is merely rude; but when zbus is built with its
/// `tokio` feature -- which any other crate in the build can turn on -- each
/// blocking call starts a runtime of its own, and that panics on a Tokio
/// thread. It did, 2026-10-05: the first failure worth announcing killed the
/// watcher, and nothing was announced again until the program restarted.
fn off_the_runtime(call: impl FnOnce() + Send + 'static) {
    tokio::task::spawn_blocking(call);
}

/// Hand it to the desktop, and carry on if the desktop will not take it.
///
/// A missing notification daemon, a container with no session bus, a
/// notification refused: all of them mean "this person will not see a popup",
/// and none of them is a reason to stop syncing. The event is in the Activity
/// screen regardless, which is the durable record — this is only the nudge.
fn show(notice: &Notice, hosted: Option<Arc<Hosted>>) {
    let mut n = notify_rust::Notification::new();
    n.summary(&notice.title).body(&notice.body).appname("QurbSpace").icon("folder-remote");
    if let Some(action) = &notice.action {
        n.action("default", "").action("act", match action {
            Action::Show(_) => "Show",
            Action::TryAgain => "Try again",
        });
    }
    let shown = match n.show() {
        Ok(shown) => shown,
        Err(e) => {
            tracing::debug!(error = %e, "could not raise a notification");
            return;
        }
    };
    // Its button, waited for here, off the runtime (`off_the_runtime`).
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(action) = &notice.action {
        shown.wait_for_action(|chosen| {
            if chosen != "act" && chosen != "default" {
                return;
            }
            match action {
                Action::Show(folder) => {
                    let _ = qurb_cli::platform::open(folder);
                }
                Action::TryAgain => {
                    if let Some(hosted) = &hosted {
                        hosted.sync_now();
                    }
                }
            }
        });
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    let _ = (shown, hosted);
}

/// Said once, the first time the window is closed: that closing it did not
/// stop qurb, and how to stop it. Without this, a window that hides instead of
/// closing looks like a program that ignored being closed.
pub fn still_running() {
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    show(
        &Notice { title: "QurbSpace is still running".into(), body: "Quit it in Settings".into(), action: None },
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: Event, path: &str, detail: Option<&str>) -> Activity {
        Activity {
            id: 1,
            at: 0,
            kind,
            path: Some(path.to_string()),
            size: None,
            device: Some(qurb_sync::DeviceId::from_bytes([1; 32])),
            detail: detail.map(str::to_string),
            count: 1,
            started: None,
            names: Vec::new(),
        }
    }

    #[test]
    fn a_file_somebody_sent_is_worth_saying() {
        let sent = row(Event::Received, "holiday.jpg", Some("sent to this device"));
        let notice =
            worth_saying(&sent, Some("phone".into())).expect("a delivery should be announced");
        assert_eq!(notice.title, "phone sent you a file");
        assert_eq!(notice.body, "holiday.jpg");
    }

    /// On a desktop a delivery goes to Downloads: the file's name, and *Show*
    /// opens that folder (N-1).
    #[test]
    fn a_file_saved_to_downloads_can_be_shown() {
        let sent = row(
            Event::Received,
            "photo.jpg",
            Some("sent to this device; saved to /data/Downloads/qurb/photo.jpg"),
        );
        let notice = worth_saying(&sent, Some("phone".into())).expect("announced");
        assert_eq!(notice.title, "phone sent you a file");
        assert_eq!(notice.body, "photo.jpg");
        assert!(matches!(&notice.action, Some(Action::Show(f)) if f == std::path::Path::new("/data/Downloads/qurb")));
    }

    /// A device that has since been forgotten still sent you something, and the
    /// notification still has to read as a sentence.
    #[test]
    fn a_delivery_from_a_device_with_no_name_still_reads() {
        let sent = row(Event::Received, "holiday.jpg", Some("sent to this device"));
        let notice = worth_saying(&sent, None).expect("a delivery should be announced");
        assert_eq!(notice.title, "Somebody sent you a file");
    }

    /// The common case, and the one that would ruin the application if it were
    /// announced: a file arriving because it is in a folder both devices sync.
    #[test]
    fn an_ordinary_synced_file_is_not() {
        let synced = row(Event::Received, "notes.txt", None);
        assert!(worth_saying(&synced, Some("phone".into())).is_none());
    }

    #[test]
    fn failures_are_worth_saying_and_carry_the_reason() {
        let failed = row(Event::Failed, "locked.docx", Some("permission denied"));
        let notice = worth_saying(&failed, None).expect("a failure should be announced");
        assert_eq!(notice.title, "Could not finish locked.docx");
        assert_eq!(notice.body, "Permission denied");
        assert!(matches!(notice.action, Some(Action::TryAgain)));
    }

    #[test]
    fn a_collection_closes_the_loop_by_name() {
        let taken = row(Event::Collected, "tickets.pdf", None);
        let notice = worth_saying(&taken, Some("phone".into())).expect("announced");
        assert_eq!(notice.title, "Delivered to phone");
        assert_eq!(notice.body, "tickets.pdf");
    }

    #[test]
    fn the_quiet_majority_stays_quiet() {
        for kind in [
            Event::Stored,
            Event::Deleted,
            Event::Sent,
            Event::Evicted,
            Event::Restored,
            Event::Conflicted,
            Event::Paired,
            // The person's own act, done in front of them.
            Event::Cancelled,
            Event::Other("teleported".into()),
        ] {
            assert!(
                worth_saying(&row(kind.clone(), "a.txt", None), Some("phone".into())).is_none(),
                "{kind:?} should not interrupt anybody"
            );
        }
    }

    /// What zbus does with its `tokio` feature on, for every blocking call:
    /// start a runtime and block on it. Run inline from the watcher -- a Tokio
    /// task -- that panics with "Cannot start a runtime from within a
    /// runtime", as it did on 2026-10-05.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_notification_is_raised_where_blocking_is_allowed() {
        let (done, raised) = std::sync::mpsc::channel();
        let watcher = tokio::spawn(async move {
            off_the_runtime(move || {
                let own = tokio::runtime::Builder::new_multi_thread().build().unwrap();
                done.send(own.block_on(async { "raised" })).unwrap();
            });
        });
        watcher.await.expect("the watcher survives raising a notification");
        assert_eq!(raised.recv_timeout(Duration::from_secs(10)), Ok("raised"));
    }
}
