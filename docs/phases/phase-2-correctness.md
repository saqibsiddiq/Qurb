# Phase 2 — Adversarial correctness

**Status:** complete — kill criterion passed 2026-09-16
**Target:** months 4–5
**Kill criterion:** convergence failures that cannot be explained.

The phase most projects skip and then regret. Phase 1 proved the system works;
this one is about what it does when things go wrong — and whether we find out
before users do.

## Progress

| area | status |
|---|---|
| property-based convergence testing | ✅ 21 properties, shrinking, persisted regressions |
| crash injection | ✅ real SIGKILL at seven kill points |
| clock skew | ✅ ±10 years, convergence unaffected |
| long-absent devices | ✅ a month offline, both directions |
| case-insensitive collisions | ✅ detected and refused |
| corruption recovery from a peer | ✅ detected damage is refetched and verified |
| large renames and mid-sync races | ✅ found two real bugs |
| fault injection in the transport | ✅ hostile and broken peers |
| concurrent writers and collection | ✅ collector runs against a live writer |

293 tests pass across six crates; clippy is clean.

**The kill criterion — convergence failures that cannot be explained — was not
met.** Every failure this phase produced was explained, and four of them were
real defects that are now fixed.

## Property-based testing

The Phase 1 convergence tests ran a hand-rolled loop over fixed seeds. That
found a real bug, which is why it existed — but it could only find bugs inside
the shapes the generator happened to produce, and a failure arrived as a 30-step
transcript to read by hand.

`proptest` replaces it with generated histories and **shrinking**: a failure
comes back as the smallest sequence that still breaks the property. Failing
cases are persisted to `tests/proptest-regressions.txt`, so a rare counterexample
becomes a permanent regression test rather than something seen once.

Twenty-one properties, in three groups.

**The version vector algebra** — comparison is antisymmetric, merge is
commutative, associative and idempotent, a merge dominates both inputs,
absorbing something already seen changes nothing, incrementing moves strictly
forward, encoding round-trips, and equal vectors encode identically however they
were built. These are the laws every higher-level guarantee rests on.

**Resolution** — the outcome is symmetric (both devices reach the same answer
with no negotiation), every resolution dominates both sides, identical content
never conflicts, and a conflict always keeps both contents. That last one is
decision 0005's promise expressed as a property rather than an example.

**Whole histories** — random sequences of edits, deletes and syncs across two
and three devices, asserting convergence of both visible state and recorded
history, that no work remains afterwards, that syncing again changes nothing,
that no content appears which nobody wrote, and that settling never takes more
than four rounds.

That last bound matters as much as convergence itself. An engine that needs
unbounded rounds to settle does not settle, and in production that looks like
two devices trading files forever while the battery drains.

## Crash injection

Not simulated. A real child process writes into a store and is killed with
`SIGKILL` — no unwinding, no destructors, no flush of anything not already
fsynced — and the store is then opened and checked.

The invariant being defended is the one the write ordering exists for: **a chunk
referenced by the index always exists on disk.** Writes go payload-first, so a
crash can leave a chunk nothing points at. Orphaned data is a cost; a dangling
reference is a corruption.

Seven kill points, plus a crash on top of a crash, plus writing again
afterwards. After each: no missing chunks, no corrupt chunks, no reference-count
drift, every indexed file readable, and everything the writer confirmed still
present.

A check that the test is not vacuous: at a 150ms kill the writer had confirmed
11 files and 12 chunk files existed on disk. That twelfth chunk is the orphan —
the kill landed mid-write, which is exactly the window being tested.

The surviving-data assertion is worth stating precisely. SQLite in WAL mode with
`synchronous=NORMAL` can lose recent commits to *power* loss, but a process kill
leaves the data with the kernel, which still writes it. So a killed process must
lose nothing it confirmed, and the tests assert that rather than assuming it.

## Clock skew

Ordering never consults wall-clock time — that is the version vector's job — and
these tests exist to prove it rather than assert it.

A device three days in the past, another ten years in the future, three devices
with three different wrong clocks. In every case convergence is unaffected and
no edit is lost. One test pins the stronger claim directly: **the same pair of
concurrent edits resolves the same way at every skew from a month behind to a
year ahead.** If it did not, two devices with different skew would rename each
other's copies and never agree.

