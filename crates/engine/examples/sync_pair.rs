//! Sync two directories against each other, printing what happened.
//!
//! ```bash
//! cargo run --release --example sync_pair -- /tmp/dev-a /tmp/dev-b
//! ```
//!
//! Both directories are treated as separate devices with separate stores and
//! separate identities. There is no network: content moves by reading the other
//! store directly, which is what [`StoreSource`] is for. A real transport slots
//! into the same seam.

use qurb_engine::{Engine, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (a_root, b_root) = match (args.get(1), args.get(2)) {
        (Some(a), Some(b)) => (PathBuf::from(a), PathBuf::from(b)),
        _ => {
            eprintln!("usage: sync_pair <dir-a> <dir-b>");
            std::process::exit(2);
        }
    };

    let mut a = open("A", &a_root)?;
    let mut b = open("B", &b_root)?;

    for round in 1..=5 {
        let a_plan = a.plan_against(&b.tree()?)?;
        let b_plan = b.plan_against(&a.tree()?)?;
        if a_plan.is_empty() && b_plan.is_empty() {
            println!("\nconverged after {} round(s)", round - 1);
            break;
        }

        println!("\nround {round}: A has {} action(s), B has {}", a_plan.len(), b_plan.len());
        for action in a_plan.iter().chain(b_plan.iter()) {
            println!("    {:<10} {}", kind(action), action.path());
        }

        let a_stats = { a.apply_plan(&a_plan, &mut StoreSource::new(b.store()))? };
        let b_stats = { b.apply_plan(&b_plan, &mut StoreSource::new(a.store()))? };

        for (name, s) in [("A", &a_stats), ("B", &b_stats)] {
            println!(
                "  {name}: adopted {} merged {} conflicts {} resurrected {} fetched {} failed {}",
                s.adopted, s.merged, s.conflicts, s.resurrected, s.fetched, s.failures.len()
            );
            for f in &s.failures {
                println!("       failed: {} -- {}", f.path.display(), f.error);
            }
        }
    }

    for (name, engine) in [("A", &a), ("B", &b)] {
        let live = engine.store().db().live_paths()?;
        println!("\n{name} ({}): {} file(s)", engine.root().display(), live.len());
        for p in live.iter().take(20) {
            println!("    {p}");
        }
        if live.len() > 20 {
            println!("    ... and {} more", live.len() - 20);
        }
    }

    Ok(())
}

fn open(label: &str, root: &Path) -> Result<Engine, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root)?;
    let store_dir = root.join(".qurb");
    // One key for both: these stand in for a single user's own devices, which
    // share a master secret.
    let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32]))?;
    let ignore = IgnoreRules::new().with_store_dir(&store_dir);
    let mut engine = Engine::new(root, store, ignore);

    let stats = engine.reconcile()?;
    println!(
        "{label} {}: stored {} unchanged {} deleted {}",
        root.display(), stats.stored, stats.unchanged, stats.deleted
    );
    Ok(engine)
}

fn kind(action: &qurb_sync::Action) -> &'static str {
    match action {
        qurb_sync::Action::Adopt { .. } => "adopt",
        qurb_sync::Action::Offer { .. } => "offer",
        qurb_sync::Action::Conflict { .. } => "conflict",
        qurb_sync::Action::Resurrect { .. } => "resurrect",
        qurb_sync::Action::Merge { .. } => "merge",
    }
}
