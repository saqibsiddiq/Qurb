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
| menu | pair with a device, list paired devices, set the rendezvous service |

The `+` button copies a file from elsewhere on the phone into the synced
directory. Sync runs one pass with a 25-second deadline and reports what
happened.

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

## Not built

- **No background sync.** `syncWithin` exists and is built for it; nothing
  schedules it. That needs `WorkManager` and a policy about when to ask.
- **No FileProvider**, so the files are invisible to the rest of the phone.
  This screen is the only way to see them.
- **No QR scanning.** Pairing codes are typed. A scanner needs a camera
  dependency and a permission; the code is designed to be read aloud anyway.
- **Nothing opens a file.** You can add and sync files, not view them.
- **Not signed.** `assembleRelease` produces an unsigned APK.
