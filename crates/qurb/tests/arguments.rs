//! The command line as a person types it: the real binary, in a home folder
//! of its own.
//!
//! The folder is optional wherever `qurb` with no arguments says it is, and
//! these are the commands where it was not: `config` with nothing after it
//! panicked, and `config`, `protect` and `join` each took their first word as
//! the folder. Run here rather than through the functions behind them, because
//! the faults were all in reading the arguments.

use std::path::Path;
use std::process::{Command, Output};

/// `qurb`, with `HOME` and the config directory inside `home`, so nothing here
/// reads or writes the real list of folders.
fn qurb(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qurb"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(cwd)
        .output()
        .expect("running qurb")
}

fn ok(output: &Output) -> String {
    assert!(
        output.status.success(),
        "qurb failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn settings(root: &Path) -> String {
    std::fs::read_to_string(root.join(".qurb/config")).unwrap()
}

#[test]
fn the_folder_can_be_left_out() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let root = home.join("qurb");
    ok(&qurb(home, home, &["init", root.to_str().unwrap()]));

    // Nothing after `config`: show the settings. This panicked.
    let shown = ok(&qurb(home, home, &["config"]));
    assert!(shown.contains("limit"), "config showed no settings:\n{shown}");

    // A setting with no folder, then with one.
    ok(&qurb(home, home, &["config", "limit=1G"]));
    assert!(settings(&root).contains("limit = 1"), "{}", settings(&root));
    ok(&qurb(home, home, &["config", root.to_str().unwrap(), "name=Study"]));
    assert!(settings(&root).contains("name = Study"), "{}", settings(&root));

    // `protect` alone says how the key is kept, rather than taking nothing as
    // a folder.
    let said = ok(&qurb(home, home, &["protect"]));
    assert!(said.contains("This key is kept: file"), "{said}");

    // `join <code>` reads the code as the code: a bad one is refused for being
    // a bad code, not for being a folder that is not set up.
    let joined = qurb(home, home, &["join", "qurb1-not-a-real-code"]);
    let said = String::from_utf8_lossy(&joined.stderr);
    assert!(!joined.status.success());
    assert!(!said.contains("give the code") && !said.contains("not set up"), "{said}");
}

/// A folder set up by a relative path is listed by where it is, and an entry
/// already listed relative is read from the home folder, so the window and the
/// command line agree on which folder it is.
#[test]
fn the_folder_list_names_the_same_folder_from_anywhere() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let work = home.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let list = home.join(".config/qurb/folders");

    ok(&qurb(home, &work, &["init", "sub/qurb"]));
    let listed = std::fs::read_to_string(&list).unwrap();
    assert!(
        listed.lines().any(|line| Path::new(line) == work.join("sub/qurb")),
        "a relative folder should be listed absolute:\n{listed}"
    );

    // An entry written relative, as the window wrote one, read from somewhere
    // else entirely.
    let old = home.join("old/qurb");
    ok(&qurb(home, home, &["init", old.to_str().unwrap()]));
    std::fs::write(&list, "# folders qurb knows about, most recent first\nold/qurb\n").unwrap();
    let elsewhere = home.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let status = ok(&qurb(home, &elsewhere, &["status"]));
    assert!(status.starts_with(&old.display().to_string()), "status opened another folder:\n{status}");
}
