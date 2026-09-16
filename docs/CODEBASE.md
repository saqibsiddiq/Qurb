# Understanding qurb from scratch

This is the one document to read if you know nothing about this project. It
assumes no prior context — not the architecture document, not the conversations
that produced it, not familiarity with sync engines. Everything else in `docs/`
goes deeper on one topic; this file is the map.

It is a **living document**. Anything that changes how the system fits together
should be reflected here in the same piece of work that changes it.

**Last verified against the code:** 2026-09-16, end of Phase 2. Phases 0–2
complete; pairing, NAT traversal and relays remain for Phase 3.

---

## 1. What this project is

qurb is a **private cloud storage system**. It aims to feel like Dropbox — you
put a file in a folder on your laptop, it appears on your phone — while being
built on the opposite premise: **your files are never stored on our servers.**

They move directly from your laptop to your phone, encrypted, over the internet.
We run servers, but they only help your devices find each other. They cannot
read your files, and in the normal case your file data never passes through
them at all.

The two goals are in tension, and that tension is the whole engineering problem:

- **Dropbox-like ease** wants a central server that is always awake, always has
  your files, and can be reached from anywhere.
- **Genuine privacy** wants no central server to hold anything.

Everything in the architecture is an attempt to get the first without giving up
the second.

---

## 2. The mental model

Five ideas carry most of the system. If you understand these, the code will make
sense.

### 2.1 Files are stored as chunks, addressed by their content

We never store "the file `/Photos/Summer.jpg`" as a unit. We split it into
pieces (**chunks**), take a cryptographic hash of each piece, and store the
piece in a directory named after that hash. This is a
**content-addressable store**, or CAS.

```
/Photos/Summer.jpg  →  [chunk A][chunk B][chunk C]
                          ↓        ↓        ↓
                       hash A   hash B   hash C
                          ↓        ↓        ↓
                   chunks/9f/86d0…  chunks/13/2f95…  chunks/a1/4b7c…
```

The filename and the data are stored separately. A database row says "this path
consists of these hashes, in this order." The chunks themselves have no idea
what file they belong to.

Two consequences follow immediately, and they are the reason for the design:

- **Deduplication is free.** If two files contain the same chunk, its hash is
  the same, so it is stored once. Copy a 4 GB video into three folders and you
  use 4 GB, not 12 GB.
- **Verification is free.** The name of a chunk *is* its hash. To check a chunk
  is intact, hash it and compare to its own filename. This is how corruption
  is detected.

### 2.2 Chunk boundaries are chosen by content, not by position

The obvious way to split a file is every N bytes. Dropbox does this with 4 MB
blocks. It has a fatal weakness: **insert one byte at the front and every
boundary after it shifts**, so every chunk hash changes, so the whole file has
to be re-sent.

Instead we use **content-defined chunking** (CDC). The algorithm slides a window
over the data and cuts wherever the content itself hits a particular pattern.
Because the cut points are determined by nearby *bytes*, not by *offset*,
inserting data shifts only the chunks immediately around the insertion.

We measured this in Phase 0 on a 196 MiB file, prepending a single byte:

| method | chunks still reusable |
|---|---|
| content-defined (FastCDC) | 304 / 305 — **99.7%** |
| fixed 4 MiB blocks | 0 / 50 — **0%** |

That table is the entire justification for the extra complexity of CDC. See
[decisions/0004-chunk-parameters.md](decisions/0004-chunk-parameters.md) for how
we chose the chunk sizes, which turned out to matter more than expected.

### 2.3 Devices talk to each other directly, through firewalls

Your laptop and your phone are both behind routers doing NAT (network address
translation). Neither has a public address you can dial. Normally that means
they can only talk *through* a server in the middle.

**UDP hole punching** gets around this. Both devices ask a public server
(**STUN**) "what does my address look like from outside?", exchange those
answers through our coordination server, and then send packets to each other
simultaneously. Each outbound packet tricks the sender's own router into
accepting the reply. If it works, the devices now have a direct path and our
servers are out of the loop.

