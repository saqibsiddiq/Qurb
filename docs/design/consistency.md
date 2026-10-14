# Consistency

The rules both apps keep, so that a screen reads the same as its counterpart
on the other platform, and so that each screen reads the same as its
neighbours. Written on 2026-10-11 after the owner reported "many UI
inconsistencies and asymmetries" (item 6 of his list). Everything below was
found by **measuring** where things sit, not by looking:

- **Desktop**: positions from the DOM, in the fixture page
  (`experiments/desktop-fixtures`), at a 1280 × 860 window, light and dark.
- **Android**: positions from the view hierarchy (`uiautomator dump`), on
  the emulator (`qurb-test`, 1080 × 2400 at 420 dpi, so 1 dp = 2.625 px),
  and from row 29 on the owner's Galaxy S23 too (1080 × 2340 at 480 dpi, so
  1 dp = 3 px and the screen 360 dp wide), over wireless debugging. What
  shows through a bar is measured as grey levels in a screenshot.

[direction.md](direction.md) and [brief.md](brief.md) say what the design
is. This file says how its parts line up.

## The rules

1. **One left edge.** On a page, the title, the subtitle, the search field,
   every section label, a breadcrumb's words, folder tiles and a row's tile
   all start on the same line. A row's padding, and the hover or ripple that
   lights it, reach out into the gutter instead of pushing the tile in.
   Desktop: `.rows` has negative inline margins equal to a row's padding
   and border. Android: the list's 12 dp padding plus a row's 8 dp padding
   is the 20 dp gutter; Home's Recent list is pulled out by 8 dp. In a
   sheet, an action's icon sits under the header's icon and its words start
   where the title does (`Sheet.action`).
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
9. **Glass over moving content is opaque where nothing blurs it.**
   Android blurs nothing behind a view, so its tab bar is opaque, in light
   and dark. At 95% words behind it could be read; at 98% too, on the S23 in
   dark and at a larger font size in light (row 29). Sheets and dialogs are
   opaque too; where the phone blurs, the blur shows around a sheet, never
   through it (rows 45 and 46).
10. **Motion never washes out words.** A light travelling across a control
    passes behind its text, dimmed.
11. **The buttons that end a sheet or a confirmation sit on the right**, the
    one that goes ahead last: *Cancel* then *Remove device*, *Back* then
    *Join*. Android's `Sheet.buttons` and the desktop's `.actions.end`. A
    column of a thing's own actions (a device's or a file's sheet) is a list,
    not an ending, and is not this rule.
12. **A device's icon says what it is**, from the kind it said it is when
    pairing ([decision 0053](../decisions/0053-approval-same-key-and-safe-copies.md)):
    a phone, a computer, or a hard drive for a replica. A device paired before
    then says so at its first sync since; until it has, and only then, the
    icon is guessed from its name.
13. **The app is *Qurb* wherever a person reads its name**: every screen,
    the launcher on both, the desktop's notifications and autostart entry,
    the tray, and the folder in Android's file picker. *qurb* in lower case
    only where it is typed or is an identifier: commands, folders, packages.
14. **A device is named, or is *another device*.** Never its identifier: a
    device never paired here -- where a file began, reaching this one
    through another -- is *another device*, as each app already said when
    no device was recorded.
15. **An error says what happened, as a sentence.** A refusal -- different
    keys, declined, a code that lapsed before anybody approved -- is not
    filed under the network. The engine words its errors to follow `error:`
    on a command line; both apps start them with a capital.
16. **Nothing that holds words has a fixed height.** Checked at 1.3× and at
    2×, Android's largest: at least a height, and taller with the text.
