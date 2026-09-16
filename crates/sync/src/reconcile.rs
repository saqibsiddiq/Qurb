//! Comparing two devices' views of a whole tree.
//!
//! [`resolve`](crate::resolve) decides about one path. This decides about all
//! of them, and about paths only one side has heard of.

use crate::resolve::{resolve, Outcome, Side};
use crate::version::FileVersion;
use std::collections::BTreeMap;

/// Something the engine should do.
///
/// Every variant describes the *end state* rather than the decision that led to
/// it, and carries the version vector that state should hold. That matters for
/// the concurrent cases: a resolution must record that it has seen both sides,
/// or the next comparison finds the same conflict again and the two devices
/// conflict with each other forever without ever converging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Take the remote's version.
    ///
    /// If its content is a tombstone this means applying a deletion; otherwise
    /// it means fetching whatever chunks are missing and writing the file.
    Adopt { remote: FileVersion },

    /// Send ours, because the remote is behind or has never seen this path.
    Offer { local: FileVersion },

    /// Concurrent edits. Two files result, and both devices compute the same
    /// two from the same inputs without needing to agree on anything.
    Conflict {
        /// Stays at the original path.
        keeps_path: FileVersion,
        /// Written alongside it, under a conflict name.
        renamed: FileVersion,
    },

    /// Concurrent delete against edit. The edit survives; whichever side
    /// deleted the file needs it back.
    Resurrect { resolved: FileVersion },

    /// Both sides already hold the same content but reached it independently.
    ///
    /// No data moves. What changes is the recorded history: without this the
    /// two versions stay concurrent forever, and the next edit on either side
    /// raises a conflict over content that never disagreed.
    Merge { resolved: FileVersion },
}

impl Action {
    pub fn path(&self) -> &str {
        match self {
            Action::Adopt { remote } => &remote.path,
            Action::Offer { local } => &local.path,
            Action::Conflict { keeps_path, .. } => &keeps_path.path,
            Action::Resurrect { resolved } => &resolved.path,
            Action::Merge { resolved } => &resolved.path,
        }
    }
}

