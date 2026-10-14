# 0063 — Backing up a phone's folders, chosen on the desktop

**Status:** Accepted as a design, 2026-10-14, with the owner's answers below.
**Nothing is built.** It depends on "All files access" reading a real phone
quickly enough, which is measured on the emulator and not yet on the S23
([experiments/file-access-speed](../../experiments/file-access-speed/README.md)).
**Date:** 2026-10-14

## What was asked

Item 3 of the owner's list of 2026-10-13: "just like we have developed a
backup for whatsapp, i want qurb to develop a backup of other files and media,
the user can choose what things he/she wants to backup in desktop using qurb,
the user can sync it whenever there is new data".

His answers the same day:

- **Where the choice is made:** on the desktop.
- **A file deleted on the phone:** kept on the desktop.
- **Seeing them on the desktop:** yes, browse and open them.

## Decision

1. **A chosen folder is backed up as WhatsApp's is**
   ([0062](0062-backing-up-whatsapp.md)): its files are kept where they lie,
   as files of the phone's Private Vault read by reference
   (`outside_sources`), under `.qurb-outside/<folder>/`, held by the computer
   that keeps the phone's vault, and offered to it on unmetered networks only.
   Nothing is copied on the phone. WhatsApp's folder becomes one such folder,
   chosen the same way.

2. **The desktop chooses, and the protocol still only asks.** A peer can ask
   and never tell, except to say it got something. So there are two asks:
   - *The desktop asks the phone what folders it has.* The phone answers only
     a computer that keeps its vault, and only with a summary: the folders of
     its shared storage, two levels deep, each with its file count and size.
   - *The phone asks that computer what to back up.* On each pass it reads
     the folders ticked for it there: a short list the computer keeps per
     phone, by the phone's device identifier.

   Only a computer of the same person, holding the phone's vault, sees the
   summary or chooses. A computer that keeps a guest's vault for another
   person ([0060](0060-a-computer-keeps-private-folders-for-several-people.md))
   is sealed out of it and chooses nothing.

3. **The phone needs "All files access"**, asked for once, saying why:
   without it Android lets an app into a folder only when it is picked on the
   phone, one folder at a time, so the desktop could neither see the phone's
   folders nor choose. It is the permission Google Play allows to backup and
   restore apps. A phone without it falls back to picking on the phone, as
   WhatsApp does now, and the desktop says so instead of showing folders.
   With it, WhatsApp's folder is read directly as well (0062 reads it through
   a picked folder: 6.4 ms a file on the emulator against 0.3 directly).

4. **Kept when deleted on the phone.** A backed-up file deleted on the phone
   stays on the computer, shown as deleted there, exactly as WhatsApp's media
   is kept (0062). It leaves the backup when the person removes it on the
   desktop, or the folder is no longer chosen and the person says to drop what
   was kept.

5. **Browsable and openable on the desktop.** A place for each phone's
   backups: its chosen folders, their files, opened by writing a copy out of
   the computer's store, or saved somewhere the person picks. This is an
   exception to [0036](0036-a-phone-keeps-its-own-files.md)'s "never shown on
   the holder", and a narrow one: backed-up folders only (WhatsApp's included)
   and only on a computer of the same person. Files added to the Private Vault
   in the app stay unseen there, as 0036 says.

6. **When it runs.** Each pass looks for new and changed files in the chosen
   folders: listing directly is cheap (304 files in 103 ms on the emulator).
   And nightly while charging, as WhatsApp's backup does, and *Back up now* on
   either device. A tick on the desktop reaches the phone at its next pass.

7. **Space is the computer's allowance, as for the vault** (0036). A computer
   with no room takes nothing more and says so; the phone's copy is never
   counted as backed up until the computer holds it.

## Why, and what was not chosen

- **Choosing on the phone only**, as WhatsApp now: the owner chose the
  desktop, where a large screen suits ticking folders.
- **Android's media permissions** (photos and videos, `READ_MEDIA_*`): no
  documents, downloads or other apps' folders, and no WhatsApp. "All files
  access" covers all of shared storage except `Android/data` and
  `Android/obb`, which no ordinary app can reach.
- **The desktop telling the phone** what was ticked: it would break the rule
  that a device only ever asks, which is what keeps a compromised peer from
  pushing anything at another. The phone asking costs one small request a
  pass.
- **A mirror that deletes on the desktop**: the owner chose to keep. A
  backup that loses a photo because it was deleted on the phone is not one.

## Costs, accepted

- "All files access" is a strong permission, and some people will refuse it.
  They get the per-folder fallback.
- A first backup of a camera folder is tens of GB. On the S23 that brings
  Samsung's throttling (0062, *Slower while the phone is in use*); it should
  start while charging, which item 1's work addresses.
- Folder names and sizes go to the person's own computer. Nothing goes
  anywhere else.

## Order of work

1. The S23 run of the spike: whether direct reads are as much faster on a
   real phone. The speed claims in item 1 rest on it too.
2. Engine: WhatsApp's backup generalised to named folders; the folder
   summary; the per-phone list on the computer; the two asks.
3. Phone: asking for "All files access"; reading the list; backing up what is
   ticked; WhatsApp read directly when the permission is there.
4. Desktop: each phone's folders with ticks; browsing and opening backed-up
   files.
5. Watched between the S23 and the laptop.

## Reversing it

Cheap before step 2. After it, each folder is rows like WhatsApp's: no longer
choosing a folder stops new files, and what was kept stays until the person
drops it. The two asks are additions to the protocol, unused by a peer that
does not send them.
