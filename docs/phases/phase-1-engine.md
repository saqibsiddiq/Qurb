# Phase 1 — The engine

**Status:** complete — kill criterion passed 2026-09-16
**Target:** months 1–3
**Kill criterion:** cannot sync 100k files cleanly without a reproducible
correctness bug.

The goal is a local storage engine that two desktops can sync through: chunking,
hashing, an index, an encrypted chunk store, filesystem watching, and QUIC
transfer. No user interface beyond a tray icon.

## Progress

| component | status |
|---|---|
| content-defined chunking | ✅ [`chunker`](../../crates/storage/src/chunker.rs) |
| compression + encryption at rest | ✅ [`format`](../../crates/storage/src/format.rs) |
| content-addressable store | ✅ [`cas`](../../crates/storage/src/cas.rs) |
| SQLite index with reference counting | ✅ [`db`](../../crates/storage/src/db.rs) |
| garbage collection | ✅ [`gc`](../../crates/storage/src/gc.rs) |
| integrity verification | ✅ [`store::verify`](../../crates/storage/src/store.rs) |
| key derivation and recovery phrase | ✅ [`qurb-keys`](../../crates/keys/) |
| filesystem watching | ✅ [`qurb-watcher`](../../crates/watcher/) |
| engine: watcher joined to store | ✅ [`qurb-engine`](../../crates/engine/) |
| vector clocks and conflict resolution | ✅ [`qurb-sync`](../../crates/sync/) |
| engine's use of the sync logic | ✅ [`peer`](../../crates/engine/src/peer.rs) |
| version vectors persisted | ✅ schema v2 |
| peer transfer over QUIC | ✅ [`qurb-peer`](../../crates/peer/) |

224 tests pass across six crates; clippy is clean.

## What was built, and why this order

The roadmap named chunk garbage collection as the first correctness problem, so
the storage crate was built around it rather than adding it afterwards.

Deduplication is what makes deletion hard. A chunk can belong to many files and
to many versions of one file, so "delete this file" cannot mean "delete its
chunks" — some other file may still need them. A collector that is slightly
wrong destroys data in a file the user never touched, and raises no error while
doing it. That is the worst failure class a storage product has, so it was
designed first and everything else arranged around it.

The mechanism: **reference counts maintained by database triggers, not by Rust
code.** A trigger fires in the same transaction as the row change that caused
it, so a count cannot drift because some code path forgot to decrement.
`Db::audit_refcounts` re-derives every count from scratch and is asserted after
every test that mutates anything.

Two invariants hold the crate together, both documented in
[the crate README](../../crates/storage/README.md):

1. A chunk referenced by the index always exists on disk. Writes go
   payload-first, so a crash leaves an orphan — wasted space — rather than a
   dangling reference, which is unrecoverable.
2. Reference counts equal the links that exist.

Encryption went in from the start rather than being retrofitted. The on-disk
format is now fixed ([decision 0007](../decisions/0007-chunk-format.md)) even
though key *management* is not built, because changing a stored format after
users have data means rewriting every chunk they own, while changing where a
key comes from does not.

## Things the tests caught

**Perfectly periodic test data defeats content-defined chunking entirely.** A
generator like `i * K as u8` looks random but repeats every 256 bytes. With
periodic input every cut lands at the same phase, so shifting the input by one
byte changes every chunk — the boundary-stability test measured 0% reuse.
Switching to xorshift fixed it. Worth remembering: any future test that
generates content needs a real generator, or it will quietly measure nothing.

**A repeated region must span several chunks before it deduplicates.** After a
repeat begins, the chunker needs a chunk or two to resynchronise its boundaries
with the earlier copy. Measured on doubled blocks:

| block size | chunks | deduplicated |
|---|---|---|
| 1 MiB | 4 | 0 |
| 2 MiB | 7 | 2 |
| 8 MiB | 27 | 12 |
| 16 MiB | 54 | 23 |

At 1 MiB the whole file is too few chunks for resynchronisation to happen at
all. This is expected behaviour rather than a defect, but it bounds what
deduplication can do for small files and is another reason the chunk sizes in
[decision 0004](../decisions/0004-chunk-parameters.md) matter.

**Retention windows compose rather than overlap.** A deleted file's payloads are
held for the retention period as a tombstone, then for the retention period
again as unreferenced chunks — up to twice the configured window before space
comes back. A comment in the garbage collector originally claimed otherwise;
the code was right and the comment was wrong. Erring toward keeping data is
correct here, but it is a real disk cost when choosing the window.

