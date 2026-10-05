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
five tabs on 2026-09-29 has not been measured yet. A release build takes
longer, because link-time optimisation does; the debug build stays quick.

## What it does

Designed from the owner's direction
([docs/design/direction.md](../docs/design/direction.md), decision
[0048](../docs/decisions/0048-the-design-direction.md)), in the same language
as the desktop window: Qurb green on a quiet environment, Inter, Lucide icons,
glass surfaces. Four places under a floating tab bar, the places reached from
them, and the sheets that rise over them:

| place | what it is for |
|---|---|
| Home | is my Qurb space okay? One state — *Everything is synced*, *Syncing…*, files waiting to reach your devices, *Not synced yet*, *Add your first device* — with a ring that turns while syncing; one action, *Send to device*; a line of facts and *Sync now*; attention when a file has two versions ([0043](../docs/decisions/0043-settling-a-conflict.md)); Recent, and *See all* for Activity. Pulling down syncs |
| Files | the shared space, folder by folder: search across all of them, breadcrumbs, folders as tiles apart from files, and each file's state in words — *On this phone*, *Available elsewhere*, *Only copy here*, *Downloading*. Tapping a file opens a sheet: its details, then open, keep on this phone, free local space (never the only copy), send to a device, save a copy, rename, move, delete (into Recently deleted). ⋯ sorts, makes a folder, saves everything here to the phone. *Add files* adds into the shared space. Back goes up a folder |
| Private Vault | a step inside Files: this phone's own files, in the same browser. *Add files* here adds privately, whatever *Keep new files private* says |
| Devices | this phone and each paired device as cards, with when each was last here; a device's sheet chooses whether it keeps a backup of the Private Vault, sends it files, and removes it after saying what that does ([0041](../docs/decisions/0041-removing-a-device.md)). *Add* scans a code, shows one on this phone, or takes one typed |
| Settings | grouped lists: this phone and its key, devices, storage (space, who has each folder — [0044](../docs/decisions/0044-sharing-with-chosen-devices.md), [0045](../docs/decisions/0045-a-folder-kept-remotely.md) — Recently deleted, freeing unused space), privacy, notifications, the recovery phrase, appearance, and advanced (background sync, rendezvous, relay, version) |
| Activity | from Home: everything this phone did, newest first |
| Recently deleted | from Files and Settings: 30 days, restore everywhere or delete for good ([0042](../docs/decisions/0042-recently-deleted.md)) |
| Transfers | a bar above the tabs while this phone syncs or has sent something not yet collected; it opens a sheet with what is waiting (stoppable) and what finished |
| setup | create an identity, show the 24 words and have three of them typed back; or restore from them |
| scan | the camera, reading the code another device shows when connecting |
| share | anything on the phone, from the system share sheet: saved to Private Vault, saved to Files, or sent to one paired device |

Light only for now: the dark theme is designed after the light one is
approved. Sheets blur what is behind them where the phone does that (Android
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
asks for it back at the next sync — *Downloading* until it is here.

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

**The 24 words are checked, as on the desktop** (decision
[0033](../docs/decisions/0033-the-phrase-on-a-screen.md)). After "I have written
them down" the phone asks for three at random positions, drawn again each time
the words are looked at again, and the engine checks them against the key: the
app draws the words once and keeps no copy to compare with. Closed before the
check, the app comes back to it. Settings shows the words again, after saying
what they are. The windows that show them are kept out of screenshots and the
recent-apps view, and the fields that take them tell the keyboard not to learn
what is typed.

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
install, and refusing it stops nothing. Nothing else: no storage permission,
because the synced directory is the app's own private storage, and no
location, contacts or anything like them.

`allowBackup` is `false` on purpose. Android's backup would copy the vault to
Google's servers, and the Keystore key wrapping it does **not** travel — so a
restored copy would be an unreadable store that looks like a working one. The
recovery phrase is the supported way to move to a new device.

## Trying it with a computer

There is no hosted rendezvous service yet, so run one:

```bash
qurb signal 0.0.0.0:9000
```

In the app: Settings → **Rendezvous service** → `ws://<that machine's LAN
IP>:9000`. From an emulator use `ws://10.0.2.2:9000`, which is how it reaches
its host.

Then, on the computer, Devices → **Add a device** → *Show a code* in the
desktop app, or `qurb pair <dir>`; on the phone, Devices → **Add** → *Scan the
other device's code*, and point the camera at it.

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
worker. Ordinary passes stay silent. See
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
- **Large files, measured.** Collecting a large file from the phone was slow
  and failed part-way (decision 0050); what fixed it is built and tested, and
  the speed before and after has not been measured on the phone yet.
- **No storage question during setup, by design.** Phones have no allowance;
  the question is the desktop's, and is built there
  ([decision 0038](../docs/decisions/0038-the-storage-question-during-setup.md)).
- **Files received before 22 September show as only on this phone** even when
  the device that sent them still has them. Builds before then did not record
  where a received file came from, and a phone cannot learn it afterwards
  without asking; it errs the safe way, never offering to free such a file.
- **Measured, and reviewed, in its designed form.** Every place was walked on
  the S23 on 2026-10-03; decision 0039's measurements have not been repeated
  for it, and the owner has not reviewed it. See
  [phases/phase-5-mobile.md](../docs/phases/phase-5-mobile.md#the-designed-app-on-the-s23).
- **A dark theme.** Light only until the light design is approved; the night
  palette of the earlier screens was removed rather than left under the new
  one.
- **Notifications.** The phone raises none; Settings says so.
- **Not yet tried on the phone**, though built and run on the emulator:
  removing a device, the share sheet sending to a device, showing a pairing
  code to another device, and sharing a folder with chosen devices between
  real ones. See
  [phases/phase-5-mobile.md](../docs/phases/phase-5-mobile.md#checked-between-the-galaxy-s23-and-the-laptop).
- **The development phone runs a debug build.** Android refuses an update
  signed by a different key, so moving it to a release build means
  uninstalling once, which deletes its key and index. Not done — see Signing.
