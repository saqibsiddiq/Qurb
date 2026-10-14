# 0067 — A folder for each person, opened with their phone

**Status:** Accepted as a design and an order of work, 2026-10-14, from the
owner's items 2 to 5 of that day and his answers. Step 1 built the same day,
not yet seen with the S23; steps 2 to 4 not built. See *Progress*. Extends
[0060](0060-a-computer-keeps-private-folders-for-several-people.md), which
built most of this for guests, to every person a computer keeps files for,
its owner included, and answers 0060's question 3 ("the owner's own files:
an ordinary folder, or locked like a guest's?") the other way.

## What was asked

The owner's list of 2026-10-14:

> 2. there must be a folder named 'qurb' in my home directory, that folder
>    contains all the users folders, users folder must contain all the
>    backedup data which the desktop user can access if approved by the the
>    users through the phone.
> 3. user can either send data in the download folder which the desktop can
>    access without any authorization or in the user folder which requires
>    user authorization through phone.
> 4. files section in qurb desktop must feature all the user folder, the
>    desktop can navigate and access the data similar as discussed in the
>    point 2 and 3.
> 5. files section in android app can be used to access desktop user folder
>    if the desktop is online, mobile can choose to restore or download any
>    data during this

His answers, asked the same day:

- **Only user folders in `~/qurb`**; today's shared files move to
  `~/qurb/Shared`.
- **Opened inside Qurb only**: a file is decrypted when it is opened, nothing
  readable is left on disk, and `~/qurb/<person>` stays a locked placeholder.
- **One folder per person**, each of their phones a folder inside it.

## What exists