17. **A step has a title and a way back.** The desktop's setting up and
    Add a device, and now the phone's setting up, whose back key goes to
    the step before rather than out of setting up.

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
| 21 | Android, Devices | *Computers you visit* in the larger heading style, unlike every section, and worded unlike the desktop's *Computers you visit as a guest* | a label, in the desktop's words |
| 22 | Android, Home and sheets | names listed with commas only (*Study desktop, test-laptop*), with a singular verb (*A, B keeps*) and *both* for three devices | *A and B*, *keep*, *each* |
| 23 | both, history | a pairing read *Added to your devices* on the phone, untrue of another person's computer, and *Paired* on the desktop | *Paired with this phone / computer*, and *Removed from this …* to match |
| 24 | Android, a device's sheet | *Added* where the desktop says *Paired* | *Paired* |
| 25 | Android, Add a device | *Type a code* where the desktop says *Enter a code* | *Enter a code* |
| 26 | desktop, setting up | *Back* a plain link on one step and a button on the others; some steps' buttons on the left, others' and every sheet's on the right | all buttons right, *Back* the same button everywhere |
| 27 | both, every device icon | guessed from the device's name, though both apps record what each device is: a phone whose name had none of the words looked for (*Ammi*, say) would show as a laptop, and a replica always did | from the recorded kind; on the emulator, test-laptop recorded as a replica showed a hard drive (the record taken out after) |
| 28 | desktop, removing a device, showing or entering a code, a device asking to join, moving a file into the Private Vault | buttons on the left, unlike every other sheet and every sheet on the phone | last button 41 to 357 px short of the right edge; now on it, and *Decline* / *Approve* on their own card's edge |
| 29 | Android, tab bar | words behind it readable: on the S23 in dark, 31 to 39 levels behind against the bar's 31; on the emulator in light at 1.3×, *Study desktop* at 246 against 251. Both inside the 5-level margin row 19 accepted, and both read | opaque in both modes; over scrolling rows, a flat 31 in dark |
| 30 | both, history and conflicts | a device never paired here shown by its identifier: *Arrived from 6d4e9586* on the S23, beside *Arrived from saqib*; the same for a conflict's version made there | *another device* (rule 14) |
| 31 | desktop, history | a queued send read *Ready to send by Galaxy S23*, the device being the one it is for; the phone says *Ready for* | *Ready for Galaxy S23* |
| 32 | desktop, send | the result said *you can close this* and not, as the phone does, to leave the file as it is: nothing is copied ([0060](../decisions/0060-a-computer-keeps-private-folders-for-several-people.md)) | *keep it as it is until then* |
| 33 | Android, every sheet with actions | an action's icon and words 4 to 8 dp right of the header's (icon 38.9 dp against 31.6, words 86.1 against 81.5): a row's padding and a smaller tile | 31.6 and 81.9 (rule 1) |
| 34 | Android, Files, Private Vault, Recently deleted | the breadcrumb and back links a fixed 36 dp: at 2× their words lost their tops | at least 36 dp, growing with the text |
| 35 | both, the app's name | *qurb* in both launchers, Android's file picker, the desktop's notifications, autostart entry and tray, and the phone's setting-up title, while every screen says *Qurb* | *Qurb* (rule 13) |
| 36 | Android, Send to | the device choice listed bare names; the desktop's says when each was last here. And a device never seen was *Not seen yet* on the phone, *Not connected yet* on the desktop | *Last seen 14 days ago* under each; *Not connected yet* on both |
| 37 | Android, Add a device | four bare choices, where the desktop's each say what they are for | a line under each |
| 38 | Android, showing a code | *Done* and *Stop showing it* side by side, doing the same thing; the desktop has one | *Stop showing it* |
| 39 | Android, entering a code | *Enter the code* and *Connect*, where its own menu and the desktop say *Enter a code* and *Join* | *Enter a code*, *Join* |
| 40 | Android, pairing | a key mismatch read *Network problem.*, then the engine's sentence in lower case; a bad code told a phone user to *Run `qurb pair`* | the sentence alone, capitalised (rule 15); *Show a new one on the other device* |
| 41 | Android, a conflict | with the other version not on the phone, two of the three choices were simply missing; the desktop greys them and says why | a line saying so |
| 42 | Android, setting up | not in the app's design: Material's display type, 32 dp margins, stock outlined and text buttons, no mark, *qurb* in lower case; a step with no title or way back, and the back key left setting up | the mark and *Qurb*, the 20 dp gutter, the desktop's choice cards and words, a title and *Back* on each step (rule 17) |
| 43 | desktop, conflicts | a version's device icon guessed from its name (rule 12) | from its kind |
| 44 | desktop, showing a code | with no code drawn, *Waiting — …* stayed centred under a left-aligned heading | on the left |
| 45 | Android, the pairing question, and every dialog | on the S23 in dark, the QR code behind the question read plainly through it, about 8 levels, under the number to compare; and the number was Material's lilac in the system font | dialogs opaque (and a sheet where nothing blurs); the number in the app's colour, Inter, even figures |
| 46 | Android, every sheet | on the S23 in dark, text behind a sheet read through it: the phone reported blur off (`mBlurEnabled=false`) and the sheet stayed 97%, the case row 45 had made opaque for dialogs only | every sheet opaque; where the phone blurs, the blur shows around it |
| 47 | both, Home | the owner's item 4 (2026-10-13): no Recent list on Home, a button for the activity instead | a clock at the top right of Home on both: on the phone the Files header's 44 dp button, on the emulator its right edge on the gutter with *Sync now*'s; on the desktop the `icon-btn` beside the state, its right edge on the content's (1135 px of 1135 in the fixtures page). Each opens Activity; back returns Home |
| 48 | both, choosing a phone's folders (0063) | first drawn, a folder's *Open* sat after its switch, so the rows with one had their switches further left; and a folder inside a chosen one looked chosen itself | *Open* before the switch, the switches in one column; a covered folder's switch fixed at 45% and its line *Backed up with DCIM*. The phone's sheet is the WhatsApp one's shape: what, over Wi-Fi only, what Android asks next, *Not now* and *Allow* |
| 49 | Android, Home and *Your folders* | the owner's item 8: *Sync now* and *Send* two buttons, of one size; *Your folders* first drawn with a title over each folder's group, saying its first row's name again, and its first group against the header | *Sync now* (secondary) and *Send* (primary) 54 dp each, 12 dp apart, filling the gutter's width; *Waiting* and *Backups* sections under them, titled as Settings' are. In *Your folders* no title over a folder's group, 16 dp between groups and above the first; a covered folder's switch fixed at 60%, its line *Backed up with Android* |
| 50 | both, Home | the owner's items 8 and 11: Home's two halves to match | on both: *Sync now* (secondary) beside *Send* (primary), one size — on the desktop 184 × 44 px each, 12 px apart, on the content's edge under the state (369 px in the fixtures page at 1180 wide), filling the width under 760 px; the facts line under them; then *Waiting* and *Backups*, titled as Settings' groups are. *Waiting* says the same things on both, in the same words, three files and three sends by name and the rest counted; *Nothing waiting* in green when there is none. *Backups* on the phone is its own two; on the desktop the two of each phone, *WhatsApp, from …* and *Folders, from …* with *Choose…* or *Change…* |
| 51 | both, showing and typing a code | the owner's item 7: six digits, and the long code made secondary | showing: the QR, then *Or type these on your phone, on this Wi-Fi* and the digits in the pairing number's style (36, semibold, even figures, spaced `482 913`), centred under it on both, the countdown under them; the whole code folded away on the desktop and small below on the phone. Typed digits wrongly too often, struck through at 35% with *Scan the code instead*. Typing: one field of six, centred, large and spaced, a number pad on the phone; joining as the sixth is typed on both |