## Long-absent devices

A device offline for a simulated month while two others exchanged thirty days of
edits and deletions. On rejoining it converges with both, and no work remains.

The more dangerous version is a device that *worked* while disconnected. Thirty
edits on each side, concurrent, plus a file only the absent device has. Both
survive, which is decision 0005 holding under a month of divergence rather than
a single conflicting pair.

And the one that reverses silently if handled badly: a deletion made while a
peer was away must not be undone when that peer offers the file back.

## Case-insensitive collisions

`README` and `readme` are two files on Linux and one on macOS or Windows.

The nuisance is that writing the second destroys the first. What makes it *data
loss* is what happens next: the reconciliation after the overwrite sees one file
where the index expected two, reports the missing one as deleted, and propagates
that deletion to every other device. One confusing merge becomes data gone
everywhere.

Now handled, as [decision 0013](../decisions/0013-case-collisions.md):

- Case sensitivity is **probed**, not assumed from the platform. macOS can be
  formatted either way and a network mount can be anything regardless of host.
- Colliding paths are **reported even where they are currently harmless**,
  because the harm arrives with the next device rather than the next write.
- A colliding write is **refused**, recorded as a per-file failure, and the rest
  of the plan continues.
- The behaviour is **configurable**, because the hazard belongs to the fleet
  rather than the local disk: a Linux desktop holding both files is fine until a
  phone joins.

Folding is ASCII-only, via SQLite's `lower()`. That misses collisions in other
scripts — Turkish dotted I among them — and catches what actually occurs. A full
Unicode fold belongs with normalisation work, which this is not.

## Repairing damage

Detecting corruption is not fixing it. Until this phase, a single bad chunk made
every file using it permanently unreadable on that device, even while a peer
held a perfect copy. `verify` reported the problem and nothing acted on it.

`Engine::repair` now discards damaged payloads and refetches them. What makes
this possible is content addressing: a damaged chunk is not a lost chunk, it is
a chunk whose bytes are wrong, and the hash says exactly what is wanted. Nothing
has to be reconciled or agreed.

Two details worth recording. The damaged payload is discarded **before** the
replacement is fetched, which widens the window in which the file is unreadable
— but the alternative leaves the bad payload in place, and the refetched content
then finds the chunk already "present" and keeps the damaged copy, so repair
silently does nothing. And the source is **not trusted**: refetched bytes are
checked against the hash before being written, because writing unverified bytes
over a file already known to be damaged turns a detectable problem into an
undetectable one.

### A bug found while building it

Repair did not work at first, for an instructive reason. `put_file` short-
circuits when the content hash already matches the index — the optimisation that
makes an unchanged file cost a stat instead of a read, and the reason a warm
re-index of 100k files takes 1.13 seconds.

It assumes matching content means the payloads are still on disk. That is true
everywhere except repair, which has just discarded them on purpose. Repair
reported success and changed nothing.

A valid optimisation on the hot path silently breaking a recovery path is
exactly the class of bug this phase exists to find. The fix is a `Payloads`
flag: every ordinary write trusts the index, and repair alone does not.

## Renames, and a performance cliff nobody would have noticed

Renaming a directory arrives as a set of additions and a set of deletions,
applied in path order. That ordering turned out to decide whether the rename was
free or catastrophically expensive.

Renaming `project` to `archive` sorts the additions first, so the content is
still on disk under the old name and nothing crosses the wire. Renaming
`project` to `renamed` sorts the deletions first — so the old paths are
tombstoned, the content lookup finds nothing, and **the entire tree is
re-transferred**.

Measured: 500 files, 500 chunks fetched for a pure rename in one direction and
zero in the other. A user tidying up a folder would have re-downloaded their
library or not, depending on the alphabet.

Two fixes. Content is now looked up **by hash across all paths including
tombstoned ones** — "do we have these bytes?" is a question about chunks, not
about names, and a deleted file's chunks survive the retention window. And
`apply_plan` now orders **additions before removals**, which also shortens the
window in which a renamed file exists at neither path.

The test now covers both sort directions, because the bug only appeared in one.

### Deletions left empty directories behind

Renaming or deleting a directory propagated correctly in the index and left the
other device holding a skeleton of empty folders. Every file was in the right
place and what the user saw was their old directory apparently half-deleted.
Empty parents are now pruned up to — never including — the synced root.

