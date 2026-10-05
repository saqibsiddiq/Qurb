# 0050 — Large files from a phone: fetched in parallel, resumed, and served in the foreground

**Status:** Accepted — built, and measured on the phone on 2026-10-05, where
fetching in parallel gained little (see *Checked, and not*); amends
[0020](0020-sync-takes-a-deadline.md) (a pass may go on answering past its
window, when its caller can) and the background worker's rule of no
foreground service (a long pass, and only a long pass, runs in the
foreground)
**Date:** 2026-10-04

## What happened

The owner added an 800 MB video on the Galaxy S23 and the laptop never got it:
"very slow, and also it failed". The laptop's log has the rest. The phone,
re-paired at 17:58 UTC on 2026-10-03, was reached over the home Wi-Fi; the
laptop began pulling the video and at 18:01:18 the fetch ended with
`reading the response failed: read error: connection lost`, two minutes and
thirty-two seconds in. Nothing fetched it again.

Three causes, each enough on its own:

1. **Chunks were fetched one at a time.** `fetch_content_into` asked for a
   chunk, waited for all of it, checked and wrote it, then asked for the next.
   A file moved at the pace of the round trip and the phone's reading, not of
   the link: under 5.3 MB/s, since 800 MB had not arrived in 152 seconds.
2. **An interrupted fetch started again from nothing.** The partial file was
   opened fresh on every attempt, so the 2½ minutes' worth already on the
   laptop counted for nothing next time.
3. **The phone stopped answering.** Its pass waits at most ten seconds for a
   device to collect and never past its window — 25 seconds in the app, 20 in
   the background. A connection already open is served past that while the
   process runs, which is how this one lasted 2½ minutes; but Android freezes
   an app nobody is looking at, and then the connection is lost.

## Decision

**Every device fetches up to eight chunks at once**, written and checked in
order (`IN_FLIGHT` in `crates/peer/src/client.rs`). The cost is memory: eight
chunks, 4 MiB at the average size and 16 MiB at the largest.

**A fetch that was cut off carries on.** The partial file beside the
destination (`.name.incoming`, which the scan ignores, or
`holding-<hash>.incoming` for a vault kept for another device) is kept when a
fetch fails. The next attempt chunks it — boundaries are chosen by content, so
a prefix chunks the same as the file's start — keeps the run of chunks that
match the sender's manifest in order, cuts off the rest, and fetches only what
follows. Nothing unchecked is kept: a partial file of another version of the
same path keeps only the chunks they share. `PeerClient::resume_point`,
used through `ContentSource::resume_into`, for the shared folder, a held vault
and a delivery into Downloads alike.

**A phone keeps answering while a device collects from it** — when its
caller can let it. `sync_serving(window, up_to)` is `sync_within(window)`
whose pass, once its own syncing is done, stays open while a chunk has gone in
the last ten seconds, or in the last minute while what was being collected is
still waiting (added 2026-10-05, below), up to `up_to` from the start.
`sync_within` is unchanged: the window is still the limit.

**A pass with a large collection runs in the foreground.** When 32 MiB or
more is waiting to be collected, the background worker declares itself a
foreground service of type `dataSync`, shows a notification with the bytes
sent so far, and serves for up to thirty minutes; *Sync now*, and every
in-app sync, hands such a pass to the worker rather than running it in the
activity. The notification goes when the pass does. Permission to show it is
asked for the first time a long transfer starts, not at install; refused, the
transfer runs anyway and the notification shows only in the system's list of
running apps.

**A pass asked for while one runs waits for it** (added 2026-10-05). *Sync
now* enqueues behind a running pass instead of replacing it, which cancelled a
long one part-way. The app watches the worker's passes while it is on screen,
shows them as syncing, and runs its own syncs after them.

## Why this, and not something else

**Push rather than pull** for large files would not help: whoever sends, the
phone has to stay up for the whole transfer.