It does not always work. Some networks (**symmetric NAT**) assign a different
external port per destination, which defeats the trick. For those we fall back
to a **relay** — a server that forwards encrypted bytes without being able to
read them. Relaying costs us bandwidth, so the percentage of connections that go
direct is a number with a dollar value attached.

### 2.4 There is no central truth, so devices must agree by themselves

With a central server, "what is the current version of this file?" has an easy
answer: whatever the server says. We have no such server. Each device has its
own opinion and they must converge.

We track causality with **vector clocks**: each device keeps a counter per
device, like `{laptop: 42, phone: 12}`, meaning "I have seen 42 changes from
laptop and 12 from phone." Comparing two vectors tells you whether one change
happened after another, or whether they happened *concurrently* — neither aware
of the other.

Concurrent changes are conflicts. The rule is
[decisions/0005-conflict-resolution.md](decisions/0005-conflict-resolution.md),
and its most important clause is: **never silently discard a user's edit.**
Both versions are kept; only the question of which one keeps the original
filename is decided automatically.

### 2.5 Encryption happens before anything leaves the device

Chunks are compressed, then encrypted, then written to disk and sent over the
network. The keys never leave your devices. Our servers see encrypted bytes and
routing metadata, nothing else.

This is called **zero-knowledge**, and it has a hard consequence that shapes the
product: if you lose your key, we cannot recover your data. Not "we won't" — we
genuinely cannot.

That is why there is a recovery phrase — 24 words that *are* the key, in a form
a person can write on paper — and why the onboarding flow that makes people
write it down is the highest-stakes screen in the application. See
[decisions/0012](decisions/0012-key-hierarchy-and-recovery.md), and note that
protecting the key at rest on the device is still an open gap.

---

## 3. How the pieces fit together

```
        YOUR DEVICES                          OUR SERVERS
  ┌─────────────────────┐              ┌──────────────────────┐
  │   Desktop           │              │  Control plane       │
  │   ┌───────────────┐ │              │  accounts, billing   │
  │   │ sync engine   │ │◄────────────►│  device registry     │
  │   │ chunk · hash  │ │   metadata   │                      │
  │   │ store · index │ │   only       │  STUN                │
  │   └───────────────┘ │              │  "what is my         │
  │   ┌───────────────┐ │              │   public address?"   │
  │   │ SQLite index  │ │              │                      │
  │   │ CAS on disk   │ │              │  Relay (DERP)        │
  │   └───────────────┘ │              │  encrypted forward   │
  └──────────┬──────────┘              │  when direct fails   │
             │                         └──────────┬───────────┘
             │  direct, encrypted (QUIC)          │
             │  ← this is the normal path         │ fallback only
             │                                    │
  ┌──────────▼──────────┐                         │
  │   Phone             │◄────────────────────────┘
  │   same engine, via  │
  │   Rust FFI          │
  └─────────────────────┘
```

The important asymmetry: **thick clients, thin servers.** Almost all the
difficulty lives on the device. The servers are a phone book and an emergency
mail forwarder.

### What happens when you save a file

This is the single most useful trace to have in your head.

```
 1. OS tells us a file changed      ✅ filesystem watcher (inotify/FSEvents/
                                       ReadDirectoryChangesW)
 2. Split into chunks               ✅ FastCDC, 128 KiB … 2 MiB
 3. Hash each chunk                 ✅ BLAKE3
 4. Ask the index: seen this hash?  ✅ SQLite lookup
      already known → record a reference, no data written
      new          → continue
 5. Compress                        ✅ Zstd, kept only when it actually helps
 6. Encrypt                         ✅ XChaCha20-Poly1305
 7. Write to the CAS                ✅ chunks/<first 2 hex>/<full hash>
 8. Record the file's chunk list    ✅ SQLite: path → ordered hashes
 9. Bump the vector clock           ✅ this device's counter += 1
10. Tell peers what changed         ✅ the tree, over QUIC
11. Peers request chunks they lack  ✅ only the missing ones move
```

