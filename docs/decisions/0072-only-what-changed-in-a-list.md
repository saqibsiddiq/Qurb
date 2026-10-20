# 0072 — Only what changed in a device's file list is sent

**Status:** Accepted, 2026-10-20. Built in `qurb_storage` (index schema 27)
and `qurb_peer` (`wire`, `server`, `trees`). The laptop has run it since
2026-10-20. Measured on a copy of the laptop's index, and **run on the S23
over the home Wi-Fi the same evening**: after a change on the laptop the
phone received the two changed entries, not its 38,032. **Not run on a
hotspot or on mobile data**: the owner no longer has either to test with. See
*On the S23* and *Not yet tested*.
**Date:** 2026-10-20

## What was asked

The owner, 2026-10-20:

> wireless debugging is on, i dont have access to hotspot and mobile data
> anymore, do the recommended fix but ive no way to test it

The recommended fix was the first of the two ways on in
[0071](0071-a-file-list-fetched-only-when-it-changed.md)'s *What is left*:
send only the rows that changed since the list the asker holds. 0071 attached
a condition: a counter that missed a change would leave a kept list stale,
which is a question of correctness and not of speed. This decision answers
that condition by never trusting the counter alone.

## Decision

1. **A list counter in the index** (schema 27). It is a single row,
   `list_counter`, plus `listed (file_id, changed)`, which records the
   counter's value at each row's last change. Triggers on `files` keep both:
   - on an insert;
   - on a deletion;
   - on an update to any column a list entry is made of: `path`, `size`,
     `content_hash`, `deleted_at`, `vector`, `modified_by` and `updated_at`;
   - on an update to `scope` and `held`, which decide where an entry is
     listed.

   Triggers, rather than calls in the store, because every write to `files`
   then moves the counter, including writes added later by code that has
   never heard of it.
2. **`Request::TreeSince { since }`.** The answer is `Response::TreeDelta {
   now, digest, changed }`. It carries:
   - `now`, the counter as it stands;
   - `digest`, the fingerprint of the asker's whole list as it is now;
   - `changed`, the asker's entries whose rows changed after `since`.

   All three are read in one transaction (`Db::at_once`), so they describe
   the same moment. When more than 20,000 entries changed, or more than half
   the list (past 64 entries), the answer is `NotFound`, and the list is sent
   whole: at that size, sending only the changes saves little.
3. **The asker rebuilds and checks.** It puts the changed entries in place of
   its kept ones, keyed by where each is listed and its path. It uses the
   result **only if the result's fingerprint is the `digest` it was sent**;
   otherwise it fetches the whole list. A missed change, a counter that went
   backwards (an index restored from its copy), or an entry that left the
   list therefore costs one whole fetch and never produces a wrong list.
4. **The fingerprint is taken in one order**, by where an entry is listed
   (shared, then sent or held, then held for the other device) and then by
   path (`wire::listed_order`). This makes a list rebuilt from its changes
   hash the same as one sent whole, which in 0071 was the order the server
   happened to send. `TreeDigest` answers with the same fingerprint.
5. **Kept lists carry their counter.** They are stored as
   `.qurb/trees/list-<fingerprint>.bin`: the fingerprint, the counter, then
   the list. A list kept by 0071's build, `tree-<fingerprint>.bin`, is not
   read; it is fetched again once and then removed.
6. **Where changes are not sent, the fingerprint still decides.** A computer
   that keeps another person's vault sealed answers `TreeSince` with
   `NotFound`, because part of the list it sends that person's device is put
   together from the sealed vault, which moves no counter. It still answers
   `TreeDigest`, and a kept list whose fingerprint matches is used without a
   fetch, as in 0071. The first version of this decision left that out, so
   such a device fetched its whole list on every pass. The test
   `a_list_sent_only_whole_is_not_sent_again_unchanged` fails without the
   fix.
7. **A file read again and found unchanged no longer moves `updated_at`**.
   It records only its time on disk. Before, every such read (since
   [0069](0069-too-recent-to-trust.md), every file read within three seconds
   of being written) gave the file a new `updated_at`. That moved the
   counter for a file that had not changed, and gave peers a new
   modification time for unchanged content. Found by the integration test,
   which failed now and then: 300 files written moments apart were re-read on
   the next scan, and all 300 were sent as changed.

## Why this way

- **The counter is only a hint.** The digest decides whether a kept list is
  used, so the counter may miss a change at no cost to correctness. It makes
  sending only the changes cheap, and nothing more. 0071's condition is met
  by never relying on the counter, rather than by proving it can never miss.
- **A change can be sent, a removal cannot.** An entry that left the asker's
  list is a changed row the server no longer shows it, and the server cannot
  say "this left your list" without knowing that the asker had seen it.
  Saying it for rows it had not seen would tell the asker the paths of other
  people's vaults. So a removal shows as a fingerprint mismatch, and the list
  is fetched whole.
- **One counter for the whole index**, not one per device: every audience's
  changes are a subset of all changes, and the filtering is done against the
  audience's own list, which the server reads anyway to fingerprint it.

## Measured

On a copy of the laptop's index, 2026-10-20, laptop and conditions as in
[experiments/list-sizes](../../experiments/list-sizes/README.md). The list
the laptop sends the S23 has 38,006 entries:

| change since the phone's kept list | bytes sent before (0071) | bytes sent now |
|---|---:|---:|
| nothing | 32 (the fingerprint) | 45 |
| one shared file edited, or one deleted | 7,941,674 | 166–194 |
| ten of the phone's own files | 7,941,674 | 2,215 |
| a thousand of them | 7,941,674 | 208,125 |