/// Work out what to do, given both sides' views.
///
/// Returns actions ordered by path, so the result is deterministic and two
/// devices comparing notes produce the same plan in the same order.
///
/// Paths present on only one side are not automatically "new". A path the
/// remote has never heard of may be one we deleted long ago, which is exactly
/// why tombstones are offered rather than skipped — otherwise the remote would
/// send the file back and the deletion would undo itself.
pub fn reconcile(local: &[FileVersion], remote: &[FileVersion]) -> Vec<Action> {
    let local: BTreeMap<&str, &FileVersion> =
        local.iter().map(|v| (v.path.as_str(), v)).collect();
    let remote: BTreeMap<&str, &FileVersion> =
        remote.iter().map(|v| (v.path.as_str(), v)).collect();

    let mut actions = Vec::new();
    let paths: std::collections::BTreeSet<&str> =
        local.keys().chain(remote.keys()).copied().collect();

    for path in paths {
        match (local.get(path), remote.get(path)) {
            (Some(l), Some(r)) => {
                let resolution = resolve(l, r);
                match resolution.outcome {
                    Outcome::InSync => {}
                    Outcome::Merge => {
                        // Both devices must land on byte-identical metadata, or
                        // they would still disagree about this path. The
                        // content is the same either way, so the choice only
                        // affects who is shown as having last touched it --
                        // decided by device id purely because both sides can
                        // compute it without consulting a clock.
                        let keeper = if l.modified_by <= r.modified_by { *l } else { *r };
                        let mut resolved = keeper.clone();
                        resolved.vector = resolution.vector.clone();
                        actions.push(Action::Merge { resolved });
                    }
                    Outcome::KeepLocal => actions.push(Action::Offer { local: (*l).clone() }),
                    Outcome::TakeRemote => actions.push(Action::Adopt { remote: (*r).clone() }),
                    Outcome::Conflict { keeps_path, renamed_to } => {
                        let (winner, loser) = match keeps_path {
                            Side::Local => (*l, *r),
                            Side::Remote => (*r, *l),
                        };
                        let mut keeps = winner.clone();
                        keeps.vector = resolution.vector.clone();

                        let mut renamed = loser.clone();
                        renamed.path = renamed_to;
                        renamed.vector = resolution.vector.clone();

                        actions.push(Action::Conflict { keeps_path: keeps, renamed });
                    }
                    Outcome::Resurrect { survivor } => {
                        let mut resolved = match survivor {
                            Side::Local => (*l).clone(),
                            Side::Remote => (*r).clone(),
                        };
                        resolved.vector = resolution.vector.clone();
                        actions.push(Action::Resurrect { resolved });
                    }
                }
            }
            // Only we have it. Offer it, tombstone included: a deletion the
            // remote never learned about would otherwise be undone the moment
            // it offered the file back.
            (Some(l), None) => actions.push(Action::Offer { local: (*l).clone() }),
            (None, Some(r)) => actions.push(Action::Adopt { remote: (*r).clone() }),
            (None, None) => unreachable!("path came from one of the two maps"),
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::VersionVector;
    use crate::device::DeviceId;

    const A: DeviceId = DeviceId::from_bytes([0xA1; 32]);
    const B: DeviceId = DeviceId::from_bytes([0xB2; 32]);

    fn vv(pairs: &[(DeviceId, u64)]) -> VersionVector {
        let mut v = VersionVector::new();
        for (d, n) in pairs {
            v.set(*d, *n);
        }
        v
    }

    fn file(path: &str, hash: u8, vector: VersionVector, by: DeviceId) -> FileVersion {
        FileVersion::file(path, [hash; 32], 100, vector, by, 1_757_462_400)
    }

    fn tomb(path: &str, vector: VersionVector, by: DeviceId) -> FileVersion {
        FileVersion::tombstone(path, vector, by, 1_757_462_400)
    }

    #[test]
    fn identical_trees_need_no_work() {
        let tree = vec![
            file("a.txt", 1, vv(&[(A, 1)]), A),
            file("b.txt", 2, vv(&[(A, 2)]), A),
        ];
        assert!(reconcile(&tree, &tree).is_empty());
    }

    #[test]
    fn a_file_only_we_have_is_offered() {
        let local = vec![file("mine.txt", 1, vv(&[(A, 1)]), A)];
        let actions = reconcile(&local, &[]);
        assert_eq!(actions, vec![Action::Offer { local: local[0].clone() }]);
    }

    #[test]
    fn a_file_only_they_have_is_adopted() {
        let remote = vec![file("theirs.txt", 1, vv(&[(B, 1)]), B)];
        let actions = reconcile(&[], &remote);
        assert_eq!(actions, vec![Action::Adopt { remote: remote[0].clone() }]);
    }

    #[test]
    fn a_tombstone_the_remote_has_not_seen_is_still_offered() {
        // Skipping it would be worse than useless: the remote still has the
        // file, would offer it back, and the deletion would undo itself.
        let local = vec![tomb("deleted.txt", vv(&[(A, 2)]), A)];
        let actions = reconcile(&local, &[]);
        assert_eq!(actions, vec![Action::Offer { local: local[0].clone() }]);
    }

    #[test]
    fn a_stale_local_version_is_adopted_from_the_remote() {
        let local = vec![file("a.txt", 1, vv(&[(A, 1)]), A)];
        let remote = vec![file("a.txt", 2, vv(&[(A, 1), (B, 1)]), B)];
        assert_eq!(reconcile(&local, &remote), vec![Action::Adopt { remote: remote[0].clone() }]);
    }

    #[test]
    fn concurrent_edits_produce_a_conflict_action() {
        let local = vec![file("a.txt", 0xFF, vv(&[(A, 1)]), A)];
        let remote = vec![file("a.txt", 0x01, vv(&[(B, 1)]), B)];

        match &reconcile(&local, &remote)[..] {
            [Action::Conflict { keeps_path, renamed }] => {
                assert_eq!(keeps_path.content.hash(), Some(&[0xFFu8; 32]), "higher hash keeps the path");
                assert_eq!(keeps_path.path, "a.txt");
                assert!(renamed.path.starts_with("a.conflict-"), "got {}", renamed.path);
                assert_eq!(renamed.content.hash(), Some(&[0x01u8; 32]));

                // Both results record that they have seen both histories, so
                // the next comparison finds them settled rather than conflicting.
                let merged = vv(&[(A, 1), (B, 1)]);
                assert_eq!(keeps_path.vector, merged);
                assert_eq!(renamed.vector, merged);
            }
            other => panic!("expected one conflict, got {other:?}"),
        }
    }

    #[test]
    fn a_concurrent_edit_beats_a_concurrent_delete() {
        let local = vec![file("a.txt", 1, vv(&[(A, 1)]), A)];
        let remote = vec![tomb("a.txt", vv(&[(B, 1)]), B)];

        let mut expected = local[0].clone();
        expected.vector = vv(&[(A, 1), (B, 1)]);
        assert_eq!(reconcile(&local, &remote), vec![Action::Resurrect { resolved: expected }]);
    }

    #[test]
    fn actions_are_ordered_by_path() {
        // Both devices must produce the same plan in the same order, or they
        // will apply changes in different sequences and diverge on any path
        // whose outcome depends on another.
        let local = vec![
            file("zebra.txt", 1, vv(&[(A, 1)]), A),
            file("apple.txt", 1, vv(&[(A, 1)]), A),
            file("mango.txt", 1, vv(&[(A, 1)]), A),
        ];
        let paths: Vec<_> = reconcile(&local, &[]).iter().map(|a| a.path().to_string()).collect();
        assert_eq!(paths, vec!["apple.txt", "mango.txt", "zebra.txt"]);
    }

    #[test]
    fn a_mixed_tree_produces_one_action_per_diverging_path() {
        let local = vec![
            file("same.txt", 1, vv(&[(A, 1)]), A),
            file("ours.txt", 1, vv(&[(A, 1)]), A),
            file("stale.txt", 1, vv(&[(A, 1)]), A),
        ];
        let remote = vec![
            file("same.txt", 1, vv(&[(A, 1)]), A),
            file("theirs.txt", 1, vv(&[(B, 1)]), B),
            file("stale.txt", 2, vv(&[(A, 1), (B, 1)]), B),
        ];

        let actions = reconcile(&local, &remote);
        let described: Vec<_> = actions
            .iter()
            .map(|a| {
                let kind = match a {
                    Action::Adopt { .. } => "adopt",
                    Action::Offer { .. } => "offer",
                    Action::Conflict { .. } => "conflict",
                    Action::Resurrect { .. } => "resurrect",
                    Action::Merge { .. } => "merge",
                };
                format!("{kind} {}", a.path())
            })
            .collect();

        assert_eq!(
            described,
            vec!["offer ours.txt", "adopt stale.txt", "adopt theirs.txt"],
            "same.txt agrees and needs nothing"
        );
    }
}