From 0060, for **guests** (another person's phone visiting a computer): a
folder kept sealed under names and keys the computer cannot read; *Open
their folder…* in the window, approved on the guest's phone behind its
fingerprint, face or screen lock (`Request::Asks`, `Request::Unlock`); files
unsealed into `$XDG_RUNTIME_DIR/qurb-open/` when opened and deleted on
locking; ten minutes idle locks it. On the guest's phone the share sheet
offers *Save to my folder on Saqib's laptop* or *Send to Saqib's laptop's
Downloads*, and the Private Vault lists what the computer keeps.

For **the owner's own phones**, the computer keeps the phone's Private Vault
(0036, 0053), its WhatsApp (0062) and its chosen folders (0063). The window
shows a phone's backups from its panel in Devices, openable at once, with
no approval: the computer holds the same key as the phone.

## Decision

**Everyone a computer keeps files for has a folder, the computer's owner
included, and each is opened only with a yes from one of that person's
phones.** In the window's Files, at the top: *Shared*, the folder that
copies to every one of the owner's devices, and one folder per person.
Inside a person's, one folder per phone, and inside that what is kept for
it: its Private Vault, WhatsApp, the folders it backs up, and what was sent
there.

### Steps, each shippable

1. **Files shows the people** (items 2 and 4, inside Qurb). *Shared* and a
   folder per person at the top of Files. A person's folder is locked. *Open*
   asks their phone, which shows *Open your folder on saqib?* behind its
   lock, as a guest's does now; approved, the window lists each phone's
   files by folder, and opening one decrypts it into the runtime directory,
   as a guest's file is. Locking, by hand, after ten minutes idle, or on
   quitting, deletes what was opened. A phone's backups stop opening from
   Devices without that yes.
2. **`~/qurb` holds people** (item 2, on disk). The synced folder moves from
   `~/qurb` to `~/qurb/Shared`, its index with it, once, while Qurb is
   stopped; the paths inside it, and so every other device, are unchanged.
   Beside it, `~/qurb/<person>/` for each person, holding nothing but a note
   saying the folder opens in Qurb.
3. **Sending, two places** (item 3). On the owner's phone, as on a guest's:
   *Send to saqib's Downloads*, which the computer's person opens freely, or
   *Save to my folder on saqib*, which goes into the phone's folder there and
   is opened only with the phone's yes. Not kept on the phone once the
   computer has it, as for a guest.
4. **The phone reaches its folder on the computer** (item 5). In the phone's
   Files, *On saqib* while the computer is reachable: the folder as the
   computer keeps it, deleted files included, each file *Download*ed to the
   phone's Downloads or *Restore*d where it was.

### Naming

The owner's folder is named after the login's name (its first word, from
the account's full name, else the user name). A guest's after the first of
their devices to visit, until it is renamed. A phone's folder inside is the
phone's name. Names are shown, never used to decide anything: a person is
the identifier 0060 files their folder under.

## The owner's folder is not sealed, and says so

A guest's folder is sealed: the computer cannot read it, cryptographically.
**The owner's is not.** The computer and the owner's phone hold the same key
(0052), and the computer reads the phone's vault to keep it, check it and
serve it back. The phone's yes is Qurb's rule, enforced by the window and
the commands it calls, not by the key: anyone at the computer's login with
the command line, or reading the index, can still see names and, with the
key on that disk, contents. Sealing the owner's phones from the owner's own
computer would mean the phone's vault kept under a key only the phone
holds, as a guest's is: the 10 GB already kept read and sealed again, and a
second phone of the same person unable to read the first's. Not now; said
in the window where it matters, and recorded here.

## Costs, accepted

- **Opening needs a phone at hand and online.** A person whose phone is off,
  lost or flat cannot open their folder at the computer. That is the point
  for a guest; for the owner it is a new step where there was none.
- **Moving the folder** changes a path people may have bookmarked or opened
  programs against. Done once, said in the window, and reversible by moving
  `~/qurb/Shared` back with Qurb stopped.
- **Nothing in `~/qurb/<person>` is readable by other programs.** The owner
  chose that: a file is opened through Qurb, which decrypts it into memory
  backed storage for the program that opens it.

## Rejected

- **A FUSE mount at `~/qurb/<person>`**, decrypting as programs read. It
  would make the placeholder a real folder; it would also put a file system
  in the kernel's path whose failure hangs every program that touches it,
  the risk 0025 refused for placeholders. And it would leave contents
  readable by any program while open, which the owner's answer rules out.
- **Writing a person's files out into `~/qurb/<person>` while open.** The
  owner's answer: nothing readable left on disk.

## Progress

### Step 1, Files shows the people — built 2026-10-14

- **Files' top** is *Shared* (*On all your devices*) and a folder for each
  person: the owner's, named from the login (*Saqib*), with how many phones
  and how much is kept; each guest's, named after their first device. A
  locked folder shows a lock.
- **Opening** asks the person's phone (`ask_person_folder`, then *Sync now*,
  which wakes the phone). The owner's phones are asked, and answer, as a
  guest's are: the computer files the ask under its own device, a phone of
  its person that it keeps the vault of collects it during sync
  (`Request::Asks` now answered to the owner's devices too), and its yes is
  `Request::Unlock` with no key, since the computer holds the same one. The
  phone says it plainly (*keeps your backups, your WhatsApp and your
  Private Vault, and shows them only after you say yes here*) rather than a
  guest's *sealed*. Heard in the background, the ask is a notification;
  opened, the app asks behind the screen lock at once.
- **Open**, the owner's folder lists each phone (one set up again keeps its
  own folder, *…, earlier*), and in each *WhatsApp*, the backed-up folders
  under their own names, and *Private Vault*. A file opened is written,
  checked chunk by chunk and whole, into `$XDG_RUNTIME_DIR/qurb-open/`
  and handed to its program; locking, ten minutes idle, or quitting deletes
  it. A guest's folder opens as 0060 built it.
- **Devices no longer opens backups without the yes**: `phone_backup` and
  `open_backed_up` refuse while the owner's folder is locked, and the
  backup sheet's *Open* goes to the folder in Files.

Tests: `the_owners_folder_opens_with_a_yes_from_the_owners_phone` (a guest
is not asked about it and cannot open it; a wrong nonce opens nothing).
Looked at in the fixtures page: the top, a locked folder, an open one. The
real-window smoke test opens Files with the new top. **Not yet seen**: the
ask reaching the S23 and its yes opening the folder on the laptop.

## Reversing it

Step 2 is the one with a lasting effect on disk, undone by moving
`~/qurb/Shared` back with Qurb stopped. The rest is how the window and the
phone present what is already kept.
