# 0004 — Chunk size bounds of 128 KiB / 512 KiB / 2 MiB

**Status:** Accepted
**Date:** 2026-09-09
**Supersedes:** the 1 MiB / 2 MiB / 4 MiB proposal in the original architecture
document.

## Decision

FastCDC is configured with a 128 KiB minimum, 512 KiB average, and 2 MiB
maximum chunk size.

## Reasoning

The original architecture document proposed 1 MiB minimum, 2 MiB average, 4 MiB
maximum. Measuring against a real corpus showed this to be a significant
mistake.

In a representative user directory (2720 files, 1.17 GiB), **94.5% of files were
smaller than the proposed 1 MiB minimum.** Every one of those files collapses
into exactly one chunk. Content-defined chunking does nothing for them — no
sub-file deduplication, and any edit re-sends the entire file. The system would
have been paying CDC's complexity while getting fixed-block behaviour for the
overwhelming majority of files.

Sweeping the parameters over the same corpus (`sweep` in the Phase 0 spike):

| config | chunks | mean | dedup | index / TiB | small-edit resend |
|---|---|---|---|---|---|
| 64 KiB avg | 17255 | 71 KiB | **19.2%** | 1.35 GiB | 71 KiB |
| 256 KiB avg | 6133 | 200 KiB | 16.0% | 492 MiB | 200 KiB |
| **512 KiB avg** | **4353** | **281 KiB** | **13.3%** | **349 MiB** | **281 KiB** |
| 1 MiB avg | 3493 | 351 KiB | 10.7% | 280 MiB | 351 KiB |
| 2 MiB avg (original) | 3045 | 402 KiB | 7.4% | 244 MiB | 402 KiB |

512 KiB is the knee of the curve. Against the original proposal it nearly
doubles the deduplication rate and cuts small-edit transfer cost by a third,
while keeping the metadata index under 350 MiB per TiB of library — small enough
for SQLite to serve quickly and small enough to be viable on a phone.

64 KiB buys more deduplication still, but 1.35 GiB of index per TiB is too much
to hold on a mobile device, and index size is the constraint that bites hardest
as a library grows.

A supporting observation: chunking proved to be **disk-bound rather than
CPU-bound**. The same corpus measured 246 MiB/s cold and 665–1178 MiB/s warm.
Since the hash is not the bottleneck, spending CPU on more, smaller chunks is
close to free.

restic independently converged on a 512 KiB average, which is weak but real
corroboration.

## Tradeoff accepted

More chunks means a larger metadata index and more per-chunk overhead — an
encryption nonce and authentication tag per chunk, and more filesystem inodes.
At 512 KiB this is acceptable; it would not be at 64 KiB.

## What would change this

These numbers come from one corpus on one machine. A user population whose files
skew much larger — video editors, for example — would shift the knee upward.
Worth re-measuring against real user data before the parameters are frozen by
stored data that would need rewriting to change.

## Reversibility

**Expensive after launch.** Changing chunk parameters does not corrupt anything,
but existing chunks keep their old boundaries, so deduplication across the
boundary change is lost until data is rewritten. Best settled before there is
stored data to preserve.
