# 0067 — A folder for each person, opened with their phone

**Status:** Accepted as a design and an order of work, 2026-10-14, from the
owner's items 2 to 5 of that day and his answers. All four steps built the
same day; step 2 run on the owner's laptop; step 1 run between the S23 and
the laptop on 2026-10-15, after two fixes found doing it; steps 3 and 4 not
yet seen working on the S23 (step 4 failed there, and is fixed). See
*Progress*. Extends
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
   is opened only with the phone's yes. (Designed as not kept on the phone
   once the computer has it; built keeping it -- see *Progress*.)
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
real-window smoke test opens Files with the new top.

**Run with the S23, 2026-10-15**, the laptop's real window driven through
WebKit's WebDriver and the owner at the phone. The first two asks never
showed on the phone, for two reasons:

- **Heard, and not shown.** The window's ask woke the phone; the phone's
  background worker synced and collected the ask (`asking=true` in its
  log), and, the app being on screen, left it to the screen, which only
  looked for asks after a sync of its own. Now the screen looks when a
  worker's pass ends, and every two seconds while one runs.
- **Not heard for half an hour.** The phone was sending the laptop its
  WhatsApp backup, a long pass that stays to be collected from for up to
  thirty minutes; a wake while it runs starts nothing, and asks were
  collected only at a pass's start. Now a pass staying to be collected from
  asks each computer it reached again every ten seconds, over the
  connection it holds (reached again if that closed), and sends a yes the
  same way, rather than at the next pass (`trade_asks`, `ASKS_EVERY`). The
  worker raises the notification for an ask while the app is away.

With both: asked at 10:55:34 from the window, during the WhatsApp send; the
S23 showed *Open your folder on saqib?* within six seconds; the owner
approved behind the screen lock; the folder was open in the window at
10:55:55. Open, it listed *SM-S911B* (*DCIM*, *WhatsApp*, with that
morning's 525 MB message database) and *SM-S911B, earlier* (*WhatsApp*,
*Private Vault* with 5 files), as the tests expect. **Not checked**:
opening a file from it in the window -- the owner locked the folder before
that was tried. The tile said *2 phones* for the S23 and its earlier self
and cut off its last word; it now counts phones in use, lists them first,
and leaves locked or open to its icon.

### Step 2, `~/qurb` holds people — built 2026-10-14

`qurb_cli::layout`. A folder at the default place, `~/qurb`, still of the
old shape -- set up, its index at its top -- is moved into `~/qurb/Shared`
when the window, the tray or `qurb run` starts, before anything watches it:
every entry, the index last, each a rename, undone as far as it got if one
fails; the list of known folders then names `Shared`. A folder somebody
chose elsewhere is not moved. New folders are made at `~/qurb/Shared`.
Beside it, a read-only placeholder for each person, holding only
*Opens in Qurb.txt*, which says whose it is and how to open it; a
placeholder holding nothing but its note goes when its person does, and
nothing else in `~/qurb` is touched. *Show the Qurb folder* opens `~/qurb`.

Run on the owner's laptop at 20:51 on 2026-10-14: `~/qurb` held only its
index (9.4 GB; the 18 shared files freed, not on disk) and became
`~/qurb/Shared/.qurb`, with `~/qurb/Saqib/` beside it; `qurb status` read
the moved folder, both phones still paired, and the daemon reconciled with
nothing stored or deleted. Nothing in the index or the config named
`~/qurb` absolutely (checked: no send read from inside it), so nothing else
had to change. Tests: `a_folder_moves_into_shared_once`,
`placeholders_follow_the_people`.

### Step 3, sending, two places — built 2026-10-14

On the phone, Home's *Send* lists, for each computer that keeps the phone's
folder, *My folder on saqib* (*Opens there only with a yes on this phone*)
and *saqib's Downloads* (*Opens there freely*), then every other device as
before. The share sheet says the same: *Save to my folder on saqib* and
*Send to saqib's Downloads*. My folder is the phone's Private Vault, which
that computer keeps, and shows under the phone in the person's folder.

**Changed from the design:** the owner's phone keeps its own copy of what it
saves to its folder, as its Private Vault always has (0036), rather than
letting go of it once the computer has it as a guest's phone does. Letting
go of the owner's own files without being asked would surprise him; *Free
local space* does it by hand.

**Not seen** on the S23.

### Step 4, the phone reaches its folder — built 2026-10-14

**A phone sees its own, not its person's.** A phone's Private Vault is
private to that phone (0036), so from the phone the folder is what the
computer keeps for *that* phone, and for its earlier selves: the same phone
set up again, recognised as Devices does (same name and kind, paired after
the older was last seen; `Db::earlier_selves`). The window, opened with a
yes, shows every phone of the owner's. That is the S23's case: the first
WhatsApp backup, 3.7 GB, is kept for the S23 from before its data was
cleared, and is reachable from the S23 now.

- **One tree for both**: `qurb_storage::person` maps what is kept to where a
  person looks for it (*WhatsApp*, a backed-up folder under its own name,
  *Private Vault*), for the window and for the phone alike.
- **`Request::Kept { dir }`**, answered to a phone of the computer's own
  person only, with that place's folders and files, each with its content
  hash. Fetching uses the ordinary manifest and chunks; content kept for an
  earlier self is now visible to the phone that replaced it, and to no other
  device.
- **On the phone**, Files' top has *Your folder on saqib* (*Backups,
  WhatsApp, deleted files · while it's on*). It reaches the computer once,
  on a port of its own so as not to collide with a sync pass, keeps the
  connection while the folder is open, and closes it when it is left. A
  file offers *Download* (to Downloads/Qurb) or *Restore*: WhatsApp's back
  into WhatsApp's folder, a backed-up folder's back where it was (both need
  "All files access"), the Private Vault's back into it. A file already
  where it is going is not replaced; the fetched one goes beside it. Each
  is checked chunk by chunk and whole before it is put in place.

Tests: `a_phone_reaches_its_folder_on_the_computer_and_what_it_kept_before`
(its own and its earlier self's listed and fetched; another device of the
same person shown neither, and refused the content by hash);
`kept_paths_show_where_a_person_looks_for_them`.

**On the S23, 2026-10-15, it failed every time**: opening the folder
panicked in the engine ("there is no reactor running"), a timeout built
outside the runtime it was meant to run on. The emulator had shown the same
panic as *isn't reachable*, which was taken for its computer being off; the
tests above go through the peer crate, not the engine the app calls, so
none opened the folder as the phone does. Fixed, and
`a_phone_opens_its_folder_on_the_computer` now does: through the engine,
opened, listed from the top, a file fetched and checked, a second fetch put
beside the first. It fails with the same panic without the fix. **Not yet
seen working on the S23.**

## Reversing it

Step 2 is the one with a lasting effect on disk, undone by moving
`~/qurb/Shared` back with Qurb stopped. The rest is how the window and the
phone present what is already kept.