**A permanent foreground service** would keep the phone answering for good,
at the price [the worker's own comment](../../android/app/src/main/java/com/qurb/SyncWorker.kt)
already declined: a notification forever. A temporary one is the arrangement
Android offers for exactly this — something the person asked for, in
progress, that must not be frozen.

**Resuming by keeping fetched chunks in the chunk store** would have worked
without reading the partial file again, but would cost a second, encrypted
copy of the file's bytes on a device whose folder is its payload store
([0024](0024-the-file-is-the-payload-store.md)) until a collection pass
removed it. Re-chunking the partial file costs one read of what is already
there.

## What it costs

- **A notification** while a long transfer runs, and one more permission
  asked for.
- **A partial file left beside its destination** when a transfer stops, until
  it completes. One whose file is deleted before it completes stays until
  somebody removes it; nothing cleans them up yet.
- **Android's limits.** A `dataSync` service runs at most six hours a day on
  Android 15 and later, which thirty minutes a pass leaves room for. Starting
  one is refused from some background states; then the ordinary pass runs, and
  resuming carries the file across as many of them as it takes.

## Checked, and not

- Tests: `a_file_cut_off_part_way_carries_on_where_it_stopped` (12 MiB, cut
  two-thirds through mid-chunk: the file arrives intact and only the rest
  crosses — it fails with resuming disabled, when all 12,582,912 bytes cross),
  `a_partial_file_that_is_not_the_content_is_not_kept`, and the existing
  network tests, which now fetch with eight in flight. The decision to keep
  answering is a function tested on its own (`keep_answering`).
- **Measured on hardware, 2026-10-05** (Galaxy S23 serving, the laptop
  collecting, home Wi-Fi at 5 GHz, links of 468 and 351 Mbit/s; details in
  [phase 5](../phases/phase-5-mobile.md#measured-on-the-s23-2026-10-05)):
  - **Eight in flight bought little**: 5.33 MB/s, against 4.77 and 3.81 MB/s
    one at a time. That contradicts the first cause above as written. The
    waits added something, but they were not what held a transfer to 5 MB/s
    on a link carrying several times that, and what does is not yet known.
    The laptop now logs each large fetch's rate with the connection's round
    trip, congestion window and losses. On the run that finished both files
    (765 MB at 4.95 MB/s) the round trip was 13 ms with nothing lost, which
    leaves the requests room for hundreds of megabytes a second. So the limit
    is in the phone's sending, its serving of each chunk or its sender's
    congestion window. It was the congestion window: a spike on the phone
    served at 63–73 MB/s over its own loopback and sent 5.2–5.3 MB/s over
    Wi-Fi with quinn's Cubic and 12.5–14.0 with BBR. See
    [0051](0051-bbr-not-cubic.md).
  - **The foreground pass held**: the phone was swiped to its home screen
    two minutes into a transfer, Samsung's freezer gave up on the process,
    and serving went on.
  - **Starting it from the background was refused**, once, after an install;
    that pass ran as an ordinary one, as *What it costs* expects.
  - **Resuming worked twice** on a 1.7 GB file, keeping 114,864,514 and then
    942,721,024 bytes.
  - **A ten-second pause ends the pass.** The laptop was restarted
    mid-transfer, which with re-reading its partial file kept it from asking
    for twelve seconds. The phone, having served nothing for `COLLECTING`,
    closed the pass at 202 seconds of its thirty minutes and was frozen ten
    seconds later. A collector that pauses longer than `COLLECTING` lost the
    phone, and the transfer waited for the next pass. Changed the same day:
    while what was being collected is still waiting, the pass waits up to a
    minute (`PAUSED`). Tested as a function; not yet watched on the phone.
  - **Both files arrived** at the next *Sync now*, carried on from where they
    stopped and checked whole.
  - **The app did not know about the pass.** Having handed it to the worker,
    it said *Everything is synced* throughout, and a second *Sync now* would
    have replaced, and so cancelled, the pass. Fixed by the rule above, and
    watched on the S23: Home read *Syncing…* through a worker's pass and
    redrew itself when it ended. A second sync asked for during one was not
    tried.

## Reversing it

Each part stands alone. `IN_FLIGHT = 1` restores fetching one at a time;
`resume_into`'s default starts again; `LONG_PASS_BYTES` set out of reach
keeps every pass ordinary.
