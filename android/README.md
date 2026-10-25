# The Android app

A real app: it installs, sets up an identity, keeps the key in the Android
Keystore, lists files, pairs with another device and syncs.

```bash
./scripts/android-app.sh            # debug APK, about 45 MB: two architectures, unshrunk
./scripts/android-app.sh install    # and install it on a connected device
./scripts/android-app.sh release    # release APK, about 11 MB; signed if a key is set up
```

**The app is meant to be light and snappy**, and the release build is where
that is decided: arm64 only, the code shrunk by R8, and the engine built with
the `mobile` profile — link-time optimisation across every crate. Measured on a
Galaxy S23: 10.7 MB installed against 46.7 MB for the debug build, the code it
keeps in memory down from 26.4 MB to 8.1 MB, and a cold start of about 175 ms.
The measurements and what they do and do not show are in
[decision 0039](../docs/decisions/0039-a-light-android-app.md), which also
measures the five-tab app against the one-list app it replaced: the same
startup, and a few megabytes more memory. The designed app that replaced the
five tabs on 2026-09-29, measured on 2026-10-08, starts as fast and holds
about 13 MB more, mostly in graphics. A release build takes
longer, because link-time optimisation does; the debug build stays quick.

## What it does

Designed from the owner's direction
([docs/design/direction.md](../docs/design/direction.md), decision
[0048](../docs/decisions/0048-the-design-direction.md)), in the same language
as the desktop window: Qurb green on a quiet environment, Inter, glass
surfaces; Phosphor icons since 2026-10-26, where the window still has Lucide's.
Three places under a floating tab bar, the places reached from
them, and the sheets that rise over them:

| place | what it is for |
|---|---|
| Home | is my Qurb space okay? One state — *Everything is synced*, *Syncing…*, files waiting to reach your devices, *Backing up to …*, *Waiting for … to collect*, *Not synced yet*, *Add your first device* — with a ring that turns while syncing; *Sync now* and *Send* side by side; a line of facts; attention when a file has two versions ([0043](../docs/decisions/0043-settling-a-conflict.md)); *Waiting* — files not on another device yet, sends not collected, backups still to go; *Backups* — WhatsApp and *Your folders*. Pulling down syncs |
| Files | since 2026-10-24 ([0073](../docs/decisions/0073-the-phone-works-with-its-computer.md)), three sections: *This phone* — Vault, Received, Deleted as tiles; *Shared* — each computer's shared folder, a card naming the computer and whether it is on; *Devices* — this phone's folder on each of its own computers. A phone keeps no shared files: what a computer keeps of the old shared area stays there, and the rest moved into the Vault when this build first opened (`FilesHome.kt`) |
| Your folder on … | from Files' *Devices*, while that computer is on: what it keeps for this phone — WhatsApp, the backed-up folders, the Private Vault, deleted files included, and what it kept before the phone was set up again — a hundred files at a time; a file is *Open*ed in the app for its type — over 16 MB, read as that app plays it, seekable, with nothing kept (`StreamProvider.kt`) — *Download*ed to Downloads/Qurb or *Restore*d where it was, checked before it is used ([0067](../docs/decisions/0067-a-folder-for-each-person.md), `KeptScreen.kt`). Its Private Vault part has *Add*, and a file there *Rename*, *Move to folder* and *Delete*; a backup can be deleted once its original is gone from the phone ([0073](../docs/decisions/0073-the-phone-works-with-its-computer.md)). No new folder: an empty one cannot be kept |
| Folders | from *Backups*, Home and Settings: the phone's folders as a tree, every one opening to any depth, each a checkbox -- ticked for all of it, a dash for some of what is inside, empty for none -- and its size; what is chosen and *Wi-Fi only* kept above the tab bar; chosen here or on the computer, the later choice winning ([0063](../docs/decisions/0063-backing-up-a-phones-folders-chosen-on-the-desktop.md), `FoldersScreen.kt`) |
| Vault | from Files: this phone's own files, by folder, searched; each file its name, its size, and an icon for where its bytes are — on this phone, on the computer keeping it, only here, on no device ([0055](../docs/decisions/0055-a-file-on-no-device-says-so.md)) — read out in words. Tapping one opens a sheet: open, keep on this phone, free local space (never the only copy), send, save a copy, rename, move, delete. *Add files* adds here |
| Received | from Files: what other devices sent to this phone, newest first, each with its sender; read from the last 500 happenings (`ReceivedScreen.kt`) |
| Shared | from Files: a computer's shared folder, opened there, nothing kept on the phone — search, folders, files; a new folder, files added, renamed, deleted into the computer's Recently deleted; the owner's phones and every guest's ([0073](../docs/decisions/0073-the-phone-works-with-its-computer.md), `SharedScreen.kt`) |
| Devices | this phone and each paired device as cards, with when each was last here; a device's sheet chooses whether it keeps a backup of the Private Vault, sends it files, and removes it after saying what that does ([0041](../docs/decisions/0041-removing-a-device.md)). *Add* scans a code, shows one on this phone (with six digits), or takes six digits on a number pad, joining as the sixth is typed ([0066](../docs/decisions/0066-six-digits-to-pair.md)) |
| Settings | four groups: this phone (name, identity, whether its key is kept), storage (space used, freeing space, copies of files you sent, Recently deleted), the app (notifications, theme, motion), connection (background sync, and *Advanced*: rendezvous, relay, version) |
| Activity | from Home: everything this phone did, newest first |
| Recently deleted | from Files and Settings: 30 days, each file's days left on a clock, restored or deleted for good ([0042](../docs/decisions/0042-recently-deleted.md)) |
| Transfers | a bar above the tabs while this phone syncs or has sent something not yet collected; it opens a sheet with what is waiting (stoppable) and what finished |
| setup | create an identity with nothing to write down; or join by scanning the code another device shows, or typing its six digits, which brings its key; or the key from the Google backup; or the 24 words |
| scan | the camera, reading the code another device shows when connecting |
| share | anything on the phone, from the system share sheet: saved to the Vault, or sent to one paired device |
| asked | *Open your folder on …?*: a computer asking to open this phone's folder there, the owner's own or one it visits; answered behind the fingerprint, face or screen lock, the yes sent within seconds; a notification when the app is away ([0060](../docs/decisions/0060-a-computer-keeps-private-folders-for-several-people.md), [0067](../docs/decisions/0067-a-folder-for-each-person.md)) |

