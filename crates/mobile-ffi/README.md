# qurb-mobile

The engine, as a phone can call it.

Android and iOS cannot call Rust directly. This crate is the seam: a small,
deliberately boring surface that [UniFFI](https://mozilla.github.io/uniffi-rs/)
turns into Kotlin and Swift. It contains no sync logic of its own — everything
delegates to `qurb-engine` — because logic behind an FFI boundary is logic that
cannot be tested from the rest of the workspace.

## Generating the bindings

```bash
./scripts/mobile-bindings.sh
```

Writes Kotlin to `target/bindings/kotlin` and Swift to `target/bindings/swift`.
Neither is committed: both platforms' build systems regenerate them, because
bindings that drift from the library they describe fail at runtime rather than
at compile time.

## Building for Android

```bash
./scripts/android-build.sh --release
```

All four architectures. The script finds the NDK; set `ANDROID_NDK_HOME` if it
cannot. SQLite is the only C dependency in the tree and the only reason the NDK
is needed at all.

Stripped, the library is 3.2–3.9 MB depending on architecture — roughly what the
engine adds to an app download, with SQLite, Zstd, BLAKE3, XChaCha20-Poly1305
and QUIC all inside it.

Measured on that phone, receiving a file over a real QUIC connection: a 32 MiB
file grows the heap by 8 MiB, 128 MiB by 5 MiB, and 512 MiB by 6 MiB. Flat as the
file grows sixteen-fold, which is the property that matters.

## Running the tests on a device

```bash
./scripts/android-test.sh x86_64
```

Builds the test binaries for Android, pushes them with `adb`, runs them. No app,
no Gradle, no JVM: the FFI is a C ABI, and a test binary exercises the same Rust
an app calls through it. On a Samsung Galaxy S23 (Android 16, arm64-v8a) all 35
binaries pass — 426 tests in 104 seconds, including the QUIC handshakes and hole
punching.

This is the only thing that answers "does it work on Android?".
Cross-compiling proves the toolchain is right and says nothing about bionic's
libc, Android's filesystem semantics, or what its kernel does to a process that
allocates.

## What the surface looks like

Setup, then a handle:

```kotlin
if (!isSetUp(root)) {
    val setup = create(root)          // a new key; or joinNew(root, code, ...) to take another device's
}
val qurb = Qurb(root, null)
qurb.scan()                           // catch up with what changed while we were not running
```

Files by path, content by file:

```kotlin
qurb.list()                           // List<FileEntry>, every file in the folder
qurb.page(0, 100)                     //   ...or a screenful at a time
qurb.export("album/photo.jpg", tmp)   // writes to tmp, returns bytes written
qurb.importFile(tmp, "album/photo.jpg")
qurb.remove("album/photo.jpg")
qurb.usage()                          // the files' size, and what qurb takes on the phone
qurb.outstanding()                    // what this device made and nobody else has
qurb.housekeep()                      // free what nothing needs; run it off the main thread
```

Each `FileEntry` says where its bytes are — `Here`, `Elsewhere` or `OnlyHere`,
decided in the storage crate so the phone and the desktop cannot disagree — and
whether it is in the phone's own vault.

A phone's own files, and a device to keep them (decision 0036):

```kotlin
Settings(..., ownFilesPrivate = true) // new files go into this phone's own vault
qurb.addHolder(peer.fingerprint)      // that device keeps a copy it never shows
qurb.holders(); qurb.removeHolder(peer.fingerprint)
qurb.freeLocal("IMG_0001.jpg")        // throws OnlyCopy if nobody else has it
qurb.fetch("IMG_0001.jpg")            // back at the next sync with a device that has it
```

Sending, and what happened:

```kotlin
qurb.sendFile(staged, "photo.jpg", peer.fingerprint)
qurb.waiting()                        // sent, not yet collected
qurb.cancelSend("photo.jpg", peer.fingerprint)
qurb.history(50, null)                // newest first; pass the last id to page back
```

Browsing by folder, as Files, Private Vault and the system file picker all do
— one index query each, so they cannot disagree:

```kotlin
qurb.browse("album")                  // Directory: its folders, and its files
qurb.browseIn("album", false)         //   ...in one area: false shared, true private
qurb.entry("album/photo.jpg")         // one file, or null
qurb.search("beach", 100)             // anywhere in a path
qurb.searchIn("beach", 100, true)     //   ...in one area
qurb.importInto(tmp, "a.jpg", true)   // added into that area, whatever the setting
qurb.rename("a.jpg", "album/a.jpg")   // keeps the file's area: shared stays shared
qurb.makeFolder("album/2026")
```

The system file picker has no notion of an area and uses the unrestricted
calls. Files and Private Vault each use the `In` forms, and *Add files* in each
adds into it — decision
[0049](../../docs/decisions/0049-adding-a-file-puts-it-where-you-are-looking.md).

Deleting, conflicts, and who has what (decisions 0041–0045):

```kotlin
qurb.recentlyDeleted()                // 30 days; restoreDeleted(id), forgetDeleted(id)
qurb.conflicts()                      // each with both sides, named by device
qurb.settleConflict(copy, "both")     // "this", "other" or "both"
qurb.sharing(); qurb.shareTargets()   // each folder's devices, and who could be chosen
qurb.setSharing("work", listOf(me, laptop))   // device ids; an empty list is every device
qurb.keepRemotely("videos")           // listed here, fetched when opened
qurb.keepLocally("videos")
qurb.removalPlan(peer.fingerprint)    // what removing it would do, before doing it
qurb.removeDevice(peer.fingerprint, deleteKept = false)
```

And two free functions: `engineVersion()` — `qurb 0.1.0 · protocol qurb/2 ·
index schema 15` — and `qrCode(text)`, the matrix a phone draws to show a
pairing code, so the app needs no image library.

`outstanding()` is the honest answer to "did it get there yet": live files this
device made whose content no other device is known to hold. It is a question
asked of the index each time, not a queue that could drift from it — and it
empties when a peer says it holds the content, which is the one message in the
protocol that asks for nothing.

The second device, from the words:

```kotlin
restore(root, "wheel push industry ...")
```

Pairing, then syncing:

```kotlin
val offer = qurb.offerPairing()       // show offer.code() as a QR code
offer.spoken()                        //   ...or read this out
val peer = offer.wait()               // blocks until someone joins

qurb.joinPairing(scannedCode)         // the other direction, usually the phone's
qurb.peers()                          // who this device trusts

qurb.syncWithin(25)                   // one pass, giving up after 25 seconds
qurb.waitingForOthersBytes()          // what a device reached would come and take
qurb.syncServing(20, 1800)            // a pass that goes on answering while a device
                                      //   collects, up to 30 minutes (decision 0050)
qurb.serving()                        // bytes handed over so far; no lock, so a
                                      //   notification can ask while a pass runs
```

`Settings.wakeToken` carries the platform's push token, so the rendezvous
service can poke this device when another has something and this one is asleep.
`null` — the default, and right on a desktop — means it is never woken and
syncs when it next looks.

The pairing code carries this device's **full** fingerprint and must travel out
of band — a QR code on screen, or digits read aloud. Sending it over the network
being paired defeats the point: someone who can change what you see has already
won.

## Four constraints, all from the platforms

**Memory.** An iOS FileProvider extension is killed at a ceiling in the tens of
megabytes. Nothing here returns a file's contents: `export` writes to a path the
caller supplies, `importFile` reads from one. Peak memory is one chunk — at most
2 MiB — however large the file is. A test asserts this rather than a comment
claiming it, so a change that reintroduces buffering fails rather than ships.

**Threading.** The engine takes `&mut self`, so one lock guards it. Calls block.
The platform side is expected to make them off the main thread, which Kotlin
coroutines and Swift's `async` both do naturally. A tokio runtime is built on
first use and kept — lazily, so an app that only browses its files never pays
for a thread pool.

**Time.** `syncWithin(seconds)` takes a deadline because both platforms grant
background work a window and kill anything that outstays one. "Sync until
finished" is how an app loses its background privileges. Running out of time
sets `SyncOutcome.timedOut` and is not an error: every file is committed as it
lands, so a pass that stops early leaves work done rather than work lost. Pass
something generous when the app is in the foreground and the user is watching;
pass what the platform granted when it is not.

The one exception is a device collecting a large file *from* the phone: every
device pulls, so the phone has to keep answering until the other side is done,
and a pass that ended at its window cut an 800 MB video off part-way.
`syncServing(seconds, servingSeconds)` keeps answering while a chunk has gone
in the last ten seconds, or the last minute while what was being collected is
still waiting, up to `servingSeconds` — for a caller the platform will
let run that long, which on Android is a worker in the foreground
([decision 0050](../../docs/decisions/0050-large-files-from-a-phone.md)).

**Errors.** A Rust error chain does not survive the crossing. Everything becomes
`QurbError`, which is flat, matchable, and short on purpose: it lists only the
cases a caller can *act* on — ask for the passphrase again, show the setup
screen, tell the user the file is gone, retry later. The original message is
kept as text for logs.

## Key protection on a phone

The master key is 32 bytes that decrypt everything the user owns. Three ways to
keep it, in increasing order of what they defend against:

| | defends against |
|---|---|
| a file in app-private storage | other apps, enforced by the kernel |
| a passphrase | someone with the unlocked phone |
| **the platform keystore** | reading the disk, and a locked phone |

The keystore is the one to use, and it needs the app's help: Android's is a Java
API needing a `Context`, and iOS's needs entitlements belonging to an app
bundle. Neither is reachable from Rust. So the app implements `KeyStore` and
passes it in:

```kotlin
val setup = createProtected(root, myKeyStore)   // first launch
val qurb = Qurb.openProtected(root, myKeyStore, Settings())
```

Whatever is chosen is recorded in the vault, and `protectionOf(root)` reports
it. Opening a keystore-protected vault *without* the keystore fails saying so
rather than falling back — a silent fallback would mean reading a key that is
not there and reporting something less useful.

### Android

`EncryptedSharedPreferences` is the short version, and on a device with a secure
element the key backing it never leaves the hardware:

```kotlin
class AndroidKeyStore(context: Context) : KeyStore {
    private val prefs = EncryptedSharedPreferences.create(
        context, "qurb-keys",
        MasterKey.Builder(context).setKeyScheme(AES256_GCM).build(),
        AES256_SIV, AES256_GCM,
    )

    override fun put(label: String, secret: ByteArray) {
        prefs.edit().putString(label, Base64.encodeToString(secret, NO_WRAP)).apply()
    }
    override fun get(label: String): ByteArray? =
        prefs.getString(label, null)?.let { Base64.decode(it, NO_WRAP) }
    override fun remove(label: String) {
        prefs.edit().remove(label).apply()
    }
}
```

### iOS

`kSecAttrAccessibleWhenUnlockedThisDeviceOnly` is the important part: unreadable
while the phone is locked, and it does not travel to a backup or another device.
A key that syncs to iCloud Keychain would defeat the point of the phrase.

```swift
final class IOSKeyStore: KeyStore {
    func put(label: String, secret: Data) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: label,
        ]
        SecItemDelete(query as CFDictionary)
        var add = query
        add[kSecValueData as String] = secret
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        guard SecItemAdd(add as CFDictionary, nil) == errSecSuccess else {
            throw QurbError.Other(detail: "could not write to the keychain")
        }
    }
    // get and remove follow the same shape; return nil on errSecItemNotFound
    // rather than throwing -- a first launch asks before anything is stored.
}
```

### What none of it does

Nothing here helps while the app is running and holding the key in memory. That
is what it means to be a program that can decrypt your files. The keystore
protects a phone that is off, locked, or being read by something that is not
this app.

## Not built yet

- **Keychain, on iOS.** The Android Keystore half is written and runs on a
  Galaxy S23 (`android/app/.../AndroidKeyStore.kt`); nothing has been written
  against the same contract for iOS.
- **`BGTaskScheduler`.** Android's background scheduling is built, on
  WorkManager; iOS's is not.
- **Selective sync, automatically.** A person can free a file, or keep a
  folder only remotely (`keep_remotely`, decision 0045), and a freed file
  downloads when opened. Nothing decides on its own what a phone keeps.
- **iOS, at all.** The `staticlib` crate type is declared and the Swift bindings
  generate, but building for iOS needs Xcode, which needs a Mac. Nothing here
  about iOS is measured.
- **The Swift bindings, compiled.** The Kotlin bindings are compiled into the
  Android app on every build; the Swift ones have never been through a Swift
  toolchain.
- **An app's conditions.** The tests run from `/data/local/tmp` as a shell user
  on a plugged-in, awake phone. Nothing measures battery cost, what survives a
  suspend, or how the platform treats a backgrounded process. See
  `docs/phases/phase-5-mobile.md`.
