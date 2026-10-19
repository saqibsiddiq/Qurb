# 0069 — A file read moments after it was written is read again

**Status:** Accepted, 2026-10-18. Built in the store and the engine, index
schema 26. Reproduced on this machine by a test that fails without it, and
found failing on Android 8 without it; with it, on 2026-10-19, the whole suite
passed on Android 8 as 32-bit x86, and `an_edit_propagates` with it as x86_64.
**Date:** 2026-10-18

## What was found

Asked on 2026-10-18 to "find and fix every possible bug", the Rust test suite
was run on an Android 8.0 emulator (API 26, x86_64) for the first time. One
engine test failed there that passes on the laptop:

```text
an_edit_propagates: left: Some("version one"), right: Some("version two")
```

The test writes `version one`, syncs, writes `version two` -- the same length
-- and syncs. The second write never left the device.

A scan takes a file whose size and modification time match the index as
unchanged, without reading it (`Engine::looks_unchanged`). That is the
trade rsync makes, and the engine's comment said git makes it too. A
filesystem's clock does not advance continuously, though: it ticks. On the
emulator's kernel (Android 8, Linux 3.18) a file's time moves in steps of a
few milliseconds; Linux before 6.13 does the same, which includes Debian 12's
6.1, Ubuntu 22.04's 5.15 and RHEL 9's 5.14; FAT steps in two seconds. A file read in the tick
it was written in, then written again the same size in that tick, keeps its
size and time, and the second write looks like none. The laptop passed only
because its kernel (7.2, with fine-grained timestamps) never gave the two
writes one time.

It matters beyond a test: a program that rewrites a file twice in quick
succession -- a save that writes, then writes again -- could leave a version on
one device that the others never get, with nothing to say so.

## Decision

**A file whose time is within three seconds of the moment it is read is
marked, and its size and time are not trusted while it is: each scan reads it
again, until a read finds it older than that.** As git treats an index entry
as new as the index itself ("racily clean").

- The mark is a row in a new table, `racy (path)`, index schema 26. It is set
  or cleared by `Store::put_file` every time it reads a file, from the file's
  time and the clock at that moment.
- A scan reads the marked paths once and does not trust a size-and-time match
  for them. A re-read that finds the same bytes stores nothing and makes no
  version (`put_manifest` already knows identical content).
- The same for files kept where they lie -- WhatsApp's, and a phone's folders
  chosen on a computer ([0062](0062-backing-up-whatsapp.md),
  [0063](0063-backing-up-a-phones-folders-chosen-on-the-desktop.md)) -- which are
  told apart by the same two numbers.
- A time more than three seconds in the future is not marked: nothing could be
  done for it, and it would be read on every scan forever.

## Why this way

- **It costs almost nothing.** Only files read within three seconds of being
  written are read again, and only until they are three seconds old. A
  library's files are not; a file just saved is, once or twice.
- **It survives a restart.** The marks are in the index, so a device that stops
  between the two writes still reads the file again when it starts. A set kept
  in memory would not.
- **`updated_at` is left alone.** It orders what changed recently in the
  windows' lists; moving it to say "checked" would reorder them.

## Costs, accepted

- **A schema change.** An index of schema 26 is refused by an older build,
  which says so (0047), and the copy made before migrating is the way back.
- **Three seconds is a guess at the coarsest clock**: FAT's two, and a margin.
  A filesystem coarser than that would still lose an edit in its tick.

## Tested

- `two_devices::an_edit_in_the_same_tick_of_the_clock_is_still_seen` gives both
  writes one time on purpose. It fails without this decision, exactly as
  Android 8 did, and passes with it.
- `outside::a_file_written_again_in_the_same_tick_is_read_again` and
  `outside::a_file_older_than_any_tick_is_not_read_again`, for files kept
  where they lie.
