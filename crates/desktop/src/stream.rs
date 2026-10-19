//! Large media from a person's folder, played without a copy.
//!
//! A file opened from a person's folder is written into memory-backed space
//! (`$XDG_RUNTIME_DIR/qurb-open`, decision 0067), since nothing of it may be
//! left readable on disk. That space is a slice of memory -- 1.6 GB on the
//! owner's laptop -- and on 2026-10-15 a 1.6 GB episode did not fit: the
//! write failed, and what it had written filled the space, which the
//! desktop's own sessions keep their sockets in.
//!
//! So a large video or song is not written anywhere. It is served to the
//! player on this machine, a range at a time, straight from the store: over
//! HTTP on 127.0.0.1, at an address with a token nobody else knows, for as
//! long as the folder is open. The players a desktop opens media with play
//! from an address and seek in it by asking for ranges.

use qurb_storage::{ChunkKey, Store};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// One file being served.
struct Served {
    person: qurb_sync::DeviceId,
    kind: &'static str,
    chunks: Vec<(blake3::Hash, u32)>,
    /// Where each chunk starts, then the file's size.
    starts: Vec<u64>,
    /// A store of its own, so a long play never holds the window's.
    store: Mutex<Store>,
}

impl Served {
    fn size(&self) -> u64 {
        self.starts.last().copied().unwrap_or(0)
    }
}

struct Server {
    port: u16,
    open: Mutex<HashMap<String, Arc<Served>>>,
}

static SERVER: OnceLock<Server> = OnceLock::new();

/// What a file is, if it is media a player streams: by its name, which is all
/// a person's folder says about it.
pub fn media_type(name: &str) -> Option<&'static str> {
    let extension = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "mkv" => "video/x-matroska",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "3gp" => "video/3gpp",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/x-wav",
        _ => return None,
    })
}

/// Serve `content` -- a file of `person`'s folder, called `name`, of `kind` --
/// from the store at `store_dir`, and return the address a player opens it at.
pub fn serve(
    store_dir: &Path,
    key: ChunkKey,
    person: qurb_sync::DeviceId,
    content: &blake3::Hash,
    name: &str,
    kind: &'static str,
) -> anyhow::Result<String> {
    let store = Store::open(store_dir, key)?;
    let chunks = store.held_layout(content)?.ok_or_else(|| anyhow::anyhow!("that file is not all here"))?;
    let mut starts = Vec::with_capacity(chunks.len() + 1);
    let mut at = 0u64;
    for (_, size) in &chunks {
        starts.push(at);
        at += u64::from(*size);
    }
    starts.push(at);
    let served = Arc::new(Served { person, kind, chunks, starts, store: Mutex::new(store) });

    let server = server()?;
    let token = token();
    server.open.lock().expect("streams").insert(token.clone(), served);
    let name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || ".-_".contains(c) { c } else { '_' }).collect();
    Ok(format!("http://127.0.0.1:{}/{token}/{name}", server.port))
}

/// Stop serving `person`'s files: their folder has been locked.
pub fn forget(person: &qurb_sync::DeviceId) {
    if let Some(server) = SERVER.get() {
        server.open.lock().expect("streams").retain(|_, s| s.person != *person);
    }
}

/// The address a player for `kind` is opened with: the desktop's own choice
/// of player for that type, else whatever opens addresses. On Windows,
/// whatever opens addresses: a type's player there is not asked for by type.
pub fn play(url: &str, kind: &str) -> anyhow::Result<()> {
    if cfg!(windows) {
        let _ = kind;
        return Ok(qurb_cli::platform::open(url)?);
    }
    let chosen = std::process::Command::new("xdg-mime")
        .args(["query", "default", kind])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_end_matches(".desktop").to_string())
        .filter(|s| !s.is_empty());
    match chosen {
        Some(app) => qurb_cli::platform::reap(std::process::Command::new("gtk-launch").args([app.as_str(), url]).spawn()?),
        None => qurb_cli::platform::open(url)?,
    }
    Ok(())
}

fn server() -> anyhow::Result<&'static Server> {
    if let Some(server) = SERVER.get() {
        return Ok(server);
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let _ = SERVER.set(Server { port, open: Mutex::new(HashMap::new()) });
    std::thread::Builder::new().name("qurb-stream".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = answer(stream);
            });
        }
    })?;
    Ok(SERVER.get().expect("just set"))
}

