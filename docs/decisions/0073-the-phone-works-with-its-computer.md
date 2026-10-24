# 0073 — The phone works with one computer: no shared files on it, and it may change its own folder there

**Status:** Accepted as the design, 2026-10-24. Being built: step 1 of the
plan below, the shared folder read, is done; the rest is not. The
screens are in the Figma redesign ([design/redesign.md](../design/redesign.md),
*Files, draft 2*); the engine does none of the new parts yet. Amends
[0067](0067-a-folder-for-each-person.md), whose phone could only browse its
folder on the computer, and makes one exception to the rule that a device
serves read-only ([crates/peer](../../crates/peer/README.md), *The server is
read-only*). Amended the same day by the owner's next list: Files lists
every computer, and the device is chosen on Home (points 1 and 5 below); and
the shared folder bounded: every guest of the computer, read-write (point 6).
**Date:** 2026-10-24

## What was asked

The owner, during the redesign, 2026-10-24:

> this page is too complicated, we need to simplify it 1. we only need private
> vault, recieved files, deleted files and the phone's folder on desktop(which
> depends on the device youve selected in the device folder) 2. there is no
> search inside the phone' folder on the desktop 3. inside the desktop folder,
> the user must be able to perform all the basic operations like rename,
> delete, etc using there phone and itll reflect in the desktop folder in real
> time

Asked back the same day, and answered:

- The shared files his list leaves out: **dropped from the phone.**
- Search in the computer's folder: **added.**
- What the computer accepts from the phone: **only from his own phones, only
  inside that phone's folder, and a deletion goes to the computer's Recently
  deleted for 30 days.**

Two days earlier he had chosen how several devices relate
([redesign.md](../design/redesign.md), item 4): every device keeps syncing,
and choosing one in Devices is for what it keeps and where Send goes.

## Decision

