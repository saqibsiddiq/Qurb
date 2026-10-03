# 0039 — A light Android app, on the platform's own views

**Status:** Accepted
**Date:** 2026-09-27

## Decision

The Android app is built to be **light and snappy**, which the project owner
set as the goal when asked which interface toolkit to use. Concretely:

- **The platform's own views**, with ViewBinding and RecyclerView, as the app
  already uses. Not Jetpack Compose.
- **Release builds are shrunk and optimised by R8**, with unused resources
  removed.
- **A release carries arm64 only.** Every phone sold in the last decade is
  arm64; x86_64 stays in debug builds, for the emulator.
- **The engine is built for the phone with its own profile** (`mobile` in
  `Cargo.toml`): link-time optimisation across every crate and one codegen
  unit, symbols stripped. Panics still unwind, so a Rust panic becomes an error
  the app can show rather than a crash.
- **The main thread never waits on the engine**, and a screen shows what the
  index already knows before doing anything slow.

## Measured

Galaxy S23 (SM-S911B), Android 16, 2026-09-27. Each build installed over the
previous one with the same signing key, so the app's data — 21 files, one
paired device — was the same throughout. Cold start is `am start -W` TotalTime,
15 launches, force-stopped between, screen woken before each; a launch behind
the lock screen aborts the run.

| | A: debug, as installed | B: release, as configured before | C: this decision |
|---|---|---|---|
| APK | 46.7 MB | 26.6 MB | **10.7 MB** |
| engine library, arm64 | 9.1 MB | 9.1 MB | **7.7 MB** |
| app code, dex | — | 14.2 MB | **3.0 MB** |
| code in memory (PSS) | 26.4 MB | 16.5 MB | **8.1 MB** |
| Java heap (PSS) | 13.6 MB | 6.3 MB | **5.5 MB** |
| cold start, median | 524 ms ¹ | 177 ms / 200 ms ² | **174 ms / 176 ms** ² |

¹ Seven launches, at the start of the session, before the screen-on check
existed; the phone was believed to be unlocked. Not directly comparable.
² First figure with the app compiled to `verify`, what a sideloaded install
gets; second to `speed-profile`, what it reaches once Android has optimised it.

What that says, plainly:

- **Size and code memory are where the gains are.** The APK is less than a
  quarter of what was installed, and the code the app keeps in memory is less
  than a third.
- **Cold start is already fast, and C does not change it measurably.** B and C
  are within a few milliseconds of each other. The large difference is debug
  against release, not anything in this decision.
- **Total memory is dominated by the screen.** The window's graphics buffers are
  about 45 MB at this phone's resolution when counted, and nothing in the app
  changes that. Totals therefore swung between 34 and 83 MB from one reading to
  the next, which is why the table gives the parts the app controls.

## The rebuilt app, measured

The same day, after the one-list app was replaced by five tabs (commit
`496d448`). Same phone, same data, same key; the old build (C above) and the new
one installed alternately and measured back to back, so both saw the same
battery, temperature and background load. Compiled to `verify` before each run.

| | C: one list | rebuilt: five tabs |
|---|---|---|
| APK | 10.7 MB | 10.8 MB |
| cold start, median of 15 | 186 ms, 171 ms | 171 ms, 173 ms |
| Java heap (PSS) | 5.4, 5.5, 5.4, 5.5 MB | 5.5, 5.6, 5.6, 5.8 MB |
| code in memory (PSS) | 7.4, 7.4, 7.2, 7.2 MB | 7.2, 7.3, 8.2, 9.5 MB |
| total (PSS) | 72.7, 72.8, 71.2, 71.3 MB | 74.1, 74.6, 75.5, 78.0 MB |

Memory is read 3 seconds after a cold launch onto Home, twice per install, in
the order the columns give; a megabyte here is `dumpsys meminfo`'s kilobytes
divided by 1024.

- **Startup did not change.** The difference between the two builds is smaller
  than the difference between two runs of the same one; in an earlier pair of
  runs `verify` and `speed-profile` swapped places.
- **Five screens cost a few megabytes**: on average 0.2 MB more heap and
  3.5 MB more in total. The code figure is the noisiest, and its two highest
  readings are both the new build's, so some of that is real.

## The designed app, not yet measured

On 2026-09-29 the five tabs were rebuilt to the design direction
([0048](0048-the-design-direction.md)) — still on the platform's views, with
no new UI library. What it added that a phone carries: three weights of Inter
(246,724 bytes of TrueType), 53 Lucide icons as vector drawables, and a few
dozen layer and shape drawables for the glass. Sheets blur what is behind them
only where the window manager does it (Android 12 and later); every other
surface is a translucent fill, which costs nothing to draw.

None of the table above has been repeated for it: not the APK size, not cold
start, not memory. Until it is, the figures here describe the five-tab app, and
"light and snappy" for the designed one is a claim with no number behind it.

## Measured and rejected

**Checking whether the phone is set up without the engine.** The first call
into the engine loads its native library and the bridge to it, and every launch
makes that call on the main thread before anything is drawn. Replacing it with a
plain file check was measured: medians of 370 against 377 ms and 351 against
363 ms, within the noise. Reverted, since it would have meant a rule the engine
owns written down twice. (Those runs predate the screen-on check, so the
absolute numbers are too high; the comparison was run under the same
conditions for both.)

## Why not Compose

Compose would bring its own UI runtime and compiler plugin into every build,
and on a fresh install its code runs interpreted until Android compiles it —
which is exactly the cold start the goal is about. The app is a handful of
lists, a few forms and a camera preview; the platform's views do all of that,
are already here, and cost nothing extra. The design system the product brief
asks for (§73) is styles, colours and a few custom views, which views handle.

## What else was found

**The phone never collected garbage.** Its screen said 100.7 MB on disk for
30.9 MB of files. Fixed the same day: the phone now runs the desktop's
housekeeping after each background sync and once per launch. See
[phase 5](../phases/phase-5-mobile.md) for what it freed and what the rest
turned out to be.

## Not done

- **Baseline Profiles.** They would compile the startup path ahead of time. With
  cold start at ~175 ms there is nothing yet to justify another library and a
  benchmark module; revisit if the rebuilt app is slower.
- **A signed release.** The APKs measured here were signed with the debug key.
  Release signing is packaging work (brief §69).
- **The camera and barcode-scanning libraries** are used only to scan a pairing
  code. What they add to the APK was not measured, and whether a lighter
  scanner is worth it is left for the rebuild.

## Reversing it

Cheap for the build settings, which are a few lines each. Moving to Compose
later would be a rewrite of the screens, which is the cost this decision avoids.
