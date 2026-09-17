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

Measured on the emulator, receiving a file over a real QUIC connection: a 32 MiB
file grows the heap by 4 MiB, 128 MiB by 4 MiB, and 512 MiB by 5 MiB. Flat, which
is the property that matters.

## Running the tests on a device

```bash
./scripts/android-test.sh x86_64
```

Builds the test binaries for Android, pushes them with `adb`, runs them. No app,
no Gradle, no JVM: the FFI is a C ABI, and a test binary exercises the same Rust
an app calls through it. On an Android 14 emulator all 34 binaries pass — 420
tests, including the QUIC handshakes and hole punching.

This is the only thing that answers "does it work on Android?".
Cross-compiling proves the toolchain is right and says nothing about bionic's
libc, Android's filesystem semantics, or what its kernel does to a process that
allocates.

## What the surface looks like

Setup, then a handle:

```kotlin
if (!isSetUp(root)) {
    val setup = create(root)          // 24 words — show them once, then never again
}
val qurb = Qurb(root, null)
qurb.scan()                           // catch up with what changed while we were not running
```

Files by path, content by file:

```kotlin
qurb.list()                           // List<FileEntry>
qurb.export("album/photo.jpg", tmp)   // writes to tmp, returns bytes written
qurb.importFile(tmp, "album/photo.jpg")
qurb.remove("album/photo.jpg")
qurb.usage()                          // logical vs on-disk
```

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
```

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

**Errors.** A Rust error chain does not survive the crossing. Everything becomes
`QurbError`, which is flat, matchable, and short on purpose: it lists only the
cases a caller can *act* on — ask for the passphrase again, show the setup
screen, tell the user the file is gone, retry later. The original message is
kept as text for logs.

## Key protection on a phone

The desktop offers three options — a file, the OS keystore, or a passphrase.
Here the vault is an owner-only file inside the app's private storage, and
`Qurb::open` takes `null` for the passphrase.

That is weaker than it sounds on a desktop and stronger than it sounds here. An
app's private directory is enforced by the kernel, not by convention, and on
both platforms it is encrypted at rest by the device's own lock screen. The
realistic attacker against a phone is someone holding the phone, and they are
stopped by the lock screen rather than by anything this crate does.

The gap is a phone with no passcode set, where file-based protection provides
nothing. Passing a passphrase to `Qurb.open` works today and is the answer until
Keychain and Android Keystore are wired up — see below.

## Not built yet

- **Keychain and Android Keystore.** The `keyring` crate the desktop uses does
  not cover mobile; both platforms need their own binding through this crate.
- **Background scheduling.** `syncWithin` is the Rust half. The platform half —
  `WorkManager`, `BGTaskScheduler`, and deciding when to ask for a window at
  all — needs an app to live in.
- **Selective sync.** A phone cannot hold a desktop's library. Deciding what it
  keeps, and what happens when someone opens a file it does not have, is
  untouched.
- **iOS, at all.** The `staticlib` crate type is declared and the Swift bindings
  generate, but building for iOS needs Xcode, which needs a Mac. Nothing here
  about iOS is measured.
- **The generated bindings, compiled.** The Kotlin and Swift are exercised only
  as the Rust functions underneath them. Neither has been through a Kotlin or
  Swift toolchain.
- **A real phone.** Everything on-device ran on an x86_64 emulator, which
  imposes none of a phone's memory pressure, thermal limits or battery
  behaviour — and never suspends the process the way a backgrounded app is
  suspended. See `docs/phases/phase-5-mobile.md`.