Steps 1–8 are built, tested, and joined together: step 1 in
[`crates/watcher`](../crates/watcher/), steps 2–8 in
[`crates/storage`](../crates/storage/), and
[`crates/engine`](../crates/engine/) deciding what each change means and driving
the rest. A directory now syncs into a local store and stays in step with it.

All eleven steps now run end to end between two devices over a real network
connection. What is missing is not the pipeline but the things around it:
devices must be told each other's identities by hand, and must be able to reach
each other directly, because pairing and NAT traversal are Phase 3.

Step 11 is where the chunking finally pays off. Inserting 16 bytes at the front
of a 200 MB file moved **248 KiB** over the wire — one chunk, 0.1% of the file —
where fixed-size blocks would have re-sent all 190 MiB.

**Ordering matters at step 7 and 8, and not in the obvious way.** The payload is
written and fsynced *before* the index records that it exists. A crash between
them leaves a chunk nothing references — wasted space, reclaimed later. The
opposite order would leave the index pointing at a payload that was never
written, which no amount of local repair can fix. Orphaned data is a cost; a
dangling reference is a corruption.

Steps 2–4 are why a one-byte edit to a large file costs kilobytes instead of
gigabytes. Step 4 is why storing the same photo twice costs nothing.

---

## 4. Where things live

```
qurb/
├── README.md              Start here — what this is, current status
├── CLAUDE.md              Conventions and working agreements
├── Cargo.toml             Rust workspace root; shared dependency versions
│
├── docs/
│   ├── CODEBASE.md        ← you are here
│   ├── glossary.md        Every term, defined plainly
│   ├── architecture.md    The target design, all subsystems
│   ├── roadmap.md         Phases, timelines, honest risk assessment
│   ├── decisions/         Why each choice was made (one file per decision)
│   └── phases/            What each phase produced, with measurements
│
├── crates/
│   ├── storage/           Local storage. The foundation everything sits on.
│   │   └── src/
│   │       ├── chunker.rs   split at content-defined boundaries, hash
│   │       ├── format.rs    compress, then encrypt (the on-disk format)
│   │       ├── cas.rs       payloads to and from chunks/<2 hex>/<full hex>
│   │       ├── db.rs        SQLite index; reference counts live here
│   │       ├── gc.rs        reclaim chunks nothing references
│   │       └── store.rs     public API and the ordering rules
│   │
│   ├── watcher/           Turns filesystem events into settled changes.
│   │   └── src/
│   │       ├── ignore.rs    what never to watch (our own store, VCS, scratch)
│   │       ├── debounce.rs  collapse bursts; pure, takes the clock as input
│   │       ├── scan.rs      full walk, for startup and after overflow
│   │       └── watcher.rs   native events wired to the above
│   │
│   ├── engine/            Decides what a change means, and does it.
│   │   ├── src/lib.rs       reconcile, apply, and the run loop
│   │   ├── src/peer.rs      compare with another device and act on it
│   │   └── examples/        sync_once: one directory into a store
│   │                        sync_pair: two directories against each other
│   │
│   ├── sync/              What to do when two devices disagree.
│   │   └── src/           Pure logic: no disk, no network, no clock.
│   │       ├── clock.rs     version vectors and their partial order
│   │       ├── version.rs   one device's view of one path
│   │       ├── resolve.rs   deciding between two versions
│   │       └── reconcile.rs deciding about a whole tree
│   │
│   ├── peer/              Reaching another device, over QUIC.
│   │   ├── src/wire.rs      the message format; bounded and hostile-input safe
│   │   ├── src/identity.rs  a device's certificate and its fingerprint
│   │   ├── src/tls.rs       mutual authentication by pinned fingerprint
│   │   ├── src/server.rs    serves a store, read-only
│   │   ├── src/client.rs    asks for trees, manifests, chunks
│   │   └── src/source.rs    plugs the client into the engine
│   │
│   └── keys/              The root secret and the way back to it.
│       ├── src/master.rs    HKDF derivation, one key per purpose
│       ├── src/phrase.rs    the 24 words, via BIP-39
│       └── src/vault.rs     where the master key lives, and its limits
│
└── experiments/
    └── phase0-spike/      Throwaway. Proved the core ideas work.
        └── src/
            ├── lib.rs            chunker + content-addressable store
            └── bin/
                ├── chunkbench.rs measures the five Phase 0 kill criteria
                ├── sweep.rs      compares chunk size configurations
                └── quictest.rs   NAT classification + QUIC throughput
```