## Filesystem watching

The second crate, [`qurb-watcher`](../../crates/watcher/), turns native
filesystem events into the changes a user actually made. The distance between
those two things is larger than it sounds: one editor save produces a dozen
events across two paths, and a large copy produces a write event every few
milliseconds for as long as it runs.

Four failure modes shaped the design, and every one of them is silent when
unhandled:

1. **Reading a file mid-write** stores a torn copy, and the storage layer's
   memory-mapped read raises SIGBUS if a file is truncated underneath it. A path
   is released only once its size and modification time have stopped changing.
2. **Watching our own store** would feed every chunk write back as an event
   causing another write. Excluded explicitly.
3. **Kernel queue overflow** during a large operation drops events, after which
   the stream no longer describes reality. Reported as `RescanRequired` rather
   than continuing as if nothing was missed.
4. **The recursive watch race** — described below, and the one that would have
   actually lost data.

The debouncer takes the current time as a parameter instead of reading a clock,
so its behaviour is pinned by unit tests with a logical clock rather than by
tests that sleep. Integration tests cover only the wiring.

### The bug worth recording

The first integration run reported `-a` — a newly created *directory* surfacing
as a deletion — and the file inside it, `a/b/deep.txt`, not at all.

Two defects. The lesser one: the code treated "cannot stat this as a file" as
evidence of deletion, which is wrong for a directory that plainly exists.

The serious one: **a platform watch for a new subdirectory is installed only
after that directory exists, so anything written into it in that window
produces no event whatsoever.** Cloning a repository or unpacking an archive
into the synced folder does exactly this, dozens of files at a time, and the
failure is completely silent — the files simply never sync, and nothing anywhere
reports a problem.

The fix is to walk any directory the watcher is told about, feeding what it
finds back through the debouncer so those files still get the quiet period and
the stability check. `files_written_into_a_brand_new_directory_are_not_lost`
covers it: thirty files written into three directories created in one burst,
all of which must be reported.

This is the kind of defect that argues for Phase 2 existing at all. It passed
every unit test, would have passed a manual smoke test of "create a file, watch
it sync," and would have shown up months later as users reporting that some
files just never arrive.

### Delivery is at least once

Fixing the race introduced duplicates: a file can surface both from the
directory walk and from its own event arriving afterwards. Rather than suppress
them, the watcher documents that delivery is at-least-once
([decision 0008](../decisions/0008-watcher-delivery-guarantee.md)).

Exactly-once is not achievable — platforms duplicate events on their own — and
deduplicating properly means knowing what was last stored for a path, which the
index already knows. A second cache in the watcher would duplicate that state
and could disagree with it, and disagreement about what has been stored is
exactly how data gets lost.

**This leaves a follow-up.** The storage layer is idempotent but not yet cheap
about it: `put_file` chunks and hashes a whole file before discovering the
content is unchanged. The engine should compare size and modification time
against the index first and skip the read entirely — the same check that makes
startup reconciliation fast. Engine work, not storage work, and not built.

## The engine

[`qurb-engine`](../../crates/engine/) is the part that decides what a change
means. It has three entry points: reconcile at startup, apply while running, and
reconcile again when the watcher reports dropped events.

Reconciliation is deliberately blunt — it re-examines every file rather than
working out what it missed. That is what makes it a safe recovery path: it needs
no record of where the gap began, so it cannot get that record wrong.

### The fast path, measured

The follow-up left open by [decision 0008](../decisions/0008-watcher-delivery-guarantee.md)
is now closed. Because delivery is at-least-once and reconciliation revisits
everything, the same file arrives repeatedly with nothing changed. Comparing
size and modification time against the index before reading turns that into a
stat.

On a real directory — 2437 files, 979 MiB:

| run | result | elapsed |
|---|---|---|
| first, cold index | 2437 stored | 12.96s |
| second | 2437 unchanged | 0.03s |

432× faster, and the difference between a startup that is instant and one that
re-reads the whole library.

The heuristic is honest about being one: a file edited in place, keeping its
exact length, within one timestamp tick would be missed. rsync and git accept
the same window. `Store::verify` is the backstop.

### What the same run says about the pipeline

979 MiB in 13 seconds is about 75 MiB/s end to end, against 665 MiB/s for
chunking alone. The gap is zstd, encryption, thousands of small file writes, and
SQLite transactions — and it is all single-threaded. Storage came to 426 MiB, or
43.5% of plaintext.

