//! That the generated bindings actually contain the API.
//!
//! This exists because they once did not, and everything else passed anyway.
//!
//! `Qurb` had its methods in two `#[uniffi::export] impl` blocks. UniFFI keeps
//! only the last one and silently discards the rest: the Rust compiled, the
//! bindings generated without a warning, and eight methods were simply absent
//! from the Kotlin and Swift. Every Rust test kept passing, because they call
//! these functions directly rather than through the bindings. It surfaced only
//! when an Android app tried to call `scan()` and the compiler said there was
//! no such thing.
//!
//! So this checks the generated *text*, which is the only artefact that can
//! show the problem.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Every method an app needs, in the spelling UniFFI gives Kotlin.
const REQUIRED: &[&str] = &[
    "scan", "list", "contains", "export", "importFile", "remove", "usage", "root",
    "peers", "offerPairing", "joinPairing", "syncWithin",
];

/// Top-level functions, which live outside the object.
const REQUIRED_FUNCTIONS: &[&str] =
    &["create", "createProtected", "restore", "restoreProtected", "isSetUp", "protectionOf"];

#[test]
fn the_kotlin_bindings_expose_the_whole_api() {
    let Some(kotlin) = generate("kotlin") else { return };
    let text = std::fs::read_to_string(&kotlin).expect("read the generated Kotlin");

    let interface = between(&text, "public interface QurbInterface {", "\n}")
        .expect("QurbInterface is missing entirely");

    let missing: Vec<_> = REQUIRED
        .iter()
        .filter(|name| !interface.contains(&format!("fun `{name}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "QurbInterface is missing {missing:?} -- are there two #[uniffi::export] impl blocks?"
    );

    for name in REQUIRED_FUNCTIONS {
        assert!(text.contains(&format!("fun `{name}`")), "no top-level `{name}`");
    }

    // The callback interface an app implements for the platform keystore.
    assert!(text.contains("public interface KeyStore"), "KeyStore is not a Kotlin interface");
}

#[test]
fn the_swift_bindings_expose_the_whole_api() {
    let Some(swift) = generate("swift") else { return };
    let text = std::fs::read_to_string(&swift).expect("read the generated Swift");

    let missing: Vec<_> = REQUIRED
        .iter()
        .filter(|name| !text.contains(&format!("func {name}(")))
        .collect();
    assert!(missing.is_empty(), "the Swift is missing {missing:?}");

    assert!(text.contains("public protocol KeyStore"), "KeyStore is not a Swift protocol");
}

/// Generate bindings from the built library, or `None` if it is not there.
///
/// Returning `None` rather than failing: `cargo test` builds the test binary
/// but not necessarily the cdylib, and a test that fails because of build
/// ordering teaches nothing. It skips loudly instead.
fn generate(language: &str) -> Option<PathBuf> {
    let Some(library) = built_library() else {
        eprintln!("SKIPPED: no built library; run `cargo build -p qurb-mobile` first");
        return None;
    };

    let out = std::env::temp_dir().join(format!("qurb-bindings-{language}"));
    let _ = std::fs::remove_dir_all(&out);

    let status = Command::new(env!("CARGO_BIN_EXE_uniffi-bindgen"))
        .args(["generate", "--library"])
        .arg(&library)
        .args(["--language", language, "--no-format", "--out-dir"])
        .arg(&out)
        .status()
        .expect("run uniffi-bindgen");
    assert!(status.success(), "uniffi-bindgen failed for {language}");

    find(&out, if language == "kotlin" { "qurb_mobile.kt" } else { "qurb_mobile.swift" })
}

fn built_library() -> Option<PathBuf> {
    // target/<profile>/deps/<test binary>, so two levels up is the profile dir.
    let mut dir = std::env::current_exe().ok()?;
    dir.pop();
    dir.pop();
    for name in ["libqurb_mobile.so", "libqurb_mobile.dylib", "qurb_mobile.dll"] {
        let candidate = dir.join(name);
        if candidate.exists() {
            assert_fresh(&candidate);
            return Some(candidate);
        }
    }
    None
}

/// Refuse to check a library older than the source it claims to describe.
///
/// `cargo test` builds the rlib it links against, not necessarily the cdylib
/// beside it, so this test can be handed a library from an earlier build. That
/// is not hypothetical: the first run of this test passed against a stale one
/// minutes after the bug it exists to catch had been fixed, which would have
/// been a false pass in the other direction just as easily.
fn assert_fresh(library: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    let (Ok(lib), Ok(src)) = (modified(library), modified(&source)) else { return };

    assert!(
        lib >= src,
        "{} is older than src/lib.rs -- run `cargo build --release -p qurb-mobile` first. \
         Checking a stale library would pass or fail for reasons unrelated to the code.",
        library.display()
    );
}

fn modified(path: &Path) -> std::io::Result<std::time::SystemTime> {
    std::fs::metadata(path)?.modified()
}

fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let to = text[from..].find(end)? + from;
    Some(&text[from..to])
}
