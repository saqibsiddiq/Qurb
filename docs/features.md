# What qurb does today

Every feature that exists, grouped by where a person meets it: the desktop
window, the phone, the command line, the services. The last sections cover
what the engine guarantees underneath all of them, and what is not built.
Written 2026-09-28 as the starting point for the design and UX pass, and
brought up to date on 2026-09-29, when the desktop's design was built, and on
2026-10-03 for the Android app's, on 2026-10-08 for what the Galaxy S23
was watched doing that day, and on 2026-10-15 for the backups of WhatsApp
and a phone's folders, six digits to pair, and a folder for each person,
with what the S23 and the laptop were seen doing with them that day.

Each line says how far it has been checked, because "built" and "works between
two real devices" are different claims:

- **✅** — verified between real devices: the Galaxy S23 and the laptop.
- **🧪** — checked by automated tests with several devices, on the Android
  emulator, or in the real desktop window by the smoke test — not yet between
  real devices.
- **◻** — built and compiled, not yet exercised end to end.

For how any of it works, [CODEBASE.md](CODEBASE.md); for why, the decision
records it links.

---

## 1. The desktop window (Linux)

One program, `qurb-desktop`, that *is* the sync daemon with a window on it
([0032](decisions/0032-the-interface-hosts-the-daemon.md)). Closing the window
leaves it syncing; *Quit* in Settings stops it. It starts at login without a
window, and a second launch shows the running one
([0040](decisions/0040-the-menu-opens-the-window.md)).

Designed on 2026-09-29 from the owner's direction
([design/direction.md](design/direction.md)): a translucent sidebar — Home,
Files, Devices, Storage, then Private Vault set apart, then Settings — and one
frosted stage the content floats in. Checked by rendering every place against
the fixture data in WebKitGTK and by the smoke test driving the real window;
not yet looked at by the owner.

### Before it runs

| | what a person can do | |
|---|---|---|
| Welcome | *Your files. Your devices. Your space.* — then new, or *I already use Qurb* | 🧪 |
| Set up a new device | choose the folder, answer how much disk Qurb may use, and it starts; nothing to write down ([0052](decisions/0052-the-key-travels-with-the-code.md)) | 🧪 |
| Join with a code | type the code a phone shows, or its six digits on the same Wi-Fi ([0066](decisions/0066-six-digits-to-pair.md)); this computer gets the key and is paired once the phone approves it, both showing the same six digits ([0053](decisions/0053-approval-same-key-and-safe-copies.md)) | 🧪 |
| Join with the 24 words | the fallback, for somebody who has them | 🧪 |
| Unlock | type the passphrase, when the key is protected by one; at login the window shows itself to ask ([0046](decisions/0046-the-window-asks-for-the-passphrase.md)) | 🧪 |

### The places