75 MiB/s is fine for ongoing sync and slow for initial import: a 1 TiB library
would take around four hours. Parallelism is the obvious answer and is not built.

### Removal is resolved against the index

The watcher reports a path is gone but cannot say whether it was a file or a
directory. So the engine asks the index what it knew about at or under that
path, and tombstones every match. `Db::live_paths_under` was added to storage
for this, with LIKE wildcards escaped so a directory named `100%` matches only
itself.

## Vector clocks and conflict resolution

The fourth crate, [`qurb-sync`](../../crates/sync/), is the part that has to be
*right*. It holds no data, touches no disk, and reads no clock — it takes two
views of the world and says what should happen. That makes it exhaustively
testable, which matters more here than anywhere else, because the failure mode
is losing someone's work.

Version vectors record how many changes from each device a version has seen, so
comparing two answers the only question that matters: did one happen after the
other, or did neither see the other? Wall-clock time is deliberately absent from
every ordering decision — device clocks disagree, and ordering by timestamp lets
a device with a wrong clock win or lose every conflict systematically. It
appears only inside conflict filenames, where it helps a person identify a
version.

### Decision 0005 was underspecified

Implementing it surfaced three cases the original rule does not describe, now
recorded in [decision 0009](../decisions/0009-conflict-edge-cases.md): a
concurrent edit against a concurrent delete, concurrent changes that happen to
produce identical content, and the version vector a resolution must carry.

The first has an asymmetry worth stating plainly. Keeping both is not available
when one side's contribution is an absence, so one of them loses. A deletion
that loses is recoverable — the tombstone and its chunks survive the retention
window. An edit that loses is gone. So the edit wins.

### The bug the convergence test caught

The unit tests all passed. The simulation did not.

When two devices reach identical content independently — the same file copied
onto each — the versions are concurrent but there is nothing to disagree about,
so resolution reported "in sync" and produced no action. Correct as far as the
user could see. But the merged history was never recorded, so the two versions
stayed permanently concurrent, and **the next edit on either device raised a
conflict over content that had never disagreed.**

The visible state was right the whole time. Only the recorded history diverged,
and only later did that turn into phantom conflict files.

Nothing short of simulating two devices over many rounds would have found it.
Each individual decision was defensible; the system as a whole did not settle.
Hence `Action::Merge`, which moves no data and exists purely to converge history.

### Convergence is a different property from correctness

`tests/convergence.rs` simulates two and three devices making random edits,
deletes and syncs, and asserts they all end agreeing — on content *and* on
history, with no work left over. 320 random seeds across both sizes.

Three devices matter separately from two. Two can be reconciled by much simpler
schemes than vector clocks; three is where a change can arrive by two different
routes and "has this device seen that edit?" stops having an obvious answer.

The simulation also bounds the rounds needed to settle. An engine that needs
unbounded rounds does not settle at all, and in production that looks like two
devices trading files forever while the battery drains.

## Wiring the sync logic in

The decision logic and the storage were built separately and then joined.

**The index learned about history.** Schema v2 adds a version vector and an
author to every file row, plus this store's own identity and the counter it
stamps onto local changes. Migrations now run in order and are recorded in
`user_version`, so an existing database picks up only what it is missing.

**The store learned whose change it is handling.** A local change advances this
device's counter; a version adopted from a peer keeps the vector it arrived
with. Getting that backwards would claim this device had seen changes it has
not, and make its history dominate versions it should have conflicted with.
Writing identical bytes deliberately does not advance the clock — otherwise a
touched file would start beating a peer's genuinely newer version.

**The engine learned to compare.** `tree()` is what it would tell a peer;
`plan_against()` runs the reconciliation; `apply_plan()` carries out the local
half — writing files, recording vectors, and fetching what it does not have.

### Content is fetched by hash, not by path

Recorded as [decision 0010](../decisions/0010-content-by-hash.md). A plan
routinely calls for content that lives under a different name on the device that
has it — a conflict rename produces a path no device has ever held — so asking
by path fails where asking by content succeeds.

It also makes renames and copies free: the engine checks whether any live path
already holds those bytes before asking anyone for them, so adopting a peer's
copy of a file already present costs an index lookup rather than a transfer.

`ContentSource` is the seam the network will slot into. Everything above it is
finished and tested; below it there is one implementation that reads another
local store.

### The two halves must not fight each other

