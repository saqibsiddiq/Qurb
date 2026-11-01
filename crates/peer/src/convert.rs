//! An Office document turned into a PDF on the computer, for the phone to show
//! (the owner's review of 2026-11-02: "laptop converts office files"): a Word
//! file, a sheet, slides. Done by LibreOffice where it is installed; where it
//! is not, nothing is converted and the phone says so. Each result kept by the
//! document's content in the system's temporary folder, so a document opened
//! again is not converted again.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The kinds converted, by their ending.
pub const OFFICE: &[&str] = &["doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp"];

/// The longest a conversion may take before it is given up.
const TIMEOUT: Duration = Duration::from_secs(90);

/// The PDF of `bytes`, a document ending in `ext`; None when it is not an
/// Office kind, LibreOffice is not here, or it could not convert it.
pub fn to_pdf(content: &blake3::Hash, bytes: &[u8], ext: &str) -> Option<Vec<u8>> {
    let ext = ext.to_ascii_lowercase();
    if !OFFICE.contains(&ext.as_str()) {
        return None;
    }
    let cache = std::env::temp_dir().join("qurb-converted");
    let done = cache.join(format!("{}.pdf", content.to_hex()));
    if let Ok(pdf) = std::fs::read(&done) {
        return Some(pdf);
    }
    let office = office()?;
    let work = cache.join(format!("work-{}", content.to_hex()));
    std::fs::create_dir_all(&work).ok()?;
    let input = work.join(format!("document.{ext}"));
    std::fs::write(&input, bytes).ok()?;
    let converted = run(&office, &input, &work).then(|| std::fs::read(work.join("document.pdf")).ok()).flatten();
    let _ = std::fs::remove_dir_all(&work);
    let pdf = converted?;
    let _ = std::fs::write(&done, &pdf);
    Some(pdf)
}

/// LibreOffice's program, by either name it is installed under.
fn office() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in ["soffice", "libreoffice"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn run(office: &Path, input: &Path, out: &Path) -> bool {
    // A profile of its own, so a LibreOffice the person has open is not
    // disturbed and does not swallow the request.
    let profile = format!("-env:UserInstallation=file://{}", out.join("profile").display());
    let Ok(mut child) = Command::new(office)
        .arg(profile)
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(out)
        .arg(input)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                return false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => return false,
        }
    }
}
