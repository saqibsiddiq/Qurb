# 0008 — The watcher delivers changes at least once

**Status:** Accepted
**Date:** 2026-09-09

## Decision

The filesystem watcher guarantees that every change is reported **at least
once**. It does not guarantee exactly once. Consumers must be idempotent.

## Reasoning

Duplicate reports arise naturally and from more than one direction. A file can
surface both from walking a newly created directory and from its own event
arriving moments later. Platforms themselves duplicate events. A rescan after
queue overflow re-reports everything it finds, including changes already
handled.

Suppressing duplicates means deciding that a reported file has not really
changed, which means knowing what was last stored for it — its size,
modification time, and content hash. **The index already holds exactly that.**
A second cache inside the watcher would duplicate that state and could drift
out of agreement with it, and disagreement between two caches about what has
been stored is precisely the class of bug that loses data.

So the deduplication happens where the knowledge already lives.

## Consequences

**The storage layer is already idempotent**, which is what makes this
affordable: storing a file whose content hash matches what is indexed updates
only the modification time and writes no chunks.

**But it is not yet *cheap*.** `Store::put_file` chunks and hashes the whole
file before discovering the content is unchanged. For a duplicate report of a
large file that is a full read for nothing. The engine should compare size and
modification time against the index first and skip the read entirely when they
match — the same check that makes startup reconciliation fast. That is engine
work, not storage work, and it is not built yet.

**Rescans are safe by construction.** Because consumers must already tolerate
repeats, the response to a dropped-event overflow can be the blunt one: walk
everything and report it. No bookkeeping is needed to work out what was missed.

## Alternatives rejected

**Exactly-once delivery.** Not achievable — the operating system is free to
report the same change twice, and no amount of filtering above it changes that.
Designing as though it were achievable would produce a system that breaks in
ways that are hard to reproduce.

**A content cache in the watcher.** Solves duplicates at the cost of a second
source of truth about stored state. The cost is not worth it, and the failure
mode when the two disagree is bad.

## Reversibility

**Low cost.** Tightening a guarantee later is safe: consumers written against
at-least-once keep working if delivery becomes stricter. Loosening one is not,
which is the reason to state the weaker guarantee now.
