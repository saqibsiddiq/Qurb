# 0064 — Many small files, quickly: what limits it, and the order of work

**Status:** Accepted as a plan, 2026-10-14. Step 1 is the first to build.
Nothing here is built yet, and each step is to be measured on the S23 before
it is called done.
**Date:** 2026-10-14

## What was asked

Item 1 of the owner's list of 2026-10-13: "this sync speed is really bad, do
we need rework our entire codebase to fix this or making it fast is simply not
possible?" Asked while the laptop was collecting his WhatsApp backup from the
S23, 36,031 files, mostly small.

## What was measured

| | measured | conditions |
|---|---|---|
| the laptop collecting small files from itself | **2.3 ms a file**, about 440 a second, 65 MB/s | 2,000 files of 150 KB, release build, loopback, this laptop, 2026-10-13 (`timing_collecting_many_small_files`) |
| the S23 reading WhatsApp's folder for its backup | 12 ms a file, four at a time: 23 MB/s | the phone on battery and Wi-Fi, 2026-10-13 (0062) |
| reading through a picked folder against directly | 6.4 ms a file against 0.3 to 0.4 | emulator, warm, 2026-10-14 ([file-access-speed](../../experiments/file-access-speed/README.md)) |
| the laptop collecting from the S23, phone idle | 17 to 28 small files a second; large files about 10 MB/s | home Wi-Fi, 2026-10-13 |
| the same while Samsung throttles Qurb | 1 to 5 files a second | 0062, *Slower while the phone is in use* |

The engine is not what is slow: the same code on one machine is fifteen to
twenty times faster than the phone delivered.

## What limits it

1. **A device answers one request at a time.** Every request takes the lock on
   the whole store (`store.lock()` in `crates/peer/src/server.rs`) and holds it
   while it reads the file's bytes. The laptop asks for 32 small files at once
   (0062), and the phone answers them strictly in turn. At about 40 ms a file
   through a picked folder on the S23 (12 ms a file four at a time), one at a
   time is about 25 files a second. That is the 17 to 28 measured.
2. **The phone opens each file through Android's document provider**, a few
   milliseconds a file even where nothing else is slow. Reading directly,
   with "All files access", costs a tenth of that on the emulator.
3. **Samsung freezes and throttles Qurb** while the screen is on, after the
   CPU the first full backup takes (0062). Not ours to remove, but ours to
   schedule around.
4. **The laptop goes to sleep** by itself when idle (GNOME's idle suspend at
   21:51 on 2026-10-13, woken at 10:06): the night's collecting did not
   happen.
5. **One round trip or more a file over Wi-Fi.** Fetching 32 ahead hides most
   of it; asking for many small files in one request would hide the rest.

## Decision: targeted changes, not a rewrite

The engine, the index, the chunk store and the protocol's shape stay. In
order of expected gain for the least change:

1. **Answer requests in parallel.** Under the lock only what needs the index:
   whether the asker may see the content, and where it is. The reading and
   checking happen after the lock is let go, so 32 requests read at once.
   Measured on the S23 against the 17 to 28 a second above.
2. **Read the phone's folders directly** where "All files access" is granted
   ([0063](0063-backing-up-a-phones-folders-chosen-on-the-desktop.md)). On
   the S23: listing ten times faster (4.3 to 5.3 s for WhatsApp's 36,055
   files, against 39.6 to 57.5), reading twice as fast a file (2.6 to 3.0 ms
   against 5.8).
3. **Keep the laptop awake while it is collecting**, through the system's
   sleep inhibitor, released when the collection ends. The owner decided:
   yes, while plugged in (2026-10-14). Built the same day (`awake.rs`).
4. **Do the heavy part while the phone charges**: a first backup of a large
   folder waits for, or finishes during, charging with the screen off, where
   Samsung's freezer is not running against it.
5. **Many small files in one request**, if steps 1 and 2 leave the round
   trips the largest part.

## Progress

**Step 1, built 2026-10-14.** `Store::locate` asks the index, under the lock,
where a chunk's bytes may be; a `ChunkReader` reads them after the lock is let
go. `read_chunk` does both, unchanged for every caller holding the store
anyway. Proved by `requests_made_at_once_are_read_at_once`: eight files whose
opening takes 200 ms each come in 0.2 s, and in 1.62 s with the reading put
back under the lock. On this laptop's loopback, where reading was never the
slow part, collecting small files is unchanged: 2.18 to 2.48 ms a file over
three runs, against 2.28 before. **Not yet measured on the S23**, where it is
meant to matter.

## Rejected

- **A rewrite.** Nothing measured points at the engine's design; the slowest
  stage on one machine is fast.
- **Raising the fetch-ahead** beyond 32: with the phone answering one at a
  time, more requests in flight only wait longer.

## Reversing it

Each step stands alone and can be undone alone. Step 1 changes no format and
no message: a device that answers in parallel gives the same answers.
