# Turning the engine into a product

A plan written against what the repository actually contains, not against what
the architecture document describes. Read
[CODEBASE.md](CODEBASE.md) first; this assumes it.

Section numbers such as §23 refer to the product brief this plan answers ("qurb
— Full Product Implementation Master Prompt"). The brief was issued again on
2026-09-25 and the whole plan was checked against the code that day. Section 2
is the result.

**Status, 2026-09-28: every feature the brief asks of the two apps is built.**
Steps 0–6, 8 and 9 of the sequence are done, except replica eviction; step 10
is done for security, packaging and versions; step 7 is done between one phone
and one laptop, over Wi-Fi and mobile data, and not between two phones or
through a relay on a server. [features.md](features.md) is the inventory,
marked by how far each piece has been checked. The data model is settled and
built — see [decisions/0029](decisions/0029-two-areas-shared-and-private.md)
and [0036](decisions/0036-a-phone-keeps-its-own-files.md) — and 0041–0047
record what was decided since.

**What comes next, as the owner set it on 2026-09-28:** the design and UX of
both apps, then the relay on a server, then a formal release for Linux and
Android. The design is built directly in the apps, from the owner's direction
([design/direction.md](design/direction.md)); its brief, with every choice the
owner made, is [design/brief.md](design/brief.md)
([0048](decisions/0048-the-design-direction.md)). Both apps were built to it
on 2026-09-29, in light only, and wait for the owner's look; the Android app's
was walked on the S23 on 2026-10-03. Dark mode follows the review. On 2026-10-03 the owner asked to focus on completing the project; §13
is the list, checked off as it goes.

**Four things the brief asks for disagreed with decisions already recorded.**
All four were decided on 2026-09-25, each the brief's way — see §3. Three are
new decision records, all since built:
[0036](decisions/0036-a-phone-keeps-its-own-files.md) (a phone's files are its
own, and another device holds them for it),
[0037](decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md) (a file
sent to a desktop is an ordinary file in Downloads) and
[0038](decisions/0038-the-storage-question-during-setup.md) (the storage
question during setup). Windows stays out of scope.

---

## 1. The thing that had to be settled first

The product specification and the implemented engine described **two different
data models**, and the difference is not cosmetic.

### What the engine did

One namespace, shared by every paired device.

```
        laptop                phone
          |                     |
          +------- one converging tree -------+
                  every path, everywhere
```

Verified in the source at the time, not inferred:

| claim | where |
|---|---|
| A syncing device wants every path | `Role::Syncing => true` — `crates/engine/src/role.rs` |
| A peer is sent the whole tree | `Request::Tree => Response::Tree(store.tree()?)` — `crates/peer/src/server.rs` |
| `tree()` is everything the device knows | `Store::tree` → `Db::all_versions` |
| Any chunk is served to any paired peer | `Request::Chunk` has no access check |
| Authorisation is pairing, and nothing else | there is no ACL anywhere in `server.rs` |

So **every paired device saw every file and could fetch any content.** That is
the Dropbox shape: add a file anywhere, it appears everywhere.

### What the specification describes

Per-device private vaults, and explicit sends between them.

```
        laptop                         phone
          |                              |
   +------+------+                  +----+----+
   | Phone vault |  <-- send only   | own vault|
   | Tablet vault|                  +---------+
   +-------------+
        no browsing
```

Three rules from the specification that the engine could not express:

1. A device's vault is **private**: other devices may send into it and may not
   read it.
2. Sending is a **copy**, and the original stays where it was.
3. The desktop may know a phone's vault *exists* without being able to list it.

### Why this cannot be a UI layer

The specification forbids fake implementations (§80) and forbids exposing
private vault contents through a remote browsing interface (§51.6). Presenting
vaults as folders inside the shared tree would satisfy neither: the desktop
would still be able to read the phone's vault over the wire, whatever the UI
chose to draw. The privacy would be a drawing, not a property.

### The decision, made

**Both.** The shared area stays and behaves as it always has; private vaults
are added beside it, enforced in the protocol rather than in the interface.
[Decision 0029](decisions/0029-two-areas-shared-and-private.md) records why,
and the three audiences the first attempt got wrong.

The desktop shell is **Tauri**, which the roadmap originally called for.

That decision answered whether vaults exist. It did not answer what goes into a
phone's vault by default, and the brief's acceptance test turns on exactly
that — see §3.1.

---

## 2. What exists, sorted the way the brief asks

The brief asks for everything to be sorted into eight kinds before anything is
built. Each entry here was checked in the source on 2026-09-25, not taken from
another document, and is kept as the record of where the work started. What has
changed since is in [§2.9](#29-since-then); a row here is not current on its
own.

### 2.1 Built, and a person can reach it without a terminal

| brief | what exists | where |
|---|---|---|
| Identity and 24 words (§34) | created, shown, three words confirmed, never written down | desktop setting-up flow; `crates/desktop/src/session.rs` |
| Pairing (§8, §9) | QR, typed code and spoken form, with a countdown; enter a code from another device | desktop Devices screen; Android scans or types |
| Sending one file (§11) | drop on the window or choose, then pick a device | desktop Send screen, `send_file` |
| Where a file's contents are (§19, §30) | here / not here / only here | desktop Files screen, `view::Availability` |
| Getting a freed file back (§30, §59) | a durable request, acted on when a device is reachable | desktop Files screen, `fetch` |
| Search (§31) | by name | desktop, `View::search` |
| Devices (§10) | who is paired, when each was last reached | desktop Devices screen; Android peers dialog |
| Activity (§27) | what happened, paged, with the reason | desktop Activity screen |
| Storage allowance (§28) | usage and a control that changes it | desktop Storage screen; the tray window's slider |
| Notifications (§45) | three: a file sent to you, one collected, one that failed | `crates/desktop/src/notify.rs` |
| Share sheet (§39) | anything on the phone into qurb, with no network | `ShareActivity.kt` |
| System file picker (§40) | the files, in Android's picker | `QurbDocumentsProvider.kt` |
| Background sync (§38) | WorkManager, and push when configured | `SyncWorker.kt`, [decision 0028](decisions/0028-waking-a-sleeping-device.md) |

### 2.2 In the engine, with no interface — or only a command

| capability | what exists | what is missing |
|---|---|---|
| Sending, from a phone (§20, §21) | the same send path the desktop uses | no FFI function |
| Freeing a chosen file's local copy (§14) | `Store::evict`, which refuses without another known holder | only the storage cap calls it; no command, no button, on either platform |
| Removing a device (§35) | `Db::forget_peer`, exercised in `crates/peer/tests/pairing.rs` | no command, no button |
| Activity, outgoing, availability, on a phone | `qurb_cli::View` | the phone does not link `qurb-cli`, so none of it reaches the FFI |
| Conflicts (§24) | both versions kept; a `Conflicted` event recorded | no query lists them and no screen explains them — a conflict copy looks like any other file |
| A passphrase on the key (§33) | `qurb protect` | the window cannot ask for one — [decision 0033](decisions/0033-the-phrase-on-a-screen.md) |
| Files sent *to* a phone | delivered and filed privately | the app's own list is `list()`, which returns only the shared area. After the fix in progress (§6, step 0) a received file shows in the system picker and not in the app |

### 2.3 Partly built

| thing | what is there | what is not |
|---|---|---|
| Transfers (§26) | outcomes, recorded and shown | a transfer in flight: no progress, speed, pause, cancel or retry. One file per send; no folders |
| Direct or relay (§36, §37) | the distinction exists in `connect.rs` and in the log | not in the daemon's status, so no screen can say it |
| Onboarding (§6, §71) | setting up and pairing in the window | the storage question — deliberately left out, see §3.3 — and the first send |
| Android app (§15–§22) | set-up, one file list, a menu, share, open, save a copy | the brief's six-section app: Home, Vault, Devices, Transfers, Activity, Settings. No phrase confirmation on the phone: `SetupActivity.kt` asks for "I have written them down" and checks none of the words |
| System file picker (§40) | works | walks the directory itself rather than asking the engine, so a freed file is simply absent and the engine's view of the file is not what the picker shows |
| Selective sync (§29) | `PinSet`, wired only to `Role::Replica` | an ordinary device takes everything |
| Storage cap (§28, §58) | works on a device with a folder | a replica cannot free anything — [decision 0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md) |
| Tray (§42) | `qurb-tray`: an icon, a menu, a window with a slider | it is a second front end. The applications-menu entry opens `qurb-desktop` since 2026-09-27 (decision 0040); the tray is installed alongside |
| Installing (§69) | `packaging/install.sh`, per user, with `--uninstall` | not a package; no autostart |
| Versions (§70) | schema migrations run at open; the wire protocol is versioned (`qurb/2`) and refuses a mismatch | no version shown anywhere; no update path |

### 2.4 Designed, not built

iOS; per-file keys; key rotation, which is what revoking a removed device's key
would need; relay selection and quotas; waking without Firebase
(UnifiedPush); upgrading a relayed connection back to direct; installers and
signed updates.

### 2.5 Undecided

What sharing means (§23). How a replica frees space. Which toolkit the rebuilt
Android interface uses.

On 2026-09-25 this list also held what a phone's vault is, where a file sent to
the desktop lands, the storage question in the first run, and Windows. All four
are now decided — §3.

### 2.6 Not possible as worded, and what to say instead

| the brief asks | the truth | what the product says |
|---|---|---|
| A remote file visible in the Linux file manager (§30, §59) | Linux has no placeholder API short of FUSE | the file stays in qurb's own listing, with *Download* |
| Background sync (§38) | Android decides when work runs; about fifteen minutes without push | "Android may delay background work" — never a promise |
| Vault privacy against your other devices (§3, §4, §33) | every device of one person holds the same master key ([0012](decisions/0012-key-hierarchy-and-recovery.md), [0023](decisions/0023-one-person-per-account.md)). A device that does not *hold* a vault's bytes cannot get them — the protocol checks — but a device that holds them could decrypt them | stated plainly wherever a device holds another's vault, see §3.1 |
| Removing a device (§35, acceptance 47) | it can be stopped being trusted. It keeps the key and whatever it already holds, until key rotation exists | "Phone A can no longer connect to your devices" — not "Phone A's access was revoked" |
| Two phones that are never awake together | they never meet unless something always-on is there | said, with the replica as the answer |

### 2.7 Can be exposed through the existing API, without engine changes

Each of these is a narrow wrapper over something that already refuses unsafe
requests by itself:

- **Phone:** send; fetch; free a file's local copy; activity; outgoing; devices
  with last seen; availability; search; the files in this phone's own vault.
  The queries live in `qurb_cli::View`, which the phone does not link, so they
  move down into a crate both link — they are not copied into the FFI.
- **Desktop:** free a file's local copy; remove a device; rename a paired
  device's local label.

### 2.8 Needs engine or protocol work first

- Direct or relay, per device, in the daemon's status.
- ~~A file sent to the desktop landing outside the store~~ — built,
  [0037](decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md).
- A phone's own files in its vault, and another device holding them for it — [0036](decisions/0036-a-phone-keeps-its-own-files.md), which lists the five engine changes.
- Selective sync for ordinary devices.
- A replica freeing space.
- Sharing between chosen devices (§23).
- Vault operations the owner performs — new folder, rename, move, delete.
  Today only a delivery creates a vault row.
- A query that lists conflicts.

### 2.9 Since then

Checked 2026-09-27. Built since the inventory above, each reachable without a
terminal:

- **The Android app, rebuilt** (§15–§22): Home, Vault, Devices, Transfers with
  the history, and Settings, on platform views
  ([0039](decisions/0039-a-light-android-app.md)). The phrase is confirmed at
  setup and can be shown again.
- **Sending from a phone**, to one device (§20, §21): the FFI's `send_file`,
  from a file in the Vault or files picked on the Devices screen.
- **Freeing a chosen file's local copy, and getting it back** (§14, §30), on
  both: the phone's Vault, `qurb free` and `qurb fetch`. The engine refuses the
  only copy. A freed *shared* file asked for on a phone was never downloaded
  until 2026-09-27 — the phone's sync lacked the step the daemon had added for
  itself; it is now part of `plan_with`, which both use.
- **Devices and history on a phone** (§10, §27): the Devices and Transfers tabs,
  including which device keeps the phone's files.
- **Files sent to a phone** show in the app: `list()` and the Vault cover the
  shared area and the phone's own vault.
- **The system file picker asks the engine** (§40): a freed file is listed and
  downloads when opened, and another app can save into qurb.
- **Transfers in flight on the desktop** (§26): progress, several files and
  folders per send, cancelling a send not yet collected.
- **Direct or relay** (§36, §37): in the daemon's status and on the Devices
  screen.
- **Onboarding** (§6, §71): the storage question, before the key
  ([0038](decisions/0038-the-storage-question-during-setup.md)).
- **The applications menu opens the window** (§42, §69)
  ([0040](decisions/0040-the-menu-opens-the-window.md)).

Since 2026-09-28:

- **Removing a device** (§35, §62): on the Devices screen of both, and
  `qurb remove-device`, saying first what it does and does not do —
  [0041](decisions/0041-removing-a-device.md). Two faults found building it
  would have left a removed device syncing; both fixed and tested.
- **Conflicts** (§24): found on every device, shown with both versions, and
  settled by keeping one, the other, or both —
  [0043](decisions/0043-settling-a-conflict.md).
- **Sharing** (§23), decided and built: a folder shared with chosen devices,
  two-way among them, never sent to the others, a device left out keeping what
  it had — [0044](decisions/0044-sharing-with-chosen-devices.md). §4.6 is
  settled; step 9 is done except for what 0044 lists as not done.
- **Selective availability** (§29): a folder kept on a device, or only listed
  there and fetched when asked for — [0045](decisions/0045-a-folder-kept-remotely.md).
- **Recovering a deleted file**: Recently deleted on every device, 30 days,
  restored everywhere — [0042](decisions/0042-recently-deleted.md). Building it
  found a device unable ever to take back bytes it had deleted.

- **Vault operations** on the phone: rename, move to a folder, new folder, and
  adding into the folder being looked at; deleting goes to Recently deleted.
  A file keeps its area when moved.

- **A passphrase in the window, and a Security section** (§33): a locked key
  is unlocked from the window, and Settings says how the key is kept and
  changes it — [0046](decisions/0046-the-window-asks-for-the-passphrase.md).

- **Installing and upgrading** (§69, §70): an Arch package, a signed Android
  release, every build reporting its version, protocol and index schema, the
  index copied before it migrates and refused when newer — no automatic
  updater, by decision — [0047](decisions/0047-versions-and-upgrades.md).

Since 2026-09-29:

- **The design of both apps** (§73), from the owner's direction —
  [0048](decisions/0048-the-design-direction.md),
  [design/brief.md](design/brief.md). The desktop's eight tabs became a
  sidebar and the phone's five became four, with Private Vault on both and
  Transfers appearing only while something moves. Light only; the owner has
  not yet reviewed either. The phone's was walked on the S23 on 2026-10-03
  ([phase 5](phases/phase-5-mobile.md#the-designed-app-on-the-s23)). On the phone, adding a file now puts it in the area on screen —
  [0049](decisions/0049-adding-a-file-puts-it-where-you-are-looking.md).

Still as the inventory says: a replica freeing space, and everything in
§2.4–§2.6.

---

## 3. Where the brief and the recorded decisions disagree

The brief says an ambiguity that affects the data model or security should be
named rather than resolved quietly (§86). These are the four. Each was put to
the project owner on 2026-09-25, and each was decided the recommended way. The
reasoning stays here because the decision records point back to it.

### 3.1 What a phone's "My Vault" is — decided: A, [0036](decisions/0036-a-phone-keeps-its-own-files.md)

**Today** a file added on a phone — through the `+` button or the share sheet —
goes into the shared area, so every device receives it. The phone's vault holds
only what other devices have sent it.

**The brief** makes a phone's own files private to it (§3, §18). Sending is the
only way one leaves. After sending a photo to the desktop, the phone may free
its copy, see it as "Available on Desktop", and download it again (acceptance
steps 16–23).

**Step 21 of that test cannot be done safely today**, and the reason is a rule
rather than a missing feature. The desktop keeps what it receives in its own
vault, recorded on the phone as a private copy — and
[decision 0030](decisions/0030-sending-a-file-to-one-device.md) says a private
copy is never enough to drop anything of the phone's own, because the phone
cannot ask for it back. A copy in the desktop's Downloads folder is worse: it
belongs to whoever uses the desktop, and qurb would not know it had been
deleted.

For the brief's flow to be safe, the desktop has to hold a second copy
**for the phone**: in qurb storage, counted against the desktop's allowance,
never shown on the desktop, and served back only to the phone. That is the
"qurb storage → Phone A vault" box in the brief's own diagram (§2.1). It needs:

- vault rows the owner creates, not only ones a delivery creates;
- a grant that lets a chosen device hold another's vault without being shown it;
- a record of that copy that the storage cap counts, because the owner *can*
  ask for it back;
- a phone's rename, move and delete reaching the device holding its vault —
  [0029](decisions/0029-two-areas-shared-and-private.md) records that vaults do
  not converge today.

**The part that has to be said rather than hidden:** the desktop holding the
phone's vault could decrypt it, because both hold the same master key. What
stops it is the protocol and the desktop's own code, not cryptography. Making it
cryptographic means a key the desktop never has, which the 24 words then cannot
restore — so losing the phone would also lose the copy the desktop was holding
for it. That trade is the decision.

| option | the phone's own files | acceptance 16–23 | cost |
|---|---|---|---|
| **A. The brief's model** | private; the desktop holds a copy it does not show | met | the four items above, and a statement that the desktop's restraint is software, not cryptography |
| B. Today's model | shared with every device | 20–23 met, 14–18 not: the desktop shows every phone photo | nothing |
| C. Private, never held elsewhere | private | 20–23 not met: nothing can be freed | nothing |

Recommended: **A**, because the brief specifies it in five places, including
its acceptance test.

### 3.2 Where a file sent to the desktop lands — decided: Downloads, [0037](decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md)

**Today** it goes into the synced folder, which by default is `~/Downloads/qurb`
([0023](decisions/0023-one-person-per-account.md)), filed privately and counted
in the store.

**The brief** puts it in `~/Downloads/qurb` as an ordinary file, outside the
allowance, which qurb does not track afterwards (§5B, §12).

The same path is currently taken by the synced folder, so for new installs one
of them has to move; existing installs stay where they are, as 0023 promises.
Doing it the brief's way also removes, on the desktop, the kind of bug the fix
in progress (§6, step 0) deals with, because a scan of the synced folder never
sees a file that is not in it.

Recommended: **the brief's way**, with the synced folder's default moving back
to `~/qurb`. On a phone nothing changes: a received file is in the phone's
vault, which is the brief's model there too.

### 3.3 The storage question in the first run — decided: during setup, [0038](decisions/0038-the-storage-question-during-setup.md)

The brief asks for it in the first run (§6), lists it in the onboarding
sequence (§71), and makes it step 3 of the acceptance test.
[Decision 0033](decisions/0033-the-phrase-on-a-screen.md) left it out on
purpose: somebody who has not yet put a file in the folder cannot budget for
it. The reason is sound, but the brief specifies the form.

Recommended: **ask during setup**, with the brief's choices (50, 100, 250,
500 GB, custom) and the disk's free space beside them, and amend 0033.

### 3.4 Windows — decided: still out of scope, §5A

The brief lists Linux and Windows (§44, §69). §5A below keeps Windows out until
Linux and Android are finished, because nothing has been run on Windows and
there is no Windows machine to run it on.

Recommended: **keep §5A**.

### 3.5 Two more that do not block yet

**Sharing (§23)** is still undecided and is not needed before step 9. Under
option A the existing shared area becomes the special case "shared with every
device", and choosing which devices share a folder is the general case.

**Removing a device (§35)** needs no decision, only honest wording — see §2.6.

---

## 4. Decisions to record before the code they govern

1. ~~**Vaults, or one namespace.**~~ Decided: both —
   [0029](decisions/0029-two-areas-shared-and-private.md).
2. ~~**The desktop UI toolkit.**~~ Decided: Tauri.
3. ~~**What a phone's vault is.**~~ Decided —
   [0036](decisions/0036-a-phone-keeps-its-own-files.md).
4. ~~**Where a file sent to the desktop lands.**~~ Decided —
   [0037](decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md).
5. ~~**The storage question in the first run.**~~ Decided —
   [0038](decisions/0038-the-storage-question-during-setup.md), amending 0033.
6. ~~**What sharing means** (§23)~~ — decided 2026-09-28: chosen devices,
   [0044](decisions/0044-sharing-with-chosen-devices.md).
7. **Replica eviction**, before any storage screen promises it for replicas.
8. ~~**Selective sync for ordinary devices**~~ — decided 2026-09-28: a
   folder kept remotely, listed and fetched on demand, not `PinSet` —
   [0045](decisions/0045-a-folder-kept-remotely.md).
9. ~~**The Android interface toolkit**~~ — decided 2026-09-27: the platform's
   own views, for a light app —
   [0039](decisions/0039-a-light-android-app.md).

## 5A. Scope: Linux and Android, completely, before anything else

The product is finished for four device pairs — Linux↔Linux, Linux↔Android,
Android↔Linux, Android↔Android — before support expands to any other operating
system or architecture.

Windows is explicitly out of scope until then. The engine is portable Rust and
would probably build; nothing has ever run on it, and shipping a platform
nobody has watched work is a claim this project has not earned. The existing
Windows gaps stay recorded as gaps rather than being closed speculatively.

What this means for every step below: a feature is done when it has been seen
to work **between a laptop and a phone**, not when it works desktop-to-desktop.
Two of the four pairs involve Android, and Android is where the surprises have
been — doze, background windows, multicast filtering, a keystore that is a Java
API.

Android↔Android needs a second Android device. An emulator can stand in while
building; it cannot verify.

## 6. Sequence

The specification's phases (§74) are sound. Reordered only where the repository
says something must come first.

**0. Finish what is in flight.** A file sent to a phone was re-filed as shared
by the phone's next folder scan and offered straight back to the laptop that
sent it. Fixed on 2026-09-25 together with ten related defects: two lost a file
outright, and one made a send from the folder under its own name do nothing —
see
[phase 5](phases/phase-5-mobile.md), "A file sent to the phone came straight
back". ✅ Verified on Linux and on the phone the same day.

**1. Decide the data model.** ✅ Done. Two areas, enforced in the protocol, and
the transfer primitive: `qurb send <file> to <device>` writes into a vault, the
recipient collects it once, and the sender releases its copy first under
storage pressure.

**2. An API the UI can use.** ✅ Done —
[decisions/0032](decisions/0032-the-interface-hosts-the-daemon.md).
`qurb_cli::View` answers the nouns (devices, files, availability, storage,
history, outgoing, search) as read-only queries against the index; the daemon's
`watch` channel carries the live state. It carries no transfer progress,
notifications or write path.

**3. Transfer and activity as first-class records.** ✅ Done for outcomes —
[decisions/0031](decisions/0031-what-happened-is-written-down.md).

**4. Desktop shell, pairing and devices** ✅ —
[`crates/desktop`](../crates/desktop/README.md). Home, files, devices,
activity, storage, send, settings, setting a device up from nothing, and
pairing on the existing infrastructure.

**5. Transfers.** Mostly done. Sending from the window is built, and so are the
three notifications. The **Downloads destination** is built — decision 0037 —
and building it found and closed two defects that were not about Downloads: a
peer's path was never checked, so a paired device could write or delete
anywhere on the disk, and a deleted delivery could arrive again once its
tombstone expired. See [phase 4](phases/phase-4-product.md), "Files sent to a
desktop go to Downloads". **Progress** is built in both directions — a
Transfers screen with a rate and time left, seen working in a real window.
**Several files or a folder** in one send is built, and so is **cancelling a
send** not yet collected, and **showing a received file in its folder**, with
the Downloads location in Settings, and **whether each device is connected
directly or through the relay**. Step 5 is done for the desktop. Stopping a
transfer already moving, and pause, are not planned: a failed file is retried by the
next sync, and a send can be cancelled until it is collected.

**6. Android product UI.** Built on
[0036](decisions/0036-a-phone-keeps-its-own-files.md), whose engine changes
are done (2026-09-27): four areas on the wire and `qurb/2`, holding, freeing a
chosen file, and fetching it back, verified between two desktops. What is left
of step 6 is the phone itself. The FFI gains send, fetch, free-local-space, activity, devices and the phone's own vault, over queries
moved down from `qurb-cli` rather than copied. The phrase is confirmed on the
phone as it is on the desktop. The system picker asks the engine instead of
walking the directory.

*Progress, 2026-09-27:* the FFI calls are done and tested, and the app is
rebuilt on platform views ([0039](decisions/0039-a-light-android-app.md)) as
five tabs — Home, Vault, Devices, Transfers (with the history the brief calls
Activity) and Settings — installed on the S23, and the phone and the laptop
verified keeping, freeing, fetching back and deleting through those screens.
The phrase is confirmed on the phone as on the desktop, and the system picker
asks the engine (2026-09-27). Step 6 is done. On 2026-09-29 the five tabs
became the design's four — Home, Files, Devices, Settings — with Private Vault
inside Files (§2.9).

**7. Cross-device flows**, including Android↔Android, offline and relay.
*Done between the Galaxy S23 and the laptop*, on Wi-Fi and on mobile data, with
push; sending, Recently deleted and settling a conflict across the two
verified. Not done: Android↔Android (one phone), and anything through a relay
on a server — the relay is next after design.

**8. Storage**: the first-run question (§3.3, built 2026-09-27 — decision
0038), freeing a chosen file's local copy, fetch, selective availability
(0045). ✅ Done except replica eviction, which is §4.7.

**9. Sharing** ✅ — decided and built 2026-09-28,
[0044](decisions/0044-sharing-with-chosen-devices.md).

**10. Security, search, activity, polish, packaging, verification.** Packaging
includes choosing one front end for the applications menu (§2.3, Tray) —
chosen 2026-09-27: the window (decision 0040). Security (0041, 0046),
packaging and versions (0047) are done; search is by name only; *polish* is
the design and UX pass, built for both apps on 2026-09-29 and awaiting the
owner's review.

## 7. Rules this plan holds itself to

From the specification, and from what this repository has already learnt the
hard way:

- The engine stays authoritative. No sync logic in Kotlin, JavaScript or the
  desktop UI; no second database.
- No screen ships on mock data. A mock is acceptable while a screen is being
  built and must not survive into a path a user can reach.
- The CLI keeps working. It is the diagnostic interface and several bugs were
  only findable through it.
- Safety invariants are not negotiable for UI convenience — in particular that
  the last known copy of anything is never dropped to satisfy a number someone
  typed into a settings box.
- Every step updates the documentation as part of the step.

## 8. What "done" means here

A feature is done when the UI exists, real data flows through it, error and
offline states exist, tests exist, documentation exists, and it works on
hardware where hardware is involved. Not when it compiles.

## 9. Known limitations that must not be papered over

- **Linux has no placeholder filesystem API.** An evicted file is absent from
  the folder. The UI must represent it and offer to fetch it; a FUSE mount to
  make a screenshot nicer is not worth a daemon whose failure takes somebody's
  folder with it.
- **Android decides when background work runs.** Push makes a change arrive in
  under a second when it is configured; without it, fifteen minutes is the
  floor and dozing makes it longer. The product must not promise otherwise.
- **The direct-connection rate is unmeasured.** A phone on mobile data has
  reached a laptop at home directly, by qurb's own traversal — on one carrier
  and one home router. How often that works across networks is still unknown,
  and it is the number the relay bill depends on.
- **Two devices that are never awake together never meet**, unless something
  always-on is in the picture.
- **Local discovery does not cover IPv6.** An IPv6-only network gets none of
  it. IPv4 is verified in both directions between a laptop and a phone with no
  server running at all.
- **A send cannot be withdrawn** once the recipient has collected it, and a
  replica cannot usefully carry one — see
  [decisions/0030](decisions/0030-sending-a-file-to-one-device.md).
- **Every device must be rebuilt together.** The wire protocol moved to
  `qurb/1` when tree entries gained a private flag and to `qurb/2` when that
  flag became four areas; an older build refuses to connect rather than
  mishandling it.
- **Your devices share one key.** A vault is private from a device that does
  not hold its bytes. It is not cryptographically private from one that does.
- **Removing a device does not take its key away.** It stops being trusted; it
  keeps what it already holds until key rotation exists.

## 10. Testing

The existing suite covers the classes that matter here: property-based
convergence, crash injection, corruption repair, hostile peers, concurrent
collection. New work extends those rather than starting a parallel tradition.

What this plan adds: API tests, UI state tests, and the end-to-end matrix in
§55 — desktop↔android, android↔android, desktop↔desktop — plus the real-hardware
behaviour in §56, which cannot be answered on an emulator.

## 11. What is not in this plan

Accounts, billing, a hosted service, and anything requiring a company to exist.
Those are Phase 6 in [roadmap.md](roadmap.md) and unchanged by this.

## 12. What I need decided

The design direction is chosen and built ([0048](decisions/0048-the-design-direction.md));
what it needs now is the owner's review at the checkpoints in
[design/brief.md §5](design/brief.md#5-the-work). After that, the relay on a
server and a formal release need no further decision except where to host the
relay. §4.7 is still open, and is needed before any storage screen promises
anything for replicas.

## 13. Completing it

Set on 2026-10-03, when the owner asked to focus on completing the project.
"Complete" is §5A and §8 together: every feature built, and seen working
between a laptop and a phone, for all four device pairs. Four milestones, in
the owner's order; each item is checked off here when it is done, not when
it is started.

**1. Design, finished and seen on the phone.**
- [x] The designed Android app installed on the S23 and every screen walked —
  2026-10-03; it had been on the phone since 2026-09-29 ([phase 5](phases/phase-5-mobile.md#the-designed-app-on-the-s23)).
  Walking it found the laptop four days out of sync, from a folder typed
  without a slash ([phase 4](phases/phase-4-product.md#a-folder-typed-without-a-slash)).
- [ ] The owner's review of both apps at the checkpoints; then dark mode.
- [ ] Moving a file into and out of Private Vault, on both
  ([brief §2](design/brief.md)) — an engine addition.
- [ ] The three notifications on the phone.
- [ ] A preview when comparing a conflict's two versions.
- [ ] The designed app measured against [0039](decisions/0039-a-light-android-app.md).

**2. Syncing from anywhere, through the owner's own server.** *Later, by the
owner's choice on 2026-10-03: the laptop and Tailscale Funnel stay until
then.*
- [ ] The rendezvous service, relay and push on the owner's VPS
  (`packaging/server/deploy.sh`, never yet run against a real server); both
  devices pointed at it; Funnel retired.
- [ ] A phone on mobile data falling back to the relay, seen; the
  direct-connection rate measured ([measuring-connectivity.md](measuring-connectivity.md)).

**3. Every pair verified on hardware.**
- [ ] On the S23: removing a device, the share sheet sending to a device,
  pairing by the phone's own code, a folder shared with chosen devices, a
  folder kept remotely. Removing the laptop, pairing again and sending to it
  are in the phone's history for 2026-09-29; the rest still to watch.
- [ ] Android↔Android — on the emulator until the owner can borrow a second
  phone, then on two real ones.
- [ ] A phone left alone for a day: battery and survival (Phase 5's kill
  criterion).

**4. Release.**
- [ ] Decided and recorded: how a replica frees space (§4.7), and whether
  there is any recovery beyond the 24 words.
- [ ] Licence files for the licence `Cargo.toml` declares.
- [ ] A signed release APK on the S23 (leaving the debug build means
  uninstalling and enrolling again from the 24 words), the Arch package
  installed, a GitHub Release, and the website's Download page pointing at it.
