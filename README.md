# qurb

Private cloud storage. Dropbox-like sync where your files stay on your own
devices — they move directly between them, encrypted end to end, and are never
stored on our servers.

**Status: Phases 0–2 complete.** Two devices sync end to end over QUIC with
encryption, conflict resolution and key management — verified at 100,000 files,
and hardened against crashes, wrong clocks, long absences, damaged disks and
hostile peers.

---

## Start here

**[docs/CODEBASE.md](docs/CODEBASE.md)** — everything needed to understand this
project from scratch. No prior context assumed.

Then, depending on what you want:

| you want | read |
|---|---|
| the vocabulary | [docs/glossary.md](docs/glossary.md) |
| what is being built when, and the honest risks | [docs/roadmap.md](docs/roadmap.md) |
| the full target design | [docs/architecture.md](docs/architecture.md) |
| why a particular choice was made | [docs/decisions/](docs/decisions/) |
| what each phase produced and measured | [docs/phases/](docs/phases/) |

## Layout

```
docs/           documentation — start with CODEBASE.md
crates/         the engine: storage, watching, sync, transport, keys
experiments/    throwaway spikes, clearly marked as such
website/        the landing page (Next.js), independent of the engine
```

The split between `crates/` and `experiments/` is deliberate. Experimental code
may cut corners provided its README says which. Code in `crates/` is meant to
last. Nothing migrates silently between them.

## Building

```bash
cargo build --release
```

## Running the Phase 0 spike

```bash
./target/release/chunkbench ~/Downloads
```

```bash
./target/release/sweep ~/Downloads
```

```bash
./target/release/quictest stun
```

```bash
cargo test --workspace
```

```bash
cargo run --release --example sync_once -- ~/Documents /tmp/qurb-store
```

`quictest stun` classifies your network's NAT, which determines whether devices
can connect directly or need a relay. It contacts public STUN servers, revealing
your public IP to them as any VPN or video-call client does. Worth running on
every network you use — the worst result sets the relay bandwidth cost.

Details and what the numbers mean:
[docs/phases/phase-0-spike.md](docs/phases/phase-0-spike.md).

## Where it stands

Built and tested in [`crates/storage`](crates/storage/): content-defined
chunking, compression and encryption at rest, a content-addressable store,
a SQLite index with trigger-maintained reference counts, deduplication across
files and versions, tombstoned deletes with restore, two-stage garbage
collection, and integrity verification.

Built and tested in [`crates/watcher`](crates/watcher/): native filesystem
events, debouncing of editor write-bursts, stability checks so nothing is read
mid-write, ignore rules, overflow-triggered rescans, and a directory walk that
closes the recursive-watch race.

Built and tested in [`crates/engine`](crates/engine/): startup reconciliation, a
size-and-mtime fast path, applying watcher changes, directory removal resolved
against the index, and per-file error isolation.

Built and tested in [`crates/sync`](crates/sync/): version vectors and their
partial order, conflict detection by causality rather than wall-clock time,
resolution that never discards an edit, tree reconciliation, and a convergence
simulation over two and three devices.

Joined together: version vectors persisted in the index, local changes stamped
with this device's counter, planning against a peer's tree, and content fetched
by hash so renames and copies cost a lookup rather than a transfer.

Built and tested in [`crates/peer`](crates/peer/): a QUIC transport with mutual
authentication by pinned certificate fingerprint, a length-bounded wire format,
and incremental transfer that moves only the chunks the receiver lacks.

Built and tested in [`crates/keys`](crates/keys/): a 256-bit master key, HKDF
derivation of one key per purpose, and a 24-word BIP-39 recovery phrase — tested
end to end, so the words on a piece of paper genuinely turn back into the user's
files. 293 tests across six crates, clippy clean.

A directory syncs into a local store — on 2437 real files (979 MiB), 12.96s for
the first pass and 0.03s for the second. **Two devices now sync over a real
network connection**, converging through concurrent edits, deletions and
resurrections, with both sides computing the same conflict filename
independently.

Transfer is incremental. Inserting 16 bytes at the front of a 200 MB file moved
**248 KiB** over the wire — one chunk, 0.1% of the file — where fixed-size
blocks would have re-sent all 190 MiB.

Measured in Phase 0: chunking at 665 MiB/s, 99.7% chunk reuse after an edit
where fixed blocks achieve 0%, byte-exact round trips over 2722 real files,
QUIC transport, and a home network that supports direct connections.

Not built: device pairing, so devices must be told each other's fingerprints by
hand; NAT traversal, so they must be able to reach each other directly; platform
keystore integration, so the master key sits in an owner-only file rather than
Keychain or DPAPI; and the control plane, relays, and every user interface.

**Phase 1 is complete.** Its kill criterion — syncing 100,000 files cleanly —
was run and passed: 4.40 GiB between two devices with every correctness check
green. A warm re-index of those 100k files takes 1.13s and an incremental sync
of 1,000 edited files takes 3.5s. A cold index takes 237s, which is slow;
58% of it is filesystem syscalls, and the fix is parallelism.

Phase 2 added property-based convergence testing with shrinking, crash injection
with real `SIGKILL`, clock-skew and month-offline scenarios, repair of damaged
chunks from a peer, and tests against peers that lie. It found four real
defects — including a writer that could reference a chunk garbage collection had
just deleted, and renames that re-transferred an entire library depending on how
the old and new names happened to sort alphabetically. 293 tests.

The largest open question is availability — a peer-to-peer design means files
are unreachable when all your devices are offline, which contradicts what the
product promises. See
[decisions/0006](docs/decisions/0006-availability-gap.md).
