//! Throwaway: where a phone's serving of a large file spends its time.
//!
//! ```text
//! phone-serving prepare <dir> <MiB> [sealed|file]   a device holding one random file
//! phone-serving id <dir>                            make a client identity, print its fingerprint
//! phone-serving read <dir>                          every chunk as a request would: no network
//! phone-serving serve <dir> <port> <client-fp>      answer that client until killed
//! phone-serving fetch <dir> <addr> <server-fp>      fetch the file three times, to nowhere
//! phone-serving loop <dir>                          serve and fetch on loopback, in one process
//! phone-serving blast-send <port>                   wait for a datagram, answer with a stream of them
//! phone-serving blast-recv <addr> <MB> <MB/s>       ask for MB at a paced rate, count what arrives
//! ```
//!
//! `WORKERS=n` sets the runtime's worker threads (the app uses two).
//! See README.md.

use anyhow::{bail, Context, Result};
use qurb_peer::{Fingerprint, Identity, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Both ends share a key, as one person's devices do.
const KEY: [u8; 32] = [42; 32];

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).with_context(|| format!("argument {i} missing"));
    let workers: usize = std::env::var("WORKERS").ok().and_then(|w| w.parse().ok()).unwrap_or(2);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()?;

    match args.first().map(String::as_str) {
        Some("prepare") => prepare(
            Path::new(arg(1)?),
            arg(2)?.parse()?,
            args.get(3).map(String::as_str) == Some("file"),
        ),
        Some("id") => {
            println!("{}", client_identity(Path::new(arg(1)?))?.fingerprint());
            Ok(())
        }
        Some("read") => read(Path::new(arg(1)?)),
        Some("serve") => {
            runtime.block_on(serve(Path::new(arg(1)?), arg(2)?.parse()?, fingerprint(arg(3)?)?))
        }
        Some("fetch") => {
            runtime.block_on(fetch(Path::new(arg(1)?), arg(2)?.parse()?, fingerprint(arg(3)?)?))
        }
        Some("loop") => runtime.block_on(on_loopback(Path::new(arg(1)?))),
        Some("blast-send") => blast_send(arg(1)?.parse()?),
        Some("blast-recv") => blast_recv(arg(1)?.parse()?, arg(2)?.parse()?, arg(3)?.parse()?),
        _ => bail!("usage: see the top of src/main.rs"),
    }
}

fn store_dir(dir: &Path) -> PathBuf {
    dir.join("sync").join(".qurb")
}

/// The device's store, opened the way it was prepared: attached to its folder
/// when the file is kept there, or not, so every chunk is a sealed one.
fn open(dir: &Path) -> Result<Store> {
    let store = Store::open(&store_dir(dir), ChunkKey::from_bytes(KEY))?;
    Ok(match std::fs::read_to_string(dir.join("mode"))?.trim() {
        "file" => store.in_tree(dir.join("sync")),
        _ => store,
    })
}

fn client_identity(dir: &Path) -> Result<Identity> {
    let at = dir.join("client");
    std::fs::create_dir_all(&at)?;
    Ok(Identity::load_or_create(&at)?)
}

fn fingerprint(hex: &str) -> Result<Fingerprint> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<std::result::Result<_, _>>()?;
    Ok(Fingerprint::from_bytes(bytes.try_into().map_err(|_| anyhow::anyhow!("not 32 bytes"))?))
}

/// Not random enough for anything but defeating compression and dedup.
fn noise(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(len);
    out
}

fn prepare(dir: &Path, mib: usize, file: bool) -> Result<()> {
    let root = dir.join("sync");
    std::fs::create_dir_all(&root)?;
    let data = noise(mib * 1024 * 1024);
    let started = Instant::now();
    if file {
        std::fs::write(root.join("bench.bin"), &data)?;
        std::fs::write(dir.join("mode"), "file")?;
        open(dir)?.put_file("bench.bin", &root.join("bench.bin"))?;
    } else {
        std::fs::write(dir.join("mode"), "sealed")?;
        open(dir)?.put_bytes("bench.bin", &data, 0)?;
    }
    let identity = Identity::load_or_create(&store_dir(dir))?;
    println!("stored {mib} MiB ({}) in {:.1}s", if file { "file" } else { "sealed" },
        started.elapsed().as_secs_f64());
    println!("server fingerprint {}", identity.fingerprint());
    Ok(())
}

/// The largest file the store holds, and its chunks.
fn largest(store: &Store) -> Result<(blake3::Hash, u64, Vec<blake3::Hash>)> {
    let (hash, size) = store
        .tree()?
        .into_iter()
        .filter_map(|v| match v.content {
            qurb_sync::Content::File { hash, size } => Some((blake3::Hash::from(hash), size)),
            qurb_sync::Content::Deleted => None,
        })
        .max_by_key(|(_, size)| *size)
        .context("no file")?;
    let chunks = store.chunk_hashes_for_content(&hash)?.context("no manifest")?;
    Ok((hash, size, chunks))
}