**The `crates/` vs `experiments/` split is load-bearing.** Anything in
`experiments/` is disposable and is allowed to cut corners, as long as its
README says which corners. Anything in `crates/` is meant to last and is held to
a real standard. Nothing should quietly migrate from one to the other — Phase 1
code gets written fresh, informed by the spike rather than copied from it.

---

## 5. What actually exists right now

Being precise about this matters, because the architecture document describes a
complete system and almost none of it is built.

### Built and tested (`crates/storage`, Phase 1)

| thing | status |
|---|---|
| Content-defined chunking | FastCDC, 128 KiB / 512 KiB / 2 MiB |
| Compression and encryption at rest | Zstd then XChaCha20-Poly1305 |
| Content-addressable store | crash-safe writes via temp file + rename |
| SQLite index | paths, chunk lists, reference counts via triggers |
| Deduplication | within a file, across files, across versions |
| Deletion, tombstones, restore | content survives a retention window |
| Garbage collection | two-stage, never touches a referenced chunk |
| Integrity verification | detects missing, corrupt, and orphaned chunks |

### Built and tested (`crates/watcher`, Phase 1)

| thing | status |
|---|---|
| Native filesystem events | inotify / FSEvents / ReadDirectoryChangesW |
| Debouncing | collapses editor write-bursts into one change |
| Stability checking | never releases a file that is still being written |
| Ignore rules | our own store, VCS metadata, editor scratch files |
| Overflow handling | dropped events demand a full rescan |
| New-directory walk | closes the recursive-watch race that loses files |

### Built and tested (`crates/engine`, Phase 1)

| thing | status |
|---|---|
| Startup reconciliation | walks the tree, agrees it with the index |
| Size and mtime fast path | 2437 files: 12.96s first run, 0.03s second |
| Applying watcher changes | upserts, removals, vanished files |
| Directory removal | tombstones everything the index holds beneath a path |
| Error isolation | one unreadable file does not stop the rest |
| Overflow recovery | a dropped-event rescan re-reconciles everything |

### Built and tested (`crates/sync`, Phase 1)

| thing | status |
|---|---|
| Version vectors | partial order, merge, deterministic encoding |
| Conflict detection | by causality, never by wall-clock time |
| Conflict resolution | both versions kept; a concurrent edit beats a delete |
| Tree reconciliation | plans describing end states, not decisions |
| Convergence | two and three devices, 320 random seeds |

### Joined together

| thing | status |
|---|---|
| Version vectors persisted | schema v2: vector, author, device identity |
| Local changes stamped | a counter per device, advanced only on real change |
| Comparing with a peer | `tree`, `plan_against`, `apply_plan` |
| Content fetched by hash | renames and copies cost a lookup, not a transfer |
| Two directories converging | including conflicts, deletions, resurrections |

170 tests pass across four crates; clippy is clean.

### Built and tested (`crates/peer`, Phase 1)

| thing | status |
|---|---|
| QUIC transport | one bidirectional stream per request |
| Mutual authentication | pinned fingerprints, handshake signature verified |
| Wire format | length-bounded; decoder has no panicking path |
| Incremental transfer | only chunks the receiver lacks cross the wire |
| Read-only serving | a peer can ask, never tell |

### Built and tested (`crates/keys`, Phase 1)

