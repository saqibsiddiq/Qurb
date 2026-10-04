# 0049 — Adding a file puts it where you are looking

**Status:** Accepted — amends [0036](0036-a-phone-keeps-its-own-files.md)'s
"everything added through the app or the share sheet" goes into the phone's own
vault; built in the FFI and the Android app
**Date:** 2026-09-29, built with the designed Android app (commit `fad2d9b`);
recorded 2026-10-03, from the code and its tests

## Decision

On a phone, a file added **with a choice of area** goes into that area,
whatever *Keep new files private* says:

- *Add files* in **Files** puts it in the shared area, which every device sees.
- *Add files* in **Private Vault** puts it in this phone's own vault.
- The share sheet's **Save to Files, on all your devices** and **Save to
  Private Vault** do the same.

*Keep new files private* — on by default, per 0036 — now governs only files
that arrive **without** a choice of area: another app saving into qurb through
the system file picker, anything the folder scan finds, and the share sheet
when no device is paired, which saves without asking.

Each browser lists its own area only. Files shows the shared area and Private
Vault shows this phone's vault; a folder holding nothing but private files is
not a folder of the shared area. The system file picker still lists both,
because it has no notion of an area to show.

A path already in qurb keeps its area. Adding a file over one that exists is an
edit, and an edit never moves a file between areas in either direction — the
rule `Store::put_file` already kept, because a file leaving the shared area
looks like a deletion to every other device.

## Why

The design ([0048](0048-the-design-direction.md)) made Private Vault a place
inside Files, with its own *Add files*. Under 0036 as written, *Add files* on
the screen titled Files would have put the file in Private Vault — so the file
a person had just added would not appear on the screen they added it from. A
button that files things somewhere other than where you pressed it is a file
that seems to vanish.

0036's purpose is kept. It exists so that a phone's photographs do not appear
on the desktop unasked. Files that reach qurb without the person choosing —
saved by other apps, or found in the folder — still go private. What changes is
that a person who has chosen, by where they are standing, gets what they chose.

## What it costs

- **An add into Files is a publish.** The file goes to every device at the next
  sync, with no confirmation on the way: the screen says where the person is
  (*Everything in your Qurb space, and where it is*), and the message after
  adding says *to Private Vault* only when that is where it went. Deleting it
  afterwards removes it everywhere, but a device that synced in between has had
  it.
- **Two ways to answer "where does a new file go?"** — the area on screen, and
  the setting for everything else. Settings words the setting as being about
  files *that arrive on this phone from other apps*, which is what it now
  governs.

## How it is built

`qurb-mobile` gains three calls, each the existing one restricted to an area:

- `import_into(source, path, private)` — `import_file` with the area given. It
  sets the store's new-files-private flag for the one write and restores it,
  under the same engine lock, so nothing else writes in between.
- `browse_in(dir, private)` and `search_in(text, limit, private)` — `browse`
  and `search` filtered to one area. `search_in` asks the index for four times
  the limit before filtering, so a page is usually full even when most matches
  are in the other area — usually, not always.

`import_file`, `browse` and `search` are unchanged, and the system file picker
uses them. The test is `each_area_lists_its_own_and_adds_into_itself` in
`crates/mobile-ffi/tests/lifecycle.rs`: four files added two to each area with
the setting on, each area listing only its own files and folders, and the
unrestricted `browse` still seeing both.

## Not done

- **The desktop.** Files reach the desktop's folder through the file manager,
  with no choice of area, so `own-files` still decides. Its Private Vault is
  browsed and has no *Add files*.
- **Moving a file between areas** — *Move to Private Vault*, *Move to shared* —
  on either device. Designed ([brief §2](../design/brief.md)) and not built;
  it needs an engine addition, since a move between areas is a deletion in one
  and a new file in the other.
- **Not verified on the phone.** Files and Private Vault were seen on the
  Galaxy S23 on 2026-10-03, each listing only its own area; adding a file into
  either was not done there.

## Reversing it

Cheap in code: the app passes no area, and every add follows the setting
again. Files already added to the shared area this way have reached the other
devices and stay there.
