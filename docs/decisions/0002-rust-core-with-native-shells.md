# 0002 — Rust core with native UI shells

**Status:** Accepted
**Date:** 2026-09-07

## Decision

All logic that is not user interface — chunking, storage, cryptography,
synchronisation state, networking — lives in one Rust library compiled for every
target. User interfaces are thin and platform-specific: Tauri on desktop,
SwiftUI on iOS, Jetpack Compose on Android.

## Reasoning

A background storage daemon runs constantly and touches every file a user owns.
That argues for a language with predictable memory behaviour, no garbage
collection pauses during large sync passes, and memory safety in code that
parses untrusted data. Rust is the mainstream option that has all three.

The mobile decision is driven by the platforms rather than by preference.
Android background execution is governed by `WorkManager`, iOS by
`BGTaskScheduler` and `FileProvider` extensions. These are native APIs with
native lifecycles and no adequate cross-platform abstraction. Meanwhile the
synchronisation state machine and cryptographic code are exactly the parts that
must not be reimplemented twice, because two implementations means two sets of
bugs in the code that decides whether user data survives.

Tauri rather than Electron because a background utility that idles at 150 MB of
RAM is a background utility users uninstall. Tauri uses the operating system's
existing web view instead of shipping a browser.

## Rejected

- **Electron** — resource cost unacceptable for an always-running daemon.
- **C++/Qt** — memory safety risk in file parsers, and high maintenance burden
  for one developer.
- **React Native / Flutter** — no adequate access to background execution,
  file provider extensions, or long-lived sockets.
- **Go for the client** — garbage collection pauses during large scans, and a
  weaker story for embedding in an iOS extension under a tight memory ceiling.

## Tradeoff accepted

Two mobile UI codebases to maintain. Tauri renders through each platform's own
web view, so the desktop front end needs real cross-platform testing rather than
"it looked fine on Linux."

## Reversibility

**Low cost.** The UI is separable from the core by construction. Replacing Tauri
with a native desktop toolkit would not touch the engine.
