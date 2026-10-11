# 0062 — Backing up WhatsApp to a computer, not to the Google account

**Status:** Proposed, 2026-10-11. Measured with
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
