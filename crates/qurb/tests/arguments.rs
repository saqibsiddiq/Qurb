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

/// What is in Recently deleted can be deleted for good from the command line,
/// by number or by path, and is then gone from the list.
#[test]
fn a_recently_deleted_file_can_be_deleted_for_good() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let root = home.join("qurb");
    ok(&qurb(home, home, &["init", root.to_str().unwrap()]));

    // Two files deleted into the trash, as the daemon does, through the store.
    {
        let store_dir = root.join(".qurb");
        let master = qurb_keys::Vault::at(&store_dir).unlock(None).unwrap();
        let key = qurb_storage::ChunkKey::from_bytes(
            master.derive(qurb_keys::Purpose::ChunkEncryption).to_bytes(),
        );
        let mut store = qurb_storage::Store::open(&store_dir, key).unwrap().in_tree(&root);
        for name in ["one.txt", "two.txt"] {
            std::fs::write(root.join(name), name).unwrap();
            store.put_file(name, &root.join(name)).unwrap();
            store.delete_to_trash(name, None).unwrap();
        }
    }
    let listed = ok(&qurb(home, home, &["deleted"]));
    assert!(listed.contains("one.txt") && listed.contains("two.txt"), "{listed}");

    let number = listed
        .lines()
        .find(|l| l.contains("one.txt"))
        .and_then(|l| l.split_whitespace().next())
        .unwrap()
        .to_string();
    ok(&qurb(home, home, &["forget", &number]));
    ok(&qurb(home, home, &["forget", "two.txt"]));
    assert!(ok(&qurb(home, home, &["deleted"])).contains("nothing recently deleted"));

    let again = qurb(home, home, &["forget", "two.txt"]);
    assert!(!again.status.success(), "a file no longer there cannot be forgotten twice");
}

/// A command's one word is that word, not the folder: these took it as the
/// folder and said it was "not set up yet".
#[test]
fn a_lone_word_is_not_taken_for_the_folder() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let root = home.join("qurb");
    ok(&qurb(home, home, &["init", root.to_str().unwrap()]));

    for args in [&["find", "holiday"][..], &["ls", "docs"], &["activity", "notes.txt"]] {
        let said = ok(&qurb(home, home, args));
        assert!(!said.contains("not set up"), "{args:?}: {said}");
    }
    // And with the folder in front, as before.
    ok(&qurb(home, home, &["find", root.to_str().unwrap(), "holiday"]));
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

/// A folder not set up yet joins with the code another device shows, and
/// comes out holding the same key -- no 24 words typed (decision 0052).
#[test]
fn a_new_folder_joins_with_a_code_and_takes_the_key() {
    use std::io::{BufRead, BufReader};

    let home_a = tempfile::tempdir().unwrap();
    let home_a = home_a.path();
    let root_a = home_a.join("qurb");
    ok(&qurb(home_a, home_a, &["init", root_a.to_str().unwrap()]));

    let mut showing = Command::new(env!("CARGO_BIN_EXE_qurb"))
        .args(["pair", root_a.to_str().unwrap()])
        .env("HOME", home_a)
        .env("XDG_CONFIG_HOME", home_a.join(".config"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("running qurb pair");
    // The person at this device approves the one that joins (decision 0053):
    // the answer waits in the pipe until `qurb pair` asks for it.
    {
        use std::io::Write;
        showing.stdin.take().unwrap().write_all(b"y\n").unwrap();
    }
    // Read on a thread to the end, so the pipe stays open for what `pair`
    // prints once paired; the code is sent back as soon as it appears.
    let output = showing.stdout.take().unwrap();
    let (found, code) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if let Some(code) = line.split_whitespace().find(|w| w.starts_with("qurb1-")) {
                let _ = found.send(code.to_string());
            }
        }
    });
    let code = code.recv_timeout(std::time::Duration::from_secs(20)).expect("qurb pair printed a code");

    let home_b = tempfile::tempdir().unwrap();
    let home_b = home_b.path();
    let root_b = home_b.join("qurb");
    let said = ok(&qurb(home_b, home_b, &["join", root_b.to_str().unwrap(), &code]));
    assert!(said.contains("another of your devices"), "{said}");

    // The showing side finishes once paired.
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = showing.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > std::time::Duration::from_secs(20) {
            showing.kill().ok();
            panic!("qurb pair did not finish after the other device joined");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    assert!(status.success());

    let key = |root: &Path| {
        qurb_keys::Vault::at(&root.join(".qurb"))
            .unlock(None)
            .unwrap()
            .derive(qurb_keys::Purpose::ChunkEncryption)
            .to_bytes()
    };
    assert_eq!(key(&root_a), key(&root_b), "the new folder holds the same key");
}
