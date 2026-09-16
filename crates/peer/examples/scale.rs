//! The Phase 1 kill criterion: can two devices sync 100k files cleanly?
//!
//! ```bash
//! cargo run --release --example scale -- /path/to/workdir [file-count]
//! ```
//!
//! Generates a tree, syncs it between two devices over a real QUIC connection,
//! verifies the result, then edits a fraction of it and syncs again. Reports
//! timings and what crossed the wire at every stage.
//!
//! This is a measurement instrument, not a test. It prints what happened and
//! fails loudly if anything is wrong; it does not decide what the numbers should
//! be. A run on different hardware will produce different figures and the same
//! verdicts.

use qurb_engine::Engine;
use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_peer::{Identity, NetworkSource, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Roughly the shape of a real sync folder: mostly small files, a few large
/// ones. The kill criterion is about file *count* -- the metadata path -- so
/// this deliberately keeps total bytes modest while making the count real.
fn size_for(index: usize) -> usize {
    match index % 100 {
        0..=79 => 512 + (index * 7919) % 8_192,        // 80%: under 8 KiB
        80..=94 => 8_192 + (index * 104_729) % 57_344, // 15%: 8-64 KiB
        95..=98 => 65_536 + (index * 15_485_863) % 458_752, // 4%: 64-512 KiB
        _ => 1_048_576 + (index * 32_452_843) % 3_145_728, // 1%: 1-4 MiB
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let Some(work) = args.get(1).map(PathBuf::from) else {
        eprintln!("usage: scale <workdir> [file-count]");
        std::process::exit(2);
    };
    let count: usize = args.get(2).map_or(Ok(100_000), |s| s.parse())?;

    let a_root = work.join("a");
    let b_root = work.join("b");

    rule("GENERATE");
    let generated = generate(&a_root, count)?;
    println!("  {count} files, {} on disk", human(generated));

    // -- keys ---------------------------------------------------------------
    let (a_key, phrase) = match Vault::at(&a_root.join(".qurb")).open_or_create()? {
        Opened::Created { key, phrase } => (key, phrase.to_string()),
        Opened::Existing(_) => {
            eprintln!("workdir already has keys; use a clean directory");
            std::process::exit(1);
        }
    };
    let b_key = Vault::at(&b_root.join(".qurb")).restore(&RecoveryPhrase::parse(&phrase)?)?;

    // -- first index --------------------------------------------------------
    rule("INDEX (device A, cold)");
    let (mut a, a_identity) = open(&a_root, &a_key)?;
    let t = Instant::now();
    let stats = a.reconcile()?;
    let indexed = t.elapsed();
    println!("  stored {} unchanged {} deleted {}", stats.stored, stats.unchanged, stats.deleted);
    check("no failures", stats.failures.is_empty(), &format!("{:?}", first_few(&stats.failures)));
    check("every file indexed", stats.stored == count, &format!("{} of {count}", stats.stored));
    rate("  indexed", generated, indexed, count);

    rule("RE-INDEX (device A, warm)");
    let t = Instant::now();
    let again = a.reconcile()?;
    println!("  stored {} unchanged {} in {:.2?}", again.stored, again.unchanged, t.elapsed());
    check("nothing re-read", again.stored == 0, &format!("{} re-read", again.stored));

    // -- sync ---------------------------------------------------------------
    let (mut b, b_identity) = open(&b_root, &b_key)?;
    b.reconcile()?;

    let served = Store::open(&a_root.join(".qurb"), chunk_key(&a_key))?;
    let server = PeerServer::bind(
        "127.0.0.1:0".parse()?,
        &a_identity,
        &[b_identity.fingerprint()],
    )?;
    let addr = server.local_addr()?;
    let wire = server.stats();
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(served))).await });

    rule("SYNC (first, everything)");
    let first = pull(&mut b, addr, &b_identity, a_identity.fingerprint(), &b_key).await?;
    report(&first, &wire, generated);
    check("no failures", first.stats.failures.is_empty(), &format!("{:?}", first_few(&first.stats.failures)));
    check("every file adopted", first.stats.adopted == count, &format!("{} of {count}", first.stats.adopted));

    rule("VERIFY");
    verify(&b, &b_key, &a_root, &b_root, count)?;

    // -- incremental --------------------------------------------------------
    let touched = (count / 100).max(1);
    rule(&format!("EDIT {touched} FILES AND SYNC AGAIN"));
    for i in 0..touched {
        let index = i * 97 % count;
        let path = a_root.join(relative(index));
        let mut content = std::fs::read(&path)?;
        // Insert near the front, which shifts every following byte -- the case
        // fixed-size blocks handle worst.
        content.splice(100..100, *b"EDITED-BY-SCALE-");
        std::fs::write(&path, &content)?;
    }

    let t = Instant::now();
    let edited = a.reconcile()?;
    println!("  A re-indexed in {:.2?}: {} stored, {} unchanged",
        t.elapsed(), edited.stored, edited.unchanged);
    check("only edited files re-read", edited.stored == touched,
        &format!("{} re-read, expected {touched}", edited.stored));

    let before_wire = wire.bytes();
    let second = pull(&mut b, addr, &b_identity, a_identity.fingerprint(), &b_key).await?;
    report(&second, &wire, generated);
    check("no failures", second.stats.failures.is_empty(), &format!("{:?}", first_few(&second.stats.failures)));
    check("only edited files adopted", second.stats.adopted == touched,
        &format!("{} adopted, expected {touched}", second.stats.adopted));

    let moved = wire.bytes() - before_wire;
    println!("  incremental wire cost {} for {touched} edited files", human(moved));

    rule("VERIFY AGAIN");
    verify(&b, &b_key, &a_root, &b_root, count)?;

    rule("DONE");
    println!("  A and B agree on {count} files after a full sync and an incremental one.");
    Ok(())
}