The trap worth recording: `apply_plan` writes a file to disk, and a
reconciliation then walks that same file. If it stamped it as a *local* change,
the adopted version would become concurrent with the peer's own copy, and the
next exchange would raise a conflict over a file that had just synced
successfully. Forever.

What prevents it is `apply_plan` recording the modification time the file
actually ended up with, so the size-and-mtime fast path recognises its own work.
Three tests pin it — adopted files, adopted tombstones, and conflict files —
because the failure would look like the system working until suddenly it did not.

### It works on real directories

Beyond the two-device tests, `sync_pair` runs it on two real directories. With
the same file edited differently on each side:

```
round 1: A has 2 action(s), B has 2
    adopt      local.txt
    conflict   notes.txt
  A: adopted 1 conflicts 1 fetched 1 failed 0
  B: adopted 0 conflicts 1 fetched 1 failed 0
converged after 1 round
```

Both directories then hold the same two files, under the same conflict
filename — computed independently on each side with no negotiation — and neither
edit was lost.

## Peer transfer over QUIC

The fifth crate, [`qurb-peer`](../../crates/peer/), is what makes two devices
two separate machines rather than two directories. It sits above the engine: the
engine knows nothing about networks, and this adapts it to one.

### Transfer is incremental, and it is measurable

Fetching a file asks first for its manifest — the chunk list — and then only for
the chunks this device does not already hold.

Measured on this machine over loopback, a 200 MB file with 16 bytes inserted at
offset 1000:

| | first sync | after the 16-byte edit |
|---|---|---|
| chunks requested | 306 | **1** |
| bytes transferred | 190.73 MiB | **248.10 KiB** |
| share of the file | 100% | **0.1%** |

An insertion shifts every byte after it, which is exactly the case fixed-size
blocks handle worst: Dropbox's 4 MB blocks would have re-sent all 190 MiB. This
is [decision 0004](../decisions/0004-chunk-parameters.md) finally paying off
where it was always meant to, and it is now a measurement rather than an
argument.

Conditions: loopback on one machine, warm cache, 12-core Linux desktop. It says
nothing about behaviour across a NAT or a slow link.

**The saving is on the wire only.** Reassembly is still whole-file — the client
concatenates chunks, hands the result to the engine, and the engine re-chunks it
to store. Network cost scales with the edit; local CPU still scales with the
file. The first sync moved 190 MiB in 1.79s; the second moved 248 KiB but still
took 884ms, almost all of it re-chunking locally.

### Identity, and the gap under it

Both ends present a self-signed certificate, check the other's fingerprint
against a list given in advance, and **verify the handshake signature** — so a
peer must hold the matching private key rather than replay a public certificate.
Recorded as [decision 0011](../decisions/0011-peer-identity-pinning.md).

Skipping the signature check is the standard way pinned TLS is got wrong, and it
fails open: everything works, including for an attacker.

`PeerClient::connect` requires the expected fingerprint and offers no way to
omit it. An API where it were optional would make "trust whoever answers" the
easy path, which is the whole attack.

**But pairing is not built**, and that is the honest limit of what this achieves.
How a device learns which fingerprint to expect — the QR-code exchange, the
device trust graph — is Phase 3. The transport is sound; the thing that decides
what to feed it is missing. `DeviceId` in the index and `Fingerprint` on the
network are also still separate identifiers, and binding them is pairing's job.

### The server is read-only

A peer can ask what this device has and ask for its bytes. It cannot tell this
device to change anything. Incoming versions are adopted by the local engine
only after reconciliation, so nothing a peer says is applied without this side
deciding it should be.

### Hostile input is a first-class case

Every field on the wire is length-prefixed and bounded. Without a cap, one
four-byte length field is an out-of-memory attack. Decoding is tested against
truncation at every offset, trailing bytes, absurd lengths, unknown tags, and
invalid UTF-8 — the decoder has no panicking path.

## Key management

The sixth crate, [`qurb-keys`](../../crates/keys/), is the last piece of Phase 1.
One 256-bit master key per user; everything else derived from it with
HKDF-SHA256 under versioned, purpose-specific labels; the master encoded as a
24-word BIP-39 phrase. Recorded as
[decision 0012](../decisions/0012-key-hierarchy-and-recovery.md).

What makes this unlike ordinary key handling is that there is no reset. The
servers hold nothing, so a lost key and a lost phrase means the data is gone —
not by policy but as a fact about the mathematics. Several choices follow
directly:

