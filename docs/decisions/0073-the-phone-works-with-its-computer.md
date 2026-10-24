# 0073 — The phone works with one computer: no shared files on it, and it may change its own folder there

**Status:** Accepted as the design, 2026-10-24. Built on Android the same
day: every step of the plan below -- the shared folder read, written and
searched; the phone's own folder there changed from the phone; the phone out
of the shared area; and Files, drafts 3 and 4 -- with the gaps under
*Progress*. The desktop window is not changed yet, and the owner's own S23 and
laptop do not run it yet. The screens are in the Figma redesign
([design/redesign.md](../design/redesign.md), *Files, draft 2* onwards). Amends
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

- **Engine and protocol work**, first written as none of it begun; built
  2026-10-24 (*Progress*, below) but for the last:
  - requests that change files, with their checks;
  - a search request answered by the computer;
  - the device chosen on Home, as a setting;
  - a backup set for each device (item 4 of the redesign): not built.
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
- **Step 2, the shared folder written, built 2026-10-24.** `SharedMkdir`,
  `SharedMove`, `SharedDelete`, and `Upload` in pieces of at most 4 MiB then
  `UploadDone` with the file's hash, answered `Noted`, `Placed { path }` or
  `Refused { why }` (`qurb_storage::shared_folder`). A folder moves and is
  deleted file by file; one with a sharing rule in it is refused, moved and
  deleted at the computer only. Four storage tests
  (`storage/tests/shared_changes.rs`): a damaged file is not put, a taken name
  is numbered; nothing hidden, outside or shared without the asker, `/etc`
  included, which the first build took as inside the folder; a taken name and a
  folder into itself refused; a folder moved and deleted, Recently deleted
  naming the phone. Three over a connection: a guest makes a folder, adds a 9
  MiB file in three pieces, renames, moves and deletes; refusals say why; the
  owner's computer and a computer visited may change nothing, the owner's phone
  may. 924 tests in the workspace pass.
  **Not done:** only a deletion is recorded as the asking device's; an added
  or renamed file reads in the computer's Activity as a change made there. No
  limit on how much a guest may add but the disk's own. Pieces of an upload
  nobody finished wait in `.qurb/incoming` until the next upload starts a day
  later. Nothing in the app uses it yet.
- **Step 3, searching, built 2026-10-24.** `Search { within, query }`, the
  shared folder or the asking phone's own folder, to whoever may list it:
  files whose place holds the words, ignoring case for ASCII only as the
  phone's own search does, the newest first, at most 500, each named by its
  place. Tested: a guest finds files at any depth and none in a folder shared
  without it, and has no folder of its own person's to search; the phone finds
  its file kept from before it was set up again, and a computer of the same
  person finds none of it (`tests/shared_folder.rs`, `tests/holding.rs`).
