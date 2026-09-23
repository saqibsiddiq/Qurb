# The Android app

A real app: it installs, sets up an identity, keeps the key in the Android
Keystore, lists files, pairs with another device and syncs.

```bash
./scripts/android-app.sh            # debug APK, about 21 MB
./scripts/android-app.sh install    # and install it on a connected device
./scripts/android-app.sh release    # unsigned release APK
```

## What it does

| screen | what it is for |
|---|---|
| setup | create an identity and show the 24 words, or restore from them |
| main | the files, what they cost on disk, and a Sync button |
| menu | pair, list paired devices, background sync, rendezvous service |
| share | anything on the phone, sent into qurb from the system share sheet |

The `+` button copies a file from elsewhere on the phone into the synced
directory. Sync runs one pass with a 25-second deadline and reports what
happened.

**It is a share target.** `ACTION_SEND` and `ACTION_SEND_MULTIPLE`, for any
type, and it needs no network to work: the file is written into the folder and
indexed there and then, with every other device switched off. There is no
outbox — "what is waiting to be delivered" is a question asked of the index,
not a list that could drift from it. The main screen says how many files are
held only by this phone, which is the honest form of "it will get there".

**It can be woken.** When another device has something and this one is asleep,
the rendezvous service pokes it and it syncs immediately — measured at seven
hundred milliseconds from the change. That needs a Firebase project; without
one the phone learns at its next scheduled look, and the SDK is not even linked.
See [decision 0028](../docs/decisions/0028-waking-a-sleeping-device.md).

**Tapping a file offers to open it, or save a copy to the phone.** That second
one matters more than it sounds: the synced directory is this app's private
storage, so a file that arrives from another device and stays there is invisible
to everything else on the phone. Without a way out, a sync product syncs into a
hole. Both actions go through the app's own `DocumentsProvider`, so there is one
path out of the store rather than two implementations of reading it.

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
insets the toolbar sits *beneath* the status bar — which looks wrong, and, worse,
makes the overflow button partly unreachable: taps in that strip go to the
status bar instead. The bug is invisible in a screenshot until you try to press
something, and it was found exactly that way.

The padding goes on the `AppBarLayout`, not the toolbar. Padding the toolbar
pushes its contents down inside a box that does not grow, so the title clips and
the overflow button is squashed — which was the first attempt at the fix.

## Permissions

`INTERNET` and `ACCESS_NETWORK_STATE`. Nothing else — no storage permission,
because the synced directory is the app's own private storage; no camera,
because pairing codes are typed; no location, contacts, or anything else.

`allowBackup` is `false` on purpose. Android's backup would copy the vault to
Google's servers, and the Keystore key wrapping it does **not** travel — so a
restored copy would be an unreadable store that looks like a working one. The
recovery phrase is the supported way to move to a new device.

## Trying it with a computer

There is no hosted rendezvous service yet, so run one:

```bash
qurb signal 0.0.0.0:9000
```

In the app: menu → **Rendezvous service** → `ws://<that machine's LAN IP>:9000`.
From an emulator use `ws://10.0.2.2:9000`, which is how it reaches its host.

Then `qurb pair <dir>` on the computer, and menu → **Pair a device** on the
phone with the code it prints.

## Background sync

`SyncWorker` is the half of [decision
0020](../docs/decisions/0020-sync-takes-a-deadline.md) that Rust cannot do.
`syncWithin(seconds)` makes a sync that ends when its window does; this decides
when to ask for a window and what to tell the system when one runs out.

WorkManager rather than an alarm or a foreground service: an alarm does not
survive Doze or a reboot, and a foreground service means a permanent
notification plus — since Android 14 — a declared type that "syncing files" does
not cleanly fit.

Every fifteen minutes, which is not a choice: it is the shortest period
WorkManager accepts, and asking for less silently becomes fifteen anyway. In
practice it is a floor rather than a promise, because Doze batches background
work and an idle phone may go hours between runs.

The three outcomes are mapped deliberately:

| what happened | what the worker says |
|---|---|
| ran out of time | `retry` — ask for another window sooner |
| nothing answered | `retry` — the other device is asleep, which is ordinary |
| keystore locked, no vault | `failure` — retrying will reach the same conclusion |

It records what it did, because a background worker is otherwise invisible:
nobody is watching when it runs, so without a trace there is no way to tell a
sync that works from one that silently stopped — and "silently stopped" is the
failure mode a sync app actually dies of. Menu → **Background sync** shows it.

## Not built

- **No bulk save.** One file at a time; there is no "save everything".
- **No way back for a dropped file.** A file the storage cap evicted is absent
  from the listing's point of view; `qurb fetch` exists on the desktop and has
  no equivalent here.
- **No reclaim or collection.** The desktop frees superseded chunks on a timer
  and can drop duplicate payloads with `qurb reclaim`; neither is exposed on
  the phone, so its store only grows.
- **Nothing for conflicts.** They arrive as extra files with long names and no
  explanation.
- **Not signed.** `assembleRelease` produces an unsigned APK.