| place | what it shows and does | |
|---|---|---|
| **Home** | one state — *Everything is synced*, *Syncing…*, *Your devices are away*, *Add your first device*, *Something needs attention* — with a ring that turns while syncing; *Sync now* and *Send* side by side, of one size; used space and devices connected; attention when a file has two versions; *Waiting* — what has not reached another device yet, sends not collected, a phone's backups still to come; *Backups* — WhatsApp and the folders kept here for each phone, with *Choose…*; *See all* for Activity | 🧪 in the real window by the smoke test; looked at on the laptop with its real numbers, 2026-10-15 |
| **Files** | at the top, *Shared* — the folder on all your devices — and a folder for each person this computer keeps files for, the owner's included ([0067](decisions/0067-a-folder-for-each-person.md)) | ✅ on the laptop, 2026-10-15 |
| | a person's folder is locked: *Ask your phone* asks it, and it opens with a yes behind the phone's screen lock; open, each phone's *WhatsApp*, backed-up folders and *Private Vault*, a phone set up again keeping its own folder (*…, earlier*); a file opened is written into memory-backed space and handed to its program; *Lock*, ten minutes unused or quitting deletes what was opened. The owner's folder is Qurb's rule, not encryption — the computer holds the same key — and the window says so; a guest's is sealed | ✅ the owner's, between the S23 and the laptop, 2026-10-15: asked during a WhatsApp backup, open 21 seconds later; a file opened |
| | inside *Shared*: search; breadcrumbs from *Qurb*; folders as tiles apart from files; each file with its state — *On this device*, *Available elsewhere*, *Only copy here*, *On no device* ([0055](decisions/0055-a-file-on-no-device-says-so.md)), *Downloading* — size and when it changed | 🧪 |
| | per file: open, show in folder, *Keep on this device*, *Free local space* (refused for the only copy), send to a device, *Move to Private Vault* / *Move to shared* ([0057](decisions/0057-moving-a-file-into-or-out-of-private-vault.md)), details, delete (into Recently deleted) | 🧪 |
| | a file's details: type, size, location, which devices hold it, when it changed, its history | 🧪 |
| | a folder's options, from its menu: which devices have it ([0044](decisions/0044-sharing-with-chosen-devices.md)); *Free local space* / *Keep here* for the whole folder ([0045](decisions/0045-a-folder-kept-remotely.md)) | 🧪 |
| | conflicts: both versions, who made each and when; keep this one, the other, or both ([0043](decisions/0043-settling-a-conflict.md)) | 🧪 |
| | Recently deleted: thirty days, when each expires; restore — back where it was: everywhere for a shared file, in Private Vault for one of the vault's — or delete for good ([0042](decisions/0042-recently-deleted.md)) | 🧪 |
| **Devices** | this computer and each paired device, whether each is connected now (directly or through the relay) or when it was last seen; a phone set up again is one card, saying when and what is still kept from before, with *Remove the earlier one…* | 🧪 · the S23 as one card ✅ on the laptop, 2026-10-14 |
| | *Add a person*: a guest code for someone else's phone, which keeps its own key and sees only what is sent to it; guests listed apart; the approval says *as a guest*; a guest's sheet says what this computer keeps for them, sealed, which nobody here can open ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)) | 🧪 against the fixtures; guests and their sealed folders watched between two folders on the laptop, and with the emulator as the guest, 2026-10-11 |
| | *Open their folder…*, on a guest's sheet: asks their phone, which approves behind its screen lock; open, the folder's files with their real names, opened from memory-backed space; *Lock*, or ten minutes unused, closes it and deletes what was opened ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)) | 🧪 in tests; with the emulator as the guest, approving by PIN, the file listed and opened, 2026-10-11; not with a real phone |
| | *Add a device*: show a code — a QR code and six digits to type on the same Wi-Fi, with a countdown ([0066](decisions/0066-six-digits-to-pair.md)) — or enter one; the new device materialises | 🧪 · paired by the digits in the real-window smoke test; not yet between the S23 and the laptop |
| | a phone's backups, from its panel: WhatsApp's ([0062](decisions/0062-backing-up-whatsapp.md)) and the folders chosen for it, ticked here or on the phone, the later choice winning ([0063](decisions/0063-backing-up-a-phones-folders-chosen-on-the-desktop.md)); opened in the person's folder in Files | ✅ on the S23 and the laptop: WhatsApp's 36,062 files, 10.1 GB; *DCIM/Screenshots* and *DCIM/Camera*, 1,483 files. **Restoring WhatsApp from it is unproven** |
| | a device's details: what is waiting for it, send it files, remove it — saying first what that will and will not do ([0041](decisions/0041-removing-a-device.md)) | 🧪 |
| **Storage** | how much Qurb takes, against the limit or the disk; *What is kept here*: your phones' backups (each phone's WhatsApp, folders and Private Vault), shared files, guests' folders, Recently deleted, each with where to go for it; the largest files that could be freed, *Free local space* for one or all; the storage limit, which never frees a phone's backup ([0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md)) | ✅ looked at on the laptop with its real numbers, 2026-10-15: 17 GB of 30 GB, 21 GB of phones' backups as files |
| **Private Vault** | this computer's own files, in the same browser; a file moves in or out with *Move to Private Vault* / *Move to shared* ([0057](decisions/0057-moving-a-file-into-or-out-of-private-vault.md)) | 🧪 |
| **Settings** | grouped lists: this device (name, folder, key protection and changing it); devices and pairings; files sent to this computer (`Downloads/qurb` by default, [0037](decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md)); keep new files private; notifications; the recovery phrase; appearance — theme: system, light or dark ([0056](decisions/0056-dark-mode.md)); advanced — rendezvous, relay, port, start at login, Activity, identity, version, Quit | 🧪 |
| **Activity** | from Home: everything this device did, newest first, with why a failure failed ([0031](decisions/0031-what-happened-is-written-down.md)) | 🧪 |

### Sending and transfers

