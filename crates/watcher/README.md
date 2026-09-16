# qurb-watcher

Watches a directory and reports the changes a *user* made — not the events a
filesystem emitted. Those are very different things.

One editor save produces a dozen events across two paths. Copying a large file
produces a write event every few milliseconds for as long as the copy runs.
Acting on raw events would re-chunk the same file repeatedly and transfer
half-written data.

```
native events      raw, noisy, per-syscall
      │
      ├─ ignore    drop our own writes, VCS metadata, editor scratch files
      ├─ debounce  collapse bursts; hold a path until it goes quiet
      ├─ stabilise re-stat before releasing, so nothing is read mid-write
      └─ emit      settled changes, or a demand to rescan
```

## Four failure modes it exists to prevent

All four are silent if unhandled, which is what makes them dangerous.

**Reading a file that is still being written.** A file is only released once its
size and modification time have stopped changing. Without this the store would
receive torn copies, and the storage layer's memory-mapped read raises SIGBUS if
a file is truncated underneath it.

**Watching our own chunk store.** The store usually lives inside the watched
tree. Every chunk written would produce an event, which would cause a write,
which would produce an event. `IgnoreRules::with_store_dir` is not a preference,
it is a correctness requirement.

**Kernel queue overflow.** Every backend has a bounded queue, and a large
operation can overrun it. Once events are dropped the stream no longer describes
reality, so the watcher reports `Event::RescanRequired` rather than continuing as
if nothing was missed.

**The recursive watch race.** A platform watch for a new subdirectory is
installed only after that directory exists. Anything written in that window
produces no event at all — which is what `git clone` and unpacking an archive do
constantly. The watcher walks any directory it is told about, so those files are
found rather than silently never synced.

## Delivery guarantee

**At least once, not exactly once.** A path may be reported more than once for
the same change, most often when a file surfaces both from a directory walk and
from its own event arriving afterwards.

Exactly-once is not achievable and not worth chasing. Knowing a reported file
has not really changed means knowing what was last stored for it, and the index
already knows that — a second cache here would duplicate that knowledge and
could disagree with it. Consumers must be idempotent.

## Layers

| module | responsibility |
|---|---|
| `ignore` | what never to watch; pure |
| `debounce` | collapsing bursts into settled changes; pure, no clock of its own |
| `scan` | full directory walk, for startup and after overflow |
| `watcher` | native events wired to the above |

`debounce` takes the current time as a parameter rather than reading a clock.
Timing logic that reads the real clock is close to untestable without sleeping,
and tests that sleep are slow and flaky. Its behaviour is pinned by unit tests
with a logical clock; the integration tests only check that the wiring is right.

## Testing

```bash
cargo test -p qurb-watcher
```

## Not yet built

- **Reconciliation.** `scan` reports what is on disk; comparing that against the
  index and deciding what to do belongs to the sync engine.
- **Symbolic links** are skipped, not represented. Following them invites cycles
  and copies data from outside the synced tree.
- **Unicode normalisation.** macOS stores NFD and Linux usually NFC, so the same
  name can differ in bytes. Case folding is handled
  ([decision 0013](../../docs/decisions/0013-case-collisions.md)) but is
  ASCII-only, and normalisation is the larger part of the same problem.
- **Scoped rescan.** Overflow triggers a walk of the whole tree; a large library
  would rather rescan only the affected subtree.
