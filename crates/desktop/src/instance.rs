//! One qurb per person, and a second launch showing the first.
//!
//! Closing the window leaves qurb running, because syncing is the point of it
//! and a laptop that stops syncing whenever its window is closed only syncs
//! while somebody is looking. Which means opening qurb from the applications
//! menu while it is already running must show the window that is there, not
//! start another -- a second one would find the folder's daemon lock taken and
//! could only say so.
//!
//! A Unix socket in the runtime directory, rather than a plugin: the first
//! instance listens, a second connects, says "show" and leaves. The runtime
//! directory is the user's own, mode 0700, so nobody else can ask; and the
//! only thing anyone could ask is for a window to be shown.
//!
//! Two launches at the very same moment can both become "first". The folder's
//! lock still lets only one of them sync, and the other says so in its window.
//!
//! On Windows, which has no Unix sockets in Rust's standard library, the first
//! listens on the loopback address instead and writes its port where the
//! socket would be. The running one answers "qurb", so a port left in that
//! file by one that crashed, since taken by something else, is not mistaken
//! for it.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};
#[cfg(not(unix))]
use std::net::{TcpListener as Listener, TcpStream as Stream};

/// Where the running instance listens: its socket, or on Windows its port.
pub fn socket_path() -> PathBuf {
    #[cfg(unix)]
    {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        // SAFETY: getuid cannot fail and has no preconditions.
        let uid = unsafe { libc::getuid() };
        dir.join(format!("qurb-desktop-{uid}.sock"))
    }
    // The temporary folder is the user's own on Windows.
    #[cfg(not(unix))]
    std::env::temp_dir().join("qurb-desktop.port")
}

pub enum Instance {
    /// Nobody else is running: this is qurb, and listens here.
    First(Listener),
    /// Another qurb is running and has been asked to show its window.
    AskedToShow,
}

/// Become the one instance, or ask the one already running to show itself.
pub fn claim(path: &Path) -> std::io::Result<Instance> {
    if let Some(mut running) = reach(path) {
        running.write_all(b"show\n")?;
        let _ = running.set_read_timeout(Some(std::time::Duration::from_secs(2)));
        let mut answer = [0u8; 4];
        if running.read_exact(&mut answer).is_ok() && &answer == b"qurb" {
            return Ok(Instance::AskedToShow);
        }
    }
    // Nothing answered: no socket, or one left by a qurb that did not exit
    // cleanly. Either way the path is ours now.
    let _ = std::fs::remove_file(path);
    Ok(Instance::First(bind(path)?))
}

#[cfg(unix)]
fn reach(path: &Path) -> Option<Stream> {
    Stream::connect(path).ok()
}

#[cfg(unix)]
fn bind(path: &Path) -> std::io::Result<Listener> {
    Listener::bind(path)
}

#[cfg(not(unix))]
fn reach(path: &Path) -> Option<Stream> {
    let port: u16 = std::fs::read_to_string(path).ok()?.trim().parse().ok()?;
    Stream::connect_timeout(&(std::net::Ipv4Addr::LOCALHOST, port).into(), std::time::Duration::from_secs(1)).ok()
}

#[cfg(not(unix))]
fn bind(path: &Path) -> std::io::Result<Listener> {
    let listener = Listener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    std::fs::write(path, listener.local_addr()?.port().to_string())?;
    Ok(listener)
}

/// Call `show` each time another launch asks, for the life of the process.
pub fn listen(listener: Listener, show: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for mut asking in listener.incoming().flatten() {
            let mut said = [0u8; 8];
            if asking.read(&mut said).is_ok_and(|n| said[..n].starts_with(b"show")) {
                let _ = asking.write_all(b"qurb");
                show();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn a_second_launch_asks_the_first_to_show_itself() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("qurb.sock");

        let Instance::First(listener) = claim(&path).unwrap() else { panic!("nobody was running") };
        let (shown, asked) = mpsc::channel();
        listen(listener, move || shown.send(()).unwrap());

        assert!(matches!(claim(&path).unwrap(), Instance::AskedToShow));
        asked.recv_timeout(Duration::from_secs(5)).expect("the running one was not asked");
        assert!(matches!(claim(&path).unwrap(), Instance::AskedToShow), "only the first time");
        asked.recv_timeout(Duration::from_secs(5)).expect("asked again, and not shown again");
    }

    /// A qurb that crashed leaves its socket behind, with nobody listening.
    /// That must not stop qurb ever starting again.
    #[test]
    fn a_socket_left_by_a_crash_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("qurb.sock");
        drop(bind(&path).unwrap());
        assert!(path.exists(), "the leftover this test is about");

        assert!(matches!(claim(&path).unwrap(), Instance::First(_)));
    }
}
