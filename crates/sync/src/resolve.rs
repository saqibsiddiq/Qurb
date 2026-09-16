//! Deciding between two versions of the same path.
//!
//! Implements [decision 0005](../../../docs/decisions/0005-conflict-resolution.md)
//! and the cases it did not cover, recorded in
//! [decision 0009](../../../docs/decisions/0009-conflict-edge-cases.md).
//!
//! The rule in one sentence: **if one version has seen the other, the later one
//! wins; otherwise keep both.**
//!
//! What is deliberately *not* here is any use of wall-clock time. Device clocks
//! disagree, and ordering by timestamp lets a device with a wrong clock win or
//! lose every conflict systematically. Timestamps appear only in conflict
//! filenames, where they help a person identify a version.

use crate::clock::{Causality, VersionVector};
use crate::version::FileVersion;

/// Which of the two versions a decision refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Both sides already hold the same version. Nothing to do at all.
    InSync,
    /// Both sides hold the same *content*, but reached it independently — the
    /// same file copied onto each device, or deleted on each.
    ///
    /// No data needs to move, but the merged history must still be recorded.
    /// Leaving the two versions concurrent would mean the next edit on either
    /// side is concurrent with the other's history and raises a conflict over
    /// content that never actually disagreed.
    Merge,
    /// The remote version supersedes ours.
    TakeRemote,
    /// Ours supersedes the remote's.
    KeepLocal,
    /// Concurrent edits to different content. Both are kept.
    Conflict {
        /// The side that keeps the original path.
        keeps_path: Side,
        /// Where the other side is written instead.
        renamed_to: String,
    },
    /// One side deleted while the other edited, without either seeing the
    /// other. The edit survives and the deletion is dropped.
    Resurrect {
        /// The side holding the surviving content.
        survivor: Side,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub outcome: Outcome,
    /// The vector the resolved state should carry.
    ///
    /// For concurrent cases this is the merge of both sides, which by
    /// construction dominates each of them — so the resolution is not itself
    /// concurrent with what it resolved, and the conflict does not recur.
    pub vector: VersionVector,
}

/// Decide between a local and a remote version of the same path.
pub fn resolve(local: &FileVersion, remote: &FileVersion) -> Resolution {
    debug_assert_eq!(local.path, remote.path, "resolving different paths");

    match local.vector.compare(&remote.vector) {
        Causality::Equal => {
            Resolution { outcome: Outcome::InSync, vector: local.vector.clone() }
        }
        Causality::After => {
            Resolution { outcome: Outcome::KeepLocal, vector: local.vector.clone() }
        }
        Causality::Before => {
            Resolution { outcome: Outcome::TakeRemote, vector: remote.vector.clone() }
        }
        Causality::Concurrent => resolve_concurrent(local, remote),
    }
}

fn resolve_concurrent(local: &FileVersion, remote: &FileVersion) -> Resolution {
    let vector = local.vector.merged(&remote.vector);

    // Two devices independently reaching the same bytes is not a conflict, even
    // though neither saw the other. Copying the same file onto both devices
    // does this, and so does deleting it on both.
    if local.same_content(remote) {
        return Resolution { outcome: Outcome::Merge, vector };
    }

    match (local.is_deleted(), remote.is_deleted()) {
        // Delete against edit. The edit wins: a deletion stays recoverable in
        // the retention window, an overwritten edit does not. Discarding the
        // edit would break the one promise decision 0005 actually makes.
        (true, false) => Resolution { outcome: Outcome::Resurrect { survivor: Side::Remote }, vector },
        (false, true) => Resolution { outcome: Outcome::Resurrect { survivor: Side::Local }, vector },

        // Two different edits. Both are kept; only the question of which keeps
        // the original filename is decided automatically.
        (false, false) => {
            let keeps_path = if wins_path(local, remote) { Side::Local } else { Side::Remote };
            let loser = match keeps_path {
                Side::Local => remote,
                Side::Remote => local,
            };
            Resolution {
                outcome: Outcome::Conflict {
                    keeps_path,
                    renamed_to: conflict_path(loser),
                },
                vector,
            }
        }

        // Handled by the same-content check above: two tombstones are equal.
        (true, true) => unreachable!("two tombstones are the same content"),
    }
}

/// Which concurrent edit keeps the original filename.
///
/// Decided by content hash, which is arbitrary but has the two properties that
/// matter: every device computes the same answer without coordinating, and the
/// answer does not depend on anything unrelated to these two versions.
///
/// It is emphatically *not* a judgement about which edit the user wanted, which
/// is why the loser is kept rather than discarded.
fn wins_path(local: &FileVersion, remote: &FileVersion) -> bool {
    match (local.content.hash(), remote.content.hash()) {
        (Some(l), Some(r)) => l > r,
        _ => false,
    }
}

