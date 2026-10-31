# 0076 — A computer backs up its own folders as a phone does, and can hold back its speed

**Status:** Accepted — built on 2026-10-31 and installed on the owner's
laptop the same day. Run in the real window by `scripts/desktop-smoke.sh`
(a folder backed up and kept by a second device); the speed limit measured
on a loopback connection. Not yet run between two of the owner's own
computers: he has one.
**Date:** 2026-10-31

## What was asked

For the redesigned desktop window, on 2026-10-28, among the features the
owner wanted: "the computer backing up its own folders to another computer"
and a bandwidth limit. On 2026-10-31: "build all the still not built".

## Decision

1. **A computer's own folders go into its own vault, read where they lie,**
   under `.qurb-outside/folders/<name>/` — exactly what a phone's chosen
   folders do under `.qurb-outside/storage/` (decision 0063), with the same
   store call (`keep_outside_folder`), by reference, nothing copied
   (decision 0062). The folders are a setting, `backup-folders`, paths
   separated by `;`; each is kept under its own name, made unique by a
   number. A folder inside the synced one is refused: it is synced already.
2. **Kept by the devices that keep this computer's vault** — `qurb holders`,
   chosen in the window as *Kept on*, one switch per of the person's other
   computers. No new kind of keeping: a computer keeping another's vault is
   the bookkeeping a computer keeping a phone's already does.
3. **A file deleted from a backed-up folder stays** with the computers
   keeping it while one of them holds it, as a phone's do; a folder taken
   off the list stops being read, and what is kept of it stays kept.
4. **A computer offers its outside folders on any network.** A phone offers
   them only on Wi-Fi (`set_outside_offered`, decision 0062); a computer is
   taken to be on a network that does not count what it carries, and the
   speed limit below is the way to hold it back.
5. **The daemon reads the folders at most every five minutes, for at most
   twenty seconds** a pass, sooner again while files are left: a new folder
   of thousands of files is read over several passes rather than holding up
   syncing.
6. **A speed limit, one rate each way, for the whole process**
   (`qurb_peer::limit`): chunk bytes served are *up*, chunk bytes fetched
   and uploads received are *down*. A token bucket allowed one second's
   burst, that may go into debt so a chunk larger than a second's allowance
   still goes. Settings `upload-limit` and `download-limit`, bytes a second;
   the window applies a change at once, since its daemon runs in the same
   process, and the daemon re-reads them each pass.
7. On the computer keeping them, the backed-up folders appear in the owner's
   folder in Files beside the phones' backups, opened with the same yes on
   the owner's phone (decision 0067).

## Measured

- **The limit holds:** 4 MB fetched over a QUIC connection on this laptop
  (loopback, `crates/peer/tests/limit.rs`): 0.23 s with no limit, 3.05 s at
  1 MB/s download — the first second's worth at once, the rest at the rate.
  Run 2026-10-31.
- **A computer's folder reaches another and stays:**
  `crates/peer/tests/holding.rs`,
  `a_computers_own_folder_is_kept_by_the_computer_it_names`: two files held
  over a real connection, none stored on the computer that backed them up,
  a deleted one still kept.
- **In the real window** (`scripts/desktop-smoke.sh`, release build, Broadway
  display, 2026-10-31): a folder chosen, the paired device switched on as
  its keeper, both files read into the vault on the daemon's next pass.

## Left undone

- **Not run between two real computers.** The owner has one; the second
  device in the smoke test is the command line on the same machine, which
  never collects.
- **How much of a folder has reached the keeper is not shown** — only what
  this computer has read of it. Counting each file's copies elsewhere per
  look was judged too slow for a large folder; a count kept by the index
  would do it.
- **The limit does not tell kinds of traffic apart.** A video streamed from
  a person's folder on another device is held to it too.
- **No limit on the phone's side** of a transfer: the Android app does not
  set one.
