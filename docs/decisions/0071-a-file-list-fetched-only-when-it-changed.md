# 0071 — A device's file list is fetched only when it changed, and the relay is tried alongside a slow direct path

**Status:** Accepted, 2026-10-19. Built in `qurb_peer` (`trees`, `wire`,
`connect`), used by the phone and the laptop's daemon. Measured on the S23 on
another phone's hotspot. **It does not finish the job**: a pass after the other
side changed anything still fetches the whole list, which on that hotspot is
more than a pass can carry -- see *What is left*. Amended 2026-10-20 by
[0072](0072-only-what-changed-in-a-list.md), which sends only the changes
and fingerprints a list in a different order (so a list kept by this build is
fetched again once); the fingerprint is still asked of a device that sends its
list only whole.
**Date:** 2026-10-19

## What was found

Testing the S23 on another phone's hotspot -- the owner's report of
2026-10-15, "unable to sync files when my phone was connected to my another
phone's wifi" -- the phone connected to the laptop, directly or through the
relay, and every pass still ended "Ran out of time before reaching a device".

Two things spent the pass's 25 seconds (the background worker's are 20):

- **The laptop's file list, every pass, whole.** A sync begins with the other
  device's list. The laptop keeps the S23's WhatsApp backup and backed-up
  folders, 37,959 files of its vault, and lists every one to it: a pass with
  nothing changed received 9.8 MB. On mobile data that is fast enough and
  costs 9.8 MB a pass; on the hotspot it did not arrive within the pass, so
  every pass started again from nothing.
- **Eleven seconds before the relay was asked.** The relay was tried only once
  every direct address had failed, each given eight seconds; on the hotspot
  nothing direct answered.

## Decision

1. **A list's fingerprint first.** `Request::TreeDigest` asks for the hash of
   the list `TreeStream` would send now (`wire::tree_digest`: the blake3 of
   its frames; the list is in path order, so the same list hashes the same).
   The asker keeps each device's last list on disk with its fingerprint
   (`trees::Trees`, `.qurb/trees/`), and fetches the list only when the
   fingerprint differs. A kept list is used only if its own fingerprint
   matches, so a damaged one is fetched again. A device too old to answer
   fails that one request -- the connection goes on -- and is asked for the
   list as before.
2. **The relay alongside, after a head start.** The announced addresses are
   raced as before; if none has answered within four seconds
   (`RELAY_HEAD_START`), the relay is tried as well, and whichever connects
   first is kept. A direct path that answers at all answers well inside that
   -- on the same network in milliseconds, punched through a router in a
   round trip or two -- so it is still the one kept wherever it works.

## Measured

On the S23 behind another phone's hotspot sharing its mobile data (Jio), the
laptop at home, 2026-10-19:

| | before | after |
|---|---|---|
| received, a pass with nothing changed | 9.8 MB, and ran out of time | 24–27 KiB, and finished |
| from start to connected through the relay | about 11 s | about 4.5 s |

## What is left

A pass after the laptop changed anything still fetches the whole list, 9.8 MB,
and on that hotspot still runs out of time: the suite there failed for that
reason after this was built. Two ways on; the first was built on 2026-10-20
as [0072](0072-only-what-changed-in-a-list.md), and has not yet been run on
that hotspot:

- **The list's changes only**: rows changed since the list the asker holds,
  which needs a change counter every write to the list moves, deletions
  included, or a kept list goes stale -- a correctness question, not a speed
  one, and to be decided before it is built.
- **Not listing a phone's own backups back to it**: the phone is their source,
  and learns what the computer keeps another way (`confirm_kept`). Most of the
  9.8 MB is these.

## Costs, accepted

- One more request each pass, 32 bytes back.
- A list kept on disk per device: about 10 MB for the laptop's, on the phone.
- A relay connection may win where a direct one would have answered after
  four seconds, and a relayed session is not moved back to direct (a gap since
  Phase 3).
