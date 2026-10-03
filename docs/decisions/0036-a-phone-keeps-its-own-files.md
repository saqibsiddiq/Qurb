# 0036 — A phone keeps its own files, and another device holds them for it

**Status:** Accepted — built in the engine, on the desktop and in the phone app, and verified between a Galaxy S23 and the laptop.
Amended by [0049](0049-adding-a-file-puts-it-where-you-are-looking.md): a file
added with a choice of area — *Add files* in Files or Private Vault, or the
share sheet's choice — goes into that area; the default below governs the rest
**Date:** 2026-09-25

## Decision

A file a device adds itself goes into **that device's own private vault**, not
the shared area. On a phone that means everything added through the app or the
share sheet. It leaves the phone only when the person sends it somewhere.

So that the person can free the phone's copy and still have the file, a device
they choose **holds the vault for its owner**: a copy of its content, kept in
qurb storage, counted against that device's allowance, never shown there, and
served back to the owner and to nobody else. The owner may then drop its local
copy, because a device holding its vault will give it back.

Chosen by the project owner on 2026-09-25, from the three options set out in
[product-plan §3.1](../product-plan.md).

## Why

It is the product brief's model, and the brief specifies it in five places,
including its acceptance test: a photo added on the phone, sent to the desktop,
freed on the phone, still listed as "Available on Desktop", and downloaded
again.

The two alternatives each lose half of that:

- **Shared by default** — today's behaviour — makes every photo on the phone
  appear on the desktop. That is the opposite of a private vault.
- **Private, held nowhere else** means "Free phone space" can never be offered.
  [Decision 0030](0030-sending-a-file-to-one-device.md) forbids counting a copy
  the phone cannot ask back, and without a holder there is no copy it can.

## What holding means, precisely

- **The holder keeps the content the way a sender keeps a delivery**
  ([0030](0030-sending-a-file-to-one-device.md)): chunked in its store, never
  backed by a file in its folder, never materialised, and absent from its
  listings, search and notifications.
- **It is served back only to the owner.** The protocol already lets a device
  see its own vault on any other device
  ([0029](0029-two-areas-shared-and-private.md)); nothing changes about who
  else may ask.
- **It is not released when the owner has it.** A delivery is released once the
  recipient takes it. A held vault is kept until the owner deletes the file, and
  then for the retention window like any other tombstone.
- **On the owner, the holder's copy counts.** It is recorded as a copy the owner
  can ask back — the non-private kind of replica record — so the storage cap and
  "Free local space" may rely on it. This is the one case in which a copy inside
  a vault counts, and it counts only for that vault's owner.
- **The holder's allowance applies, and it never drops a held copy to make
  room.** The owner may already have freed theirs. A holder with no room takes
  nothing more, and the owner's file stays "only on this phone", which cannot be
  freed.

## Privacy, stated rather than implied

All of one person's devices hold the same master key
([0012](0012-key-hierarchy-and-recovery.md),
[0023](0023-one-person-per-account.md)). A holder could decrypt what it holds.
What stops the desktop showing the phone's vault is the protocol and the
desktop's own code, not cryptography.

Making it cryptographic was considered and rejected. It needs a vault key the
holder never has, and the 24 words could not restore that key either — so
losing the phone would also lose the copy the desktop was keeping for it. The
backup would fail in exactly the case it exists for.

So the product says what is true: *"Kept on Desktop for this phone. Desktop
doesn't show it."* Never *"Desktop can't read it."*

## What changes in the engine

1. **Vault rows the owner creates.** Today only a delivery creates one.
2. **A holding grant.** The owner names the devices that may fetch its vault,
   and its server checks the grant on `Tree`, `Manifest` and `Chunk` the way
   0029's checks work. The `Audience` enum gains the case "holds this vault".
3. **A replica record the owner counts** for eviction.
4. **The owner's rename, move and delete reach the holder.** 0029 records that
   vaults do not converge. A held vault has one writer, its owner, so this is
   propagation from one device rather than reconciliation between several.
5. **Collecting must not undo freeing.** The owner keeps a row for every held
   path, freed or not, and a held copy is never offered back to its owner as a
   delivery. Otherwise the next sync would re-download everything the person
   had just freed.

Point 2 is a new request or a new meaning for an old one, so it probably means a
protocol version bump like 0030's. That is settled when it is built.