## Hostile peers

Everything before this assumed the other end was a working copy of this
software. These tests assume the opposite: a peer that sends wrong bytes, claims
content it lacks, answers with nonsense, replies to the wrong question, or goes
silent mid-session.

The property being defended is absolute: **a peer can waste our time, but it
cannot corrupt our store.** Content is addressed by hash, so every byte that
arrives is checked against what was asked for. After each hostile behaviour the
store still verifies clean and holds zero chunks.

This matters more than it might seem. Relays are Phase 3 and will forward
traffic we do not control, and a stolen device keeps its pinned identity.
Authentication says *who* is talking; it says nothing about whether they are
telling the truth.

## Concurrent writers

SQLite permits one writer at a time, and this system genuinely has several: the
engine writing, the peer server reading from its own connection, and garbage
collection taking the write lock for its deletions. That reasoning had never
been exercised.

**The reasoning was wrong, and the test found it.** Running the collector against
a live writer produced a foreign-key violation within seconds.

The writer decides which chunks it already holds *before* opening the
transaction that references them. Collection can remove one in that gap — and it
only ever removes chunks nothing references, which is exactly what a chunk about
to be referenced looks like until the reference exists. The write lock protects
the collector's own work; it does nothing about a check somebody else made
earlier.

What saved this from being data loss is the foreign key on `file_chunks`.
SQLite refused the insert rather than letting a dangling reference through, so
the failure was a rejected write with an opaque error rather than an index
pointing at nothing. A schema constraint written for tidiness turned out to be
the last line of defence.

The fix re-establishes every chunk **inside** the transaction, which is where
the lock genuinely does exclude collection: the transaction opens as a writer
rather than upgrading later, each chunk is confirmed present in both the index
and the store, and anything that vanished is written again before the reference
is made. The ordinary case costs one query per chunk and writes nothing.

`rewriting_content_that_collection_just_removed_succeeds` forces the race
deterministically rather than waiting for it — write, delete, collect twice so
the payloads are genuinely gone, then write the identical content again.

One thing this also made deliberate rather than accidental: the busy timeout is
now set explicitly. Without one, the loser of a write race gets an immediate
"database is locked" instead of waiting its turn, and relying on a library
default for behaviour under contention is not a decision anyone made.

## What this phase found

Five real defects, none of which any Phase 1 test would have caught:

1. **A writer could reference a chunk collection had just removed.** The
   check and the reference were in different transactions, and the write lock
   only covered the second. Caught by a foreign key rather than by the design.
   The most serious of the five.
2. **Repair silently did nothing**, because a hot-path optimisation assumed
   payloads existed whenever the content hash matched.
3. **Renames re-transferred entire trees**, depending on how the old and new
   names happened to sort.
4. **Deletions left empty directories**, so a rename looked half-applied.
5. **Contention behaviour was undecided**, inherited from a library default.

Every one is the same shape: **correct in isolation, wrong in combination.**
Each component did what its own tests said. The defects lived in the seams —
between an optimisation and a recovery path, between an ordering and a lookup,
between two connections to one database.

That is what an adversarial phase is for, and it is the argument for having run
it before the performance work. Parallelising a storage engine on top of a
collector that could already lose a race against a single writer would have been
building on sand.

It is also worth noting what did *not* break. Convergence held under every
randomised history, every clock, and every absence thrown at it; the crash tests
never produced a dangling reference; no hostile peer got a byte onto disk. The
core the earlier phases built is sound. What this phase found were the joins.

## Deliberately not covered

- **Power loss**, as opposed to process death. SQLite with `synchronous=NORMAL`
  can lose recent commits when the machine loses power, which a `SIGKILL` does
  not exercise. Testing it properly needs a virtual machine that can be cut off
  mid-write.
- **Filesystem-level corruption** beyond a flipped bit in a chunk — a truncated
  index, a damaged WAL, a disk returning stale reads.
- **Unicode normalisation.** Case folding is handled; macOS storing NFD where
  Linux stores NFC is the larger half of the same problem and is untouched.
- **Adversarial input to the storage layer** from a hostile *local* actor. The
  threat model assumes the machine is trusted, and the master key sitting in an
  owner-only file already reflects that.
