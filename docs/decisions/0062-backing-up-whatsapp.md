# 0062 — Backing up WhatsApp to a computer, not to the Google account

**Status:** Accepted, 2026-10-12, with the owner's answers at the end, which
moved where the backup is kept from an ordinary folder to the phone's Private
Vault. Measured with
[experiments/whatsapp-access](../../experiments/whatsapp-access/README.md) on
the emulator and the owner's S23. **Restoring is unproven**, and is said so
everywhere this touches.
**Date:** 2026-10-11

## What was asked

The owner, on 2026-10-11: "if we can somehow use qurb to backup whatsapp? it
uses the storage of the email id with a hard cap of 15gb on free tier". Then
"start"; and, with no spare number to test restoring with: build the backup
side, and keep restoring marked unproven.

WhatsApp on Android backs up to the person's Google account, and since
December 2023 those backups count against the account's storage, 15 GB on
the free tier and shared with Gmail and Photos
([9to5Google](https://9to5google.com/2023/11/14/whatsapp-backup-google-account-storage/)).

## What is known

Measured on 2026-10-11 (the experiment's README has the conditions):

- **WhatsApp keeps a backup on the phone as well**, in
  `Android/media/com.whatsapp/WhatsApp`: chat backups in `Databases`, other
  settings in `Backups`, and every photo, video, document and voice note in
  `Media`. On the S23: 37,722 files, 9.65 GB.
- **It backs up chats incrementally.** A full `msgstore.db.crypt14` of 498 MB
  about weekly, and an increment of 58 to 154 KB each night.
- **An app can be given that folder** with one tap in Android's own picker,
  which opens right at it, and keeps the grant: on the emulator (Android 14)
  and the S23 (Android 16, One UI 8.5). No storage permission is needed.
- **It can read it** (the S23: every file listed, a backup read through) and
  **write into it** (the emulator; not tried on the S23). On a phone WhatsApp
  has not run on yet, it can make the folder.
- **Listing the whole folder takes 54 s** on the S23, 209 folder queries,
  most of it the 27,823 images in one folder.
- **The owner's chat backups are `.crypt14`**: WhatsApp's *end-to-end
  encrypted backup* is off.

**Not known: whether WhatsApp restores from a backup put back this way.** The
chat backup is encrypted. Reports differ on whether a new phone can open a
`.crypt14` backup copied into place: some say the key comes from WhatsApp's
servers once the number is verified, others that it stays on the old phone.
Testing it needs WhatsApp registered on the emulator (its official APK runs
there) with a number that is nobody's active WhatsApp, since registering a
number logs it out elsewhere. There is none yet. Photos, videos and
documents are ordinary files, and come back whatever the answer.

## Proposal

1. **Setting up, on the phone.** Settings → *Back up WhatsApp* says what it
   does and what it does not yet (restoring), then opens Android's picker at
   WhatsApp's folder: *Use this folder*, *Allow*. It goes to the computer
   that keeps this phone's Private Vault, unless another is chosen.
2. **What goes.** The chat backups (`Databases`, `Backups`) always; the
   media too unless the owner chooses chats only (question 3); never
   WhatsApp's caches, the folders whose names start with a dot (`.Statuses`,
   `.Links`, `.Shared`, `.Thumbs`, …).
3. **When.** Once a day, while charging and on Wi-Fi, through the background
   worker, after WhatsApp's own backup at night; and *Back up now*. A day
   costs about 100 KB of chats and that day's media. The first run carries
   everything, 9.65 GB on the S23, and is meant for the home network.
4. **How it travels.** As a send does since 0060 step 1: read in place
   through the app's document opener, nothing copied on the phone, each file
   checked against its hashes when the computer collects it, and one changed
   in between going the next day instead. **As one backup, not 37,722
   sends**: one line in Transfers (*WhatsApp backup: 1,204 of 37,722 files*)
   and one entry in the history for each run.
5. **Where it lands on the computer** (question 1). Proposed: an ordinary
   folder beside where sends land ([0037](0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md)),
   `Downloads/qurb/WhatsApp from SM-S911B`, each file replaced in place when
   it changes rather than arriving as a second copy. The photos open on the
   laptop like any others, and nothing needs qurb to read them.
6. **What is kept** (question 2). The chat backups exactly as the phone has
   them now, since a restore needs one consistent set -- the full backup and
   its increments, nothing stale beside them. Media kept even after it is
   deleted on the phone, which also lets WhatsApp's media be cleared off the
   phone to free space while the computer keeps it.
7. **Restoring -- unproven.** On a new phone, once WhatsApp is installed and
   before its restore step, *Restore WhatsApp* in qurb writes the folder back
   into `Android/media/com.whatsapp/WhatsApp`, and WhatsApp is expected to
   offer the local backup. Built once the question above has an answer.
8. **The Google backup** (question 4). Proposed: keep it on for chats until a
   restore has been watched working; until then it is the only proven way
   back. Turning *Include videos* off in WhatsApp's backup frees 2.44 GB of
   the 15 at once on the S23.

## Why not something else

- **Copying the folder into qurb on the phone**: 9.65 GB twice on the phone.
  [0060](0060-a-computer-keeps-private-folders-for-several-people.md) already
  decided qurb keeps no copy of what it carries.
- **Putting it in the phone's Private Vault**, indexed where it lies. The
  vault deletes what the phone deletes (to Recently deleted, then gone),
  which is wrong for an archive of media, and the computer keeps a vault
  where nobody using it sees the files, so a photo would need qurb to get
  back. Possible later if the owner prefers it (question 1).
- **Watching the folder every sync**: a full listing is 54 s, and a phone
  syncs in short windows.
- **WhatsApp's key, or root**: no. qurb carries WhatsApp's own backup files
  as they are and never opens them.

## Questions for the owner

1. **Where on the laptop?** An ordinary folder you can browse (proposed), or
   hidden inside the phone's Private Vault, restorable only through qurb?
2. **Media deleted from the phone:** kept on the laptop (proposed), or
   deleted there too?
3. **What to back up:** chats and media, about 9.6 GB (proposed), or chats
   only, about 0.5 GB?
4. **The Google backup:** kept on until a qurb restore is proven (proposed),
   or off now, accepting that restoring is unproven?

## Order of work, each piece shippable

1. **The engine**: a backup as a standing send of one folder to one device,
   kept by path, replaced in place, carried as one; the computer writing it
   into its folder.
2. **The phone**: setting up, the nightly run, *Back up now*, its progress.
3. **The desktop**: the backup shown under the phone in Devices -- last run,
   size, *Open folder*.
4. **Restoring**: putting it back on a phone, and watching WhatsApp restore
   from it, with a spare number.

A few days for 1 to 3: a guess, to be corrected in the phase document.

## The owner's answers, 2026-10-12

1. **Hidden in the phone's Private Vault on the laptop**, not an ordinary
   folder: kept out of sight, and brought back only through qurb.
2. **Media deleted on the phone is kept on the laptop.**
3. **Chats and media.**
4. **WhatsApp's Google backup stays on** until a restore from qurb's copy has
   been watched working.

## The design, as answered

Answer 1 replaces points 5 and 6 above, and undoes the first reason under
*Why not something else*, whose objection answer 2 settles.

- **The phone's vault, read where it lies.** Each file of WhatsApp's folder
  is a file in the phone's own vault, under `.qurb-outside/whatsapp/`
  followed by its path in WhatsApp's folder -- a name the person's own files
  cannot have, kept out of every list as the sharing rules are. Its chunks are recorded and none stored:
  its bytes stay in WhatsApp's folder and are read from there, through the
  grant, whenever they are asked for and checked against its hashes -- as a
  send's are (0060), but for as long as the file is there, not until it is
  collected. Recorded in a table of their own, `outside_sources`.
- **The laptop keeps it as it keeps the vault**: in its chunk store, never
  in a folder, never shown, dropped only when the phone deletes the file.
  Nothing new on the laptop's side.
- **A file changed in WhatsApp's folder is a new version** of its vault
  file, found by size and time when the folder is next listed.
- **A file gone from WhatsApp's folder**: under `Media`, it is *freed* on
  the phone -- as *Free local space* frees a file -- once the laptop is known
  to hold it, and so stays in the vault and on the laptop (answer 2); a
  medium the laptop never got is simply gone. Under `Databases` and
  `Backups`, it is deleted, so that the laptop keeps one consistent set of
  chat backups.
- **Never listed, never a deletion by the folder scan.** The phone's Files
  and Private Vault do not show it; Settings shows the backup
  -- when it last ran, how much, on which computer. The scan of qurb's own
  folder leaves these rows alone, as it leaves a freed file.
- **A run may stop part-way.** Indexing reads each new file once, to chunk
  it: about 9.65 GB the first time on the S23. A run stops at its time
  budget and the next carries on.
- **Restoring** is as point 7 says, and needs one more thing nothing has
  yet: a new phone reaching the vault of the phone it replaces, which the
  laptop keeps under that old phone's identity.

## Progress

### Step 1, the engine — built 2026-10-12

`outside_sources` (schema 22) records where each file is read from;
`Store::keep_outside` reads a file once to chunk it, into this device's own
vault, storing nothing; `Store::outside_gone` deletes it, or keeps it as a
freed file when the caller says so and a computer holds it; and
`Store::keep_outside_folder` does both for a whole listing, up to a deadline.
Chunks are served from the file through `chunk_from_disk`, which `read_chunk`,
`held_chunks` and `verify` already ask. Nothing changed on the device keeping
the vault: it collects these rows as it collects any.

Tests: nine in `storage/tests/outside.rs` -- kept with no copy and offered to
the holder; not taken for a deletion by the folder scan; new, changed and
unchanged; a photo gone and kept, gone and never held, a chat backup gone;
a run stopped at its deadline; a file changed behind the listing not served
as the old one; a file that comes back -- and, over a real connection,
`a_folder_kept_where_it_lies_is_held_by_the_desktop` in
`peer/tests/holding.rs`.

### Step 2, the phone — built 2026-10-12

- **The engine's side.** `back_up_whatsapp(files, seconds)` takes the app's
  listing, leaves out WhatsApp's caches (any path with a part starting with
  a dot), keeps media gone from WhatsApp and deletes chat backups gone from
  it, and runs in slices of three seconds so that the app is never kept
  waiting on the engine. `whatsapp_backup()` counts it for Settings.
- **Not on mobile data.** The backup is offered to the computer keeping the
  vault only while the app says the phone is on a network that does not
  count what it carries (`set_unmetered`, before every sync, from Android's
  own idea of metered). Until then the computer is not shown it, which costs
  nothing: a holder does nothing about a file missing from a list. Counted
  then in what keeps the phone serving, so a large first backup runs in the
  foreground (decision 0050) on Wi-Fi.
- **Out of every list.** `.qurb-outside/` is excluded wherever the sharing
  rules are: Files, Private Vault, search, the total of files, what *Clear
  data* would lose, and Home's files waiting to reach a device.
- **The app.** Settings → WhatsApp → *Back up WhatsApp*: what it does, that
  restoring is untried and the Google backup should stay on, then Android's
  picker at WhatsApp's folder. Only that folder is accepted, read only. Each
  night, charging and on Wi-Fi, a worker lists it -- skipping its caches --
  and hands the listing over, then syncs; *Back up now* does the same at
  once. *Stop backing up* gives the grant back; what the computer keeps
  stays.

Checked on the emulator, against a stand-in folder of 305 files (38 MB): set
up through Settings and the picker, the first run listed 304 files in 0.4 s
-- the `.Statuses` cache left out -- and read 38 MB through WhatsApp's
documents; a second, after a photo was added and a chat increment removed,
added one, deleted one, and left 303 unchanged. Settings said *0 of 304
files on test-laptop so far*, test-laptop being a device that is never
on. Tests: `whatsapp_is_backed_up_where_it_lies` (the FFI);
`a_folder_kept_outside_is_in_no_list_a_person_is_shown` and
`the_backup_is_counted` (storage).

**Not yet:** the S23 backing up to the laptop for real; restoring (step 4).

### Step 3, the desktop — built 2026-10-12

The phone's panel in Devices says what the computer keeps of its WhatsApp:
*WhatsApp — 37,722 files, 9.6 GB, kept here for it, out of sight* (in the
fixtures). Nothing more: the files stay in the chunk store, never in a
folder, as answer 1 asked (`Db::held_outside_for`, `view::Device::whatsapp`).
The workspace's 845 tests pass.

### On the S23, the first time — 2026-10-12

Set up from Settings on the owner's S23 (Android 16, One UI 8.5), charging,
on the home Wi-Fi, with the laptop keeping its vault.

| | |
|---|---|
| listing WhatsApp's folder | 36,010 files in 76 s, its caches left out |
| the first run | 6,631 files, 3.5 GB, in its 7 minutes: 8.3 MB/s, 63 ms a file, one file at a time |
| the laptop collecting small files | 4 to 8 a second, 0.5 to 1 MB/s |
| the laptop collecting large files | 10 to 13 MB/s, round trips of 15 to 25 ms |
| at 19:39 | 5,648 files, 3.3 GB on the laptop |

**Too slow, the owner said, and it was.** Each end did its files one after
another and waited on every one: the phone on Android's document provider
for each file it read, the laptop on several round trips per file to a
phone whose Wi-Fi, with the screen off, answers slowly. On this machine the
same work is 0.4 ms a file to read and 3.7 to collect, so the time was the
waiting. Changed:

- **The laptop fetches ahead.** The engine tells the network source every
  file a plan will ask for (`ContentSource::coming`), and small ones (4 MB
  or less) are fetched 32 at a time, at most 64 MB held; what arrived is told
  to the phone in batches rather than a round trip after each file
  (`finished`). Collecting 2,000 small files over a connection on this
  machine went from 3.7 ms a file to 2.3; the gain on Wi-Fi is the round
  trips it overlaps, not yet measured.
- **The phone reads four at a time.** New files are read and chunked on
  four threads and recorded in order on one. Not yet measured on the S23.
- **A run that leaves files carries on at once**, on Wi-Fi, rather than
  waiting for the next night: 3.5 GB of 9.65 went in the first window.

**And a bug the run found.** When the phone went away part-way through, at
14:07, every file the laptop was fetching for it failed, and each was
written into the laptop's history by name -- 5,027 of them, which the
desktop's Activity and its *could not finish* notifications read. A file
kept for another device is now only logged when it fails
(`a_held_file_that_fails_is_not_in_the_history`), and the history leaves out
the backup's paths, so the 5,027 already written are no longer shown. The
notification also said *qurb* in lower case; it says *Qurb*.

**Not finished.** The phone has not connected since 14:08: its wireless
debugging went off, which Android does when a phone leaves Wi-Fi, so it is
probably away from home. The backup carries on when it is back on Wi-Fi.
Collection still happens only while the phone serves -- a pass in the
foreground of up to half an hour after each run (decision 0050) -- and
between passes, nothing.

**Why nothing came after the first run** (2026-10-13). The laptop held
6,631 files, exactly what the first run read. A run stopped at the first
file it could not read, and a run that had failed did not carry on -- and a
file WhatsApp moves or deletes in the minute the listing takes is ordinary.
So every run after the first stopped at once. A file that cannot be read is
now skipped for the rest of the run and tried again in the next, and the
rest go on (`a_file_gone_before_it_is_read_does_not_stop_the_run`). This
reading of it is inferred from the laptop's count; the phone's log was not
there to read, its wireless debugging having gone off.

The times in the laptop's log are UTC: collecting went on until 19:38 IST,
not 14:08 as first read above, and at 21:14 the phone synced over mobile
data, where the backup is held back, as designed.

### The S23 set up again — 2026-10-13

The owner had cleared Qurb's data on the S23 on the evening of 2026-10-12, to
start over. What that took with it, and what it did not:

- **Gone on the phone:** its index, its key, its settings -- the WhatsApp
  grant among them, and the rendezvous address, back to the emulator's
  default.
- **Kept on the laptop:** everything it held for the phone, the 6,631 files
  of the first backup and the phone's Private Vault, filed under the
  phone's old identity. The app cannot yet give them to a phone set up
  again: the restoring gap above.
- **WhatsApp itself:** untouched.

Set up again by joining the laptop's code (`qurb pair` beside the running
window, approved there once the phone showed the same number, 599 487); the
laptop made to keep the new phone's vault from Devices; the rendezvous set
back to the laptop's (`wss://saqib.tail86283c.ts.net`, the laptop's service
through Tailscale Funnel); the WhatsApp backup set up again from Settings at
18:57. It starts from nothing.