/// Where a losing version is written: `name.conflict-<device>-<when>.ext`.
///
/// The suffix goes before the final extension so the file still opens in the
/// application it belongs to. `report.docx` becomes
/// `report.conflict-a1b2c3d4-2026-09-09-143022.docx`, not
/// `report.docx.conflict-…`, which most systems would treat as having no known
/// type at all.
pub fn conflict_path(version: &FileVersion) -> String {
    let (dir, name) = match version.path.rfind('/') {
        Some(i) => (&version.path[..=i], &version.path[i + 1..]),
        None => ("", version.path.as_str()),
    };

    // A leading dot is part of the name, not an extension separator: `.bashrc`
    // has no extension.
    let split = name[1..].rfind('.').map(|i| i + 1);
    let (stem, ext) = match split {
        Some(i) => (&name[..i], &name[i..]),
        None => (name, ""),
    };

    format!(
        "{dir}{stem}.conflict-{}-{}{ext}",
        version.modified_by.short(),
        format_utc(version.modified_at)
    )
}

/// `YYYY-MM-DD-HHMMSS` in UTC.
///
/// Formatted by hand rather than by pulling in a date library: this is the only
/// place the project needs calendar arithmetic, and the algorithm is small and
/// exactly testable. UTC rather than local time so the same conflict produces
/// the same filename on every device.
fn format_utc(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86_400);
    let secs = unix_seconds.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}-{:02}{:02}{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// Days since the unix epoch to a calendar date.
///
/// Howard Hinnant's `civil_from_days`, which shifts the year to start in March
/// so that the leap day falls at the end and needs no special case.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // -- causally ordered: no conflict ---------------------------------------

    #[test]
    fn identical_versions_are_in_sync() {
        let l = file("a.txt", 1, vv(&[(A, 2)]), A);
        let r = file("a.txt", 1, vv(&[(A, 2)]), A);
        assert_eq!(resolve(&l, &r).outcome, Outcome::InSync);
    }

    #[test]
    fn a_version_that_has_seen_the_other_wins_outright() {
        let older = file("a.txt", 1, vv(&[(A, 1)]), A);
        let newer = file("a.txt", 2, vv(&[(A, 2)]), A);

        assert_eq!(resolve(&newer, &older).outcome, Outcome::KeepLocal);
        assert_eq!(resolve(&older, &newer).outcome, Outcome::TakeRemote);
    }

    #[test]
    fn a_deletion_that_saw_the_edit_wins() {
        // Not a conflict: whoever deleted it knew about the edit.
        let edit = file("a.txt", 1, vv(&[(A, 1)]), A);
        let delete = tomb("a.txt", vv(&[(A, 1), (B, 1)]), B);
        assert_eq!(resolve(&edit, &delete).outcome, Outcome::TakeRemote);
    }

    // -- concurrent ----------------------------------------------------------

    #[test]
    fn concurrent_identical_content_is_not_a_conflict() {
        // The same file copied onto both devices independently.
        let l = file("a.txt", 7, vv(&[(A, 1)]), A);
        let r = file("a.txt", 7, vv(&[(B, 1)]), B);

        let res = resolve(&l, &r);
        assert_eq!(res.outcome, Outcome::Merge, "no transfer, but history must still converge");
        assert_eq!(res.vector, vv(&[(A, 1), (B, 1)]), "the merge records both histories");
    }

    #[test]
    fn concurrent_deletions_are_not_a_conflict() {
        let l = tomb("a.txt", vv(&[(A, 2)]), A);
        let r = tomb("a.txt", vv(&[(B, 3)]), B);
        assert_eq!(resolve(&l, &r).outcome, Outcome::Merge);
    }

    #[test]
    fn concurrent_different_edits_keep_both() {
        let l = file("a.txt", 0xFF, vv(&[(A, 1)]), A);
        let r = file("a.txt", 0x01, vv(&[(B, 1)]), B);

        let res = resolve(&l, &r);
        match res.outcome {
            Outcome::Conflict { keeps_path, renamed_to } => {
                assert_eq!(keeps_path, Side::Local, "the higher hash keeps the path");
                assert!(renamed_to.starts_with("a.conflict-b2b2b2b2-"), "got {renamed_to}");
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn the_conflict_winner_does_not_depend_on_which_side_asks() {
        // Both devices must independently reach the same answer, or they would
        // each rename the other's copy and end up with two conflict files and
        // no original.
        let l = file("a.txt", 0xFF, vv(&[(A, 1)]), A);
        let r = file("a.txt", 0x01, vv(&[(B, 1)]), B);

        let from_a = resolve(&l, &r);
        let from_b = resolve(&r, &l);

        let winner_hash = |res: &Resolution, local: &FileVersion, remote: &FileVersion| {
            match &res.outcome {
                Outcome::Conflict { keeps_path: Side::Local, .. } => local.content.hash().copied(),
                Outcome::Conflict { keeps_path: Side::Remote, .. } => remote.content.hash().copied(),
                other => panic!("expected a conflict, got {other:?}"),
            }
        };
        assert_eq!(winner_hash(&from_a, &l, &r), winner_hash(&from_b, &r, &l));
    }

    #[test]
    fn a_concurrent_edit_survives_a_concurrent_delete() {
        // Decision 0005's central promise: an edit is never silently discarded.
        // A deletion stays recoverable for the retention window; an edit that
        // loses is gone.
        let edit = file("a.txt", 1, vv(&[(A, 1)]), A);
        let delete = tomb("a.txt", vv(&[(B, 1)]), B);

        assert_eq!(
            resolve(&edit, &delete).outcome,
            Outcome::Resurrect { survivor: Side::Local }
        );
        assert_eq!(
            resolve(&delete, &edit).outcome,
            Outcome::Resurrect { survivor: Side::Remote }
        );
    }

    #[test]
    fn a_resolution_dominates_both_sides_so_the_conflict_does_not_recur() {
        let l = file("a.txt", 0xFF, vv(&[(A, 3), (B, 1)]), A);
        let r = file("a.txt", 0x01, vv(&[(A, 1), (B, 4)]), B);

        let res = resolve(&l, &r);
        assert!(res.vector.dominates(&l.vector));
        assert!(res.vector.dominates(&r.vector));
    }

    // -- conflict filenames --------------------------------------------------

    #[test]
    fn the_suffix_goes_before_the_final_extension() {
        let v = file("report.docx", 1, VersionVector::new(), A);
        assert_eq!(conflict_path(&v), "report.conflict-a1a1a1a1-2025-09-10-000000.docx");
    }

    #[test]
    fn conflict_names_keep_the_directory() {
        let v = file("work/notes/plan.md", 1, VersionVector::new(), A);
        assert!(conflict_path(&v).starts_with("work/notes/plan.conflict-"));
        assert!(conflict_path(&v).ends_with(".md"));
    }

    #[test]
    fn a_file_without_an_extension_just_gets_the_suffix() {
        let v = file("README", 1, VersionVector::new(), A);
        assert_eq!(conflict_path(&v), "README.conflict-a1a1a1a1-2025-09-10-000000");
    }

    #[test]
    fn only_the_final_extension_is_preserved() {
        let v = file("archive.tar.gz", 1, VersionVector::new(), A);
        assert_eq!(conflict_path(&v), "archive.tar.conflict-a1a1a1a1-2025-09-10-000000.gz");
    }

    #[test]
    fn a_leading_dot_is_part_of_the_name_not_an_extension() {
        let v = file(".bashrc", 1, VersionVector::new(), A);
        assert_eq!(conflict_path(&v), ".bashrc.conflict-a1a1a1a1-2025-09-10-000000");
    }

    // -- date formatting -----------------------------------------------------

    #[test]
    fn dates_format_correctly() {
        // Checked against an independent implementation, including the cases
        // hand-rolled calendar arithmetic usually gets wrong.
        assert_eq!(format_utc(0), "1970-01-01-000000");
        assert_eq!(format_utc(946_684_800), "2000-01-01-000000"); // a century leap year
        assert_eq!(format_utc(1_709_164_800), "2024-02-29-000000"); // a leap day
        assert_eq!(format_utc(1_757_462_400), "2025-09-10-000000");
        assert_eq!(format_utc(1_757_515_022), "2025-09-10-143702");
        assert_eq!(format_utc(1_767_225_599), "2025-12-31-235959"); // a year boundary
        assert_eq!(format_utc(2_147_483_648), "2038-01-19-031408"); // past 2^31 seconds
    }

    #[test]
    fn dates_before_the_epoch_do_not_panic() {
        // A file with a nonsense timestamp must still produce a usable name
        // rather than crashing the sync engine.
        assert_eq!(format_utc(-1), "1969-12-31-235959");
    }
}