- The phrase is returned **exactly once**, by `Opened::Created`, at the only
  moment it can be produced. There is deliberately no `phrase()` method, because
  offering one would imply it could be asked for later.
- `Vault::restore` refuses to overwrite an existing key, which would orphan
  every chunk already stored — still on disk, encrypted under a key that exists
  nowhere.
- Purposes are a closed enum rather than a caller-supplied string, because two
  purposes sharing a label would silently produce the same key for both.
- Keys are redacted in `Debug` and wiped on drop.

BIP-39 comes from a library rather than being hand-rolled. The encoding is
trivial; the wordlist is not, and a phrase people mis-transcribe means lost data
here. A test pins a known phrase to a known key, so a change of library cannot
silently change what an existing phrase means.

### Testing the claim that actually matters

The unit tests prove a phrase rebuilds a key. That is a weaker statement than
the one users depend on, which is that the words on their paper turn back into
their *files* — through derivation, the chunk cipher, and the on-disk format.
`tests/recovery.rs` tests that path end to end, including the wrong phrase
failing to decrypt rather than producing plausible rubbish.

### The whole stack, with real keys

`transfer` now derives its chunk key instead of hardcoding one. Device A creates
a key and shows its phrase; device B enrols with that phrase, which is what
makes them a set rather than strangers. 40 MB then crossed a QUIC connection
between them, with both key files at mode 0600.

### The weakness being accepted

The master key is in a file readable only by its owner. That is not key
protection: it defends against other users on the machine, not against anything
that can read the disk. The platform keystore — Keychain, DPAPI, Secret
Service — is three separate integrations and is not built.

This is recorded as a known gap rather than presented as a design, because a
user reading "end-to-end encrypted" would reasonably assume more than it
provides on a compromised machine.

## Deliberately not done yet

- **Key storage.** The master key sits in an owner-only file rather than the
  platform keystore. The largest security gap in the project.
- **Per-file keys.** The architecture describes deriving a key per file so one
  leaked key exposes one file. Every chunk currently uses the same derived key —
  simpler, and weaker.
- **Key rotation.** Changing the master key means re-encrypting every chunk, and
  there is no mechanism for it.
- **An escape hatch for lost phrases.** Every consumer product in this space
  eventually adds one, and each trades away some of the zero-knowledge property.
  Still an open product decision.
- **Concurrency.** One connection, one writer. Garbage collection already takes
  SQLite's write lock for its deletions, which is what will make it safe against
  a concurrent writer, but nothing has exercised that path.
- **Streaming reads.** `read_file` assembles a whole file in memory. Acceptable
  on desktop; not acceptable inside an iOS FileProvider extension, which will
  need a chunk-at-a-time API. Worth fixing before mobile, not before sync.
- **Case-insensitive filesystems.** `README` and `readme` are distinct paths
  throughout. macOS and Windows disagree, and that must be resolved before
  either ships.
- **Symbolic links** are skipped rather than represented.
- **Parallelism.** The engine processes one file at a time.
- **Moves.** A rename is stored as a new file plus a tombstone rather than
  recognised as the same content moving. Chunks deduplicate locally, and a peer
  adopting a rename now finds the content already on disk rather than fetching
  it, so the cost is a re-read rather than a transfer.
- **Pairing.** Devices must be told each other's fingerprints. The largest gap.
- **NAT traversal.** Connections are direct to a known address. Phase 0 measured
  that hole punching works from this network; none of it is implemented.
- **Tree paging.** The whole tree crosses in one message, capped at 64 MiB. A
  large library needs incremental exchange rather than a full dump per sync.
- **Concurrent chunk fetches.** Chunks are requested one at a time. The server
  already serves them concurrently and QUIC allows it; the client does not yet,
  which leaves throughput on the table over a real link.
- **Streaming adoption.** `apply_plan` holds a whole file in memory before
  writing it, so adopting a very large file needs its size in RAM.

## The kill criterion: 100k files

**Passed.** Every correctness check green, first run, no retries.

Run with `cargo run --release -p qurb-peer --example scale -- <dir> 100000`.
Conditions: one Linux desktop, 12 cores, 15 GiB RAM, NVMe, both devices in one
process over loopback. 100,000 files across a two-level tree, 4.40 GiB,
distributed like a real sync folder — 80% under 8 KiB, 1% between 1 and 4 MiB.

