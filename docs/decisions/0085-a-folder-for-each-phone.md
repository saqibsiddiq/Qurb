# 0085 — `~/qurb` holds Shared and a folder for each phone

**Status:** Accepted and built, 2026-11-10 (`999486a`, `1e1af78`), at the
owner's word during the S23 pass ([testing.md](../testing.md), pass 4).
Tried on the owner's laptop and S23: the folder opens straight at the phone's
contents. Amends [0083](0083-files-open-in-the-file-manager.md) and
[0067](0067-a-folder-for-each-person.md). Per-phone approval built
2026-11-12: shown by a test; on the S23, the only phone, only its own yes can
be seen.

## What was asked

The owner, 2026-11-10, after opening his folder and finding *Saqib* (his
login's name, no picture) holding *sakib* and *My computer*: "qurb folder has
shared and sakib, sakib should be the folder in which the phone is backing up
everything, if another phone device adds in qurbspace, it should make another
folder named after that device; also my computer is [not] neccessary, delete
it." Asked to confirm: stop showing *This computer* but keep its files, yes;
a guest's phone gets its own folder, yes; each phone asks for its own folder
only, yes.

## Decided

- **One folder a phone.** `~/qurb` holds `Shared` and, for each of the
  owner's phones, a folder named as the phone is named in QurbSpace; its tile
  in Files carries the phone's picture. Opened, the phone's WhatsApp, chosen
  folders and Private Vault are at its top (`Contents::Phone` in
  `crates/desktop/src/mount.rs`); an earlier setup of the same phone stays
  inside it, *…, earlier*. A guest's phones get folders of their own, sealed,
  as before.
- **This computer is not shown.** The laptop's own private vault no longer
  appears in any folder. Its files stay in the vault; nothing was deleted.
- **A folder left mounted by a QurbSpace that stopped** (killed, crashed,
  replaced by an update) fails every look with "Transport endpoint is not
  connected". It is lazily unmounted (`fusermount3 -uz`) whenever the folders
  are laid out (`mount::clear_stale`).
- **An open phone folder is read once.** Each look the file manager made
  read everything kept for the phone again, and the owner found it "very
  slow". The whole tree is now built by one read
  (`qurb_storage::person::tree`) and answered from for thirty seconds. Not
  measured: the owner's report is the evidence of the slowness, and the
  owner's look after the change has not happened yet.

- **Each phone opens only its own folder** (2026-11-12). An ask for a
  phone's folder is filed under that phone -- the one in use now, following a
  folder's newest identity to whatever replaced it -- and the computer
  answers *Asks* and *Unlock* from one of the owner's phones about that
  phone's own folder only, knowing the phone by its certificate. No change to
  the wire: the phone never named a folder, and still does not need to.
  Another phone of the owner's is not asked, and its yes opens nothing
  (`a_phones_folder_opens_with_a_yes_from_that_phone_only` in
  `crates/peer/tests/guests.rs`). A phone's previews, files, search results
  and Recently deleted show on the computer only while that phone's folder is
  open. The phone asks "Open this phone's folder on …?".

  It was one yes for all, filed under the computer: until then any of the
  owner's phones approving opened every one of them.

## Left undone

- A guest's folder is still one for all their phones, sealed with their
  person's key, and any of their phones opens it; the phone's question says
  "this phone's folder" there too, which is not quite so for a guest with
  two phones.
- A phone folder whose phone is gone for good -- removed, never set up again
  -- can no longer be opened: no phone is left to say yes. Before, any other
  of the owner's phones could.

Measured 2026-11-12 on the owner's laptop, with everything kept locally and
the page cache warm: the S23's folder, 74,745 entries, walked in 2.0 to 2.9 s
([testing.md](../testing.md), pass 4). No figure was taken before the change.