Light or dark, as Settings → Theme says or as the phone is set
([decision 0056](../docs/decisions/0056-dark-mode.md)): the colour roles have
night values in `values-night/`, and the drawables name roles too. Sheets blur what is behind them where the phone does that (Android
12 and later, when the device allows); elsewhere a surface is a translucent
fill over the still environment, which looks the same and costs nothing.
Motion follows the phone's animation setting: with animations off, nothing
moves.

**Files that arrive on the phone are private by default** — decision
[0036](../docs/decisions/0036-a-phone-keeps-its-own-files.md). They go to no
other device until the person chooses one on the Devices screen to keep them,
and that device keeps them where nobody using it sees them. *Keep new files
private* in Settings turns that off, from the next file on; files already here
stay where they are.

**A file added with a choice of area goes there**, whatever that setting says
— decision [0049](../docs/decisions/0049-adding-a-file-puts-it-where-you-are-looking.md).
*Add files* in Files puts it in the shared area, which every device sees; in
Private Vault, in this phone's own. So do the share sheet's *Save to Files* and
*Save to Private Vault*. The setting decides the rest: what other apps save
into qurb through the system picker, and a share when no device is paired.

**Freeing space is refused for the only copy.** *Free local space* is offered
only for a file another device is known to hold, and the engine refuses it
anyway when that is not so, so no screen can get it wrong. A freed file stays in
the list, marked *Available elsewhere*, and *Keep on this phone* in its sheet
asks for it back at the next sync — *Downloading* until it is here. A file no
device this phone syncs with has is marked *On no device*, and its sheet says
why and offers only *Delete*; the system file picker shows it with that
summary and does not try to open it.

The screens are plain classes holding their views, not Fragments: built the
first time each is shown, kept for the life of the activity, and changing tab
swaps one child view for another; the places reached from a tab go on a small
stack that Back leaves. No screen reads anything on the main thread. Each draws
what the index already knows, and the scan for changes made while the app was
closed runs after that and redraws only if it found something. What they are
built from — rows, file states, groups, toggles, attention, empty states,
sheets — is one file, `Kit.kt`, the counterpart of the desktop's `core.js`.

**A change made in the app goes at once.** Sending a file, adding, renaming,
moving or deleting one, choosing a device to keep the Private Vault, and adding
a device each start a sync straight away (`MainActivity.madeChange`); a change
made while one is running gets another pass after it. A phone has no daemon to
pass a change on the way the desktop does, so before this a change waited for
the next background pass — an hour away once push works — or for *Sync now*.
A send says how it went: *Sending to Laptop…*, then *Sent to Laptop* if the
laptop collected it during the pass, or *Waiting for Laptop* if it is off.