| thing | status |
|---|---|
| Master key | 256-bit, from the OS CSPRNG |
| Key derivation | HKDF-SHA256, one key per purpose, versioned labels |
| Recovery phrase | 24 words, BIP-39, checksummed |
| Recovery, end to end | the phrase turns back into the user's files |
| Key hygiene | redacted in `Debug`, wiped on drop, owner-only on disk |

293 tests pass across six crates; clippy is clean.

**Two devices now sync over a real network connection**, converging through
concurrent edits, deletions and resurrections, with both sides computing the
same conflict filename independently, using keys derived from a recovery phrase.

**Tested at 100,000 files** — 4.40 GiB between two devices, every correctness
check green: all paths present on both sides, reference counts agreeing, every
chunk decrypting and hashing to its own name, sampled files byte-identical. A
warm re-index of those 100k files takes 1.13s and an incremental sync of 1,000
edited files takes 3.5s. A cold index takes 237s, which is slow — 58% of it is
filesystem syscalls, and the fix is parallelism, which nothing in the design
prevents. See [phases/phase-1-engine.md](phases/phase-1-engine.md).

**What is missing is everything around it.** Devices must be told each other's
fingerprints by hand and must be able to reach each other directly — pairing,
NAT traversal, and relays are all Phase 3. And the master key sits in an
owner-only file rather than the platform keystore, which is the largest security
gap in the project.

### Hardened (Phase 2, complete)

| thing | status |
|---|---|
| Property-based convergence | 21 properties, shrinking, persisted regressions |
| Crash injection | real `SIGKILL` at seven points; no dangling references |
| Clock skew | ±10 years changes nothing about who wins |
| Long-absent devices | a month offline, in both directions |
| Case-insensitive collisions | probed, reported, and refused |
| Corruption repair | damaged chunks refetched from a peer and verified |
| Large renames | free in both sort directions; empty directories pruned |
| Hostile peers | wrong bytes, nonsense, silence — none reaches disk |
| Concurrent collection | the collector runs against a live writer |

The phase found five real defects, all of the same shape — correct in isolation,
wrong in combination — living in the seams between components rather than inside
them. See [phases/phase-2-correctness.md](phases/phase-2-correctness.md).

**A directory now syncs into a local store**, and can be tried:

```bash
cargo run --release --example sync_once -- ~/Documents /tmp/qurb-store
```

Two directories, treated as two devices:

```bash
cargo run --release --example sync_pair -- /tmp/dev-a /tmp/dev-b
```

The same, but over a real QUIC connection, reporting what crossed the wire.
Device A creates a key and B enrols with its recovery phrase:

```bash
cargo run --release --example transfer -- /tmp/dev-a /tmp/dev-b
```

What enrolling a device looks like on its own:

```bash
cargo run -p qurb-keys --example enrol -- /tmp/device-a
```

The Phase 1 kill criterion — generate a tree, sync it, verify it, edit it, sync
again. Needs roughly 20 GiB free at 100k files:

```bash
cargo run --release -p qurb-peer --example scale -- /tmp/scale 100000
```

What is missing is everything about a *second device*.

### Measured in the Phase 0 spike

| thing | result |
|---|---|
| Chunking throughput | 665 MiB/s warm, 246 MiB/s cold — disk-bound, not CPU-bound |
| Boundary stability | 99.7% chunk reuse after an edit, vs 0% for fixed blocks |
| Round-trip integrity | 2722 real files, byte-exact |
| QUIC transport | 287 MiB/s loopback |
| NAT classification | home network is cone NAT; more networks still to test |

### Designed but not built

Platform keystore integration, per-file keys, device pairing, NAT traversal, the
Go control plane, relays, the desktop UI, both mobile clients, search, updates,
billing.

### The gaps that matter most

Two things are known-missing rather than merely unbuilt, ranked by how expensive
they get if deferred:

1. **The availability question.** If all your devices are offline, your files
   are unreachable. That is the design working as intended, and it contradicts
   what the Dropbox-like promise leads people to expect. The fix — an always-on
   node, whether a NAS, a cheap VPS, or an optional paid encrypted pin — touches
   the storage engine, not just the network layer, so it must be decided early.
   **This is the largest open question in the project.**

2. **Key recovery.** Zero-knowledge means a lost key is lost data. Every
   consumer product in this space eventually adds some escape hatch. Choosing
   which compromise to make is better done on paper now than under pressure
   later.

Chunk garbage collection was the third item here and is now built. It was
deliberately the first thing written in Phase 1, because a collector that is
even slightly wrong destroys data in files the user never touched and raises no
error doing it. See [phases/phase-1-engine.md](phases/phase-1-engine.md).

---

## 6. Running things

```bash
cargo build --release
```

```bash
# the test suites -- start here to see what each layer guarantees
cargo test --workspace
```

```bash
# Measure chunking, dedup, round-trip integrity, boundary stability
./target/release/chunkbench ~/Downloads
```

```bash
# Compare chunk size configurations against a real corpus
./target/release/sweep ~/Downloads
```

```bash
# Classify this network's NAT — run on every network you care about
./target/release/quictest stun
```

`quictest stun` contacts public STUN servers, which reveals your public IP to
them exactly as any VPN or video-call client does. Nothing else in the spike
touches the network.

Set `SPIKE_SCRATCH` to a disk-backed path before running `chunkbench` with no
arguments. It defaults to `/tmp`, which on many Linux systems is a RAM-backed
tmpfs that the 2 GiB synthetic corpus will fill.

---

## 7. Suggested reading order

1. This file.
2. [glossary.md](glossary.md) — skim it, then use it as a reference.
3. [roadmap.md](roadmap.md) — what is being built when, and the honest risks.
4. [phases/phase-0-spike.md](phases/phase-0-spike.md) — the measurements, and
   the design error they caught.
5. [architecture.md](architecture.md) — the full target design.
6. [decisions/](decisions/) — read these when you want to know *why*, or when
   you are about to change something and want to know what it would break.
7. [crates/storage/README.md](../crates/storage/README.md) — the two invariants
   the storage layer is built around.
8. The storage source, in this order: `store.rs` (the API and the ordering
   rules), `db.rs` (reference counting), then `gc.rs`.
9. [crates/watcher/README.md](../crates/watcher/README.md) — the four silent
   failure modes filesystem watching has to prevent.
10. [crates/engine/README.md](../crates/engine/README.md) — how a change becomes
    work, and the one heuristic the engine leans on.
11. [crates/sync/README.md](../crates/sync/README.md) — why concurrency means
    conflict, and why convergence is a different property from correctness.
12. [crates/peer/README.md](../crates/peer/README.md) — what actually crosses
    the wire, and what pinned identity does and does not protect.
13. [crates/keys/README.md](../crates/keys/README.md) — why a lost phrase is
    unrecoverable, and what the key file does and does not defend against.

---

## 8. Conventions

- **Rust** for anything on a device: engine, storage, crypto, networking.
- **Go** for cloud services, when they exist.
- Decisions go in `docs/decisions/`, numbered, never deleted. If a decision is
  reversed, the old file gets a status line pointing at its replacement. The
  record of what we believed and why is worth more than a tidy directory.
- Every phase gets a document in `docs/phases/` recording what it produced and
  what it measured, written as part of the phase rather than after it.
- Claims about performance carry the measurement or they are not made. "Fast"
  is not a specification.

---

## 9. Keeping this file honest

The failure mode for a document like this is drifting out of sync with the code
until it becomes actively misleading — worse than having no document at all.

Update it whenever you change how the system fits together: a new subsystem, a
changed data flow, a moved directory, a corrected assumption. Update the
"last verified" date at the top when you have actually checked the whole file
against the code rather than just edited one section.

Do not update it for routine implementation work that does not change the
shape of the system. This is a map, not a changelog.