## What does not change

- **The shared area.** Existing files stay where they are and nothing migrates,
  as in 0029.
- **Sending.** A send into another device's vault is still 0030.
- **What a desktop adds to its own folder** is still the shared area, until
  sharing (brief §23) is decided.

## Settled when it is built, not now

- **One folder, two namespaces.** A phone's folder already holds the shared
  area and its vault side by side, and a name can mean either. Found on
  2026-09-24 and patched by refusing the clash (see
  [phase 5](../phases/phase-5-mobile.md), "A file sent to the phone came straight
  back"). Making the vault a phone's *default* makes clashes ordinary rather
  than rare, so this decision has to give the two areas separate places on
  disk, or an equally structural answer — not rely on the refusal.
- **Which devices hold by default.** The brief's picture is "the desktop". The
  engine has no notion of device kind, so the first version lets the owner
  choose, offering every paired device that has a folder and an allowance.
- **Whether a replica can hold.** It is the natural holder, being always on.
  0030 records that a replica cannot usefully carry a *delivery*; holding is a
  different shape and may fit.
- **What the owner sees while its holder is off.** The file stays "Available on
  Desktop"; asking for it says Desktop is offline (brief §25).

## How it is built

Worked out against the code on 2026-09-27, before writing it.

**Four kinds of tree entry, not a flag.** A tree entry said only "private" or
not. Holding needs direction: once two devices can hold each other's vaults, a
bare "this is a vault" cannot say whose. Each entry now carries one of:

| kind | meaning, from the side receiving it |
|---|---|
| shared | the shared area, as before |
| sent | something the other device sent into your vault (decision 0030) |
| held | your own file, which the other device holds for you |
| hold | the other device's own file, for you to hold for it |

A build that knew only the old flag would read the two new kinds as "sent" and
file another device's private files into its own folder, so the protocol goes
from `qurb/1` to `qurb/2` and the two refuse to talk.

**On the holder, a held row is marked as such** (`files.held`, index schema
V12). It sits in the owner's vault like a send does — scope is the owner, bytes
in the chunk store, nothing in the folder — and differs in three ways that each
matter:

- it is **never released**. Releasing a send once the recipient has it is right;
  releasing a held copy once the owner reports having its own file would delete
  the backup the moment it was made;
- it is **never offered as a delivery**, so the owner does not "receive" its own
  files back;
- it is **not waiting to be collected**, so it is not on the sender's list of
  things on their way.

A send the owner has collected becomes held when the owner's list names it:
the same bytes, now kept for the owner rather than on the way to them.

**The owner keeps a list of holders** (`holders`), and its server shows its own
vault, as *hold*, only to a device on that list — tree, manifest and chunk
alike, checked the way decision 0029's checks are.

**A phone's new files go into its own vault.** A store flag, set by the phone
app: a file this device adds that has no row yet goes into its own vault rather
than the shared area. A desktop leaves it off.

**The holder mirrors, and only on instruction.** An entry of kind *hold* is
taken into a held row; a *hold* tombstone tombstones it. A file that is simply
absent from the owner's list is **left alone**. The alternative, deleting what
the owner no longer lists, would make a phone that was wiped, or whose index was
lost, delete its own backup on the holder at the first sync — the backup
failing in exactly the case it exists for. The cost is that a holder away for
longer than the owner keeps tombstones — seven days — keeps a file the owner
deleted. That is disk, not data.

**The owner counts the holder's copy.** The holder confirms each file it holds
with the same `Got` a delivery uses. On the owner the content is in its own
vault, not the holder's, so it is recorded as an ordinary replica, which is
what the storage cap and *Free local space* may rely on.

**The owner fetches back by content.** Its freed files stay in its index; asking
for one works as for the shared area, from the holder's *held* entries.

**Known gaps, stated before building:**

- **A replaced phone cannot get its vault back yet.** Setting up a phone from the
  24 words gives it a new device identity, and the holder keeps the old phone's
  vault under the old one. The bytes are safe; nothing yet lets the new phone
  ask for them. That is the recovery flow, and it is a separate piece of work.
- **The holder's allowance is not checked yet.** 0036 says a holder with no room
  takes nothing more. The first version takes everything it is asked to hold,
  and says so.
- **Renames are a deletion and an addition.** The holder re-files the content
  under the new name without moving it again, because it already has the bytes.

## Progress

- **2026-09-27 — the four areas and `qurb/2`.** On the wire; nothing produced
  or acted on them yet.
- **2026-09-27 — storage.** Index schema V12: `files.held` and the `holders`
  list. A store can be told to file new local files in its own vault; a holder
  can keep a file for its owner (`hold_file`) and drop it only on the owner's
  tombstone (`unhold`); freeing space covers a device's own vault. Held rows
  are left out of everything that releases or lists sends. Ten tests in
  `crates/storage/tests/holding.rs`; the two guards that keep a backup from
  being released were each removed and their tests seen to fail.
  Found on the way: the list of sends waiting to be collected included every
  row in this device's *own* vault, so a device keeping received files in its
  folder would have listed them as waiting for itself.
- **2026-09-27 — the server and the engine.** A device's own vault is shown,
  as *hold*, only to its holders -- tree, manifest and chunk. A holder keeps
  what it is shown and drops it on a tombstone; the owner fetches its own freed
  files back from the entries marked *held*. The desktop gained `own-files =
  private`, `qurb holders` and `qurb free`.

  Tracing it end to end found four places the holder would have named the
  phone's files on the desktop, all closed: the home screen's "Recently" list,
  the Transfers screen's progress for what is being collected, and the history
  line and notification written when the owner takes a file back.

  **Verified** with two devices on the laptop, one set to `own-files =
  private`: a 3 MiB photo was held by the other within about 2 seconds and
  appeared nowhere on it -- not in its folder, its listing or its history.
  Freeing it on the phone was refused until then, and allowed after. Fetched
  back in about 8 seconds, SHA-256 identical. Deleting it on the phone made the
  holder let go within about 2 seconds. Also 6 engine tests and a network test;
  the privacy checks were each removed and their tests seen to fail.
- **2026-09-27 — the phone's interface.** The FFI can now keep a phone's new
  files private (`Settings.ownFilesPrivate`), name and list holders, free a
  file (`OnlyCopy` when nobody else has it), fetch it back, send, cancel a
  send, and read history; tested through the same calls the app will make.
- **2026-09-27 — the phone app.** Files added on the phone are private by
  default, with *Keep new files private* in Settings to turn that off from the
  next file (`set_own_files_private`, tested). The Devices screen chooses who
  keeps them; the Vault says where each file's bytes are and offers *Free phone
  space* and *Download*; Home says how many files are only on the phone and
  what to do about it. Installed on a Galaxy S23 and its screens checked
  against the phone's real data.
- **2026-09-27 — verified on hardware.** The Galaxy S23 and the laptop's own
  `~/qurb`, on the same Wi-Fi, with no rendezvous service reachable from the
  phone, driven through the phone's screens. The desktop chosen on the Devices
  screen; a 3 MiB file of random bytes added through the system picker went in
  private; the next sync left it held on the desktop (`held = 1`, not in the
  folder) and the phone showing it as on both. Freeing it took qurb on the
  phone from 115.1 to 112.1 MB; *Download* had it back within twelve seconds,
  and a copy saved out of the app was SHA-256 identical to the original.
  Nothing on the desktop named it: not its history, its folder, its Downloads,
  its log or `qurb status`. Deleted on the phone, the desktop let go about a
  second into the next sync. The desktop also took `sent-to-phone.bin`, which
  the laptop had sent to the phone days before and so sits in the phone's own
  vault.

  It did not work the first time. The phone reached the desktop and the desktop
  never came back for the file: each sync pass on the phone left its discovery
  running and ended before the desktop could dial it. Both fixed; see
  [decision 0020](0020-sync-takes-a-deadline.md#a-pass-that-waits-to-be-collected-from).
- ~~**Not yet:** the app using any of it. The setting stays off until the
  rebuilt screens can name a holder, so a phone's files still go to the shared
  area.~~ Written before the 2026-09-27 entries above, and overtaken by them.
- **2026-09-29 — the designed app.** Private Vault became a place inside
  Files, each with its own *Add files*, and an add goes into the area it was
  made from — [0049](0049-adding-a-file-puts-it-where-you-are-looking.md).

## Reversing it

Moderate before anybody has used it and expensive after. Once phones hold
private files that exist nowhere in the shared area, returning to
shared-by-default means deciding, file by file, whether to publish each one.
