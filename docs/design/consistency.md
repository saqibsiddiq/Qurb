# Consistency

The rules both apps keep, so that a screen reads the same as its counterpart
on the other platform, and so that each screen reads the same as its
neighbours. Written on 2026-10-11 after the owner reported "many UI
inconsistencies and asymmetries" (item 6 of his list). Everything below was
found by **measuring** where things sit, not by looking:

- **Desktop**: positions from the DOM, in the fixture page
  (`experiments/desktop-fixtures`), at a 1280 × 860 window, light and dark.
- **Android**: positions from the view hierarchy (`uiautomator dump`), on
  the emulator (`qurb-test`, 1080 × 2400 at 420 dpi, so 1 dp = 2.625 px).

[direction.md](direction.md) and [brief.md](brief.md) say what the design
is. This file says how its parts line up.

## The rules

1. **One left edge.** On a page, the title, the subtitle, the search field,
   every section label, a breadcrumb's words, folder tiles and a row's tile
   all start on the same line. A row's padding, and the hover or ripple that
   lights it, reach out into the gutter instead of pushing the tile in.
   Desktop: `.rows` has negative inline margins equal to a row's padding
   and border. Android: the list's 12 dp padding plus a row's 8 dp padding
   is the 20 dp gutter; Home's Recent list is pulled out by 8 dp.
2. **One right edge.** A row's trailing button and a text button (*See
   all*, *Sync now*, a section's action) end on the column's right edge. On
   Android a text button is pulled out by its 12 dp padding, with no minimum
   width, since Material centres short words inside one.
3. **Section titles are the small uppercase label**, flush left: one style
   for a list (`.list-label`, `Text.Label` in a list) and the same look over
   a group of settings (`.group-title`, `Kit.groupTitle`). The one larger
   heading is Home's *Recent*, on both.
4. **Every page has a title and one line saying what it is for**, in the
   same words on both apps where the page is the same: *Your devices, and
   whether each is reachable*; *How Qurb works on this computer / phone*;
   *Private to this computer / phone. Only this … can see these files; a
   device you choose can keep a backup.*
5. **A list labels each kind whenever it has some**: *Folders*, then
   *Files*.
6. **Every device card shows its presence the same way**, a dot and words,
   this device's own card included.
7. **A back link names where it goes**: *Files*, *Settings*, *Home*. Never
   *Back*.
8. **Dividers in a group run edge to edge.** One inset at one end only read
   as lopsided.
9. **Glass over moving content is nearly opaque where nothing blurs it.**
   Android blurs nothing behind a view, so its tab bar is 98% opaque; at
   95%, words scrolling behind it could still be read.
10. **Motion never washes out words.** A light travelling across a control
    passes behind its text, dimmed.

## Found and fixed, 2026-10-11

| | where | what was wrong | measured |
|---|---|---|---|
| 1 | desktop, Settings | group titles inset 4 px from every other label | 326 px against 322 |
| 2 | desktop, Files and Vault | a breadcrumb's words inset by its button's padding | 328 against 322 |
| 3 | Android, Files | the same | 26 dp against 20 |
| 4 | Android, Settings | group titles inset by 4 dp | 24 dp against 20 |
| 5 | Android, Files | section labels and row tiles inset by 2 dp, folder tiles 1 dp *outside* the gutter | 22, 22 and 19 dp against 20 |
| 6 | Android, Home | Recent's rows inset 8 dp; a negative margin written as `layout_marginHorizontal` was not applied | tiles at 28 dp, now 20 |
| 7 | Android, Home | *See all* ended 12 dp short of the right edge, then 9 dp more from Material's minimum width | now on the edge |
| 8 | desktop, all lists | row tiles inset 9 px from the title above them; trailing buttons 13 px short of the right edge | now 322 and 1202 |
| 9 | desktop, Storage | the card's top padding 24 px larger than its bottom, left from a heading removed earlier | 49 px against 25 |
| 10 | desktop, Storage | its two sections titled in the larger heading style, unlike every other screen | now labels |
| 11 | desktop, Files and Vault | *Files* labelled, *Folders* not; the phone labels both | both labelled |
| 12 | both, Settings | the only page without a subtitle | added |
| 13 | desktop and Android, Devices | the subtitle in two wordings | one |
| 14 | desktop, Devices | this computer's card without the presence dot the phone's own card has | added |
| 15 | desktop, Private Vault | its meaning said in a subtitle and again in a strip below it; the phone says it once | once, in the subtitle |
| 16 | Android, Recently deleted | back link read *Back*; every other page names its destination | *Files* or *Settings* |
| 17 | desktop, Files | *Recently deleted* shown only when something was deleted, inside folders and among search results too; the phone shows it at the top of Files only, always | as the phone |
| 18 | Android, group dividers | inset at the start only | edge to edge |
| 19 | Android, tab bar | text behind it readable at 95% opacity | 98%; behind-text now within 5 of 255 levels of the bar |
| 20 | desktop, transfers chip | the travelling light passed over the count, which vanished mid-sweep, in dark most of all | behind the words, at 35% |

Checked after: the desktop smoke test, in the real window, both modes (*no
command failed*), and every screen re-measured in the fixtures and on the
emulator.

## Left as it is, deliberately

- **Home's state is larger than other titles** (32 px against 30), and its
  column starts after the status ring. The state leads the screen (direction
  §5); on the phone the ring sits above it instead, for width.
- **A top-level page's title is larger than a page opened from it**, on the
  phone (*Files* against *Private Vault*, *Recently deleted*).
- ***Qurb* is the name of the top folder**, in breadcrumbs, a file's
  location and Recent, on both apps, while the page is *Files*. It names
  the folder on disk and is used the same way everywhere.
- **The desktop has no *Sync now*.** It syncs continuously; the phone syncs
  in windows ([decision 0020](../decisions/0020-sync-takes-a-deadline.md)).
- ***Add a device* is *Add* on the phone**, for width.

## Not yet checked

Stated so that nobody mistakes this pass for a complete one:

- Sheets and dialogs, one by one, on either platform: the device sheet, the
  file details, the send flow, pairing, removing a device, conflicts.
- Setting up: onboarding on both.
- The Android app in dark mode, and at font scales above 1.0.
- A real phone. Every Android measurement here is the emulator's; the S23
  is 1080 × 2340 at a different density.
