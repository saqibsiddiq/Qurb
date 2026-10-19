# 0070 — The first release, 0.1.0: what is built, for what, and how far it is checked

**Status:** Accepted, 2026-10-18. Built on the owner's laptop on 2026-10-18
and 19; nothing published, as he asked. What has run on what is in
[releasing.md](../releasing.md), and is the part of this to read.
**Date:** 2026-10-18

## What was asked

The owner, 2026-10-18:

> build for all the distros of linux, windows and every android phone for
> formal release

Asked and answered the same day: Windows built here with MinGW's tools and
run under Wine, and labelled as not run on Windows (his scoping of 2026-09-28
still stands: Linux and Android first, because they are what can be tested);
version 0.1.0; build only. In the event MinGW's tools came inside Zig, and
Wine is not installed, so the Windows programs have not run at all.

## Decision

**Every Linux distribution:**

- **A `.deb`** (Debian, Ubuntu, Mint, Pop!_OS and the rest of that family) and
  **an `.rpm`** (Fedora, openSUSE Tumbleweed, and others new enough), each holding the
  window, the tray and the `qurb` program, made by Tauri's bundler with
  `crates/desktop/tauri.release.conf.json`; and **the Arch package** as
  before (`packaging/arch`), now named by the release's version.
- **The programs need glibc 2.34 or newer**, measured: nothing in them asks
  for a later symbol (`objdump -T`). That is Ubuntu 22.04, Debian 12 and
  Fedora 35 onwards, and not openSUSE Leap 15 (2.31) -- no lower than Tauri
  2's own floor, which is WebKitGTK 4.1. Building on this machine (glibc 2.44) did not raise it
  except in two places, both fixed: the tray asked for glibc 2.43's `atan2f`,
  and for `libxdo.so.4`, which only Arch names so.
- **The `qurb` program alone, linked statically** against musl, for x86_64
  and arm64: no library asked of the system at all, so it runs on any Linux
  -- an old server, a Raspberry Pi, a distribution not named above. It is
  what a rendezvous service and relay run as (0068). Built with Zig as the C
  compiler, for SQLite's sake.

**Every Android phone:** one APK for each of the four ABIs Android has --
arm64-v8a, armeabi-v7a, x86, x86_64 -- one holding all four, and an app
bundle for a store; Android 8.0 (API 26) and later; signed with the release
key (0047).

**Windows:** `qurb-desktop.exe` and `qurb.exe`, cross-built for
`x86_64-pc-windows-gnu` with Zig standing in for MinGW and LLVM's
`llvm-windres` for its resource compiler, zipped with a README that opens by
saying it has not run on Windows. Not signed.

**All of them:** function names kept, so a backtrace is readable; the rest
of the debug information dropped. `SHA256SUMS` beside them.

## Why this way

- **deb and rpm, not one package for all.** Each uses the system's own
  WebKitGTK, kept up to date by the distribution -- the engine that shows the
  window is a browser engine, and one frozen into a package goes stale.
- **No AppImage in 0.1.0.** One was built and measured: it carries 191 of the
  building machine's libraries, and WebKitGTK's ask for glibc 2.44, so it
  would start only on the newest distributions -- which the deb, rpm and Arch
  package serve already -- and fail everywhere else. An AppImage has to be
  built on the oldest system it is for, and this machine has no container to
  be one.
- **The static program** is what makes "every distribution" true for the
  part that does not need a window.
- **Every ABI** because phones that are not arm64 exist: older and cheap ones
  run 32-bit ARM, and x86 is Chromebooks and emulators. The everyday build
  stays arm64 only, as 0039 decided: a phone in the house is arm64.
- **Zig rather than installing MinGW**: it carries MinGW's headers and
  libraries itself, needs no root, and is one download.

## Costs, accepted

- **Windows has not run.** Built is not working; the README in the zip, and
  every document that mentions it, says so.
- **32-bit Android is run only as x86 on an emulator**, not on a 32-bit ARM
  phone. And a file is read by mapping the whole of it into memory, which a
  32-bit process has at most 4 GB of, in pieces: a large enough file cannot
  be mapped there, and is not read, with an error. Where that begins on a
  real phone is not measured.
- **The arm64 static program has not run**: nothing here runs arm64 Linux
  binaries.
- **The Windows window needs `WebView2Loader.dll` beside it**, which Windows
  does not have; a program built with MinGW's tools ships it, and the zip
  does. Found by reading the program's imports, not by running it.

## Reversing it

Nothing is published, so there is nothing to withdraw: `dist/0.1.0/` is
deleted and built again.
