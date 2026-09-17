# qurb-engine

Joins the filesystem watcher to the store. The watcher says something changed;
the store can save and remove files. This crate holds the decisions in between.

```
startup ──► reconcile   walk the disk, compare with the index,
                        store what is new, tombstone what is gone

running ──► apply       act on settled changes from the watcher

overflow ─► reconcile   the event stream stopped describing reality,
                        so fall back to comparing everything
```

## Two rules that shape everything here

**A file is not re-read unless it looks different.** The watcher delivers
changes at least once ([decision 0008](../../docs/decisions/0008-watcher-delivery-guarantee.md))
and reconciliation revisits every file, so the same path arrives repeatedly with
nothing changed. Comparing size and modification time against the index first
turns that from a full read into a stat.

Measured on a real directory — 2437 files, 979 MiB:

| run | result | elapsed |
|---|---|---|
| first | 2437 stored | 12.96s |
| second | 2437 unchanged | 0.03s |

**One bad file must not stop the others.** A file with no read permission, or
one deleted between being reported and being read, is recorded in
`SyncStats::failures` and the run continues. An engine that aborts on the first
failure leaves everything else unsynced for a reason the user cannot see.

## The mtime heuristic, stated plainly

Matching size and modification time is taken as proof that content is unchanged.
That is not strictly true: a file edited in place, keeping its exact length,
within the same timestamp tick would slip through.

The window is small, and the alternative is reading every file on every
reconciliation — the difference above between 0.03 seconds and 13. rsync and git
make the same trade. The backstop is `Store::verify`, which re-reads everything
and is meant to run occasionally rather than per change.

## Removal is resolved against the index

The watcher reports that a path is gone. It cannot say whether it was a file or
a directory, because there is nothing left to inspect.

So the question is turned around: instead of asking the filesystem what was
removed, the engine asks the index what it knew about at or under that path.
`Db::live_paths_under` answers it, and every match is tombstoned.

## Comparing with another device

[`qurb-sync`](../sync/) decides what should happen when two devices disagree.
The `peer` module carries it out.

```
tree()          what this device would tell a peer it has, tombstones included
plan_against()  qurb_sync::reconcile, given the peer's tree
apply_plan()    do the local half, fetching content it does not already hold
```

Content is requested **by hash, not by path**
([decision 0010](../../docs/decisions/0010-content-by-hash.md)). The content a
plan calls for often lives under a different name on the device that has it —
that is exactly what a conflict rename produces — so asking by path would fail
where asking by content succeeds. It also makes renames and copies free: the
engine checks whether any live path already holds those bytes before asking
anyone for them.

`ContentSource` is the seam where the network will go. Everything above it is
finished; below it there is one implementation that reads another local store,
which is what the two-device tests and the `sync_pair` example use.

### Vectors and the fast path must not fight

`apply_plan` writes a file to disk. A reconciliation then walks that same file.
If it stamped it as a *local* change, the adopted version would become
concurrent with the peer's own copy and the next exchange would raise a conflict
over a file that had just synced successfully — forever.

What prevents it is `apply_plan` recording the modification time the file
actually ended up with, so the size-and-mtime fast path recognises its own work.
Three tests pin this: for adopted files, adopted tombstones, and conflict files.

## Storing many files at once

Reconciliation makes two passes, because they cost completely different things.
Deciding whether a file changed is a stat and an index lookup — 100k of them take
about a second. Storing one that did change is a read, a chunking pass,
compression, encryption and an fsync, most of which is waiting.

So the decision is made in order, and the storing is handed to workers, each with
its own connection to the same store. SQLite still permits one writer at a time,
but those transactions are short and everything around them is not.

Measured on 20,000 files: **487 files/s with one worker, 830–888 with four.**
The gain is from overlapping waits rather than from computation — chunking,
hashing and encryption together are under a tenth of the time — which is why it
stops improving once the disk has enough requests in flight. Eight workers were
no better than four.

## Repairing damage

`Engine::repair` discards chunks that verification found missing or corrupt and
refetches them from a peer. Possible only because content is addressed by hash:
a damaged chunk is not a lost chunk, and the hash says exactly what is wanted.

The refetched bytes are verified before being written. The peer is not trusted —
writing unverified bytes over a file already known to be damaged would turn a
detectable problem into an undetectable one.

## Applying a plan: additions before removals

A rename arrives as a set of additions and a set of deletions. Applying them in
path order made the cost depend on the alphabet: renaming `project` to `archive`
found the content still on disk and moved nothing, while renaming it to
`renamed` deleted the old paths first and re-transferred the entire tree.

Additions now run first, and content is looked up by hash across all paths
including tombstoned ones — a deleted file's chunks survive the retention
window, and "do we have these bytes?" is a question about chunks rather than
names.

## Receiving a file without holding it

Content arrives a chunk at a time and goes straight to disk. Nothing assembles a
whole file in memory, so peak memory is one chunk — at most 2 MiB — regardless
of the file's size. Adopting a 1 GiB file used to grow the heap by 1024 MiB and
now grows it by 1:

```bash
cargo run --release -p qurb-engine --example peak_memory -- buffered 1024
cargo run --release -p qurb-engine --example peak_memory -- stream 1024
```

Both shapes are kept so the comparison stays checkable. The reason it matters is
[decision 0018](../../docs/decisions/0018-file-contents-never-cross-the-ffi.md):
an iOS FileProvider extension is killed at a ceiling in the tens of megabytes,
and a path that buffers works for documents and kills the process for video.

Content cannot be verified until its last byte arrives, so a file is assembled
under a staging name beside its destination and renamed once the hash checks
out. Unverified bytes are therefore never visible at the real path, and an
interrupted transfer leaves nothing that looks complete. The watcher ignores
that staging name; without it the half-written file would be indexed, its rename
read as a deletion, and both the phantom and its removal sent everywhere.

## Two kinds of device

An engine is either **syncing** a directory someone uses, or acting as a
**storage-only replica** — always on, holding content so the other devices need
not all be awake at once.

A replica switches off two behaviours, and both would be destructive rather than
merely wrong:

**It does not infer deletion from an empty directory.** A syncing device decides
a file is gone by walking its tree and not finding it. A replica has nothing on
disk by design, so the same inference would tombstone the entire library and
propagate those deletions to every device that trusted it. `Engine::reconcile`
returning early for a replica looks like a no-op; removing it would destroy data
everywhere, quietly.

**It does not materialise files.** Storing chunks *and* writing every file costs
roughly twice the space for a copy nobody reads.

One consequence: a replica is the only path that still buffers a whole file,
because there is no file on disk to stream into. Replicas are machines somebody
keeps switched on rather than phones, so the memory ceiling does not apply — but
the gap is real and is marked where it lives in `peer.rs`.

A replica can hold a subset — `PinSet::under(["work"])` — because "hold
everything" is the expensive answer and the useful one is usually "hold what I
reach for". See
[decision 0006](../../docs/decisions/0006-availability-gap.md).

## Trying it

```bash
cargo run --release --example sync_once -- ~/Documents /tmp/qurb-store
```

Two directories against each other, as two devices:

```bash
cargo run --release --example sync_pair -- /tmp/dev-a /tmp/dev-b
```

Run it twice on the same pair. The second run should store nothing and finish
almost instantly.

## Not yet built

- **Backpressure.** A huge batch of changes is applied in one pass with no
  bound on how long that takes.
- **Moves.** A renamed file is stored again under its new path and tombstoned
  under the old one, rather than recognised as the same content moving. The
  chunks are deduplicated and a peer adopting the rename finds the content
  already on disk, so the cost is a local re-read rather than a transfer.