/// A token nobody can guess: 128 bits from the kernel.
fn token() -> String {
    let mut bytes = [0u8; 16];
    if getrandom(&mut bytes).is_err() {
        // Not reached on Linux; and a token from the clock is still one a
        // process on this machine would have to guess within the window.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        bytes.copy_from_slice(&now.as_nanos().to_le_bytes());
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn getrandom(bytes: &mut [u8]) -> std::io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(bytes)
}

/// The most of a request's head read: a request line and a few headers.
const HEAD: u64 = 16 * 1024;

/// One request: a range of a file, or all of it, or its headers alone.
fn answer(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(std::io::Read::take(stream.try_clone()?, HEAD));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut range = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.trim().eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
        }
    }
    let mut out = stream;
    let token = target.trim_start_matches('/').split('/').next().unwrap_or("");
    let served = SERVER.get().and_then(|s| s.open.lock().ok()?.get(token).cloned());
    let Some(served) = served.filter(|_| matches!(method, "GET" | "HEAD")) else {
        return out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    };

    let size = served.size();
    let (start, end, partial) = match range.as_deref().and_then(|r| parse_range(r, size)) {
        Some((start, end)) => (start, end, true),
        None if range.is_some() => {
            return write!(out, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
        None => (0, size, false),
    };
    let mut head = match partial {
        true => format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{size}\r\n", end.saturating_sub(1)),
        false => "HTTP/1.1 200 OK\r\n".to_string(),
    };
    head.push_str(&format!(
        "Content-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        served.kind,
        end - start
    ));
    out.write_all(head.as_bytes())?;
    if method == "HEAD" || start >= end {
        return Ok(());
    }

    let mut index = match served.starts[..served.chunks.len()].binary_search(&start) {
        Ok(i) => i,
        Err(after) => after.saturating_sub(1),
    };
    while index < served.chunks.len() && served.starts[index] < end {
        // The folder locked while this played: stopped, within a chunk. By
        // the window, or by itself -- idle ten minutes, or the computer
        // asleep longer -- which until 2026-10-18 left the address playing
        // on. Playing is using it, so the idle time starts again.
        let still = SERVER.get().and_then(|s| s.open.lock().ok().map(|open| open.contains_key(token)));
        if still != Some(true) || qurb_peer::openings::key(&served.person).is_none() {
            return Ok(());
        }
        let bytes = {
            let store = served.store.lock().map_err(|_| std::io::Error::other("a reader failed"))?;
            store.read_chunk(&served.chunks[index].0).map_err(std::io::Error::other)?
        };
        let from = start.saturating_sub(served.starts[index]) as usize;
        let to = ((end - served.starts[index]) as usize).min(bytes.len());
        // A player that seeks elsewhere closes this connection; the write
        // fails, and that is the end of this answer.
        out.write_all(&bytes[from.min(to)..to])?;
        index += 1;
    }
    Ok(())
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n`, as the half-open range it names
/// within `size`; `None` for anything else or anything outside it.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (from, to) = spec.split_once('-')?;
    let (start, end) = match (from.trim(), to.trim()) {
        ("", last) => {
            let n: u64 = last.parse().ok()?;
            (size.saturating_sub(n), size)
        }
        (first, "") => (first.parse().ok()?, size),
        (first, last) => (first.parse().ok()?, last.parse::<u64>().ok()?.saturating_add(1).min(size)),
    };
    (start < end && start < size).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_read_as_players_send_them() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 100)));
        assert_eq!(parse_range("bytes=10-19", 100), Some((10, 20)));
        assert_eq!(parse_range("bytes=90-200", 100), Some((90, 100)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 100)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=20-10", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
        assert_eq!(media_type("Lanterns_S01E08_Dirt and Stars.MKV"), Some("video/x-matroska"));
        assert_eq!(media_type("notes.pdf"), None);
    }

    fn get(url: &str, range: Option<&str>) -> (String, Vec<u8>) {
        use std::io::Read;
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut stream = TcpStream::connect(host).unwrap();
        let range = range.map(|r| format!("Range: {r}\r\n")).unwrap_or_default();
        write!(stream, "GET /{path} HTTP/1.1\r\nHost: {host}\r\n{range}\r\n").unwrap();
        let mut all = Vec::new();
        stream.read_to_end(&mut all).unwrap();
        let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&all[..split]).to_string(), all[split + 4..].to_vec())
    }

    /// A file served the way a player reads it: whole, in ranges across its
    /// chunks, from the end, and refused at a wrong token.
    #[test]
    fn a_file_is_served_a_range_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let key = ChunkKey::from_bytes([3; 32]);
        let mut store = Store::open(&dir.path().join("store"), key.clone()).unwrap();
        let mut state = 7u64;
        let film: Vec<u8> = (0..3_500_000)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (state >> 33) as u8
            })
            .collect();
        store.put_bytes("film.mkv", &film, 0).unwrap();
        let content = blake3::hash(&film);
        let person = qurb_sync::DeviceId::from_bytes([9; 32]);
        let asked = qurb_peer::openings::ask(&person);
        assert!(qurb_peer::openings::unlock(&person, &asked, [3; 32], |_| true));
        let url = serve(&dir.path().join("store"), key, person, &content, "film.mkv", "video/x-matroska").unwrap();

        let (head, body) = get(&url, None);
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert_eq!(body, film);
        let (head, body) = get(&url, Some("bytes=1000000-2999999"));
        assert!(head.starts_with("HTTP/1.1 206") && head.contains("bytes 1000000-2999999/3500000"), "{head}");
        assert_eq!(body, &film[1_000_000..3_000_000]);
        let (_, body) = get(&url, Some("bytes=-100"));
        assert_eq!(body, &film[film.len() - 100..]);
        let (head, _) = get(&url, Some("bytes=4000000-"));
        assert!(head.starts_with("HTTP/1.1 416"), "{head}");

        let wrong = url.replace(url.split('/').nth(3).unwrap(), "0000");
        assert!(get(&wrong, None).0.starts_with("HTTP/1.1 404"));

        // Locked other than from the window -- idle, or asleep -- and the
        // address plays nothing more.
        qurb_peer::openings::lock(&person);
        assert!(get(&url, None).1.is_empty(), "played on after the folder locked");
        forget(&person);
        assert!(get(&url, None).0.starts_with("HTTP/1.1 404"), "still served once forgotten");
    }
}
