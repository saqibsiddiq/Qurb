//! What a device is for.
//!
//! Until now every device was the same thing: a watched directory backed by a
//! store. That shape is wrong for the one device a peer-to-peer system most
//! needs — something always on, holding content so the others do not all have
//! to be awake at once.
//!
//! See ../../docs/decisions/0006-availability-gap.md.

/// Which paths a replica holds.
///
/// Partial replication is not a refinement to add later. "Hold everything" is
/// the expensive answer, and the useful one is usually "hold what I actually
/// reach for" — recent photos, current work. Building the selector in from the
/// start keeps that an option rather than a rewrite.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PinSet {
    prefixes: Vec<String>,
}

impl PinSet {
    /// Hold everything the peers have.
    pub fn everything() -> Self {
        Self { prefixes: Vec::new() }
    }

    /// Hold only paths under these prefixes.
    ///
    /// Matching is by path prefix at a directory boundary, so `work` covers
    /// `work/report.txt` and the directory itself, but not `workshop/x`.
    pub fn under(prefixes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let prefixes: Vec<String> = prefixes
            .into_iter()
            .map(|p| p.into().trim_end_matches('/').to_string())
            .filter(|p| !p.is_empty())
            .collect();
        Self { prefixes }
    }

    pub fn is_everything(&self) -> bool {
        self.prefixes.is_empty()
    }

    pub fn wants(&self, path: &str) -> bool {
        if self.prefixes.is_empty() {
            return true;
        }
        self.prefixes.iter().any(|prefix| {
            path == prefix
                || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
        })
    }

    pub fn prefixes(&self) -> &[String] {
        &self.prefixes
    }
}

/// What this device does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Syncs a directory a person uses. The ordinary case.
    Syncing,

    /// Holds content, with no directory behind it.
    ///
    /// A replica exists so the others do not all have to be online at once. It
    /// stores chunks, serves them, and originates nothing — it never edits,
    /// never deletes, and never advances its own clock, so it can never win a
    /// conflict or propagate a change of its own.
    ///
    /// Two behaviours must be switched off for it, and both would be
    /// destructive rather than merely wrong:
    ///
    /// **It must not materialise files.** Writing every file to disk as well as
    /// storing its chunks costs roughly twice the space for a copy nobody
    /// reads.
    ///
    /// **It must not infer deletion from an empty directory.** A syncing device
    /// decides a file is gone by not finding it on disk. A replica has nothing
    /// on disk by design, so the same inference would tombstone everything and
    /// propagate that to every device that trusted it.
    Replica(PinSet),
}

impl Role {
    pub fn is_replica(&self) -> bool {
        matches!(self, Role::Replica(_))
    }

    /// Whether this device should hold a given path.
    pub fn wants(&self, path: &str) -> bool {
        match self {
            Role::Syncing => true,
            Role::Replica(pins) => pins.wants(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_wants_everything() {
        let pins = PinSet::everything();
        assert!(pins.is_everything());
        for path in ["a.txt", "deep/nested/file.bin", ""] {
            assert!(pins.wants(path));
        }
    }

    #[test]
    fn prefixes_match_at_a_directory_boundary() {
        let pins = PinSet::under(["work", "photos/2026"]);

        assert!(pins.wants("work"));
        assert!(pins.wants("work/report.txt"));
        assert!(pins.wants("work/deep/nested.txt"));
        assert!(pins.wants("photos/2026/june/a.jpg"));

        // The trap a naive starts_with would fall into.
        assert!(!pins.wants("workshop/notes.txt"));
        assert!(!pins.wants("photos/2025/a.jpg"));
        assert!(!pins.wants("elsewhere.txt"));
    }

    #[test]
    fn a_trailing_slash_does_not_change_the_meaning() {
        assert_eq!(PinSet::under(["work/"]), PinSet::under(["work"]));
        assert!(PinSet::under(["work/"]).wants("work/a.txt"));
    }

    #[test]
    fn an_empty_prefix_is_ignored_rather_than_matching_everything() {
        // Otherwise a stray empty string in configuration silently turns a
        // careful selection into "hold the entire library".
        let pins = PinSet::under(["", "work"]);
        assert!(!pins.is_everything());
        assert!(!pins.wants("elsewhere.txt"));
    }

    #[test]
    fn a_syncing_device_wants_every_path() {
        assert!(Role::Syncing.wants("anything/at/all"));
        assert!(!Role::Syncing.is_replica());
    }
}
