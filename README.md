# qurb

Private cloud storage. Dropbox-like sync where your files stay on your own
devices — they move directly between them, encrypted end to end, and are never
stored on our servers.

**Status: Phases 4 and 5 in progress.** Phases 0–2 are complete: two devices
sync end to end over QUIC with encryption, conflict resolution and key
management, verified at 100,000 files and hardened against crashes, wrong
clocks, long absences, damaged disks and hostile peers. Phase 3 built pairing,
NAT traversal, a rendezvous service and a relay — its kill criterion, how often
the direct path works, needs a second machine and is still unmeasured. Phase 4
has a daemon and no interface. Phase 5 runs the engine on Android, where it
pairs and syncs — on an emulator, with no app around it and no iOS build.

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
| how to try it on your own hardware | [docs/trying-it.md](docs/trying-it.md) |

## Layout

```
docs/           documentation — start with CODEBASE.md
crates/         the engine: storage, watching, sync, transport, keys, FFI
scripts/        cross-compiling for Android, generating mobile bindings
experiments/    throwaway spikes, clearly marked as such
website/        the landing page (Next.js), independent of the engine
```

The split between `crates/` and `experiments/` is deliberate. Experimental code
may cut corners provided its README says which. Code in `crates/` is meant to
last. Nothing migrates silently between them.

## Trying it

There is a program now. On the first device:

```bash
qurb init ~/Sync
```

It prints 24 words, which are the only copy of your key. On the second device,
using those words, then introduce them and start both:

```bash
qurb enrol ~/Sync "wheel push industry ..."
```

`qurb pair` on one, `qurb join` on the other, `qurb run` on both. See
[crates/qurb/README.md](crates/qurb/README.md).

To watch the whole thing work in one process instead, including the servers:

```bash
cargo run --release -p qurb-peer --example demo -- /tmp/device-a /tmp/device-b
```

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
and incremental transfer that moves only the chunks the receiver lacks. Devices
pair by an invite carrying the inviter's full fingerprint across an out-of-band
channel — a QR code, or a code read aloud — after which a listener takes its
guest list from the trust store rather than from a caller. STUN discovers this
machine's public address and classifies the router; hole punching opens a path
on the same socket QUIC then runs over. A device can also run as a storage-only
replica: always on, holding content so the others need not all be awake, with no
directory behind it.

Built and tested in [`crates/keys`](crates/keys/): a 256-bit master key, HKDF
derivation of one key per purpose, and a 24-word BIP-39 recovery phrase — tested
end to end, so the words on a piece of paper genuinely turn back into the user's
files. 433 tests across ten crates, clippy clean.

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

Built and tested in [`crates/signal`](crates/signal/): a rendezvous service that
tells two devices to punch at the same moment, under identifiers derived from
the master key so it cannot link a group of devices to a person.

Two devices that have never spoken can now pair out of band, find each other
through the rendezvous service, connect, and sync — every layer at once.

Built and tested in [`crates/relay`](crates/relay/): a relay that forwards
opaque datagrams with a full QUIC session running inside, so it can neither read
what it carries nor forge it.

Reaching a peer tries every direct address at once and falls back to the relay
when none answers, with identity pinned exactly as hard either way.

Built and tested in [`crates/qurb`](crates/qurb/): the daemon and the commands
around it. Running it for the first time found three bugs the whole test suite
had missed, including an invite that offered `0.0.0.0` as an address — true, and
impossible to connect to.

Built and tested in [`crates/mobile-ffi`](crates/mobile-ffi/): the surface a
phone calls, generating Kotlin and Swift from the Rust. A phone pairs out of
band, finds the other device through the rendezvous service, connects over QUIC
and syncs — with `syncWithin(seconds)`, because both platforms kill background
work that outstays its window.

**It runs on Android.** `./scripts/android-test.sh` pushes the test binaries to
a device and runs them: 426 of the 433 pass there, QUIC handshakes and hole
punching included. Receiving a 512 MiB file on the device grows the heap by
5 MiB.

Mobile also forced three fixes in the core: files no longer pass through memory
whole (adopting a 1 GiB file grew the heap by 1024 MiB and now grows it by 1);
filenames are normalised to NFC, without which a `café` synced to a Mac
duplicates itself without limit; and the connector no longer advertises
`0.0.0.0` as its address, which broke two devices on a network with no route to
the internet.

The master key can be handed to the platform's own keystore, which the app
supplies because neither Android's nor iOS's is reachable from Rust — the
contract is tested against a fake, and no platform implements it yet.

Not built: an app of any kind, an interface, installers, signed updates.
Everything on-device ran on an **x86_64** emulator — the ARM build that would
ship to a phone is compiled and never run, because the emulator refuses an ARM
image on an x86 host. iOS has not been built at all; that needs a Mac.

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
the old and new names happened to sort alphabetically. 293 tests at the time;
420 now.

The master key can be kept in a file, in the operating system's keystore, or
wrapped with a passphrase — `qurb protect` explains what each defends against.

Availability — files being unreachable when every device is switched off — is
answered by storage-only replicas, in
[decisions/0006](docs/decisions/0006-availability-gap.md).