| | | |
|---|---|---|
| Send to device | a sheet: drop files or folders or choose them, pick a device, then the file travels there — *ready for it*, *sending*, *sent* ([0030](decisions/0030-sending-a-file-to-one-device.md)); from Home, a file, a device, or files dropped anywhere on the window. A file sent to that device before is named, with when, and sent again only if asked — *Send it again* or *Leave it out* ([0059](decisions/0059-a-send-is-not-its-bytes.md)) | 🧪 — sending itself ✅ from the command line; the question looked at against the fixtures |
| Transfers | a chip that appears while something moves or waits, opening a panel: progress with the time left, sends waiting to be collected (cancellable), and what finished | 🧪 |
| Notifications | three things only: a file sent to you, one of yours collected, one that failed — and a switch to turn them off | 🧪 |
| Staying awake | while it collects from a device and is on mains power, the computer is kept from suspending, and let go when the collection ends | ✅ on the laptop, 2026-10-14 |

### The `qurb` folder

`~/qurb` holds people: `~/qurb/Shared`, the folder that copies to every one
of the owner's devices, and `~/qurb/<person>`, a read-only placeholder for
each person this computer keeps files for, holding one note saying the folder
opens in Qurb. A folder set up before 2026-10-14 is moved into `Shared` once,
index and all, when Qurb starts ([0067](decisions/0067-a-folder-for-each-person.md)).
✅ run on the laptop, 2026-10-14.

### The tray icon

`qurb-tray`: the same daemon with an icon — recently synced files, *Open
folder*, *Quit*, and a small window with the status and the storage slider.
Where there is no tray (GNOME), it opens that window instead. Superseded as the
main way in by the desktop window; its icon is the mark, and its small window is
not redesigned. Kept for desktops that want only an icon. 🧪

---

## 2. The Android app

Kotlin over the engine. The key is kept in the Android Keystore
([0021](decisions/0021-the-platform-supplies-the-keystore.md)). Files that
arrive on the phone are **private by default** — they stay on the phone and on
devices chosen to keep them — and that can be switched off
([0036](decisions/0036-a-phone-keeps-its-own-files.md)); a file added from
Files or from Private Vault goes into that area
([0049](decisions/0049-adding-a-file-puts-it-where-you-are-looking.md)).

Designed on 2026-09-29 from the owner's direction, in the desktop's language:
four tabs under a floating bar — Home, Files, Devices, Settings — with Private
Vault inside Files, Activity from Home, and Transfers as a bar that appears
while something moves. Light and dark ([0056](decisions/0056-dark-mode.md)). **Every place walked on the Galaxy S23 on
2026-10-03**; it had been on the phone since 2026-09-29. The marks are for the
*features*: most were earned through the screens the design replaced, which
called the same engine functions; where a mark comes from the designed screens
themselves, the row says so.

### Setting up

| | | |
|---|---|---|
| A new key | made and kept in Block Store, backed up end-to-end encrypted when the phone has a screen lock; nothing to write down ([0052](decisions/0052-the-key-travels-with-the-code.md)) | ✅ on the S23, 2026-10-08: read back from Block Store; kept across a reinstall, not across *Clear data* — on a second copy of the app |
| Joining | scan the code another device shows, or type its six digits on a number pad, joining as the sixth is typed ([0066](decisions/0066-six-digits-to-pair.md)); the key comes with it | 🧪 in the FFI's tests; the number pad on the emulator |
| The key from a backup, or the 24 words | the fallbacks | 🧪 |

The S23 was set up before the three-word check existed; the flow as it is now
was walked through on the emulator.

### The places