/// Each chunk as the server answers a request for it, timed by step.
fn read(dir: &Path) -> Result<()> {
    let store = open(dir)?;
    let (_, size, chunks) = largest(&store)?;
    let (mut visible, mut reading) = (Duration::ZERO, Duration::ZERO);
    for chunk in &chunks {
        let t = Instant::now();
        anyhow::ensure!(store.chunk_visible_to(chunk, qurb_storage::db::Audience::Unplaced)?);
        visible += t.elapsed();
        let t = Instant::now();
        let bytes = store.read_chunk(chunk)?;
        reading += t.elapsed();
        std::hint::black_box(bytes);
    }
    let total = visible + reading;
    println!(
        "{} chunks, {size} bytes: visibility {:.1} ms/chunk, read+open+check {:.1} ms/chunk, {:.1} MB/s overall",
        chunks.len(),
        visible.as_secs_f64() * 1e3 / chunks.len() as f64,
        reading.as_secs_f64() * 1e3 / chunks.len() as f64,
        size as f64 / total.as_secs_f64() / 1e6,
    );
    Ok(())
}

async fn serve(dir: &Path, port: u16, client: Fingerprint) -> Result<()> {
    let identity = Identity::load_or_create(&store_dir(dir))?;
    let trust = qurb_peer::tls::TrustList::new(vec![client]);
    let server = PeerServer::bind(SocketAddr::from(([0, 0, 0, 0], port)), &identity, &trust)?;
    println!("serving {} on {}", identity.fingerprint(), server.local_addr()?);
    server.serve(Arc::new(Mutex::new(open(dir)?))).await;
    Ok(())
}

async fn fetch(dir: &Path, addr: SocketAddr, server: Fingerprint) -> Result<()> {
    let identity = client_identity(dir)?;
    let local_dir = dir.join("client").join("store");
    std::fs::create_dir_all(&local_dir)?;
    let local = Store::open(&local_dir, ChunkKey::from_bytes(KEY))?;
    let client = PeerClient::connect(addr, &identity, server).await?;
    let tree = client.tree().await?;
    let (hash, size) = tree
        .into_iter()
        .filter_map(|v| match v.content {
            qurb_sync::Content::File { hash, size } => Some((hash, size)),
            qurb_sync::Content::Deleted => None,
        })
        .max_by_key(|(_, size)| *size)
        .context("the server holds no file")?;
    for round in 1..=3 {
        let t = Instant::now();
        let n = client.fetch_content_into(&local, hash, &mut std::io::sink()).await?;
        let secs = t.elapsed().as_secs_f64();
        println!("round {round}: {n} of {size} bytes in {secs:.2}s = {:.2} MB/s", n as f64 / secs / 1e6);
    }
    client.close();
    Ok(())
}

async fn on_loopback(dir: &Path) -> Result<()> {
    let client = client_identity(dir)?.fingerprint();
    let identity = Identity::load_or_create(&store_dir(dir))?;
    let trust = qurb_peer::tls::TrustList::new(vec![client]);
    let server = PeerServer::bind("127.0.0.1:0".parse()?, &identity, &trust)?;
    let addr = server.local_addr()?;
    let store = Arc::new(Mutex::new(open(dir)?));
    tokio::spawn(async move { server.serve(store).await });
    fetch(dir, addr, identity.fingerprint()).await
}

/// Raw UDP, no congestion control: what the path carries when nothing holds
/// back. Answers to whoever asked, so the asker's firewall lets it through.
/// The request says how much and how fast: `<MB> <MB/s>`.
fn blast_send(port: u16) -> Result<()> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", port))?;
    loop {
        let mut ask = [0u8; 64];
        let (n, to) = socket.recv_from(&mut ask)?;
        let ask = String::from_utf8_lossy(&ask[..n]).to_string();
        let mut parts = ask.split_whitespace();
        let mb: f64 = parts.next().unwrap_or("50").parse()?;
        let rate: f64 = parts.next().unwrap_or("0").parse()?;
        let datagram = [7u8; 1400];
        let count = (mb * 1e6 / 1400.0) as u64;
        let started = Instant::now();
        for i in 0..count {
            if rate > 0.0 {
                let due = Duration::from_secs_f64(i as f64 * 1400.0 / (rate * 1e6));
                while started.elapsed() < due {
                    std::hint::spin_loop();
                }
            }
            let mut d = datagram;
            d[..8].copy_from_slice(&i.to_le_bytes());
            while socket.send_to(&d, to).is_err() {}
        }
        println!("sent {count} datagrams to {to} in {:.2}s", started.elapsed().as_secs_f64());
    }
}

fn blast_recv(to: SocketAddr, mb: f64, rate: f64) -> Result<()> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    socket.send_to(format!("{mb} {rate}").as_bytes(), to)?;
    let expected = (mb * 1e6 / 1400.0) as u64;
    let (mut got, mut first, mut last) = (0u64, None, Instant::now());
    let mut buf = [0u8; 2048];
    while let Ok((n, _)) = socket.recv_from(&mut buf) {
        if n == 1400 {
            got += 1;
            first.get_or_insert_with(Instant::now);
            last = Instant::now();
        }
    }
    let secs = first.map(|f| (last - f).as_secs_f64()).unwrap_or(0.0).max(1e-3);
    println!(
        "asked {mb} MB at {} : {got} of {expected} datagrams ({:.1}% lost), {:.2} MB/s received",
        if rate > 0.0 { format!("{rate} MB/s") } else { "full speed".into() },
        100.0 * (expected - got.min(expected)) as f64 / expected as f64,
        got as f64 * 1400.0 / secs / 1e6,
    );
    Ok(())
}
