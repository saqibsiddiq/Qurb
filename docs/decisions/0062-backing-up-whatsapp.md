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
  is a file in the phone's own vault, under `WhatsApp backup/` followed by
  its path in WhatsApp's folder. Its chunks are recorded and none stored:
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
  and Private Vault do not show `WhatsApp backup/`; Settings shows the backup
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
