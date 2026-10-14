# 0065 — The scale qurb is built for

**Status:** Accepted as targets and an order of work, 2026-10-14. Measured at
100,000 and a million files the same day: **above a few hundred thousand
files a library cannot sync at all yet**, the whole file list being too large
for one message. Fixing that is next.
**Date:** 2026-10-14

## What was asked

Item 2 of the owner's list of 2026-10-13: "i want to make it scalable". Asked
which scale he meant first, he chose three: **big libraries** (one person with
a very large collection), **a family or team server** (several people sharing
one always-on computer or his VPS), and **a service for many users**
(thousands of separate people on servers he runs). Not chosen: many devices
for one person.

## Where qurb stands

| | measured | conditions |
|---|---|---|
| 100,000 files: re-indexing unchanged | 1.13 s | Phase 1: one Linux desktop, 12 cores, 15 GiB, NVMe, both devices in one process over loopback ([phase 1](../phases/phase-1-engine.md#the-kill-criterion-100k-files)) |
| 100,000 files: syncing 1,000 edited | 3.50 s, 24 MiB on the wire | the same |
| 100,000 files: first index and sync | 237 s and 476 s, 4.4 GiB; per file, mostly syscalls and a `fsync` per chunk | the same |
| the rendezvous service | 9,000 devices in 208 MB, 22 KiB each, introductions 0.10 ms | [service-capacity](../../experiments/service-capacity/README.md) |
| the relay | limited by its bandwidth, not its CPU or memory | the same |
| a device answering requests | one at a time, whatever it is asked | [0064](0064-many-small-files-quickly.md), limit 1 |

## Targets

**Big libraries.** One person, **1,000,000 files and 1 TB**:

- re-indexing an unchanged library within 15 seconds;
- syncing 1,000 changed files within 10 seconds, on a local network;
- a first sync limited by the disk and the network, not by per-file
  overhead: within twice what copying the same files would take;
- browsing and searching on the desktop without waiting.

**A family or team server.** One always-on computer or VPS keeping the files
of **10 people and 50 devices**, each person's private, as
[0060](0060-a-computer-keeps-private-folders-for-several-people.md) already
keeps guests' vaults:

- every device served at once rather than in turn (0064, step 1);
- a separate allowance per person, and the computer saying who uses what;
- running without a screen, as a service.

**A service for many users.** Servers the owner runs for **10,000 people**:

- meeting and relaying them: the rendezvous and relay already carry this on
  one small server by the measurements above;
- keeping their files: a storage server holding many people's sealed vaults,
  which it cannot read, as a guest's computer holds them now (0060). This
  part does not exist and is the largest piece of work here;
- what a service needs beyond software, accounts, payment and terms, is the
  owner's to decide and is not designed here.

## Expected limits, to be measured first

1. **The whole file list is exchanged on every sync.** 100,000 paths took 196
   ms on loopback; a million, over Wi-Fi to a phone, every pass, will not do.
   A device should send only what changed since the last exchange.
2. **One file in the chunk store per chunk**, written with its own `fsync`:
   at a million small files, a million files and syscalls in qurb's store.
   Small chunks packed together would cut both.
3. **One lock and one database connection per store**, which serialise every
   request (0064). A server with fifty devices meets this first.

## Order of work

1. **Measure at a million files** on this laptop with the existing `scale`
   example, and record each stage against the targets.
2. Answer requests in parallel (0064, step 1). It serves item 1 and every
   target here.
3. Exchange only what changed since the last sync.
4. Pack small chunks.
5. A server role: qurb running headless on the VPS, keeping several people's
   sealed vaults with an allowance each, measured with 10 people and 50
   devices.
6. The storage service for many users: designed in its own record once the
   owner has decided what the service is.

## Progress

**A first sync had become quadratic, found measuring for step 1,
2026-10-14.** On this laptop (i5-13420H, 12 threads, 15 GiB, NVMe, btrfs),
both devices in one process over loopback, with the scale example:

| files | first sync, before | per file | after V24 | per file |
|---|---|---|---|---|
| 5,000 | 12.1 s | 2.4 ms | 7.7 s | 1.5 ms |
| 10,000 | 35.7 s | 3.6 ms | 15.2 s | 1.5 ms |
| 20,000 | 95.8 s | 4.8 ms | 30.6 s | 1.5 ms |

A 100,000-file run before the fix had slowed to about 20 files a second by
its 25,000th file. Sampled with gdb, the serving side spent its time in
`Db::any_file_with_content`, asked for every file served: the index on
`content_hash` had been narrowed to live rows by a later migration, so a
question that also accepts deleted rows read the whole of `files` each
time, under the store's lock. Schema V24 indexes every row; a test reads
SQLite's plan and fails on a scan (`content_is_found_through_an_index`).

The example itself had stopped measuring anything since decision 0024: it
served from a store not told where its folder was, so the receiving side
fetched nothing and listed every file as elsewhere. Fixed first (d81da43).

**100,000 files, after V24**, the same laptop and conditions, against
Phase 1's run of the same example:

| stage | Phase 1 | 2026-10-14 |
|---|---|---|
| first index | 236.8 s, 422 files/s | 87.3 s, 1,146 files/s |
| re-index, nothing changed | 1.13 s | 1.27 s |
| file list exchanged | 196 ms | 324 ms |
| first sync, 4.4 GiB | 476 s | 169.2 s |
| 1,000 edited: re-index | 4.93 s | 4.24 s |
| 1,000 edited: sync | 3.50 s, 24.1 MiB | 2.80 s, 24.1 MiB |
| deep verify | 43.4 s | 26.8 s |

Every check passed, 500 sampled files byte-identical after each sync.

**A million files, 2026-10-14**, the same laptop and example, 43.99 GiB
generated:

| stage | measured | target |
|---|---|---|
| first index | 1,623 s, 616 files/s | -- |
| re-index, nothing changed | 27.5 s | within 15 s: **missed** |
| first sync | **failed**: `ReadToEnd(TooLong)` | -- |

The sync could not start. The whole file list goes in one message, and one
message is capped at 64 MiB (`MAX_MESSAGE`); a million entries, each with
its path, content hash, size, version history and author, do not fit. At
100,000 the list fits, so the limit is somewhere between: **qurb cannot sync
a library that large at all today**, which is expected limit 1 above, met as
a hard failure rather than a slow one. Re-indexing grew 22 times for 10
times the files, and the first index slowed per file once the files no
longer fitted in the laptop's 15 GiB of memory.

Next, in this order: the file list in pages, so a large library syncs at
all; then only what changed since the last exchange; then the re-index's
growth.

## Reversing it

Targets are cheap to change. Of the work, step 3 changes the protocol and
step 4 the chunk store's format, each with its own record and a migration
when it comes to that.
