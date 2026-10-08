# 0058 — A full replica keeps everything and says so

**Status:** Accepted — the behaviour the code already has, now decided;
settles product plan §4.7
**Date:** 2026-10-08

## The question

A storage-only replica, an always-on box such as the owner's planned VPS,
can be given a storage limit like any device. Over it, an ordinary device
drops local copies that another device is known to hold
([0025](0025-a-storage-cap-that-cannot-lose-data.md)). A replica has no
folder, so it has no local copies to drop. Freeing space there would mean
dropping chunk payloads outright. 0025's third rule said a replica evicts
nothing, "because it is the only holder of everything it has". The product
plan left open whether that should stay so, before any storage screen
promised otherwise.

## Decision

**Never.** Asked on 2026-10-08, the owner chose: a replica over its limit
drops nothing. It keeps everything and says it is over, as the daemon does
for any device that cannot get under: *over the storage limit, and keeping
it: nothing left is safe to drop*. The only things it frees are the ones any
device frees:

- garbage past the retention window;
- Recently deleted, oldest first;
- copies of sends their recipients have.

A replica's whole job is to be the copy that can be relied on while the
person's other devices are off. A replica that dropped what two computers
also hold would have nothing to offer whenever both were off, which is when
it matters.

## Rejected

- **Dropping content at least two computers are recorded as holding**,
  oldest first. It saves disk, and makes the replica unreliable exactly when
  it is needed.
- **Leaving it undecided until the VPS exists.** The behaviour already
  exists, and a storage screen should be able to say what a full replica
  does.

## What it costs

A replica's disk has to hold everything it is given. When it fills, it says
so, and the person adds disk, gives it less to hold (`qurb replica <dir>
--only <path>`), or deletes files. Nothing is lost by its being full: what
is not on it is still on the devices that made it.

## Checked

`crates/storage/tests/storage_cap.rs`: `a_replica_evicts_nothing`, where even
content another device holds is refused. The warning is the daemon's
storage-limit check (`crates/qurb/src/daemon.rs`), the same for every
device.