- **Step 4, a phone's own folder changed from the phone, built 2026-10-24.**
  Each change is the phone's own vault changing, carried to the computer as
  its other changes are, and the app syncs at once after each (for at most
  15 seconds), so the computer follows while both are connected. Rename and
  move are `rename_file`; a file whose bytes were freed to the computer, as
  most there are, is renamed as a new version pointing at the same chunks and
  the old path deleted (`Store::rename_freed`), so not a byte crosses either
  way. Delete is `delete_kept`: this phone's own files only, into the
  phone's own Recently deleted, and a backup only once its original is gone
  from the phone -- refused with why while it is still there. Adding files is
  the Vault's own import, into the folder on screen.
  **A deletion without the bytes, fixed the same day.** The emulator check
  below found that deleting a file whose bytes were freed to the computer put
  nothing in Recently deleted -- only a file on the phone's disk went there --
  though the app said it would, and the computer let go of its copy once its
  seven-day window and the chunks' own had passed. Now such a file, or a
  backup's read where it lay, goes into the phone's Recently deleted as an
  entry of no bytes (`Store::delete_to_trash_by`), which emptying Recently
  deleted to make room passes over. Restoring it puts the deleted row's chunk
  list back at its path as a new version (`Store::restore_freed`; a backup's
  into the Vault by its own name), which the computer holds again from the
  chunks it kept. For that, every device keeps a vault's deletions at least
  as long as Recently deleted keeps a file, 30 days (`gc::collect`); the
  shared area's keep the seven days given, and a window of zero still
  collects everything.
  **Undone at the computer, as item 3 has it, built the same day.** A
  Private Vault file deleted on the phone is in the computer's Recently
  deleted too, naming the phone (`Store::unhold`), so that a mistake, or a
  lost phone in the wrong hands, can be undone there. Restoring it there
  (`Store::restore_held`, `qurb restore`, the window's Restore) puts it back
  in the phone's folder on the computer as a version of the computer's after
  the phone's deletion: the deletion, offered again by a phone not yet told,
  is not taken again (`holding` compares the two). The phone takes it back
  at its next sync -- a new step, `Action::Restored`, planned only for a path
  whose last word on the phone is a deletion of that very content, by a
  version after it (`Store::restored_elsewhere`), so nothing else a computer
  offers becomes a file of the phone's -- from the bytes its own Recently
  deleted kept, or left on the computer as before, and out of the phone's
  Recently deleted. Either restore empties the other side's entry. Deleting
  it for good on the computer lets the bytes go at the next collection. Not
  a backup: WhatsApp's and the folders' files go when their original goes
  from the phone, as the owner chose, and listing those would bury the rest;
  one deleted from the phone's folder is in the phone's own Recently deleted.
  Nor a guest's, sealed. **A gap:** the deleted row on the computer is dated
  by the phone's clock, as every row from another device is, so a deletion
  that reached the computer days late, or from a phone whose clock is wrong,
  is kept less than the 30 days its entry shows -- restoring it then says
  the file is no longer there. **And another:** a different file the phone
  makes at that path before it next syncs is the phone's word, and replaces
  the one restored on the computer, which is then lost there. Tested: listed, restored with a version after
  the deletion, deleted for good and collected, a backup's and a guest's not
  listed, the phone's own restore emptying the entry, the phone taking back
  a freed file and one whose bytes it kept and nothing else
  (`tests/held_deletion.rs`); the computer keeping the restored file while
  the phone offers its deletion again, then both settled
  (`crates/engine/tests/holding.rs`); through the engine as the app runs it
  (`a_phone_changes_its_own_folder_on_the_computer`); and on the emulator,
  the command line built for Android as the computer, paired fresh and
  keeping the Vault: *letter.txt* added, freed, deleted from the phone's
  folder there; `qurb deleted` listing *sdk_gphone64_x86_64/Private
  Vault/letter.txt deleted on sdk_gphone64_x86_64*; `qurb restore` saying
  the phone takes it back at its next sync; the phone's Recently deleted
  listing it until that sync and not after, the Vault listing it again
  (*Available elsewhere*), Activity saying *restored* by the computer, and
  its 23 bytes downloaded from the computer intact. The window's Restore was
  not run.
  Tested: a freed file renamed into a new folder, the same chunks, nothing
  written, the old path gone (`tests/freed_rename.rs`); a freed file deleted,
  listed in Recently deleted, its row outliving the seven days and gone after
  30, restored on its own chunks, and a backup's restored into the vault by
  its name (`tests/freed_deletion.rs`); through the engine as the app calls
  it, the phone's folder on the computer following a rename into another
  folder, the file freed and deleted, gone there, restored from the phone's
  Recently deleted and listed there again, and a path not the phone's own
  refused (`a_phone_changes_its_own_folder_on_the_computer`); and in the app
  on the same emulator, the whole round trip, the restored file's bytes
  downloaded from the computer afterwards. **Not tested:** a backup's
  deletion refused while its original is on the phone.
  **Not built: a new folder.** A folder with nothing in it is not a thing the
  index can hold -- it lists files, and a folder is where they are -- so an
  empty one made on the phone would exist nowhere but on that screen. Moving
  a file into a new name's folder makes it.
- **Step 5, the phone out of the shared area, built 2026-10-24.** An
  engine switch, `shared_area`, on by default and off on a phone
  (`Settings::shared_area`, false unless set): a sync takes none of another
  device's shared files, nothing added goes into the shared area, and moving
  a file out of the Vault is refused. On each opening with it off,
  `Store::leave_shared_area` does what the owner chose: each shared file a
  computer keeps is forgotten on the phone -- row and bytes, no tombstone, so
  nothing is deleted anywhere else and it stays in that computer's Shared
  folder; one only the phone has, or another phone, moves into its Vault (a
  phone's copy is not a safe last one, decision 0053); one with neither its
  bytes here nor a computer known to keep it is left, to be tried again at the
  next opening. **Bytes on the phone go only on the computer's word,
  asked**, changed before it reached the S23: the owner's laptop, about to be
  updated, turned out to hold none of its 17 shared files -- freed on
  2026-09-29, before decision 0053, on the strength of the S23's copies
  before the S23 was set up again -- and a phone re-checks a computer's copy
  only of a file it freed itself, so the S23's record of the laptop keeping
  a file may be stale and its own copy the last. So a file whose bytes are on
  the phone is let go of only once the computer, asked at a sync, has said it
  holds it (`Db::confirmed_safe_elsewhere`); until then it is left as it is,
  and a phone's sync asks about such files (`unconfirmed_holdings`, 16 a
  pass), a no moving it into the Vault at the next opening. A row whose bytes
  are not on the phone still goes on the record alone: forgetting it loses
  nothing. The area's deletions and sharing rules are forgotten, and
  folders left empty removed -- any empty folder in the phone's folder, not
  only those this emptied, which a phone could not make anyway. A file the
  phone moved into its Vault that a computer had without the phone knowing
  stays in that computer's Shared folder as well: two copies, neither lost.
  Logged once, as *left the shared area* with the three counts. Tested
  (`tests/leaving_shared.rs`, the stale record included, `crates/engine/tests/out_of_the_shared_area.rs`,
  `a_phone_leaves_the_shared_area`). The engine's own tests that stand in for
  a desktop with this engine set it on.
- **Step 6, the app, begun 2026-10-24: the Shared page** (Files draft 4).
  The phone's engine reaches the shared folder through the connection it
  opens to a computer for the phone's own folder there (`KeptFolder::shared`,
  `search`, `make_folder`, `rename`, `delete`, `add_file`), with
  `QurbError::Refused` carrying the computer's reason; tested through the
  engine as the app calls it (`a_phone_changes_the_computers_shared_folder`).
  Android: *Shared* in Files for each of the owner's computers and each one
  visited, and its page (`SharedScreen.kt`). Checked on the emulator against
  the command line built for Android, paired as the phone's own computer: the
  folders, an empty one included; a folder made; a 6.3 MB picture added from
  the picker, its bytes the same there; renamed; found by searching *PASTA*;
  the folder deleted, and the computer's `qurb deleted` naming the phone; a
  file opened. On the S23 the same day, as a guest of a throwaway computer on
  the laptop: listed, a folder made, a picture added, renamed, found,
  deleted, Recently deleted there naming the S23. The owner's laptop runs a
  Qurb from before this, whose Shared the S23 cannot open until it is updated.