Checked after: the desktop smoke test, in the real window, both modes (*no
command failed*), and every screen re-measured in the fixtures and on the
emulator. Rows 21 to 26 were found the same day, running the guest flow on
the emulator and then looking at sheets, dark mode and setting up. Rows 27
and 28 came from going through pairing and removing one screen at a time:
the desktop's measured in the fixtures with headless Chromium at 1280 × 860,
each row's buttons measured with and without the change; the phone's icon on
the emulator. Rows 29 to 46 came from the rest of the list below, going
through the send flow, conflicts, the phone's pairing and removing screens
and its setting up one by one, every place and sheet at 1.3× and 2×, and the
S23: Rust tests for the desktop and mobile crates, the desktop smoke test,
and each changed screen measured again after.

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
- **The desktop's conflict choices are on the left**, under the two
  versions, in the order of the columns above them: *Keep this version*,
  *Keep the other version*, *Keep both*. They are a choice tied to what is
  above, not a sheet's ending (rule 11). The phone lists the same three, in
  the same words and order, as a sheet's actions.
- **The phone's errors and its pairing question are dialogs**, not sheets:
  themed to the app (`ThemeOverlay.Qurb.Dialog`), and modal, because the
  engine waits on the answer. Their buttons are words, as a dialog's are,
  where the desktop's question has a green *Approve*.
- **The phone's *Send to* goes on when a device is chosen**, then picks
  files; the desktop's sheet holds files and a device and sends on *Send*.
  The same choice, each in its platform's order.
- **A sheet's way out is *Cancel* on the phone and *Back* in the desktop's
  Add a device**: on the phone each step is its own sheet, and closing it
  is going back.
- **At 2× the floating *Add files* covers the last row** until the list is
  scrolled, which it can be: the list leaves room for it. A device card's
  name ends in an ellipsis at large sizes.

## Not yet checked

Stated so that nobody mistakes this pass for a complete one:

- **Checked since**: the Android app's four places and two sheets in dark;
  the device sheet and file sheet on both; Add a device on both; the
  desktop's setting-up steps. Rows 21 to 26. Then the desktop's pairing
  screens, removing a device and moving into the Private Vault, one by one,
  and device icons on both. Rows 27 and 28. Then the send flow and
  conflicts on both, the phone's pairing, removing and setting up (on a
  second user of the emulator, so nothing was cleared), every place and
  sheet at 1.3× and 2×, and the S23. Rows 29 to 46.
- **The S23, screen by screen**, in dark: the four places, Add a device,
  entering and showing a code, a device's sheet, removing (dismissed, not
  done), Send to, and the pairing question -- a throwaway device on the
  laptop, named *Test joiner*, asking to join the phone's code, declined.
  Every edge where the emulator puts it, scaled: icons under the header's
  at 32 dp, words at 82, buttons ending 20 dp in. Rows 29 and 45 were found
  there.
- **The S23 in light, at larger font sizes, and setting up**: only on the
  emulator. Its own setting is dark, and setting up needs a phone with
  nothing on it.
- **The phone's line for a conflict whose other version is not here yet**
  (row 41) was not seen: it needs a conflict whose bytes stayed on the
  other device.
- **The desktop at larger text sizes.** Its sizes are in pixels; how it
  behaves under the desktop's text scaling was not checked.