| place | what it shows and does | |
|---|---|---|
| **Home** | one state — *Everything is synced*, *Syncing…*, files waiting to reach your devices, *Backing up to …* while a backup is on its way, *Waiting for … to collect*, *Not synced yet*, *Add your first device*; *Sync now* and *Send* side by side; when it last synced; *Waiting* — files not on another device yet, sends not collected, backups still to go; *Backups* — WhatsApp's and the phone's folders; pull down to sync | ✅ on the S23, 2026-10-15 |
| | a file with two versions: attention, then a sheet with both — keep this version, the other, or both ([0043](decisions/0043-settling-a-conflict.md)) | ✅ |
| **Files** | the shared area a folder at a time: search across every folder, breadcrumbs, folders as tiles, sort by name, newest or largest; each file's state — *On this phone*, *Available elsewhere*, *Only copy here*, *On no device* ([0055](decisions/0055-a-file-on-no-device-says-so.md)), *Downloading* | 🧪 |
| | per file, in a sheet: open, keep on this phone, free local space (never the only copy), send to a device, save a copy, rename, move to a folder, *Move to Private Vault* / *Move to Files* ([0057](decisions/0057-moving-a-file-into-or-out-of-private-vault.md)), delete (into Recently deleted) | 🧪 · freeing and getting back ✅ · both *Move* buttons, rename and search ✅ on the S23, 2026-10-08 |
| | *Add files* into the folder on screen; new folder; save everything here to the phone at once | ✅ *Add files* on the S23, 2026-10-05, and into a shared folder while a sync scanned, 2026-10-08 ([0049](decisions/0049-adding-a-file-puts-it-where-you-are-looking.md)); the rest 🧪 |
| **Private Vault** | from Files: the phone's own files, in the same browser; *Add files* here adds privately, whatever the setting says | 🧪 in the FFI's tests |
| **Your folder on …** | from Files, while the computer keeping this phone's files is on: what it keeps for this phone — WhatsApp, the backed-up folders, the Private Vault, deleted files included, and what it kept before the phone was set up again — a hundred files at a time; each file *Open*ed in the app for its type — a large one played as it arrives, seeking included — *Download*ed to Downloads/Qurb or *Restore*d where it was, checked before it is put there ([0067](decisions/0067-a-folder-for-each-person.md)) | ✅ browsed on the S23, 2026-10-15; a 1.6 GB episode played six seconds after *Open* and sought to 42:40; downloading and restoring not yet seen there |
| **Backups** | WhatsApp's folder kept on the computer, on Wi-Fi, without copying it on the phone ([0062](decisions/0062-backing-up-whatsapp.md)); *Your folders*, from Home and Settings: a switch for each folder of the phone, *Back up now*, how much the computer holds ([0063](decisions/0063-backing-up-a-phones-folders-chosen-on-the-desktop.md)); needs "All files access" | ✅ on the S23 and the laptop; **restoring WhatsApp unproven** |
| **Devices** | this phone and each paired device as cards, with when each was last seen; *Add* — scan a code, **show a code on this phone**, or type one | ✅ scan and show, on the S23, 2026-10-08 |
| | *Visit someone's computer*: join another person's computer as a guest with its guest code, keeping this phone's key; *Computers you visit* listed apart ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)) | 🧪 on the emulator as the guest, end to end with the desktop, 2026-10-11; not yet on a real phone |
| | *Keep my files here*, on a computer it visits: the Private Vault kept there sealed, so nobody there can open a name or a byte; this phone keeps none of its own copies, and fetches each back when opened, keeping it a day ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)); kept through the relay when the computer cannot reach the phone directly ([0061](decisions/0061-the-relay-carries-meetings.md)) | 🧪 on the emulator as the guest, end to end with the desktop, 2026-10-11; not yet on a real phone |
| | that folder, on the phone: Private Vault says *Your folder, kept on …*, and a file there and not here reads *On …*; the share sheet offers *Save to my folder on …* and *Send to …'s Downloads* ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)) | 🧪 the folder's wording on the emulator, 2026-10-11; the share sheet's choices not tried |
| | *Open your folder on …?*: a computer this phone visits, or the owner's own, asking to open its folder there, answered behind the fingerprint, face or screen lock; refused without a screen lock; then says whether it opened; the same ask never put twice; heard within seconds even during a long backup, and as a notification with the app away ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md), [0067](decisions/0067-a-folder-for-each-person.md)) | ✅ the owner's, on the S23, 2026-10-15, shown six seconds after the laptop asked; as a guest 🧪 on the emulator, 2026-10-11 |
| | a device's sheet: *Keep a backup of my Private Vault* — a device that keeps a copy of the phone's own files, so the phone can free space | ✅ |
| | send files to it; remove it, saying first what that does ([0041](decisions/0041-removing-a-device.md)) | ✅ both in the S23's history, 2026-09-29; removing watched, 2026-10-08 |
| **Settings** | grouped lists: this phone (name, key protection); devices; storage — space used, who has each folder (choose devices, keep on this phone or download when opened), Recently deleted, *free unused space*; privacy — *Keep new files private*; notifications — the desktop's three: sent to you, delivered, failed (`Notices.kt`); the recovery phrase; appearance — theme: as the phone is set, light or dark ([0056](decisions/0056-dark-mode.md)); advanced — background sync, rendezvous, relay, version | 🧪 · on the S23, 2026-10-08: who has each folder, the laptop turned off and on again; the *sent to you* notification; dark |
| **Activity** | from Home: what happened, newest first, sixty at a time | ✅ |
| **Recently deleted** | from Files and Settings: thirty days, when each expires; restore — back where it was: everywhere for a shared file, in Private Vault for one of the vault's — or delete for good ([0042](decisions/0042-recently-deleted.md)) | ✅ |
| **Transfers** | a bar above the tabs while the phone syncs or has a send not yet collected; its sheet shows what is waiting, with *Stop*, and what finished | 🧪 stop sending |

