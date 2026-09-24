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

            let named = |id: &qurb_sync::DeviceId| {
                devices.iter().find(|d| &d.id == id).map(|d| d.name.clone())
            };

            for row in fresh
                .iter()
                .filter_map(|row| worth_saying(row, row.device.as_ref().and_then(named)))
                .take(AT_ONCE)
            {
                show(&row);
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
}

/// `who` is the other device's name, already resolved: history records a device
/// by id, and an id is the right key and the wrong thing to put in a
/// notification.
fn worth_saying(row: &Activity, who: Option<String>) -> Option<Notice> {
    let what = row.path.clone().unwrap_or_default();

    match &row.kind {
        // A file somebody sent *to this device*, rather than one that arrived
        // because it was in a shared folder. The detail is what distinguishes
        // them, and it is set at the point of adoption.
        Event::Received if row.detail.as_deref() == Some("sent to this device") => Some(Notice {
            title: match &who {
                Some(name) => format!("{name} sent you a file"),
                None => "Somebody sent you a file".to_string(),
            },
            body: what,
        }),

        Event::Collected => Some(Notice {
            title: "Delivered".to_string(),
            body: match &who {
                Some(name) => format!("{name} has {what}"),
                None => format!("{what} was collected"),
            },
        }),

        Event::Failed => Some(Notice {
            title: "qurb could not finish something".to_string(),
            body: match &row.detail {
                Some(why) => format!("{what}: {why}"),
                None => what,
            },
        }),

        _ => None,
    }
}

/// Hand it to the desktop, and carry on if the desktop will not take it.
///
/// A missing notification daemon, a container with no session bus, a
/// notification refused: all of them mean "this person will not see a popup",
/// and none of them is a reason to stop syncing. The event is in the Activity
/// screen regardless, which is the durable record — this is only the nudge.
fn show(notice: &Notice) {
    let outcome = notify_rust::Notification::new()
        .summary(&notice.title)
        .body(&notice.body)
        .appname("qurb")
        .icon("folder-remote")
        .show();

    if let Err(e) = outcome {
        tracing::debug!(error = %e, "could not raise a notification");
    }
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
        assert!(notice.body.contains("permission denied"));
    }

    #[test]
    fn a_collection_closes_the_loop_by_name() {
        let taken = row(Event::Collected, "tickets.pdf", None);
        let notice = worth_saying(&taken, Some("phone".into())).expect("announced");
        assert_eq!(notice.body, "phone has tickets.pdf");
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
            Event::Other("teleported".into()),
        ] {
            assert!(
                worth_saying(&row(kind.clone(), "a.txt", None), Some("phone".into())).is_none(),
                "{kind:?} should not interrupt anybody"
            );
        }
    }
}
