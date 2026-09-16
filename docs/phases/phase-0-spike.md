# Phase 0 — Spike

**Status:** complete
**Duration:** ~1 day (budgeted 2–3 weeks)
**Code:** [`experiments/phase0-spike/`](../../experiments/phase0-spike/)
**Verdict:** green — no kill criterion tripped, one real design error caught

## Purpose

Answer, with measurements rather than citations, whether the core technical
assumptions hold — before committing months to building on them.

Deliberately throwaway code. No encryption, no compression, no database, no
error recovery. The goal was evidence, not foundations.

## Conditions

All measurements on one Linux machine: 12 cores, 15 GiB RAM, NVMe storage.

Real corpus: `~/Downloads`, 2722 non-empty files, 1.18 GiB. Its shape matters
for interpreting the results — **94.5% of files are under 1 MiB**:

| size | share |
|---|---|
| < 64 KiB | 82.3% |
| 64 KiB – 1 MiB | 12.1% |
| 1 – 4 MiB | 3.7% |
| > 4 MiB | 1.8% |

A synthetic corpus was also used, but it flatters the system — its deduplication
figure is manufactured by a planted duplicate region and means nothing.

## Results

| # | criterion | threshold | result |
|---|---|---|---|
| 1 | chunking throughput | ≥ 150 MiB/s | **PASS** — 665 warm, 246 cold |
| 2 | boundary stability | ≥ 90% chunk reuse | **PASS** — 99.7% vs 0% fixed |
| 3 | round-trip integrity | whole-file hash matches | **PASS** — 2722/2722 files |
| 4 | QUIC throughput | ≥ link speed | **PASS** — 287 MiB/s loopback |
| 5 | NAT traversal | cone NAT on most networks | **PARTIAL** — 1 of N tested |

### 2 — Boundary stability, the load-bearing result

Prepending a single byte to a 196 MiB file:

| method | chunks reusable |
|---|---|
| FastCDC | 304 / 305 — **99.7%** |
| fixed 4 MiB blocks | 0 / 50 — **0%** |

This is the entire justification for content-defined chunking over the
Dropbox-style fixed-block model, and it is now measured rather than assumed.
Practically: editing the first byte of a 2 GB video re-sends about 3 MB instead
of 2 GB.

### 5 — NAT, partially answered

The home network reports the same external port from two different STUN
operators, meaning **endpoint-independent (cone) NAT** — hole punching should
work, and direct connections should be the norm from here.

One network is encouraging, not conclusive. Cellular, office, and public wifi
still need testing. The worst network in the supported set determines the relay
bandwidth bill.

## What the spike changed about the plan

### Chunk parameters were wrong

The architecture document proposed 1 MiB / 2 MiB / 4 MiB bounds. Against a real
corpus this badly underperforms: 94.5% of files fall below the 1 MiB minimum and
collapse into a single chunk each, so content-defined chunking does nothing for
them at all.

Sweeping parameters over the same corpus:

| config | chunks | mean | dedup | index / TiB | small-edit resend |
|---|---|---|---|---|---|
| 64 KiB avg | 17255 | 71 KiB | **19.2%** | 1.35 GiB | 71 KiB |
| 256 KiB avg | 6133 | 200 KiB | 16.0% | 492 MiB | 200 KiB |
| **512 KiB avg** | **4353** | **281 KiB** | **13.3%** | **349 MiB** | **281 KiB** |
| 1 MiB avg | 3493 | 351 KiB | 10.7% | 280 MiB | 351 KiB |
| 2 MiB avg (doc) | 3045 | 402 KiB | 7.4% | 244 MiB | 402 KiB |

512 KiB average is the knee: nearly double the deduplication of the original
proposal, a third less data re-sent per small edit, and an index that stays
under 350 MiB per TiB — small enough for a phone. Adopted as
[decision 0004](../decisions/0004-chunk-parameters.md).

Catching this at week two rather than month eight is the entire return on the
phase. Chunk parameters are cheap to change now and expensive once there is
stored data whose boundaries depend on them.

### Chunking is disk-bound, not CPU-bound

Identical corpus: 246 MiB/s cold cache, 665–1178 MiB/s warm. The hash is not
the bottleneck; the disk is.

Two consequences. Smaller chunks cost almost nothing in CPU terms, which
supports the parameter change above. And the Phase 1 budget for adding Zstd
compression and ChaCha20-Poly1305 encryption to this same path is larger than it
appeared.

### Conflict resolution rule was unsound

Not a spike measurement, but identified while reviewing the plan. The original
"highest vector generation index wins" rule cannot order genuinely concurrent
changes and makes outcomes depend on unrelated devices' activity. Replaced by
[decision 0005](../decisions/0005-conflict-resolution.md), whose central clause
is that no edit is ever silently discarded.

## Deliberately not done

- No encryption or compression — Phase 1.
- **No reference counting, so no chunk deletion.** The spike can only add data.
  This is the first correctness problem Phase 1 must solve.
- `mmap` without handling concurrent truncation, which would SIGBUS against a
  racing writer.
- QUIC peer verification disabled; real peer identity comes from Noise IK over
  static Curve25519 keys, not the web PKI.

## Bugs found and fixed during the phase

- `chunkbench` panicked on a directory with no files, via an `.expect()` that
  an empty file list reached. Now fails with a usable message.
- The synthetic corpus filled `/tmp`, which on this machine is a 7.7 GiB
  RAM-backed tmpfs. `SPIKE_SCRATCH` now documented as needing a disk-backed
  path.

## Remaining before Phase 1

1. Run `quictest stun` on cellular, office, and public wifi.
2. Run `sweep` against a corpus of large media files to check whether the
   512 KiB knee holds for a different file-size distribution.