**It is a share target.** `ACTION_SEND` and `ACTION_SEND_MULTIPLE`, for any
type, and it needs no network to work: the file is written into the folder and
indexed there and then, with every other device switched off. There is no
outbox — "what is waiting to be delivered" is a question asked of the index,
not a list that could drift from it. Home says how many files are held only by
this phone, which is the honest form of "it will get there".

**It can be woken.** When another device has something and this one is asleep,
the rendezvous service pokes it and it syncs immediately — measured at seven
hundred milliseconds from the change. That needs a Firebase project; without
one the phone learns at its next scheduled look, and the SDK is not even linked.
See [decision 0028](../docs/decisions/0028-waking-a-sleeping-device.md).

**Opening a file, and saving a copy to the phone, matter more than they
sound.** The synced directory is this app's private storage, so a file that
arrives from another device and stays there is invisible to everything else on
the phone. Without a way out, a sync product syncs into a hole. Opening goes
through the app's own `DocumentsProvider`, the same path the system file picker
uses; saving a copy exports through the engine into a cache file and copies
that where the person chose (below).

**In the system file picker and the Files app, qurb lists what the engine
knows**, not what is on disk: a file freed from this phone is still there, says
it is not on this phone, and is downloaded when it is opened — asked for and
synced while the opening app waits, up to 25 seconds. Another app can save into
qurb through the picker; a name already taken gets `(2)`, and closing the file
starts a scan and a sync. Deleting and renaming from a file manager are still
refused: a deletion becomes a tombstone on every device, and there is no undo.

