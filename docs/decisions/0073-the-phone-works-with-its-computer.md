# 0073 — The phone works with one computer: no shared files on it, and it may change its own folder there

**Status:** Accepted as the design, 2026-10-24. **Nothing is built.** The
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
- **A phone stops holding the shared area.** What happens to shared files
  already on a phone at the upgrade, which today has the S23's test files and
  nothing else, is to be decided when it is built.
- **Real time** means while both are connected. With the computer off, its
  folder cannot be opened, and nothing is queued for later.
