//! Throwaway. See README.md.
//!
//! For each paired device in a copy of an index: the bytes of its file list
//! sent whole, and of the changes alone after a few made-up changes, and
//! whether the list rebuilt from those changes has the whole list's
//! fingerprint. The copy is changed; never point this at a live index.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use qurb_peer::wire::{self, Response};
use qurb_storage::db::{Audience, Db, TrustedPeer};
use qurb_sync::FileVersion;

fn main() -> Result<()> {
    let path = std::env::args().nth(1).context("usage: list-sizes <a copy of index.db>")?;
    let db = Db::open(Path::new(&path))?;
    let raw = rusqlite::Connection::open(&path)?;
    for peer in db.trusted_peers()? {
        measure(&db, &raw, &peer)?;
    }
    Ok(())
}

fn measure(db: &Db, raw: &rusqlite::Connection, peer: &TrustedPeer) -> Result<()> {
    let audience = Audience::Device(&peer.device_id);
    let list = db.versions_for(audience)?;
    let whole: usize = wire::tree_frames(&list).iter().map(Vec::len).sum();
    let mut keys = HashSet::new();
    let doubled = list
        .iter()
        .filter(|v| {
            let (class, path) = wire::listed_order(v);
            !keys.insert((class, path.to_string()))
        })
        .count();
    println!(
        "\n{} ({:?}): {} entries, {} bytes whole, {} entries sharing a key",
        peer.name, peer.relation, list.len(), whole, doubled
    );

    let scope = peer.device_id.as_bytes().to_vec();
    let changes: [(&str, &str); 5] = [
        ("nothing changed", ""),
        (
            "one shared file edited",
            "UPDATE files SET size = size + 1, updated_at = updated_at + 1 WHERE id =
               (SELECT id FROM files WHERE scope IS NULL AND deleted_at IS NULL ORDER BY id LIMIT 1)",
        ),
        (
            "one shared file deleted",
            "UPDATE files SET deleted_at = unixepoch() WHERE id =
               (SELECT id FROM files WHERE scope IS NULL AND deleted_at IS NULL ORDER BY id DESC LIMIT 1)",
        ),
        (
            "ten of its own files recorded again",
            "UPDATE files SET updated_at = updated_at + 1 WHERE id IN
               (SELECT id FROM files WHERE scope = ?1 AND deleted_at IS NULL ORDER BY id LIMIT 10)",
        ),
        (
            "a thousand of its own files recorded again",
            "UPDATE files SET updated_at = updated_at + 1 WHERE id IN
               (SELECT id FROM files WHERE scope = ?1 AND deleted_at IS NULL ORDER BY id LIMIT 1000)",
        ),
    ];
    let mut held = list;
    for (what, sql) in changes {
        let mark = db.list_mark()?;
        if !sql.is_empty() {
            if sql.contains("?1") {
                raw.execute(sql, [&scope])?;
            } else {
                raw.execute(sql, [])?;
            }
        }
        // What the server does for `TreeSince`, timed.
        let started = Instant::now();
        let (now, tree, changed) =
            db.at_once(|db| Ok((db.list_mark()?, db.versions_for(audience)?, db.listed_since(mark)?)))?;
        let digest = wire::tree_digest(&tree);
        let delta: Vec<FileVersion> = tree
            .iter()
            .filter(|v| {
                let (class, path) = wire::listed_order(v);
                changed.contains(&(class, path.to_string()))
            })
            .cloned()
            .collect();
        let answered = started.elapsed();
        let sent = delta.len();
        let bytes = Response::TreeDelta { now, digest, changed: delta.clone() }.encode().len();
        let rebuilt = rebuilt(held, delta);
        let same = wire::tree_digest(&rebuilt) == digest;
        println!(
            "  {what}: {sent} entries, {bytes} bytes as changes, answered in {:.0} ms, rebuilt list {}",
            answered.as_secs_f64() * 1000.0,
            if same { "matches" } else { "DOES NOT MATCH" }
        );
        held = tree;
    }
    Ok(())
}

/// As `qurb_peer::trees` rebuilds a kept list, which is private there.
fn rebuilt(kept: Vec<FileVersion>, changed: Vec<FileVersion>) -> Vec<FileVersion> {
    let mut by_entry: BTreeMap<(u8, String), FileVersion> = kept
        .into_iter()
        .map(|v| {
            let (class, path) = wire::listed_order(&v);
            ((class, path.to_string()), v)
        })
        .collect();
    for v in changed {
        let (class, path) = wire::listed_order(&v);
        by_entry.insert((class, path.to_string()), v);
    }
    by_entry.into_values().collect()
}
