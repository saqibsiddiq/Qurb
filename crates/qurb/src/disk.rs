//! The filesystem a folder is on: how big it is, how much of it is free, and
//! whether this process may write there.
//!
//! One place for it. Until 2026-10-18 the window, the tray and setup each
//! asked Linux's `statvfs` themselves, which is all a build for anything else
//! needed to fail on; Windows is asked through `GetDiskFreeSpaceExW`.

use std::path::Path;

/// Total and available bytes on the filesystem holding `dir`, or the nearest
/// folder above it that exists; `(0, 0)` if it cannot be asked.
///
/// Available rather than free: some of the free space is reserved for root,
/// and offering it to somebody choosing an allowance, or writing into it,
/// would be counting on space that cannot be had.
pub fn space(dir: &Path) -> (u64, u64) {
    let mut at = dir;
    while !at.exists() {
        match at.parent() {
            Some(parent) => at = parent,
            None => return (0, 0),
        }
    }
    asked(at).unwrap_or((0, 0))
}

/// The bytes available on the filesystem holding `dir`.
pub fn available(dir: &Path) -> u64 {
    space(dir).1
}

#[cfg(unix)]
fn asked(dir: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` writes into the struct and reads a NUL-terminated path,
    // both of which hold here. A failure leaves the struct untouched, which is
    // why the return value is checked before anything is read out of it.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        let block = stat.f_frsize as u64;
        Some(((stat.f_blocks as u64).saturating_mul(block), (stat.f_bavail as u64).saturating_mul(block)))
    }
}

#[cfg(windows)]
fn asked(dir: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: a NUL-terminated wide path, and three u64s to write into.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };
    (ok != 0).then_some((total, available))
}

/// Whether this process could create something in `dir`.
///
/// Asked of the operating system rather than inferred from the mode bits: the
/// answer depends on the effective user, the group list, and on Linux the
/// filesystem's own opinion, none of which a permission triple reports.
#[cfg(unix)]
pub fn is_writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return false };
    // SAFETY: `access` reads a NUL-terminated path and returns a status. It
    // changes nothing.
    unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

/// Whether this process could create something in `dir`: on Windows, by
/// creating something, since its access lists answer nothing simpler.
#[cfg(not(unix))]
pub fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".qurb-write-test-{}", std::process::id()));
    let made = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_has_space_and_one_not_made_yet_is_asked_of_its_parent() {
        let dir = tempfile::tempdir().unwrap();
        let (total, available) = space(dir.path());
        assert!(total > 0 && available <= total, "{total} {available}");
        assert_eq!(space(&dir.path().join("not/made/yet")).0, total);
        assert!(is_writable(dir.path()));
    }
}
