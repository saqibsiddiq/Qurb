# Releasing qurb

How a release is built, what is in it, who each package is for, and — the part
that matters most — **what has been run on what, and what has not**. Read the
last two sections before handing anything to anybody.

**This file describes 0.1.0**, asked for on 2026-10-18 and built on 2026-10-18
and 19 on the owner's laptop (CachyOS, Linux 7.2, glibc 2.44, x86_64).
Nothing was published: the owner asked for the builds alone, and they are in
`dist/0.1.0/` on that machine. Why it is shaped as it is:
[decision 0070](decisions/0070-the-first-release.md).

## Building one

```sh
./scripts/release.sh            # everything below, then SHA256SUMS
./scripts/release.sh linux      # or one part: linux, server, android, windows, sums
```

The version is the one in `crates/desktop/Cargo.toml`; it is also in
`crates/desktop/tauri.conf.json` and `android/app/build.gradle.kts`
(`versionName`, and `versionCode`, which must rise with every Android release).
Change them together. Everything lands in `dist/<version>/`, which git ignores.

What the machine needs:

| for | what |
|---|---|
| Linux packages | `cargo install tauri-cli`; `makepkg` (Arch) |
| the static `qurb`, Windows | [Zig](https://ziglang.org) 0.14 (`ZIG=` names it if it is not on `PATH`) and `cargo install cargo-zigbuild`; the Rust targets `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-pc-windows-gnu`; `llvm-strip`; for Windows, MinGW's `windres` or LLVM's `llvm-windres` |
| Android | the SDK and NDK, and the release key (android/README.md, *Signing*) |

Do not edit `scripts/release.sh` while it runs: bash reads a script as it goes,
and one release here failed with a syntax error that way.

The programs keep their function names, so a backtrace from a release says
where it was; the rest of the debug information is left out. With it the `qurb`
program alone was 327 MB.

## What is in 0.1.0

| file | MB | for |
|---|---|---|
| `qurb_0.1.0_amd64.deb` | 27.0 | Debian 12+, Ubuntu 22.04+, Mint 21+, Pop!_OS and the rest of that family: the window, the tray and `qurb` |
| `qurb-0.1.0-1.x86_64.rpm` | 27.0 | current Fedora and openSUSE Tumbleweed, and any rpm distribution with glibc 2.34 and WebKitGTK 4.1 packaged: the same three. Not openSUSE Leap 15, whose glibc is 2.31 |
| `qurb-0.1.0-1-x86_64.pkg.tar.zst` | 22.3 | Arch and what is built on it (CachyOS, EndeavourOS, Manjaro): `pacman -U` |
| `qurb-0.1.0-x86_64-linux-static.tar.gz` | 8.3 | `qurb` alone, for any x86_64 Linux at all — a server's rendezvous service and relay, a replica — with the systemd units and the server README |
| `qurb-0.1.0-aarch64-linux-static.tar.gz` | 8.0 | the same for 64-bit ARM: a Raspberry Pi, an ARM server |
| `qurb-0.1.0-android.apk` | 41.1 | any Android phone, 8.0 or later: all four ABIs in one |
| `qurb-0.1.0-android-arm64-v8a.apk` | 12.5 | the same for one ABI: nearly every phone of the last decade |
| `qurb-0.1.0-android-armeabi-v7a.apk` | 10.0 | older and cheaper phones running 32-bit ARM |
| `qurb-0.1.0-android-x86_64.apk`, `-x86.apk` | 13.6, 13.9 | Chromebooks, x86 tablets, emulators |
| `qurb-0.1.0-android.aab` | 23.1 | an app bundle, for a store to split by itself |
| `qurb-0.1.0-windows-x86_64.zip` | 19.4 | Windows 10 and 11, x86_64: `qurb-desktop.exe`, `qurb.exe`, `WebView2Loader.dll`, a README — **not run on Windows** |
| `SHA256SUMS` | | each file's SHA-256, made last |

What each needs of the machine it goes on:

- **The deb and the rpm**: glibc 2.34 or newer and WebKitGTK 4.1 — the
  distributions named above. Measured: no symbol any of the three programs
  asks of glibc is newer than 2.34 (`objdump -T`; the 2.39 ones Rust's
  standard library asks for are weak, used only if there). The packages
  depend on WebKitGTK and GTK 3 by name, and the appindicator library for the
  tray. The two carry the same programs, byte for byte, but for three bytes in
  `qurb-desktop` where Tauri's bundler writes which package it is.
- **The static `qurb`**: nothing. It asks the system for no library.
- **Android**: 8.0 (API 26) or later. Signed with the project's release key —
  SHA-256 `4F:AE:59:0B:9C:0E:C3:05:60:7D:F0:4C:AE:35:82:CA:FE:05:A4:2A:FD:09:12:A3:00:40:30:03:01:3F:FD:2A`,
  APK signature scheme v2 — and every native library aligned for 16 KB pages
  (`zipalign -c -P 16`). Push wakes it where Google Play services are; it
  syncs on its schedule where they are not.
- **Windows**: Windows 10 or 11 with Microsoft Edge WebView2, which both
  normally have; the C runtime they include. Not signed: SmartScreen warns of
  an unknown publisher.

## What has run on what

| package | run | how |
|---|---|---|
| deb (and so the rpm's programs) | on the laptop, from the package's files, not installed with `dpkg` | `qurb`: two folders paired and synced over loopback — a new file, an edit back, 8 MiB intact, a deletion, each in about a second. `qurb-desktop`: the whole window driven end to end by `scripts/desktop-smoke.sh` — set up, every place, paired by six digits, a send, a device removed, a passphrase set and unlocked; no command failed |
| Arch package | on the laptop, from its files, not installed | the same two tests, passed |
| static `qurb`, x86_64 | on the laptop | the same two-folder sync, passed |
| static `qurb`, arm64 | **not run** | nothing here runs arm64 Linux programs |
| Android, universal | Android 14 emulator, x86_64, without Google Play services | installed fresh; set up, every tab; no crash, R8's shrinking included |
| Android, x86 | Android 8.0 emulator, as a 32-bit process on an x86_64 one, without Google Play services | installed; set up, every tab, the engine answering its version; Block Store absent and setup going on without it |
| Android, arm64-v8a, armeabi-v7a | **not installed on a phone** | the S23 runs builds signed with the debug key, and a release over it means uninstalling, which deletes its key and index — the owner's call. arm64 is what the S23 has run all along, from the same code in the `fast` build |
| the engine on Android 8.0 | the whole Rust suite on the emulator, 2026-10-19: as 32-bit x86, all 775 tests in 68 programs passed; as x86_64, 774, the one left the load-sensitive test below | `./scripts/android-test.sh x86` and `x86_64` |
| Windows | **not run**, not even under Wine | Wine is not installed on the laptop, and installing it needs its owner (`sudo pacman -S wine`); the programs' imports were read instead, which is how the missing DLL was found |
| the app bundle | not given to any store | — |

The laptop's own suite on the code released: 900 tests, none failing, and
`cargo clippy` clean; Android lint, no errors; `cargo audit`, no
vulnerabilities (eleven warnings of unmaintained or unsound crates, all deep in
Tauri's GTK stack or in a throwaway experiment).

## Known gaps

- **Windows has not run.** Every Windows-specific path — the single-instance
  check on a loopback port, starting at sign-in from the registry, the
  folders under `%USERPROFILE%` and `%APPDATA%`, opening with `start` — is
  written and built, and none of it has been seen working. The computer is not
  kept awake while a phone collects from it, there. Large media opens in the
  browser, which plays some formats and not others.
- **No AppImage.** One was built and measured: it carried 191 of this
  machine's libraries, and WebKitGTK's needed glibc 2.44, so it would have
  started only where the other three Linux packages already serve. An
  AppImage must be built on the oldest system it is for; with a container to
  be that system, it can be added.
- **Linux packages run on this machine only.** No Debian, Ubuntu, Fedora or
  openSUSE install has been tried; the glibc and library names are measured,
  not watched.
- **32-bit Android is run only as x86**, on an emulator. And a file is read by
  mapping all of it into memory, which in a 32-bit process — 4 GB of
  addresses at most, in pieces — a large enough file cannot be; it is then not
  read, with an error. Where that starts on a real phone is not measured.
- **The arm64 static program has not run.**
- **One test is sensitive to a loaded emulator.**
  `a_device_that_is_off_does_not_starve_one_that_is_on` passes on the laptop
  and alone on the emulator, and failed in two full runs there: its stand-in
  desktop re-announces itself between passes, and a phone asking in that gap
  gives up on it for its pass. A real computer's daemon stays announced.
- **Nothing is published**: no GitHub release, no website download, no store.
