//! Which of a phone's folders are backed up, as a tree (the owner's list of
//! 2026-10-24, item 1; decision 0063).
//!
//! The choices are rows of a folder, ticked or not, and when: the later
//! choice of a folder wins wherever it was made, which is all that is kept
//! in step between a phone and its computer. What a tree needs is read from
//! them here, on both: the deepest choice at or above a folder decides it,
//! so a folder inside a ticked one can be unticked, and one inside that
//! ticked again. A folder no choice reaches is not backed up. Ticking or
//! unticking a folder sets every choice inside it the same way
//! ([`Db::choose_here`](crate::db::Db::choose_here)), so that "everything
//! inside" holds for what is there and for folders added later.

use crate::db::Choice;

/// How much of a folder is backed up: the tree's tick, dash, or empty box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    All,
    Some,
    None,
}

/// Whether `inner` is `outer` or inside it.
fn at_or_under(inner: &str, outer: &str) -> bool {
    inner == outer || inner.strip_prefix(outer).is_some_and(|rest| rest.starts_with('/'))
}

/// Whether `inner` is strictly inside `outer`.
fn under(inner: &str, outer: &str) -> bool {
    inner.strip_prefix(outer).is_some_and(|rest| rest.starts_with('/'))
}

/// Whether the files directly in `folder` are backed up: the deepest choice
/// at or above it, or none.
pub fn backed_up(choices: &[Choice], folder: &str) -> bool {
    choices
        .iter()
        .filter(|c| at_or_under(folder, &c.folder))
        .max_by_key(|c| c.folder.len())
        .is_some_and(|c| c.chosen)
}

/// The tree's mark for `folder`: all of it, some of what is inside, or none.
/// Some, when a choice inside it says otherwise than it does.
pub fn tick(choices: &[Choice], folder: &str) -> Tick {
    let here = backed_up(choices, folder);
    if choices.iter().any(|c| under(&c.folder, folder) && c.chosen != here) {
        return Tick::Some;
    }
    match here {
        true => Tick::All,
        false => Tick::None,
    }
}

/// The folders to walk for a backup: each ticked one not inside another
/// ticked one. A walk from one takes the files [`backed_up`] says, leaving
/// out what is unticked inside it and taking what is ticked again deeper.
pub fn roots(choices: &[Choice]) -> Vec<String> {
    let mut ticked: Vec<&str> = choices.iter().filter(|c| c.chosen).map(|c| c.folder.as_str()).collect();
    ticked.sort_unstable();
    let mut out: Vec<String> = Vec::new();
    for folder in ticked {
        if !out.iter().any(|outer| under(folder, outer)) {
            out.push(folder.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choices(rows: &[(&str, bool)]) -> Vec<Choice> {
        rows.iter().map(|(folder, chosen)| Choice { folder: folder.to_string(), chosen: *chosen, changed_at: 0 }).collect()
    }

    /// The deepest choice decides: a folder unticked inside a ticked one,
    /// and one ticked again inside that; a name that only starts the same is
    /// not inside.
    #[test]
    fn the_deepest_choice_decides() {
        let c = choices(&[("Download", true), ("Download/ADM", false), ("Download/ADM/2025", true)]);
        assert!(backed_up(&c, "Download"));
        assert!(backed_up(&c, "Download/Telegram"), "a folder inside a ticked one, never chosen itself");
        assert!(!backed_up(&c, "Download/ADM"));
        assert!(!backed_up(&c, "Download/ADM/old"));
        assert!(backed_up(&c, "Download/ADM/2025/March"));
        assert!(!backed_up(&c, "Downloads"));
        assert!(!backed_up(&c, "Music"));
    }

    /// The tree's marks, as drawn in the redesign: a dash where something
    /// inside says otherwise.
    #[test]
    fn a_tick_a_dash_or_nothing() {
        let c = choices(&[("Download", true), ("Download/ADM", false), ("Download/ADM/2025", true), ("DCIM", true)]);
        assert_eq!(tick(&c, "Download"), Tick::Some);
        assert_eq!(tick(&c, "Download/ADM"), Tick::Some);
        assert_eq!(tick(&c, "Download/ADM/2025"), Tick::All);
        assert_eq!(tick(&c, "Download/Telegram"), Tick::All);
        assert_eq!(tick(&c, "DCIM"), Tick::All);
        assert_eq!(tick(&c, "Music"), Tick::None);
        let inner = choices(&[("Pictures/Instagram", true)]);
        assert_eq!(tick(&inner, "Pictures"), Tick::Some);
        assert_eq!(tick(&inner, "Pictures/Reddit"), Tick::None);
    }

    /// Walked once each: a ticked folder inside another ticked one is in its
    /// walk already, and one ticked again under an unticked one too.
    #[test]
    fn each_ticked_folder_not_inside_another_is_walked() {
        let c = choices(&[
            ("DCIM/Camera", true),
            ("DCIM", true),
            ("DCIMX", true),
            ("Download/ADM", false),
            ("Download/ADM/2025", true),
            ("Download", true),
            ("Music", false),
        ]);
        assert_eq!(roots(&c), vec!["DCIM", "DCIMX", "Download"]);
        assert_eq!(roots(&choices(&[("Pictures", false), ("Pictures/Instagram", true)])), vec!["Pictures/Instagram"]);
    }
}