### Outside the app

| | | |
|---|---|---|
| **Share sheet** | anything on the phone can be shared into qurb, with no network and no other device switched on | ✅ |
| **Send** | from Home: for a computer keeping this phone's files, *My folder on …* (opens there only with this phone's yes) or *…'s Downloads* (opens freely), then every other device; the share sheet says the same ([0067](decisions/0067-a-folder-for-each-person.md)) | 🧪 on the emulator; not yet on the S23 |
| | it asks where: *Save to Private Vault*, *Save to Files, on all your devices*, or *Send to* a paired device; with none paired it saves without asking | ✅ the question, *Send to* the laptop, and *Save to Files* reaching the laptop four seconds later, on the S23, 2026-10-08 |
| **The system file picker and Files app** | qurb's files appear there, listed from the index; a freed file downloads when opened; other apps can save into qurb | 🧪 |
| **Background sync** | WorkManager, every 15 minutes — every hour once a push has arrived in the last week | ✅ |
| **Push** | a change on the laptop wakes the sleeping phone, through Firebase; about five seconds from a change on the laptop to the phone syncing it, on mobile data with the screen off | ✅ |
| **Syncing from mobile data** | through the rendezvous service, directly to the laptop | ✅ — needs the rendezvous set in Settings; a phone whose data is cleared loses it and falls back to the emulator's address, which nothing points out yet ([phase 5](phases/phase-5-mobile.md#the-last-of-the-phone-and-a-setting-lost-three-days-before)) |
| **A large file collected from the phone** | 32 MiB or more waiting: the sync runs in the foreground under *Sending to your devices*, with the bytes sent so far, for as long as a device collects — up to half an hour, waiting a minute for a device that pauses part-way ([0050](decisions/0050-large-files-from-a-phone.md)); paced by BBR ([0051](decisions/0051-bbr-not-cubic.md)) | ✅ on the S23, 2026-10-05: it went on serving after the app was left, at about 5 MB/s with Cubic. With BBR, 10.6–12.1 MB/s; a collector pausing 20 seconds was waited for ([phase 5](phases/phase-5-mobile.md#through-the-app-with-bbr--and-a-phone-cleared)) |
| **Copies of files it sent** | none kept since 2026-10-10: a send is read from where the file is when collected, and a copy of a share-sheet file goes once it arrives ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)). Copies from sends made before were kept after they arrived ([0030](decisions/0030-sending-a-file-to-one-device.md)); shown in Settings and on Android's storage screen with their size, and let go of only when asked by name, after a warning that a recipient may have deleted its copy since; freeing space in general leaves them | 🧪 · the storage screen's line, the Settings row and its warning seen on the S23, 2026-10-07; letting go not tapped |

---

## 3. The command line

`qurb` with no arguments lists these. `[dir]` defaults to the folder already
set up, so most commands need no path.