- **Step 6, the rest of Files, built 2026-10-24** (drafts 3 and 4).
  `FilesHome.kt`: *This phone* as three tiles -- Vault, Received, Deleted --
  then a *Shared* card for each computer, then *Devices*, each of the
  phone's own computers leading to its folder there (`KeptScreen.kt`),
  where the Private Vault part has *Add*, and a file of it *Rename*, *Move
  to folder* and *Delete*. *Received* (`ReceivedScreen.kt`) lists what was
  sent to the phone, the newest first, with who sent it, read from the last
  500 entries of the history. Settings lost *Who has each folder* and *Keep
  new files private*, a file's sheet *Move to Files* and *Move to Private
  Vault*, and Sharing into Qurb its choice of area: all of it the shared
  area's. The Files pages lost their `·` joins: a file is its name and size,
  where it is the icon before the size; Recently deleted has each file's
  days left on a clock and Restore as its mark. Checked on the emulator
  (`qurb-34g`, the debug build), set up from nothing, the command line built
  for Android running on it as the phone's own computer and keeping its
  Vault: Files home as drawn; *diary.txt* added to the Vault from the
  picker; the phone's folder there listing it, renamed *journal.txt* from
  that page and listed so, then deleted from it, the folder empty; the
  phone's own Recently deleted listing it with 30 days, restored, back in
  the Vault; a file sent by `qurb send` from the computer listed in Received
  under the computer's name; Shared opened from its card. The phone's folder
  page was given *Search* after that, item 4's search of the whole folder by
  the computer (`KeptFolder::search`, step 3), and checked on the same
  emulator with a second computer paired: *journal* found; moved into *Notes*
  with *Move to folder* and found there; *soup.txt* added inside *Private
  Vault › Notes* and there on the computer. Deleting it is where the fault
  under step 4 showed.
  **Not done or not checked:** a new folder in the phone's folder (step 4);
  a backup deleted there; a phone that held shared files leaving the area, on
  a device;
  counts on the tiles and cards, which the design shows and nothing yet
  provides; a page open when a sync brings something new keeps what it
  showed until opened again or pulled, as every page in the app does;
  Received misses a send older than the last 500 happenings; `FilesScreen.kt`
  still carries the shared area's browsing, which nothing opens now; the
  desktop window; and the S23 and the owner's laptop, which run builds from
  before this.
