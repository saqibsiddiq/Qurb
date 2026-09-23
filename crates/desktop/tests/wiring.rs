//! The page and the handler list agree about what the commands are called.
//!
//! This exists because of what cannot be tested here. Tauri resolves a command
//! by name at runtime, so a typo on either side compiles, links, ships, and
//! then fails as an empty screen the first time somebody opens that tab. The
//! logic behind every command has tests; the *name* is the one thing only the
//! two files together can be wrong about.
//!
//! Driving the real window would catch it, and cannot be done here: synthetic
//! input does not reach a Tauri window under a Wayland compositor. So the two
//! files are read and compared instead, which catches exactly this class of
//! mistake and nothing else. It is not a substitute for using the application.

use std::collections::BTreeSet;

fn page() -> String {
    include_str!("../ui/app.js").to_string()
}

fn wiring() -> String {
    include_str!("../src/main.rs").to_string()
}

/// Every `invoke("name")` the page makes.
fn asked_for() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for piece in page().split("invoke(\"").skip(1) {
        if let Some(name) = piece.split('"').next() {
            names.insert(name.to_string());
        }
    }
    names
}

/// Every `commands::name` registered with Tauri.
fn registered() -> BTreeSet<String> {
    let text = wiring();
    let start = text.find("generate_handler![").expect("no handler list");
    let list = &text[start..text[start..].find(']').expect("unterminated handler list") + start];

    list.split("commands::")
        .skip(1)
        .filter_map(|piece| piece.split(|c: char| !c.is_alphanumeric() && c != '_').next())
        .map(|name| name.to_string())
        .collect()
}

#[test]
fn the_page_asks_for_nothing_that_is_not_registered() {
    let missing: Vec<_> = asked_for().difference(&registered()).cloned().collect();
    assert!(
        missing.is_empty(),
        "the window calls commands that are not registered, which fails at runtime \
         as an empty screen: {missing:?}"
    );
}

#[test]
fn nothing_is_registered_that_the_page_never_asks_for() {
    // Not a correctness problem, but a command nothing calls is either dead or
    // a screen somebody forgot to finish, and both are worth noticing.
    let unused: Vec<_> = registered().difference(&asked_for()).cloned().collect();
    assert!(unused.is_empty(), "registered but never called: {unused:?}");
}

/// A sanity check on the parsing itself, so that a rename of `generate_handler`
/// or a change of quoting style fails loudly rather than silently comparing two
/// empty sets and passing.
#[test]
fn both_sides_were_actually_found() {
    assert!(asked_for().len() > 10, "found {} invocations — parsing is wrong", asked_for().len());
    assert!(registered().len() > 10, "found {} commands — parsing is wrong", registered().len());
}
