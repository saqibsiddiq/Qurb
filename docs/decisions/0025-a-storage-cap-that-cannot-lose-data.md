# 0025 — A storage cap that cannot lose data

**Status:** Accepted
**Date:** 2026-09-22

## Decision

A folder may be given a limit — `qurb config <dir> limit=10G`. When a device is
over it, qurb frees space by **dropping local copies of files while keeping
everything the index knows about them**: the path, the content hash, the chunk
list, the version vector. The file leaves the folder; the file does not leave
qurb.

Three rules make this safe, and all three are refusals:

1. **A file is never dropped unless another device is known to hold its exact
   content.** No knowledge, no eviction.
2. **A device that cannot free enough stays over its limit** and says so. It
   does not approach the limit by deleting the only copy of something.
3. **A storage-only replica evicts nothing**, because it is the only holder of
   everything it has.

Rule 2 is the one worth being explicit about, because it is the one that looks
like a bug: the user typed a number, and qurb is over it on purpose. That is
correct. A limit is a promise about disk, not about data, and no number a
person types into a settings box is worth more than the only copy of their
work.

## How "known to hold" is established

A `replicas` table records, per content hash, which devices are known to have
those bytes. A row is written when this device **adopts a version some other
device made** — the device named in `modified_by` created that content and has
it.

This is deliberately narrow, and it has one consequence worth stating plainly:
**a device never records a replica for content it originated itself.** A photo
the phone took and sent to the desktop may be dropped on the desktop. The same
photo may not be dropped on the phone, because as far as the phone can prove,
it is the only holder. That asymmetry is the right way round — the device that
made something is the one that should keep it.

Measured on the development laptop, 2026-09-22, two devices on loopback with a
2 MiB limit and 4.8 MiB of content: the device that received both files dropped
one and came in at 1.9 MiB; the device that made both files dropped nothing and
reported itself 2.8 MB over, with nothing safe to drop.

### What this does not cover

If **every** device holding some content evicts it, the content is gone. The
record says "device D had these bytes at time T", not "device D has them now".
Today that cannot happen in practice — a device only evicts content it did not
originate, so the originator is still holding it — but it becomes reachable the
moment a device can lose content some other way while still being listed as a
replica.

The fix, when it is needed, is to ask rather than remember: a `Have { content }`
request answered by checking the peer can actually produce every chunk. That is
a protocol addition and is not made now, because the evidence available without
it is sufficient for the arrangement that exists today, and a protocol change
should be made when it is needed rather than in anticipation.

## Eviction must never look like deletion

This is where a storage cap destroys data, and it does it quietly.

A syncing device decides a file was deleted by not finding it during a scan.
Eviction removes a file. Without care, the sequence is: the device is short of
space, so it drops a file, so the next scan concludes the user deleted it, so
the deletion propagates to every other device — and the file is gone
everywhere, because the device was low on disk.

So the index records whether this device is *holding* each file, separately
from whether the file exists:

- `files.materialised` is set to 0 **before** the file is unlinked, never
  after. The window in the safe direction — marked but still present — costs at
  worst one needless fetch. The window in the unsafe direction costs the file.
- The scan skips a missing file whose row says it was dropped on purpose.
- The watcher's removal handler does the same, because eviction generates an
  ordinary removal event like any other.

Verified end to end on 2026-09-22 with two devices: after eviction, the other
device still held both files and neither device recorded a deletion.

## Getting a file back

`qurb fetch <path>` sets a `wanted` flag in the index rather than performing a
transfer. The command runs in a different process from the daemon, the daemon
may not be running, and no peer may be reachable — so the request is written
down and acted on when a peer next is. Measured: the file returned on the
following sweep and was byte-identical to the original.

This is the same property mobile needs for offline sharing, arrived at from the
other direction: a request that survives being offline.

## Why not the alternatives

**Placeholder files.** Windows (CfAPI) and macOS (FileProvider) let an evicted
file keep its name and size in the file manager and fetch on open. Linux has no
equivalent short of a FUSE mount, which means a daemon whose failure takes the
user's folder with it. On Linux an evicted file is simply absent, and
`qurb status` is where you find out what is not here. This is a real gap in the
experience, stated rather than papered over.

**Refuse new content instead of evicting.** Simpler and safe, but it makes a
full device stop syncing, which is the opposite of what the feature is for.
Both happen in the end: eviction first, and a device that cannot evict stays
over its limit rather than refusing to sync.

**Evict by size, or by age on disk.** Coldest-first, by last touch, with size
breaking ties. Size alone evicts exactly the large files someone keeps a cap
for in order to be able to hold.

## What this depends on

A cap is only as good as the space it can actually free, and until this work
**garbage collection had never run** — `Store::gc` had no caller outside tests.
It is now on a five-minute timer in the daemon, ahead of the limit check, with
a seven-day retention window. The order is deliberate: collecting frees
superseded and deleted content, which costs the user nothing, and only then is
it fair to drop copies of files they still have.