**Nothing to write down** (decision
[0052](../docs/decisions/0052-the-key-travels-with-the-code.md), which
replaced 0033's check on 2026-10-05). *Set up Qurb here* makes a key and opens
the app. *I already use Qurb* scans the code a computer or phone shows, or
takes it typed, and the key arrives with it (`joinNew`). *Use my 24 words
instead* is the fallback. The key is kept in Block Store (`Backup.kt`); when
the phone has a screen lock it is backed up end-to-end encrypted, and setup
offers it back as *Use the key from your Google backup*. A phone that began
setup under the old rules and never typed its words back can keep that key or
join instead; joining puts the unused key aside, provided the folder is still
empty. Settings can still show the words.

Saving streams through a cache file rather than a byte array, because `export`
writes a chunk at a time precisely so a large file never has to fit in the heap
— reading it back into memory at the last step would throw that away.

## Three pieces, kept apart on purpose

**The engine is Rust.** Everything here is a thin layer over
[`qurb-mobile`](../crates/mobile-ffi/), which is a thin layer over the same
engine the desktop daemon runs. No sync logic lives in Kotlin.

**Cargo is not wired into Gradle.** `scripts/android-app.sh` builds the native
libraries, strips them, copies them into `jniLibs`, regenerates the Kotlin
bindings, and *then* runs Gradle. That means a Rust change is an explicit step
rather than something that happens invisibly inside an IDE, and the app builds
from a clean checkout on a machine with no NDK as long as the `.so` files are
already in place.

**`jniLibs/` and `java/uniffi/` are generated and not committed.** Both are
regenerated every build. Committing them would mean a stale binary or stale
bindings could ship without anyone noticing — and bindings that drift from the
library they describe fail at runtime rather than at compile time.

## The keystore

[`AndroidKeyStore.kt`](app/src/main/java/com/qurb/AndroidKeyStore.kt) implements
the `KeyStore` interface that `qurb-mobile` declares but deliberately does not
fill in — see
[decision 0021](../docs/decisions/0021-the-platform-supplies-the-keystore.md).

Two layers, because the Android Keystore holds *keys* and performs operations
with them rather than storing arbitrary bytes:

- a 256-bit AES key lives in the Keystore, generated once, never exported;
- the 32-byte master key is encrypted under it, and the ciphertext goes in
  ordinary SharedPreferences.

Verified on a device. After setup, the vault file is five bytes —

```
$ adb shell run-as com.qurb od -An -c files/qurb/.qurb/master.key
   Q   R   B   K 004
```

— which is the magic and a format byte saying "the platform has it". The key
itself is nowhere in the app's files. Reading the whole data directory yields a
ciphertext and an IV and nothing that decrypts them.

`setUserAuthenticationRequired` is deliberately **not** set: it would demand a
fingerprint every time the key is touched, including during a background sync
when nobody is holding the phone, and sync would simply stop happening.

## Window insets

Android 15 draws apps edge to edge whether they ask or not. Without handling
insets a screen's heading sits *beneath* the status bar — which looks wrong,
and, worse, taps in that strip go to the status bar instead. The bug is
invisible in a screenshot until you try to press something, and it was found
exactly that way, on the first version of the app.

The shell pads the screen area for the status bar and any display cutout, once,
and lifts the floating tab bar, with the Transfers bar above it, clear of the
gesture bar.

## Permissions

`INTERNET` and `ACCESS_NETWORK_STATE`, to sync; `CHANGE_WIFI_MULTICAST_STATE`,
to hear devices on the same Wi-Fi answer; `CAMERA`, asked for only when
scanning a code to add a device, and refusable — the code can be typed
instead. `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC` and
`POST_NOTIFICATIONS`, for one thing only: a long transfer runs in the
foreground under a notification, so Android does not freeze the app while a
device collects a large file from it ([decision 0050](../docs/decisions/0050-large-files-from-a-phone.md)).
The notification permission is asked for the first time that happens, not at
install, and refusing it stops nothing. `USE_BIOMETRIC`, to confirm it is the
person before a computer may open their folder. `MANAGE_EXTERNAL_STORAGE`
("All files access"), for one thing: backing up the phone's own folders where
they lie ([decision 0063](../docs/decisions/0063-backing-up-a-phones-folders-chosen-on-the-desktop.md)),
granted by the person on the system's page after Settings says why; without
it only WhatsApp's folder, picked once, is read, and the synced directory is
the app's own private storage either way. Android 10 and older have no "All
files access": there the same reach is `READ_EXTERNAL_STORAGE` and
`WRITE_EXTERNAL_STORAGE`, asked in a dialog, declared up to API 29 only.
Nothing else: no location, contacts or anything like them.

## Every Android since 8.0

`minSdk` is 26. Until 2026-10-18 nothing older than the S23's Android 16 had
run the app, and lint found what that hid: on Android 8, 9 and 10 it stopped,
at calls those versions do not have -- "All files access" asked of on every
pass, headings marked for accessibility on every screen, and on 9 the screen
lock asked for confirmation. Each is now asked of a way that version has
(`ViewCompat` for headings; the screen lock's own page before Android 10).
WhatsApp's folder is accepted where Android 10 and older keep it,
`WhatsApp/` at the top of storage. On 8.0 the navigation bar sits on a scrim,
since 8.0 draws its buttons white whatever the page, and the semibold weights
read as bold. Checked on an Android 8.0 emulator without Google Play
services, which also showed Block Store failing as it should there: setup
went on without it. Lint reports no errors since.

`./scripts/android-app.sh release-all` builds every ABI -- arm64-v8a,
armeabi-v7a, x86 and x86_64 -- as an APK each, one holding all four, and an
app bundle ([releasing](../docs/releasing.md)). The everyday release stays
arm64 alone (decision 0039).

`allowBackup` is `false` on purpose. Android's backup would copy the vault to
Google's servers, and the Keystore key wrapping it does **not** travel — so a
restored copy would be an unreadable store that looks like a working one. The
supported way to move to a new device is to join with a code from one of the
person's devices, or the key Block Store kept (decision 0052).

## Trying it with a computer

There is no hosted rendezvous service yet, so run one:

```bash
qurb signal 0.0.0.0:9000
```

In the app: Settings → **Rendezvous service** → `ws://<that machine's LAN
IP>:9000`. An emulator with nothing set uses `ws://10.0.2.2:9000`, which is
how it reaches its host; a phone with nothing set uses none, and says so --
only devices on the same Wi-Fi -- rather than an address that leads nowhere.

Then, on the computer, Devices → **Add a device** → *Show a code* in the
desktop app, or `qurb pair <dir>`; on the phone, Devices → **Add** → *Scan the
other device's code*, and point the camera at it, or *Enter a code* and type
the six digits on the same Wi-Fi.

## Background sync

`SyncWorker` is the half of [decision
0020](../docs/decisions/0020-sync-takes-a-deadline.md) that Rust cannot do.
`syncWithin(seconds)` makes a sync that ends when its window does; this decides
when to ask for a window and what to tell the system when one runs out.

WorkManager rather than an alarm or a foreground service: an alarm does not
survive Doze or a reboot, and a foreground service means a permanent
notification plus — since Android 14 — a declared type that "syncing files" does
not cleanly fit.

Every fifteen minutes — the shortest period WorkManager accepts — or **every
hour once pushes are demonstrably arriving** (one in the last seven days).
With push, a change on another device wakes the phone within seconds, and the
scheduled pass is left with what push does not cover: files added to the
phone's folder by another app, and telling a rendezvous service how to wake the
phone if it has lost that. An hour covers those at a quarter of the wake-ups.
"Demonstrably", rather than "the app was built with Firebase", because a
rendezvous service without push credentials never sends one. Either period is
a floor rather than a promise: Doze batches background work, and an idle phone
may go longer between runs.

The three outcomes are mapped deliberately:

| what happened | what the worker says |
|---|---|
| ran out of time | `retry` — ask for another window sooner |
| nothing answered | `retry` — the other device is asleep, which is ordinary |
| keystore locked, no vault | `failure` — retrying will reach the same conclusion |

It records what it did, because a background worker is otherwise invisible:
nobody is watching when it runs, so without a trace there is no way to tell a
sync that works from one that silently stopped — and "silently stopped" is the
failure mode a sync app actually dies of. Settings → **Background sync** shows
it.

**A long pass runs in the foreground.** When 32 MiB or more is waiting for
another device to collect, the worker declares itself a foreground service of
type `dataSync`, shows *Sending to your devices* with the bytes sent so far,
and keeps answering for as long as a device is collecting — up to thirty
minutes — instead of ending at its window. Android freezes an app nobody is
looking at; an 800 MB video the laptop was collecting stopped there, every
time. *Sync now*, and every sync the app starts, hands such a pass to the
worker. Ordinary passes stay silent. While the app is on screen it watches the
worker's passes (`SyncWorker.running`), shows them as syncing and holds its
own syncs until they end. `runNow` appends behind a running pass, because
replacing it cancelled a transfer part-way. Watched on the S23 on 2026-10-05:
Home read *Syncing…* through a worker's pass and redrew itself when it ended. See
[decision 0050](../docs/decisions/0050-large-files-from-a-phone.md).

A pass that reached a device and has something waiting for it stays open up
to ten seconds, until that device has collected it: every device pulls, and a
phone that finishes its own syncing in a second would otherwise close before
the desktop could dial back. Found when a desktop chosen to keep the phone's
files never received one.

A device that does not answer before the window closes is *unreachable*, not
*out of time*. The difference decides what happens next — out of time is a
retry, with exponential backoff — and it was once got wrong, so that a phone
whose computer was switched off reported "no paired devices" and pushed its
next sync further and further away. See
[decision 0020](../docs/decisions/0020-sync-takes-a-deadline.md#found-on-a-phone).

## A second copy, for trying what wipes data

```bash
cd android && gradle assembleDebug -Pqurb.idSuffix=.storetest
```

builds the app as `com.qurb.storetest`. It installs beside the real one with
its own data, its own keystore and its own Block Store, and it builds without
push, since Firebase knows only `com.qurb`. That is how *Clear data* and a
reinstall were tried on the S23 without touching the person's app
([phase 5](../docs/phases/phase-5-mobile.md#finishing-what-was-left)). Clear
its data before uninstalling it: that removes its key from Block Store, and
an uninstall alone does not.

## The build the phone runs

`./scripts/android-app.sh fast` is a release in everything but its key:
shrunk and optimised by R8, not debuggable, profileable, and signed with the
debug key, so it installs over a phone that has run debug builds without
uninstalling -- which would delete that phone's key and index. On the S23 on
2026-10-15 it started 3.1 times faster than the debug build it replaced
(median 545 → 174 ms), measured the same way. It cannot be read with
`run-as`; install the debug build for that, over it, and back.

## Signing

A release APK (`./scripts/android-app.sh release`) is signed when
`~/.config/qurb/signing.properties` exists — or the file `$QURB_SIGNING`
names — holding `storeFile`, `storePassword`, `keyAlias` and `keyPassword`.
Without it the release builds unsigned, as before. The key for this project was
generated on 2026-09-28 at `~/.android/qurb-release.jks` (RSA 4096, alias
`qurb`, SHA-256 `4F:AE:59:0B:…:3F:FD:2A`), with a random password in that
properties file; both are readable by their owner only and are **not** in the
repository.

Back both up. Android installs an update only if it is signed by the same key
as the installed app, so losing the key means every installed copy can only be
replaced by uninstalling — which deletes the phone's key and index. See
[decision 0047](../docs/decisions/0047-versions-and-upgrades.md).

## Not built

- **Progress in the app.** A long transfer from the phone shows how far it
  has got in its notification; inside the app, the Transfers bar still shows
  what is waiting and what happened, not bytes in flight.
- **Large files, measured outside the app.** Collecting a large file from the
  phone was slow and failed part-way (decision 0050). Measured on the S23 on
  2026-10-05, the long pass kept serving after the app was left and resuming
  worked, but the speed stayed near 5 MB/s: quinn's default congestion
  controller, Cubic, read Wi-Fi's stray losses as congestion. With BBR
  (decision 0051) it goes at 10.6–12.1 MB/s through the app. A collector that
  pauses mid-file is waited for up to a minute, which was watched. Started
  after the app has left the screen, as when it is left during a long
  import, the long pass is refused by Android. It then runs as an ordinary
  one and leaves *Tap to finish sending*, watched on 2026-10-08
  ([phase 5](../docs/phases/phase-5-mobile.md#through-the-app-with-bbr--and-a-phone-cleared)).
- **Clear data lost what only the phone held**, on 2026-10-05: 18 files the
  laptop had freed because the phone kept them, and its Private Vault. Since
  decision 0053 *Clear data* opens `ManageSpaceActivity`, which lists what
  exists only here before anything goes; uninstalling asks to keep the data;
  the laptop no longer frees what only a phone also has; and the phone's
  vault is kept by its first computer. Watched on the S23 on 2026-10-07,
  except for uninstalling.
- **A send keeps no copy** since 2026-10-10 (decision 0060). A file from the
  picker is read from where it is when the other device collects it, the app
  keeping its permission to read the document until then; a file from the
  share sheet is copied into the app's files and the copy deleted once
  collected; a file already in qurb is read from the folder. Changing or
  deleting the file first calls the send off, and the history says why.
- **Copies of files the phone sent before that** were kept after they
  arrived (decision 0030), and a phone has no storage cap to let go of them. Settings shows
  *Copies of files you sent* when there are any, and the Clear-data screen
  offers *Let go of copies of sent files*. Both warn first that a recipient
  may have deleted its copy since. *Free unused space* does not touch them
  (`SentCopies.kt`). The storage screen's line, the Settings row ("1.6 GB,
  kept here after they arrived") and its warning were watched, the warning
  closed with *Keep*; letting go was not tapped.
- **Notifications** (`Notices.kt`), the desktop's three and nothing else:
  somebody sent you a file, a device collected one you sent, something
  failed. Read from the history after each sync, at most three at once,
  never while the app is on screen, and never about history from before the
  build that brought them. Settings → Notifications turns them off, or asks
  Android to allow them.
- **Pairing is approved** (`Approval.kt`): the device showing the code asks
  whether to let a device in, with the six digits it should be showing, and
  the phone joining shows its digits meanwhile (decision 0053). Watched both
  ways with the laptop's command line, and declining, on 2026-10-07.
- **No storage question during setup, by design.** Phones have no allowance;
  the question is the desktop's, and is built there
  ([decision 0038](../docs/decisions/0038-the-storage-question-during-setup.md)).
- **Files received before 22 September show as only on this phone** even when
  the device that sent them still has them. Builds before then did not record
  where a received file came from, and a phone cannot learn it afterwards
  without asking; it errs the safe way, never offering to free such a file.
- **Reviewed in its designed form.** Every place was walked on the S23 on
  2026-10-03 and the app measured there on 2026-10-08 (decision 0039: the
  same startup, about 13 MB more memory, mostly graphics); the owner uses it
  and his lists say what he found. See
  [phases/phase-5-mobile.md](../docs/phases/phase-5-mobile.md#the-designed-app-on-the-s23).
- **Not yet tried on the phone**, though built and run on the emulator:
  removing a device, the share sheet sending to a device, showing a pairing
  code to another device, sharing a folder with chosen devices between real
  ones, typing six digits, sending to *My folder on …*, and downloading or
  restoring from *Your folder on …* (browsed on the S23 on 2026-10-15).
- **Restoring WhatsApp from the computer** has not been done, so the backup
  is not proven to bring WhatsApp back (decision 0062). Until it is, keep
  WhatsApp's own Google backup on. See
  [phases/phase-5-mobile.md](../docs/phases/phase-5-mobile.md#checked-between-the-galaxy-s23-and-the-laptop).
- **The development phone runs a debug build.** Android refuses an update
  signed by a different key, so moving it to a release build means
  uninstalling once, which deletes its key and index. Not done — see Signing.
