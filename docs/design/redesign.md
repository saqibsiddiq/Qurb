# The redesign of 2026-10

**Status:** started 2026-10-20. Home is at draft 1. Nothing is built in the apps
yet: everything here is a design in Figma.

The owner, 2026-10-20:

> can u run figma, i would like to redesign our application again with your
> help, i want to go through every page, pop up, text, notfication

The owner's answers, the same day: Android first, then the desktop window;
refine the current look ([direction.md](direction.md)) rather than start a new
one; put the app as it is into Figma first, so each design sits next to what it
replaces.

## Where it is

The Figma file *Qurb — redesign*, in the owner's drafts:
<https://www.figma.com/design/tUcZJoTac4123UAfLNF115>. Its three pages, the
most a free Figma plan allows:

- **Android.** On the left, *as it is*: 62 screenshots from the S23 (light
  theme, 2026-10-20) and 7 from the emulator, for setup and the empty states,
  in rows numbered as in the checklist. On the right, *redesign*: the new
  screens, built from the Foundations parts, as editable layers.
- **Desktop.** The 14 places of the desktop window, captured from
  `experiments/desktop-fixtures` as editable layers.
- **Foundations.** The app's colours as Figma variables, its type scale as
  text styles, and the components the new screens are made of.

The checklist is [inventory.md](inventory.md): every page, pop-up, message
and notification in both apps, with its words.

## Not captured, and why

- Notifications, the pairing approval, and *… asks to open your folder*:
  each appears only when something real happens. They are designed from their
  words in the checklist.
- The 24 words: a screenshot of them would have put the key in Figma.
- *Free unused space*, the privacy and notification switches, and *Save a copy
  to this phone* act as soon as they are touched, so they were not opened.

## The owner's list of 2026-10-23, on Home

| # | the owner | where it stands |
|---|---|---|
| 1 | The first page as simple as possible | Draft 1: one status in the middle, said once |
| 2 | Only a Sync and a Send button | Draft 1 |
| 3 | Which device does a phone with several sync with; how does it switch; how does the computer manage several? | Answered: every paired device, at once, each pass; nothing to switch (see below) |
| 4 | Devices in the phone app to select one device at a time, each with its own files backed up | Open: a question back to the owner (see below) |
| 5 | Backups like the Activity button at the top | Draft 1: a Backups button beside the clock |
| 6 | The sync circle in the centre; Waiting only when something is queued | Draft 1 |
| 7 | Why a conflict at all, if the phone and the laptop carry their own files? | Answered (see below) |

**3, as the code has it.** A phone's pass reaches every paired device at once
and syncs each one that answers (`crates/mobile-ffi`, `Qurb::sync_within`); the
computer's daemon does the same (`sync_all`). *Files* is one set, the same on
every device. The Private Vault, and with it the WhatsApp and folder backups,
goes to each device chosen to keep it, and a folder can be limited to chosen
devices ([0044](../decisions/0044-sharing-with-chosen-devices.md)).

**4, the question back.** Either the phone works with only the device
selected, and the others wait; or every device goes on syncing, and selecting
one is for choosing what it keeps and where Send goes. Recommended: the
second. A backup set for each device is not in the engine today: every device
keeping the vault keeps all of it.

**7.** Files are not separate copies: after a sync both devices hold the same
files. A conflict is the same file changed on two devices before they meet,
and Qurb keeps both versions rather than lose one. The four on the S23 were
made on purpose by the network tests of 2026-10-19, one for each network.