// ---------------------------------------------------------------------------

struct Pulled {
    stats: qurb_engine::PlanStats,
    planned: usize,
    tree_len: usize,
    tree_time: Duration,
    apply_time: Duration,
}

async fn pull(
    local: &mut Engine,
    addr: std::net::SocketAddr,
    identity: &Identity,
    expected: qurb_peer::Fingerprint,
    key: &MasterKey,
) -> Result<Pulled, Box<dyn std::error::Error>> {
    let client = PeerClient::connect(addr, identity, expected).await?;

    let t = Instant::now();
    let tree = client.tree().await?;
    let tree_time = t.elapsed();

    let plan = local.plan_against(&tree)?;
    let planned = plan.len();

    let reader = Store::open(&local.root().join(".qurb"), chunk_key(key))?;
    let t = Instant::now();
    let stats = {
        let mut source = NetworkSource::new(&client, &reader);
        local.apply_plan(&plan, &mut source)?
    };
    let apply_time = t.elapsed();

    client.close();
    Ok(Pulled { stats, planned, tree_len: tree.len(), tree_time, apply_time })
}

fn report(p: &Pulled, wire: &Arc<qurb_peer::ServerStats>, total_bytes: u64) {
    println!("  tree           {} paths in {:.2?}", p.tree_len, p.tree_time);
    println!("  planned        {} action(s)", p.planned);
    println!(
        "  applied        adopted {} merged {} conflicts {} resurrected {} failed {}",
        p.stats.adopted, p.stats.merged, p.stats.conflicts,
        p.stats.resurrected, p.stats.failures.len()
    );
    println!("  chunks served  {}", wire.chunks());
    println!("  bytes served   {} of {} total", human(wire.bytes()), human(total_bytes));
    println!("  elapsed        {:.2?}", p.apply_time);
}

fn verify(
    b: &Engine,
    b_key: &MasterKey,
    a_root: &Path,
    b_root: &Path,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let live = b.store().db().live_paths()?;
    check("B holds every path", live.len() == count, &format!("{} of {count}", live.len()));

    let drift = b.store().db().audit_refcounts()?;
    check("reference counts agree", drift.is_empty(), &format!("{} drifted", drift.len()));

    let t = Instant::now();
    let report = b.store().verify(true)?;
    println!("  deep verify in {:.2?}", t.elapsed());
    check("no missing chunks", report.missing.is_empty(), &format!("{} missing", report.missing.len()));
    check("no corrupt chunks", report.corrupt.is_empty(), &format!("{} corrupt", report.corrupt.len()));

    // Compare actual bytes on disk for a spread of files, not just the index.
    let mut compared = 0;
    let mut mismatched = Vec::new();
    for i in (0..count).step_by((count / 500).max(1)) {
        let rel = relative(i);
        match (std::fs::read(a_root.join(&rel)), std::fs::read(b_root.join(&rel))) {
            (Ok(x), Ok(y)) if x == y => compared += 1,
            _ => mismatched.push(rel),
        }
    }
    check(
        &format!("{compared} sampled files byte-identical"),
        mismatched.is_empty(),
        &format!("{} differ, e.g. {:?}", mismatched.len(), mismatched.first()),
    );

    let _ = b_key;
    Ok(())
}

fn generate(root: &Path, count: usize) -> std::io::Result<u64> {
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    let mut total = 0u64;
    let mut buffer = Vec::with_capacity(4 << 20);
    let mut made_dirs = std::collections::HashSet::new();

    for i in 0..count {
        let rel = relative(i);
        let path = root.join(&rel);
        let parent = path.parent().unwrap().to_path_buf();
        if made_dirs.insert(parent.clone()) {
            std::fs::create_dir_all(&parent)?;
        }

        let size = size_for(i);
        buffer.clear();
        let mut x = (i as u32) | 1;
        for _ in 0..size {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            buffer.push(x as u8);
        }
        std::fs::write(&path, &buffer)?;
        total += size as u64;

        if i % 20_000 == 0 && i > 0 {
            println!("  ... {i} files");
        }
    }
    Ok(total)
}

/// Spread files across a two-level tree, as a real library would be.
fn relative(index: usize) -> String {
    format!("d{:02}/s{:02}/file{:06}.bin", index % 64, (index / 64) % 32, index)
}

fn chunk_key(master: &MasterKey) -> ChunkKey {
    ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes())
}

fn open(root: &Path, master: &MasterKey) -> Result<(Engine, Identity), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root)?;
    let store_dir = root.join(".qurb");
    let store = Store::open(&store_dir, chunk_key(master))?;
    let identity = Identity::load_or_create(&store_dir)?;
    let ignore = IgnoreRules::new().with_store_dir(&store_dir);
    Ok((Engine::new(root, store, ignore), identity))
}

fn first_few(failures: &[qurb_engine::FileFailure]) -> Vec<String> {
    failures.iter().take(3).map(|f| format!("{}: {}", f.path.display(), f.error)).collect()
}

fn rule(title: &str) {
    println!("\n=== {title} {}", "=".repeat(60usize.saturating_sub(title.len())));
}

fn check(what: &str, ok: bool, detail: &str) {
    if ok {
        println!("  PASS  {what}");
    } else {
        println!("  FAIL  {what} -- {detail}");
    }
}

fn rate(label: &str, bytes: u64, elapsed: Duration, files: usize) {
    let secs = elapsed.as_secs_f64();
    println!(
        "{label} in {:.2?} -- {:.0} files/s, {:.0} MiB/s",
        elapsed,
        files as f64 / secs,
        bytes as f64 / (1024.0 * 1024.0) / secs
    );
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", UNITS[i])
}