| stage | result |
|---|---|
| index, cold | 236.8s — 422 files/s, 19 MiB/s |
| re-index, warm | **1.13s** — 88,500 files/s, nothing re-read |
| tree exchange | 100,000 paths in 196ms |
| first sync | 476s, 103,709 chunks, 4.13 GiB, all 100,000 adopted |
| deep verify | 43.4s — no missing chunks, no corrupt chunks, no refcount drift |
| edit 1,000 files, re-index | 4.93s — exactly 1,000 re-read |
| incremental sync | **3.50s**, 24.1 MiB on the wire |
| verify again | clean |

Correctness held everywhere it was checked: 100,000 paths present on both sides,
reference counts agreeing with the links they describe, every chunk decrypting
and hashing to its own name, and 500 sampled files byte-identical between the
two directories.

**The two fast-path numbers are the ones worth keeping.** A warm re-index of
100k files takes 1.13 seconds, and syncing 1,000 edited files out of 100,000
takes 3.5. The size-and-mtime check and the version-vector comparison both do
what they were built to do at this scale.

### Where the slow numbers come from

422 files/s on a cold index is slow, and 476s to move 4.4 GiB is slow. Both were
attributed by measurement rather than guessed at, and two guesses were wrong
along the way.

Per file, at 1.98ms:

| cost | per file | share |
|---|---|---|
| `fsync` per chunk written | 0.62ms | 31% |
| compress + encrypt (`seal`) | 0.10ms | 5% |
| chunk + hash | 0.07ms | 3% |
| SQLite transaction | 0.06ms | 3% |
| **unattributed — filesystem syscalls** | **1.13ms** | **58%** |

The measurements: `fsync` costs 0.616ms on this filesystem against 0.029ms for
an unsynced write; 20,000 per-file SQLite transactions cost 0.059ms each;
`seal` runs at 452 MiB/s on incompressible data at this chunk size
(`cargo run -p qurb-storage --example seal_cost`).

The majority is neither cryptography nor the database but **syscalls**. Storing
one file costs an open, a stat, an mmap and an munmap; storing one chunk costs
an existence check, a `create_dir_all`, a temp-file create, a write, an fsync
and a rename. At 100k files and 104k chunks that is on the order of 1.6 million
syscalls.

### A bug this found in our own code

`Store::put_file` chunked the file and then read it *again* into a heap buffer —
every byte read twice, and the whole file held in memory. Invisible at test
sizes.

It is fixed by mapping once and chunking from the same mapping. Measured at 5,000
files: **485 → 505 files/s, about 4%.** Small, because the second read was
served from the page cache. The reason to keep the fix is memory rather than
speed: a 4 GB file no longer needs 4 GB of RAM to store.

Worth stating plainly because the first framing was wrong. A 20k-file run after
the fix showed 506 files/s against the 100k run's 422 and looked like a 20%
improvement; it was not, it was a different scale. The like-for-like comparison
is the one above.

### What to do about it, in order of value

1. **Parallelism.** The engine stores one file at a time on a 12-core machine.
   This is the largest available win and nothing about the design prevents it —
   chunks are content-addressed and independent.
2. **Batch the fsyncs.** One per chunk is 31% of the time. Syncing once per
   batch of chunks keeps the crash-safety property that matters — no index entry
   pointing at an unwritten payload — because the index is written after.
3. **Skip `create_dir_all` for shard directories that already exist.** There are
   256 of them and they are created once; checking every chunk write is waste.
4. **Request chunks concurrently.** The client asks for one at a time while the
   server already serves them in parallel and QUIC allows many streams.

None are done. All are ordinary optimisation rather than design change, which is
the useful thing the criterion established: at 100k files this system is
*correct and slow*, not *wrong*.

## Next

Phase 1 is complete: every component built, and the kill criterion run and
passed.

1. **Phase 2, adversarial correctness**, before optimising and before adding
   surface. Property testing over random interleavings, crash injection, clock
   skew, a device offline for a simulated month. The scale run exercised the
   happy path at size; Phase 2 is about everything else.
2. **Then the performance work above.** It is worth doing, and it is worth doing
   *after* there are tests that would catch an optimisation breaking
   correctness. Parallelising a storage engine without that is how data gets
   lost.

Pairing, NAT traversal and relays are Phase 3 and should stay there.

Still owed from Phase 0: `quictest stun` on cellular, office, and public wifi.

Also due this phase, per the roadmap: a decision on
[0006 — availability](../decisions/0006-availability-gap.md). Not necessarily
which option ships, but whether the engine must replicate chunks to a node that
is not the user's own device. The data model now exists, so this is the last
comfortable moment to answer it.