**Found: the phone could not reach the laptop on its own** -- every attempt
to the laptop's address timed out (`192.168.1.4:37716 timed out`), while the
laptop reached the phone each time. The backup does not need that
direction, since the laptop collects, but Home on the phone said *Not synced
yet* meanwhile.

Not the firewall, though it looked like it. A UDP packet sent from the phone
(toybox `nc`) to `192.168.1.4` never arrived; the same packet to
`192.168.1.2` did. The laptop's Wi-Fi had reconnected at 18:38:46 IST and
come back with a new lease, `.2` instead of `.4` (NetworkManager's log).
The laptop's qurb had started the evening before and read its addresses once,
at startup, so it went on offering `.4` -- in its beacons, in what the
rendezvous service held for it, and in its answer each time the phone asked
to meet. Nothing answered there.

Fixed in `crates/peer` the same day: a connector listening on every
interface reads its addresses again every five seconds and, on a change,
announces again to the rendezvous (which already replaced a device's
addresses on a second announce), answers introductions with the new ones,
and puts them in its beacons at once. Tested against a real rendezvous
service (`a_device_that_moves_is_offered_where_it_is_now`, which fails with
the re-announce taken out) and on the loop itself. The public address STUN
found at start is still not looked at again; QUIC owns the socket by then.
A stale one costs a peer a few packets in the race, and the local addresses
and the relay are what this keeps right.

Watched on the devices the same evening. The laptop ran the new build
from 19:26 IST, offering `192.168.1.2`, and the S23 the new app from
19:40. At 19:40:52 the phone reached the laptop on its own for the first
time since 18:38 (`connected ... candidate=192.168.1.2:59972`, *reached on
the local network*), and its pass ended `reached=1`. Restarting the laptop
re-read its addresses anyway, so that run does not show a change being
followed. That is shown only by the two tests, until the laptop next
changes address with qurb running.

It did, at 21:00:45 the same evening: the laptop's Wi-Fi dropped for five
seconds (NetworkManager re-associated and took the same lease back). qurb
logged both changes on its next look, `192.168.1.2` gone at 21:00:45 and
back at 21:00:50, and was connected to the phone again at 21:00:54.

### Faster, measured on the S23 — 2026-10-13

The same phone and laptop, the phone on battery at 17% and on the home
Wi-Fi, the backup started again from nothing at 18:57 IST:

| | 2026-10-12 | 2026-10-13 |
|---|---|---|
| listing WhatsApp's folder | 36,010 files in 76 s | 36,031 in 43 s |
| the phone reading new files | 8.3 MB/s, 63 ms a file, one at a time | **23 MB/s, 12 ms a file**, four at a time |
| the first run, in its 7 minutes | 6,631 files, 3.5 GB | 35,472 files, 9.69 GB -- all but 559 |
| the laptop collecting small files | 4 to 8 a second | 17 to 28 a second, fetched 32 at a time |

The first 6,631 files cost the laptop a copy, not a transfer: it already
held their content from the first backup, under the old phone's identity.

### Slower while the phone is in use -- 2026-10-13

From about 19:13 IST the laptop's collection fell from 600 to 2,000 files
a minute to between 64 and 160. It rose in steps of exactly 32, the
laptop's fetch-ahead batch, one batch every 15 to 30 seconds. The laptop
was not the one waiting. Measured at 19:30, it was at 0% CPU and its Wi-Fi
was receiving 11 KB/s.

On the phone, Samsung had moved Qurb's process into a group of its own
named `abnormal`. Qurb was the only process in it. Read from `/proc` and
`/dev/cpuset` over ADB at 19:23 and after:

| | Qurb, in `abnormal` | other foreground apps | the app in front |
|---|---|---|---|
| CPUs it may run on | 0-2, the three little cores | 0-6 | 0-7 |
| CPU group | `background` | `foreground` | `top-app` |
| disk priority (blkio) | `low` | -- | -- |

Its busiest thread was waiting on the disk (state D). Android itself had it
in the *active* standby bucket (10) and allowed to run in the background.
It is not exempt from battery optimisation, which is the S23's default.
Samsung's freezer (`FreecessHandler`) logged trying to freeze it from 19:04
and giving up each time, because of the foreground service. At 19:19:51 it
restricted Qurb's jobs on an app switch (`restrictJobsByOlaf ...
reason=APP_SWITCH`). The owner was using the phone throughout, with
Instagram Lite, Gmail and Reddit in front in turn.

So while the phone is in use, its manufacturer sets the pace, not qurb.
The backup still finishes: what the phone has not served by the end of one
pass is collected in the next, and the nightly run is while charging.

**Not known yet:** what puts an app in `abnormal` or takes it out. Nothing
published was found. It was not checked whether it lifts with the screen
off, while charging, or with Qurb's battery setting at *Unrestricted*,
which only the owner should change.

The same evening Samsung's freezer was seen at work on it directly:
`/dev/freezer/abnormal/freezer.state`, read once a second over ADB, went
from `THAWED` to `FROZEN` and stayed there four seconds, while Qurb's
threads, the main one included, sat in state D. The laptop's count did not
move from 19:30:54 to past 19:38. Whether Qurb's battery setting at
*Unrestricted* keeps it out of `abnormal` is the next thing to try, with
the owner's say-so.

The owner also saw the transfer notification glitching. Two long passes
were running at once, each posting it with its own figures. That fix is in
[0050](0050-large-files-from-a-phone.md).

**Unrestricted does not keep Samsung off it.** With the owner's say-so,
Qurb's battery setting was put to *Unrestricted* at 19:40 (`cmd deviceidle
whitelist +com.qurb`). It helped once: a long pass then went into the
foreground from the background, which Android had refused before. The
owner started a backup at 19:45. For two minutes the laptop took 160 files
and 90 to 150 MB every 30 seconds, with Qurb in a Samsung group named
`moderate`. At 19:47 it was back in `abnormal`, frozen by 19:48, and down
to 32 files a 30 seconds. Samsung's own log names the mechanism:
`FreecessController.freeze` from `FreecessHandler.handleLcdOnFreeze`, at
19:46:56, 19:47:20 and 19:48:02. That is freezing apps not in use *while
the screen is on*, foreground service or not. Samsung's own list of
"never sleeping apps" is the setting not yet tried.

**Away and back, 20:08 to 20:25.** The phone left the home Wi-Fi at about
20:08, with the laptop at 22,440 files, and came back as `192.168.1.5`
instead of `.3`. The laptop reached it at the new address at 20:25:09 with
no help: the phone's own addresses are read at the start of each pass.

**When the phone goes away mid-collection, the laptop now stops asking.**
At 20:35:46 the laptop's connection to the frozen phone timed out, and it
went on through its plan: 10,408 files failed at once, each logged with
`connection lost`. Each would have failed the same way. Changed the same
evening: a plan asks nothing more of a source whose connection has ended.
What is left is counted and logged once, and taken at the next sync
(`a_device_that_goes_away_leaves_the_rest_for_next_time`). The phone's
side, a pass ending because it was frozen, is in
[0050](0050-large-files-from-a-phone.md). Both are on the laptop and the
S23 since 20:53. The laptop's half was watched at 21:00:53, when its own
Wi-Fi dropped mid-collection. The log has one failed file and then *the
device went away part-way; the rest next time, files=11007*, in place of
11,007 warnings.

**Complete, 2026-10-14 at 10:31 IST.** The laptop held all 36,031 files,
9.79 GB, of the backup the S23 started from nothing at 18:57 the evening
before. The night did not help: the laptop suspended itself when idle at
21:51 and woke at 10:06, with 5,311 files still to come (decision 0064,
limit 4). Those came between 10:21 and 10:31, about nine files a second,
the phone not frozen. WhatsApp's next six new files arrived at 13:51 on an
ordinary pass. Wall time from start to finish: 15.5 hours, of which the
laptop was asleep for 12.

### Files kept and not known to be -- 2026-10-15

All morning the S23's Home said 130 WhatsApp files were still to go to the
laptop (97 in *WhatsApp Documents/Private*, 31 excluded stickers, a sent
document and a database), while the laptop's index held every one of the
36,062, the same content, marked kept. The phone had never been told: the
laptop says *got it* in batches as it collects, and a batch lost with its
connection was only logged. The holdings report that makes up for lost
ones covers only what a computer holds in its own folder, and a phone's
vault never is. Being 54 MB, more than the 32 MB that makes a pass a long
one, they also made every pass on the phone a long one, under its
*Sending to your devices* notification, waiting for the laptop to collect
what it already had.

Now the phone asks: each sync, up to 64 of its vault's files with no record
of the computer keeping them, at random, all at once (`confirm_kept`).
Installed on the S23 at 11:30: 121 then 59 then 0 in two syncs; Home
said *Everything is synced* and *All 36,062 files, 10.1 GB, on saqib*, and
the notification was gone. Test:
`a_kept_file_the_phone_was_not_told_of_is_learned_of_by_asking`, where a
file not collected yet is asked about and left as it was.

