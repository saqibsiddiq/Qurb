# 0063 — Backing up a phone's folders, chosen on the desktop

**Status:** Accepted, 2026-10-14, with the owner's answers below. Built the
same day, all four steps, and seen on the S23 and the laptop: "All files
access" measured on the S23 first
([experiments/file-access-speed](../../experiments/file-access-speed/README.md)),
then a folder ticked on the laptop backed up from the phone. Amended the same
day: chosen on either device, the later choice winning.
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

## Progress

**Step 2, the engine, built 2026-10-14.** Tables (schema V23): on the phone,
its own folders as last listed (`own_folders`) and the folders last said to
be chosen (`chosen_here`); on the computer, each phone's folders as it last
said (`known_folders`) and the ticks (`backup_choices`). Two requests,
`Folders` and `Chosen`, each answered only to whom it belongs. The phone asks
`Chosen` on every pass with a computer keeping its vault; the desktop asks a
phone of its person's `Folders` on every sync. On the phone, `list_folders`
walks shared storage once and counts what is beneath each folder two levels
deep, and `back_up_chosen` keeps each chosen folder where it lies under
`.qurb-outside/storage/<folder>/`, with WhatsApp's tested loop. A folder
inside another chosen one is backed up once; anything absolute, climbing out,
hidden, or in `Android/data` or `Android/obb` is refused, so a computer can
only point the phone at its own ordinary folders. Tested: who may see a
phone's folders and choices (`only_a_computer_keeping_the_vault_sees_a_phones_folders`,
`a_phone_is_told_what_was_chosen_for_it_and_nothing_else`), and the phone's
side end to end (`folders_chosen_on_the_computer_are_backed_up_where_they_lie`).

**Steps 3 and 4, the phone and the desktop, built 2026-10-14.** On the
phone, Settings' WhatsApp group became *Backups*, with *Back up your folders*:
without the permission it says what it is for and opens Android's own page;
once granted, the folders are listed at once and the nightly run scheduled.
Each sync pass on Wi-Fi lists the folders at most every six hours and backs
up what was chosen for 15 seconds, before counting what is waiting, so new
files go in the same pass; the nightly run, while charging, gives it seven
minutes, as WhatsApp's. On the desktop, a phone of the person's own shows
how many folders are backed up here, and *Choose folders to back up…* opens
its folders with a switch each, two levels deep; a folder inside a chosen one
is shown ticked, fixed and dimmed, *Backed up with DCIM*. A chosen folder's
*Open* lists what is kept here, and a file opens by being written out under
the runtime directory, as a guest's file is (0060). Checked on the emulator
(the sheet, the system's page, the folders listed in 109 ms once allowed)
and in the desktop fixtures (the panel, the folders, the listing, a phone
that has not listed them yet).

**Step 1, measured on the S23 2026-10-14** ([file-access-speed](../../experiments/file-access-speed/README.md)):
with "All files access", listing WhatsApp's 36,055 files took 4.3 to 5.3 s
against 39.6 to 57.5 s through the picked folder, and reading took 2.6 to
3.0 ms a file against 5.8. Direct reads are worth having for speed as well as
for choosing on the desktop.

**Watched between the S23 and the laptop, 2026-10-14.** The owner allowed
"All files access" in the system's page from the phone's Settings; the
phone listed 28 folders (DCIM/Camera: 1,386 files, 7.5 GB), and the laptop
had all 28 at its next sync with it, 17:56:28. He ticked DCIM/Screenshots in
the desktop app; *Back up now* on the phone read its 97 files, 61.5 MB, in
578 ms, directly, and the laptop held all 97 within seconds.

Two things it found, both changed the same day:

- **The tick did not take the first time.** The sheet saved with a button,
  and on a list of 28 folders the button was below the sheet's edge; the
  owner ticked, and nothing was chosen. Every switch in Qurb applies at once,
  as its stylesheet says; these now do too, with a note saying when the
  phone picks it up.
- **A newly chosen folder waited for a background pass**, up to an hour.
  After a sync now, a folder chosen since the last one starts backing up at
  once, on Wi-Fi.

And, from the measured speed: where the phone has the permission, WhatsApp's
folder is listed and read directly too, and a file found at a new address
with the same size and time is recorded there without being read again
(`a_file_found_another_way_is_read_from_there_without_reading_it_again`), so
the 36,031 already kept move to the fast path for nothing.

Watched on the S23 at 18:36: all 36,152 kept files' addresses became direct
paths, none read again (`unchanged=36055`, recorded in 2.5 s). The listing
itself took 24 s, the app asking each file for its kind, size and time
separately; the Rust walk, one look at each entry, listed all of the phone's
storage for the folder summary in 4.7 s of the same run. Listed by it
instead, at 18:44 from *Back up now*: 11.5 s, as a background job, against
39.6 to 57.5 through the picked folder.

## Amended 2026-10-14: chosen on either device

The owner's item 10 of 2026-10-14: the folders are chosen on the phone too,
and from its Home. Both devices may now change the choice, so each folder's
choice is kept on both with when it was made, in milliseconds, unticks
included, and the later one wins wherever it was made (schema V25:
`phone_choices` on the computer, `own_choices` on the phone). The two asks
carry them: the phone's answer to `Folders` brings its choices, the
computer's answer to `Chosen` brings its own, and each side keeps what is
later than its own. One sync each way and the two agree; still nothing is
pushed at either. Tested by `the_later_choice_wins_wherever_it_was_made` and,
through the real requests, `a_choice_made_on_either_side_reaches_the_other`.
The clocks are the devices' own; two choices of the same folder within a
clock's error of each other could go either way, which for a choice a person
makes by hand is accepted.

## Reversing it

Cheap before step 2. After it, each folder is rows like WhatsApp's: no longer
choosing a folder stops new files, and what was kept stays until the person
drops it. The two asks are additions to the protocol, unused by a peer that
does not send them.