In every case the list rebuilt from the changes had the whole list's
fingerprint. Answering took the laptop 51–97 ms, about what 0071's
`TreeDigest` took, since both read and fingerprint the whole list.

The S23's pass after a change received 9.8 MB on 2026-10-19 (0071). The
7.9 MB measured here is the list message alone, without QUIC's framing and
encryption or anything else the phone received in that pass, and the list has
changed since. The two numbers are not the same measurement.

The suite: 908 tests, none failing, `cargo clippy` clean. New tests:

- `peer/tests/trees.rs` `only_what_changed_in_a_list_is_sent`. A computer
  and a phone are each a real engine, over QUIC on loopback. The list is sent
  once and then only as changes, through an edit, an addition and a
  deletion. It is sent whole when 250 of 300 entries changed, and when the
  kept copy is damaged.
- `peer/tests/trees.rs` `a_list_sent_only_whole_is_not_sent_again_unchanged`.
- `storage/tests/listing.rs`, on the counter: each kind of change moves it,
  and a re-read that finds nothing new does not.
- the unit test `trees::a_list_rebuilt_from_its_changes_is_the_list`.
- the wire round trip, and a hostile peer whose changes do not add up to the
  fingerprint they come with.

## On the S23

2026-10-20, 19:27–19:40, the S23 on the home Wi-Fi beside the laptop, over
wireless debugging. The owner was using the phone at the time, WhatsApp among
other things, so the phone's Wi-Fi counter (`/proc/net/dev`, wlan0) counts
their traffic as well as qurb's.

| | the phone's build | received on Wi-Fi meanwhile | what the app said |
|---|---|---|---|
| two passes, nothing changed | 2026-10-19's (0071), the laptop on this | 8,156 and 8,187 KiB | 8.6 s and 7.9 s; the whole list each time, since the two builds fingerprint differently |
| a file written on the laptop: the first pass after the update | this | 10,487 KiB | the phone woken by push, the file there 12.3 s after it was written; the whole list, once, as expected |
| another file written on the laptop | this | 2,200 KiB | the file there 7.2 s after it was written; `its file list brought up to date changed=2 files=38032` |

The second change is what this decision is for. The phone's log says two
entries were sent. The 2.2 MB counted meanwhile is the phone's whole Wi-Fi,
not qurb's alone, and has not been broken down. It is not the two files the
phone refuses on every pass (`a file sent to this device already has that
name`, as the message then read, left over from the conflict test of
2026-10-19): those are refused before anything is fetched. See the phase-5
document's section for 2026-10-20.

The laptop, for its part, fetched the phone's whole list once after the
phone's update (19:29:08), and its syncs every 20 s after that did not write
the kept list again: nothing had changed.

The phone's index migrated to schema 27 on the first start of the new build
(`index.before-schema-27.db`, 44 MB). That start was not timed.

## Not yet tested

- **On a hotspot or on mobile data.** What 0071 measured failing (a pass
  after a change ran out of time on another phone's hotspot) has not been run
  again, and the owner no longer has either network to run it on. The
  measurement above says the bytes are gone; whether that makes the pass fit
  is not shown.
- **The phone rebuilding the list, timed apart.** It reads its kept 8 MB list
  and checks its fingerprint every pass, as in 0071, and now also writes it
  again after each change. On the S23 everything from "connected" to
  "brought up to date" in its log took 0.54 s: reading the kept list, asking,
  rebuilding, checking and writing. That is one pass, and its parts were not
  timed separately.

## What still sends the whole list

Each of these is correct, because the fingerprint catches it, and each costs
one whole list:

- the first sync with a device, and the first after this build arrives on
  either side;
- a device too old to answer `TreeSince`;
- more than 20,000 changes, or more than half of the list;
- **an entry leaving the list without its row changing**:
  - a folder shared without the asker (the sharing rules are files, not
    rows);
  - the asker starting or stopping to be one of the devices that keep this
    one's vault;
  - a phone moving between Wi-Fi and mobile data: the folders it keeps
    outside its vault, WhatsApp's among them, are in the list it sends the
    computers keeping it only on a network that does not count what it
    carries (0062). Each move costs the computer one whole list from the
    phone;
  - a row removed for good;
  - an entry moving between the shared area and a vault. A move between
    sent and held is an ordinary change;
- a computer keeping another person's vault sealed (then only when its
  fingerprint has changed).

## Costs, accepted

- **A schema change.** An index at schema 27 is refused by an older build,
  which says so (0047). The copy made before migrating
  (`index.before-schema-27.db`) is the way back. The first sync after it
  sends each device its whole list once, since no list kept before has a
  counter.
- **A write to `files` does a little more**: one update to the counter and one
  row replaced in `listed`, inside the same statement.
- **Builds from 0071 and from this one fingerprint lists differently**, so
  across that pair the list is fetched whole on every pass. Only the S23's
  build of 2026-10-19 and the laptop's of the same day had 0071; 0.1.0 has
  neither.
- **A trigger that replaces a row must delete it and then insert it.** SQLite
  lets the conflict rule of the statement that fires a trigger override the
  trigger's own `OR REPLACE`. Written that way, the triggers failed with
  `UNIQUE constraint failed: listed.file_id` in the store's concurrency
  tests, so they delete and then insert. Recorded here because the obvious
  way passes until it does not.
