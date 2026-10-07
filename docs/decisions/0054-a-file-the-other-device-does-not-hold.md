# 0054 — A file the other device does not hold is listed, not failed every sync

**Status:** Accepted — built and tested; watched on the S23 on 2026-10-07;
extended by [0055](0055-a-file-on-no-device-says-so.md), which shows such a
file as on no device
**Date:** 2026-10-07

## What happened

On 2026-09-29 the laptop freed 18 shared files because the phone kept them.
On 2026-10-05 the phone's data was cleared, and the phone's copies went with
it ([phase 5](../phases/phase-5-mobile.md#through-the-app-with-bbr--and-a-phone-cleared)).
The laptop still lists the 18 files, as not here, held by the phone's
identity of the time, which no longer exists.

The phone, set up again, saw them in the laptop's tree and asked for them.
The laptop answered with each file's chunk list, which it still had, and
then had none of the chunks. Every sync, every fifteen minutes, the phone
recorded 18 failures ("fetching content failed: peer does not have
content …"), and its Home showed little else under *Recent*.

Two faults made that happen:

1. **A manifest meant "known", not "held".** The server answered from the
   chunk list a freed file keeps. A device that frees a file still knows
   what it was made of; it no longer has the bytes.
2. **"Not held" was a failure like any other.** The engine had no way to
   tell "the other device does not have it" from "the connection dropped",
   so it tried again on every sync, and recorded each attempt.

## Decision

**A manifest is an answer to "do you hold this".** The server answers only
when every chunk is in its chunk store or in a live file in its folder, by
the index alone (`Store::held_chunks`). Otherwise it answers *not found*,
before any bytes move. It does not read the folder's file to be sure, which
would mean reading a whole video to answer one request. A file changed under
the index still fails at the chunk, as before, and is retried.

**"Not held" reaches the engine as such.** The client reports it as
`Error::NotHeld`, and `NetworkSource` passes it on as the engine's
`ContentUnavailable`. Every other failure stays a failure to retry.

**A shared file not here, that nobody asked for, is then recorded as it
is**: listed, and elsewhere (`Store::know_elsewhere`), the state freeing a
file leaves. Its leftover `.incoming` file is removed, and its earlier
failures are dropped from the history, since they no longer say anything
true. Opening it later asks for it, as with any freed file.

**A file that is here, or that somebody asked for, still fails.** A file
on disk keeps its version rather than being replaced by a listing. A file
somebody asked to have back should be reported as not having come.

**A failure that repeats is recorded once.** A path whose last history entry
is the same failure is not given another. It is still counted as a failure
of that sync. This covers every failure that repeats until something
changes, not only this one: a name taken by a file sent here, a file no
device it meets has.

## Why this, and not something else

- **Retrying quietly, without listing the file.** It would have stopped the
  noise, but kept a manifest round trip per such file on every sync, and the
  phone would not show that the files exist.
- **A fourth availability, "on no device".** That is the honest way to show
  these 18 files, and it was not built here. Built the same day, after the
  owner removed the old phone identity:
  [0055](0055-a-file-on-no-device-says-so.md).
- **Marking such files wanted**, so that they arrive by themselves when a
  device that has them is met. It would turn the same files back into a
  failure on every sync with every device that does not have them.

## What it costs

- **A file listed as elsewhere is not fetched by itself later.** If the
  device asked first does not hold it and another device does, this device
  lists it and does not download it until it is opened or asked for. Before,
  it failed with the first device and arrived from the second. Phones and
  folders kept only remotely already work this way. On a desktop meant to
  hold everything it is a change, with three or more devices only.
- **Both ends must be updated.** An older server still answers with the chunk
  list of a file it freed, and the client then fails at the first chunk, as
  before.
- **History says a repeated failure once**, so how many syncs it failed is
  in the log, not on screen.

## Checked, and not

- `crates/peer/tests/delivery.rs`:
  `a_file_the_peer_does_not_hold_is_listed_not_failed_every_sync` (fails
  without the engine change), and
  `a_file_asked_for_that_the_peer_does_not_hold_still_fails`, which also
  checks the failure is recorded once over two syncs.
- **On the S23, 2026-10-07.** Before the change, the phone's history held
  216 failures for the 18 files by 18:57, and 18 more at each sync after
  that. With both devices updated, the phone's 19:44 sync recorded none. It
  listed the 18 files as elsewhere and removed their 18 leftover `.incoming`
  files, and none other. Its history now holds no failures. Its three files
  that are here were untouched.
- **Not watched**: a file asked for that no device has, on the phone; and the
  three-device case under *What it costs*, which only the reasoning covers.

## Reversing it

The server's manifest goes back to `chunk_hashes_for_content`, and the
engine's `ContentUnavailable` arm in `take` is removed. Recording a failure
once is one condition in `apply_plan_reporting`.
