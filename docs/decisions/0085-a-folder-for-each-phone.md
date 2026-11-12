# 0085 — `~/qurb` holds Shared and a folder for each phone

**Status:** Accepted and built, 2026-11-10 (`999486a`, `1e1af78`), at the
owner's word during the S23 pass ([testing.md](../testing.md), pass 4).
Tried on the owner's laptop and S23: the folder opens straight at the phone's
contents. Amends [0083](0083-files-open-in-the-file-manager.md) and
[0067](0067-a-folder-for-each-person.md). **Per-phone approval is not built:**
see *Left undone*.

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

## Left undone

- **One yes still opens all the owner's phones' folders.** The ask carries no
  phone, so any of the owner's phones approving opens every one of them. Each
  phone opening only its own needs the ask to name the phone, on both the
  desktop and the phone: a protocol change, not yet made.
- The folder speed after the change has not been seen by the owner.