| command | what it does |
|---|---|
| `init [dir]` | set up a device with a new key; nothing to write down |
| `join [dir] <code>` | a folder not set up takes the key of the device showing the code — the long code, or its six digits on the same network — and pairs with it; prints the number the other device should show |
| `pair [dir]` | shows a code and its six digits ([0066](decisions/0066-six-digits-to-pair.md)); asks in the terminal to approve each device that uses it, by the number it shows ([0053](decisions/0053-approval-same-key-and-safe-copies.md)) |
| `pair [dir] --guest` / `visit [dir] <code>` | a guest code for another person's device / visit another person's computer as a guest, keeping this device's key ([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)) — watched between two folders on the laptop |
| `enrol <dir> "<24 words>"` | set up a device with an existing key |
| `pair` / `join <code>` | show a pairing code (QR in the terminal) / join one |
| `run` | the daemon, without a window |
| `replica <dir> [--only <path>]` | an always-on device that holds content for the others and shows nothing |
| `status` | what this device holds and trusts; a device set up again listed with the one it replaced |
| `ls [path]` / `find <text>` | what the folder holds and where each file's bytes are / search names |
| `activity [path]` | what happened, newest first, or to one file |
| `fetch <path>` / `free <path>` | bring a freed file back / free a local copy another device keeps |
| `private <path>` / `unprivate <path>` | move a file into this device's Private Vault / out of it to every device ([0057](decisions/0057-moving-a-file-into-or-out-of-private-vault.md)) |
| `send <files and folders> to <device> [--again]` / `cancel <name> to <device>` | send to one device, asking first about anything sent there before ([0059](decisions/0059-a-send-is-not-its-bytes.md)) / take a send back before it is collected |
| `holders [add\|remove <device>]` | which devices keep this one's own files |
| `conflicts [keep <copy> this\|other\|both]` | list conflicts, settle one |
| `share [<folder> with <device>,… \| with everyone]` | which devices a folder goes to |
| `keep <folder> here\|remote` | keep a folder here, or only list it |
| `deleted` / `restore <#n or path>` / `forget <#n or path>` | Recently deleted / put one back, everywhere / delete one for good, here |
| `remove-device <device> [--delete-kept] [--yes]` | stop trusting a device; says what that does first |
| `config [key=value …]` | `name`, `signal` (the rendezvous service), `relay`, `port`, `limit`, `own-files`, `downloads`, `notifications` |
| `protect file\|keystore\|passphrase` | change how the key is kept |
| `verify [--deep]` / `reclaim` | check the store against itself / free duplicates an older store holds |
| `version` | the build, its protocol, its index schema |
| `signal` / `relay` / `netcheck` | run the rendezvous service / run the relay / what this network allows |

---

## 4. The services

Both run by the owner, on their own machine or server. Neither ever sees a
file, a filename or who a person is
([0016](decisions/0016-what-signalling-learns.md)).

| service | what it does | where it runs today |
|---|---|---|
| **Rendezvous** (`qurb signal`) | introduces devices on different networks, tells a device at once when another has work for it, keeps that message for a device that is away, and wakes a sleeping phone by push ([0022](decisions/0022-the-service-announces-arrivals.md), [0028](decisions/0028-waking-a-sleeping-device.md)) | on the laptop, as a user unit, reachable from outside through Tailscale Funnel ✅ |
| **Relay** | carries encrypted traffic when no direct path exists: inside the rendezvous service at `/relay`, over a WebSocket, found by every device with nothing set ([0068](decisions/0068-the-rendezvous-service-relays.md)); or `qurb relay` on a port of its own | 🧪 end to end in tests; **running on the laptop since 2026-10-15**, reachable through its Funnel; not yet seen carrying a sync from the S23 on another network |
| **Local discovery** | devices on the same Wi-Fi find each other with encrypted beacons and no server at all ([0034](decisions/0034-finding-each-other-with-no-server.md)) | ✅ |

`packaging/server/` has the units, TLS and firewall rules for a server of your
own, and `deploy.sh` for setting one up — written, not yet run against a real
server.

---

## 5. What the engine guarantees, everywhere

The parts nobody sees, and the reason the features above can be trusted.