1. **Files on the phone has two sections**: *This phone* (the Private Vault,
   Received, and Deleted), then *Devices*, each computer's folder. Nothing is
   selected for Files. (First written as four places, with the folder of a
   computer selected in Devices; the owner changed it the same day: "i dont
   like the idea of selecting device for files section".)
2. **The phone has no shared files.** The area every device holds the same
   (`files.scope` NULL) is not shown on the phone, and the phone takes no part
   in it. A file added on the phone goes into its Vault, or is sent to a
   device. Computers keep their shared folder among themselves.
3. **The phone may change its own folder on the computer**: open, save a copy,
   send, rename, move, delete, add files, make a folder. Each change happens on
   the computer when the phone asks, while both are connected.
   - Only a phone of the computer's own person (relation *own*), never a guest.
   - Only paths inside that phone's own folder there.
   - A deletion goes to the computer's Recently deleted for 30 days, so a
     mistake, or a lost phone in the wrong hands, can be undone at the
     computer.
4. **The computer's folder can be searched from the phone**, across its
   subfolders, while the computer is on.
5. **Home shows one device, chosen on Home.** Its status is that device's, a
   list opened from its name on Home changes it, and Send goes to it. Every
   device goes on syncing in the background. The owner: "selecting device
   should be for home section, and we can change the selected device in the
   home section itself".

6. **The computer's shared folder is reachable from phones, live**, as a
   phone's own folder there is: opened on the computer, nothing copied to the
   phone, needing the computer on. Not only the owner's phones: **the phones
   of guests too** (people visiting the computer,
   [0060](0060-a-computer-keeps-private-folders-for-several-people.md)). Added
   the same day: "we forgot to include shared folder in phone ui, which can be
   accessed by all the phone users", and, asked back, opened on the computer,
   for the owner's phones and his guests'.
   - **Every guest of the computer, and read-write.** Asked which guests and
     whether they may change it, the owner, 2026-10-24: "shared folder can be
     accessed by all the users of the desktop and its read-write". So every
     person the computer keeps a folder for (0060, 0067), not a list set on
     the computer; and each may open, add, rename, move and delete there, as
     the computer's own person may. A deletion goes to the computer's Recently
     deleted for 30 days, as item 3 has it for a phone's own folder.
## Why

- **The owner's use is a phone working with its computer**: backups of
  WhatsApp and chosen folders, a private corner, sending, and reaching what
  the computer keeps. The shared area, on the S23, held only test files.
- **Changing the folder from the phone** is what makes the computer's folder
  more than a backup to look at. Limiting it to the owner's own phones, to that
  phone's folder, and keeping deletions for 30 days, keeps what made serving
  read-only worth having: a device in the wrong hands cannot destroy anything
  for good.

## What it costs, and what is open

- **Engine and protocol work**, none of it begun:
  - requests that change files, with their checks;
  - a search request answered by the computer;
  - the device chosen on Home, as a setting;
  - a backup set for each device (item 4 of the redesign).
- **Guests reach more than what was sent to them, and can change it.** Under
  0060 a guest sees only its own sends and what the computer keeps for its
  person; the shared folder is a deliberate widening. Bounded by the owner the
  same day (item 6): every guest, read-write. A guest's phone in the wrong
  hands can delete what is in the shared folder, though not for good: the
  computer's Recently deleted holds it for 30 days. Removing a guest from the
  computer is what takes the reach away.
- **The rule that a device serves read-only gets an exception.** It is to be
  written into `crates/peer/README.md` when built, with the tests a hostile
  peer would need to fail (`tests/hostile.rs`).
- **A phone stops holding the shared area.** Decided 2026-10-24, asked: each
  shared file a computer keeps is dropped from the phone, and stays in that
  computer's Shared folder; one only the phone has moves into its Vault, so
  nothing is lost.
- **Real time** means while both are connected. With the computer off, its
  folder cannot be opened, and nothing is queued for later.

## Backups in a phone's folder there

Asked 2026-10-24 how far the phone may change its backups -- WhatsApp, and the
folders it backs up -- the owner chose: **opened, saved and sent like any
file, and deleted once the original is gone from the phone.** Rename, move, a
new folder and adding files are for the Private Vault part of the folder only.

Why that and not more: a backup follows the phone's own file. While the
original is on the phone, the next backup would put back a copy deleted or
renamed on the computer, so offering it would be offering something that
undoes itself.

## Plan, 2026-10-24

The shared folder and a phone's own folder are different work. The shared
folder is the computer's own, plain files in a folder on its disk, so a change
to it is one the computer makes when asked: the exception to read-only serving
this decision names. A phone's folder there is the phone's vault, which the
computer holds and the phone has the say over, so a change to it is the
phone's, carried to the computer as its other changes are.

In this order, each built with its tests before the next:

1. **The shared folder, read** (item 6). A request listing a folder of it, to
   the computer's own person's phones and to its guests, live; never to
   another computer, which syncs it. Guests may fetch its files' bytes, which
   they could not. Folders shared with only some devices stay theirs: a guest
   is in no such list.
2. **The shared folder, written** (item 6). Requests to make a folder, rename
   or move, delete -- to the computer's Recently deleted, saying which device
   did it -- and add a file, sent in pieces and checked against its hash
   before it is put in place. Each made by the computer on its own folder, as
   a change of its own, refused with a reason a person can read: a name
   taken, a path outside the folder. Tested as a hostile peer would try it
   (`tests/hostile.rs`): `..`, the store's own folder, another person's.
3. **Searching** the shared folder, and a phone's own folder (item 4).
4. **A phone's own folder, changed from the phone** (item 3): rename, move,
   delete, a new folder, adding files, in its Private Vault; delete for a
   backup whose original is gone (above). The phone's own vault changes, and
   the computer follows at once while connected.
5. **The phone leaves the shared area** (item 2), as decided above.
6. **The app**: Files, drafts 3 and 4.

Item 5, the device chosen on Home, was built with Home (draft 3).

## Progress

- **Step 1, the shared folder read, built 2026-10-24.** `Request::Shared {
  dir }`, answered to the computer's guests and its own person's phones in
  `Kept`'s shape (`crates/storage/src/shared_folder.rs`); a guest may fetch
  the bytes of a file in it, and is still never sent it to sync. Five tests in
  `crates/peer/tests/shared_folder.rs`: a guest lists it and fetches a file,
  and its list to sync stays empty; a folder shared without it is hidden, by
  name and by hash; the owner's phone is answered and the owner's computer is
  not; a computer visited is shown nothing of its visitor's; `..`, `.qurb` and
  `/etc` list nothing. The last found the store's own folder listed from disk
  when asked for by name, `.qurb`, in the first build; a hidden folder is
  never read from disk now. `a_guest_is_shown_only_what_was_sent_to_it` in
  `tests/guests.rs` changed with it: a shared file's bytes, refused to a guest
  until now, are given. The protocol stays `qurb/2`: a computer from before
  this answers `Shared` *not found*. Nothing in the app uses it yet.
