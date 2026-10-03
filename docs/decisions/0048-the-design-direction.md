# 0048 — The design direction

**Status:** Accepted — revised the same day twice: for the owner's full
direction, and to drop Figma (the design is built in the apps themselves)
**Date:** 2026-09-28

## Decision

The desktop window and the Android app are designed together, in one visual
language, **directly in the apps**. The first plan was Figma first — Figma's
MCP server building frames the owner would review before any code — and the
owner dropped it the same day: "remove figma from the plan, start
implementing the design plan". The owner's direction is [design/direction.md](../design/direction.md), word for
word; how it meets the product, and the table that places every feature on a
screen, is [design/brief.md](../design/brief.md). The parts with consequences
beyond looks:

- **The desktop's eight tabs become a translucent sidebar**: Home, Files,
  Devices, Storage; Private Vault set apart; Settings. Send is Home's primary
  action and a drop target everywhere; Transfers appear as a panel only while
  something moves; the history is reached from Home's *Recent*.
- **The phone's five tabs become four** — Home, Files, Devices, Settings.
  Transfers appear when active; Private Vault is reached from Files.
- **The phone's "Vault" becomes "Private Vault"**, the device's own area, and
  exists on the desktop too — which needs moving a file into and out of it, an
  engine addition that follows the design.
- **One look on both** — glass materials over a quiet environment, Qurb green
  `#2F6B57` fixed rather than Android's wallpaper colours, Inter — qurb's own
  rather than GNOME's.
- **A file-state language of nine plain states**, and no implementation words
  on any screen; *free local space* never looks like deleting.
- **The design draws only what is true.** Where the direction asks for
  something qurb does not do — accounts, versions beyond a conflict, a
  percentage on the phone — the brief records what is drawn instead.

The first answers, given before the direction — a teal accent, an Activity
section, "Files" replacing "Vault" — are kept in the brief, struck through
where the direction replaced them.

## Why

**One language** because the product is one thing on two devices; a person
moving from phone to laptop should find the same words in the same places.
Fewer sections because several of the old ones were one idea split by how it
was built — *Send* and *Transfers* are both about sending, *Activity* is what
Home's *Recent* already begins — and because four tabs fit a phone better than
five. Storage keeps a section on the desktop because freeing space without
losing files is the one idea qurb has that other storage apps do not, and it
needs room to be explained.

**In the apps rather than in a design tool**, in the end, because the owner
chose it: the design is checked in the real window — rendered against the
fixture data in WebKitGTK, the window's own engine, and driven end to end by
the smoke test — and reviewed there at the checkpoints.

## What it costs

- Renaming the tabs and moving Transfers changes words the phone's users have
  seen; there are none yet but the owner, so now is the cheap time.
- Glass is expensive to draw live on a phone. The environment behind it is
  still, so most surfaces can be a translucent fill over a pre-blurred
  background, with real blur kept for sheets and dialogs (brief §4).
- Inter is a font the phone must carry; subset, it costs a few hundred
  kilobytes against a 10.7 MB install
  ([0039](0039-a-light-android-app.md)).

## Progress

- **2026-09-29 — the desktop window** (commit `faed984`), in
  `crates/desktop/ui`: the sidebar, the glass, Inter and Lucide bundled, every
  place in the brief. Rendered against the fixture data and driven end to end
  by the smoke test; see [phase 4](../phases/phase-4-product.md#the-design-in-the-window).
- **2026-09-29 — the Android app** (commit `fad2d9b`), on the platform's own
  views as [0039](0039-a-light-android-app.md) chose: four tabs — Home, Files,
  Devices, Settings — under a floating bar; Private Vault inside Files,
  Activity from Home, Recently deleted from Files and Settings; Transfers as a
  bar that appears while something moves; actions in sheets. Lucide icons
  generated as vector drawables by `scripts/android-icons.py`, and the new mark
  as the launcher icon. Adding a file now goes into the area on screen,
  [0049](0049-adding-a-file-puts-it-where-you-are-looking.md). See
  [phase 5](../phases/phase-5-mobile.md#the-designed-app).
- **The font's cost**, predicted above as a few hundred kilobytes: three Inter
  weights as TrueType, 246,724 bytes together before the APK compresses them.
  What the designed app adds to the 10.7 MB install has not been measured.
- **Light only, on both.** The Android app's earlier night palette was removed
  rather than left under the new design; dark mode is designed after the light
  one is approved.
- **2026-10-03 — the mark on Linux.** The applications menu's icon
  (`packaging/qurb.svg`, and `crates/desktop/icons/icon.png` rendered from it)
  and the tray icon (`crates/tray/src/icon.rs`, drawn from the same
  coordinates) are the mark; until then they drew the old blue ring.
- **2026-10-03 — the website**, rebuilt with the apps as its reference: the
  window's tokens, materials and components, its icons, Inter served from the
  site, and the product drawn from the apps' own parts. It calls the product
  *Qurb*, as the apps do, where it used to say *Qurb Cloud*. See
  [website/README.md](../../website/README.md).
- **Not yet done:** the owner's review at the three checkpoints; dark mode;
  moving a file into or out of Private Vault.