| guarantee | |
|---|---|
| **Files go directly between devices, encrypted.** The servers never hold them. | ✅ |
| **A file costs its size once.** The file in the folder is its own storage; no second copy ([0024](decisions/0024-the-file-is-the-payload-store.md)). | ✅ |
| **An edit sends only what changed.** A 16-byte insert into a 200 MB file moved 248 KiB, between two devices on one machine. | 🧪 |
| **A large file cut off part-way carries on where it stopped**, rather than starting again, and is fetched several chunks at a time ([0050](decisions/0050-large-files-from-a-phone.md)). | ✅ resumed twice from the S23, 2026-10-05; several at a time barely raised the speed |
| **Nothing is waiting on the other device being awake.** Added while every other device is off, a file goes when one is next reachable. | ✅ |
| **An edit is never silently lost.** Two devices changing one file keep both versions ([0005](decisions/0005-conflict-resolution.md)). | ✅ |
| **A deletion can be undone for thirty days**, on every device. | ✅ |
| **The storage limit never deletes the only copy.** It frees only what another device is known to hold, and says when it cannot. Freeing on the phone, with the laptop keeping its files, is verified. | 🧪 limit · ✅ freeing |
| **A folder shared with chosen devices is refused to the others**, by the device serving it, however they ask — tree, manifest or chunk. | 🧪 |
| **A file sent to one device reaches only that device.** | ✅ |
| **A removed device is refused at once**, on connections already open too. | 🧪 |
| **Hostile input goes nowhere**: `../` paths, qurb's own store, wrong bytes, nonsense and silence from a peer are refused before anything is written. | 🧪 |
| **A crash cannot corrupt the index**: payload before index, killed at seven points in tests; migrations atomic. | 🧪 |
| **An upgrade keeps everything**: the index is copied before it migrates; a newer index is refused by an older build. The S23 upgraded from schema 12 to 15 with its data intact. | ✅ |
| **Filenames are the same everywhere** (NFC), and names that collide ignoring case are refused. | 🧪 |
| **Tested at 100,000 files and 4.4 GiB** between two devices, and **at a million files**, which synced in 38 minutes the first time once the file list went in frames ([0065](decisions/0065-the-scale-qurb-is-built-for.md)). | 🧪 |
| **A phone knows what its computer keeps.** What the computer holds for it is reported as it collects; a report lost on the way is made good by the phone asking ([0062](decisions/0062-backing-up-whatsapp.md)). | ✅ on the S23, 2026-10-15: 130 WhatsApp files the phone thought still to go, learned of in two syncs |

---

## 6. Installing

| | | |
|---|---|---|
| Arch and derivatives | `cd packaging/arch && makepkg -si` — built, not yet installed on the laptop | ◻ |
| Any Linux, one user | `packaging/install.sh` — what the laptop runs | ✅ |
| The rendezvous service | `packaging/install-rendezvous.sh`, with push | ✅ |
| Android | `./scripts/android-app.sh release`, signed with the project's key; the phone itself runs a debug build | ◻ release · ✅ debug |
| A release, 0.1.0 | `./scripts/release.sh`: a deb, an rpm and the Arch package (glibc 2.34+); `qurb` alone, static, for any Linux, x86_64 and arm64; Android, every ABI, an APK each, one for all, an app bundle; Windows, zipped — built 2026-10-18, not published; what has run on what is [releasing.md](releasing.md) | ◻ |

---

## 7. Not built

Stated plainly so that a design does not assume it:

- **The relay on a server of the owner's own.** Since 2026-10-15 the
  rendezvous service on the laptop carries one, reached through its Funnel,
  so a phone on a network that blocks a direct path syncs through the laptop
  itself, while the laptop is on; not yet seen doing so on the S23. A VPS is
  still the plan for the services.
- **The owner's review of the design**, on both.
- **A published release.** 0.1.0 is built, nothing published
  ([releasing.md](releasing.md)).
- **Windows, run.** Built since 2026-10-18 and never run on Windows.
- **iOS and macOS.** Linux and Android first.
- **Placeholders on Linux.** A freed file is absent from the folder, not shown
  greyed out ([0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md)).
- **Restoring WhatsApp from the computer**, proven. The backup is kept and
  checked; putting it back on a phone and WhatsApp reading it has not been
  done ([0062](decisions/0062-backing-up-whatsapp.md)). Until it is, Google's
  backup should stay on.
- **The owner's folder sealed from the owner's own computer.** It opens with
  the phone's yes, which is Qurb's rule; the computer holds the same key
  ([0067](decisions/0067-a-folder-for-each-person.md)).
- **Six digits across networks.** They find the other device by multicast on
  the same Wi-Fi; elsewhere, or where multicast is blocked, the QR code.
- **An automatic updater**, deliberately ([0047](decisions/0047-versions-and-upgrades.md)).
- **Recovery on a computer.** A phone keeps its key in Block Store; a
  computer has nothing that leaves the machine, so a person with only
  computers who loses them all loses the key. And with every device lost the
  files are gone anyway, unless a replica keeps a copy
  ([0052](decisions/0052-the-key-travels-with-the-code.md)).
- **A passphrase on the phone** (the Android Keystore and the phone's own lock
  stand in), stopping a send mid-transfer, a replica freeing space, push
  without Firebase, and accounts or billing of any kind.
- **Unmeasured**: battery over a day on the phone, and how often a direct
  connection works across other networks.
