# The desktop's page-by-page review, 2026-11-03

The phone app was reviewed page by page on 2026-11-02 (recorded in
[phase 5](../phases/phase-5-mobile.md#the-owners-page-by-page-review-2026-11-02)).
The desktop follows, with one rule from the owner: **the same philosophy as
the phone** -- very minimal, intuitive, and the phone's patterns rather than
desktop-only ones. Mockups are in [mockups/](mockups/). **Everything below
was built on 2026-11-03** (commits 4bb89b0 to 5d3fb79), checked in the
fixture window and the emulator, and **not yet tried together on the owner's
devices**, which is next. Where the build differs from the mockup, it says.

## Agreed, to build

**Home**

- **H-1. Waiting, redesigned on both apps**
  ([mockup, part A](mockups/devices-waiting-2026-11-03.html)).
  - One row per device that files are going to or coming from, with an arrow
    for which way. Backups, new files and sends to one device are one row;
    "only on this phone" is not a category of its own.
  - Each row shows the files' own **previews**, stacked, the one moving now
    on top with a progress ring over it. No icons.
  - A number, a size, and a chip only when the row is waiting on something:
    *Off*, *Open the app*.
  - A row opens the Waiting page: each device's previews in a grid, the moving
    one ringed, the queued ones with a small clock, sends with ×, a pause on a
    backup.
  - While anything moves, the circle at the top carries a ring, as Google
    Photos' profile picture does.
- **H-2. The device circle works as the phone's**
  ([mockup, part B](mockups/devices-waiting-2026-11-03.html)).
  - Clicking the circle opens that device's page; the dropdown goes.
  - The page is the phone's: picture, name, On/Off, ⓘ; *Reachable from
    anywhere* when so; one *Keeps here* card (Folders, WhatsApp, Private
    Vault; a person's sealed folder); *Remove* in red; *Switch device* in
    blue.
  - *Switch device* is the phone's chooser as a small centred window;
    right-clicking the circle opens it directly.
  - Identity, when paired and an earlier setup move behind ⓘ.
- **H-3. The laptop says when the phone has to be opened.** "Open QurbSpace
  on Galaxy S23" with a phone icon wherever the computer waits on the phone:
  the private-folder ask and Waiting (*Open the app*). Since decision 0082
  there is no instant wake-up, and opening the app answers in a second or two.
- **H-4. A computer's ask for the phone's folder lives 20 minutes, not 5**
  (`openings::ASK_LIFETIME`), so the phone's next background pass, about every
  15 minutes, usually finds it and notifies without the app being opened.
  Still only a request: nothing opens without the yes on the phone.

**Notifications, as they stand (asked 2026-11-03).** Since Firebase went
(decision 0082), the phone hears of a send or an ask only on its background
pass -- every 15 minutes at best, hours while dozing -- or when opened. The
notifications themselves exist ("… sent you a file", "… asks to open your
folder"). UnifiedPush through a public ntfy server would make them instant
for free, at the cost of a second app on the phone and someone else's server;
not taken.

**Files** (the owner, 2026-11-03: "maintaining a separate file system in the
qurb application is not necessary")

- **F-1. Files opens folders in the computer's own file manager.** QurbSpace
  keeps no file browser of its own: no folder pages, previews, selection or
  Received log. A first mockup that copied the phone's browser
  ([files-desktop-2026-11-03.html](mockups/files-desktop-2026-11-03.html)) is
  superseded.
  - *Shared* opens `~/qurb/Shared`; *Received* opens the received folder
    (`~/Downloads/qurb` by default). Both are ordinary folders already.
- **F-2. A person's folder is mounted while unlocked**, so the file manager
  opens `~/qurb/<person>` like any folder after the yes on their phone; files
  are decrypted in memory as they are read, nothing readable is written to
  disk, and ten minutes unused locks it back to the placeholder. FUSE on
  Linux; Windows would need a driver of its own, later. This changes
  [decision 0067](../decisions/0067-a-folder-for-each-person.md)'s *opened
  inside Qurb only* and needs a decision record of its own when built.
- **F-3. A computer keeps everything in Shared**: no local copies freed there,
  so the folder the file manager shows is always whole. The size limit stays
  for phones.
- **F-4. One locked folder per person.** The computer's own Vault joins its
  owner's folder, as a *This computer* folder beside the phone's Vault and
  backups, rather than a place of its own.

- **F-5. Files is a launcher**
  ([mockup](mockups/files-launcher-2026-11-03.html)): *This computer* (Shared,
  Received, each opening in the file manager) and *People* (each person's
  locked folder). A locked one asks the phone and shows "Open QurbSpace on
  your phone" until the yes; open, its tile is green with *Lock*.

**Settings** ([mockup](mockups/settings-desktop-2026-11-03.html))

- **S-1.** The phone's shape: one profile card (name and picture); *Manage
  storage* → *This computer*; *App* -- Activity, Notifications, Theme, Start
  at login, Update; *Connection* -- Speed limit, Advanced; *Quit QurbSpace* a
  red button at the end.
- **S-2.** *This computer*'s storage: free space, one bar by what fills it, a
  row for Shared, each person's folder and Recently deleted (which opens to
  put files back or empty it).
- **S-3.** Advanced: Identity, Key, Received files' place; *Meeting point*
  (was Rendezvous) and Relay; Version.
- **S-4.** *Back up folders* leaves Settings: a computer's own folders kept by
  another computer are set on that computer's page, *Keeps here* → Folders.
- **S-5.** *New files go to the Vault* is removed: where a file is put, Shared
  or the person's folder, says whether it is shared.

**Setting up and adding a device** ([mockup](mockups/setup-desktop-2026-11-03.html))

- **P-1.** First run is one *Start* button: the key and `~/qurb` made without
  asking. Choosing the folder and how much space it may take are gone (the
  folder is fixed; a computer keeps everything, F-3).
- **P-2.** *Add your phone* follows: QR code, six digits, countdown, *Later*.
  The same screen is what *Add a device* shows afterwards.
- **P-3.** Approving: the phone's picture and name and the six digits to
  compare, *Decline* / *Approve*; then Home.
- **P-4.** *Add a device* later: two cards, *My phone* and *Someone else's*;
  *Enter a code instead* a small link.
- **P-5.** *I already use QurbSpace*: the code from the phone typed here; the
  key comes from the phone, no words typed. The unlock screen stays.

**Activity** ([mockup](mockups/activity-desktop-2026-11-03.html))

- **Y-1.** The phone's Activity: chips *All, Backups, Transfers, Problems*
  (Problems counting failures), by day, a batch one row ("128 files backed
  up") with its files' previews and a badge for what happened, the other
  device's picture and name under it, the time at the end; words only for a
  problem, in amber.
- **Y-2.** An entry opens: when, from or to which device, size, its files as
  previews (a click opens one in the file manager); for a problem *Why* and
  *Tried* first, and *Try again* where it can be.

**The smaller pages** ([mockup](mockups/small-pages-desktop-2026-11-03.html))

- **K-1. Two versions** is the phone's sheet from Home's mark: both versions
  side by side with previews (text with its changes marked), each with its
  device's picture and age; *Keep this computer's*, *Keep Galaxy S23's*,
  *Keep both*. The separate page goes.
- **K-2. Recently deleted** (in This computer's storage): preview, name, the
  device that deleted it, ↺ and ×; days left only in the last week, in amber;
  *Empty* asking first.
- **K-3. Speed limit**: one card, ↑ and ↓ rows of chips; no headings or
  sentence; *Off* for None.
- **K-4. Key** is the phone's key sheet: safe or not, where this computer keeps
  it (the row chooses), which devices also have it, *Show the 24 words*
  asking first.

**Notifications and updates** ([mockup](mockups/notices-desktop-2026-11-03.html))

- **N-1.** The phone's words and buttons: "… sent you a file" with *Show*,
  "Delivered to …", "Could not finish …" with *Try again*, "QurbSpace is still
  running".
- **N-2.** An update in place restarts the program once the window is closed,
  nothing moves and no folder is mounted; the sidebar's button is gone.

**The phone, asked the same day**

- The preview's labels sat beside their icons and its bars used fixed gaps:
  centred, and fitted to the system bars. Seen fixed on the emulator.
- Videos play in the phone's own player, streamed from the computer.
- Manage storage counted file sizes once per identity a phone had had: 53 GB
  for the 26 GB the laptop held. Now the disk each part takes, each chunk once.
- Recently deleted is back in Manage storage, with Empty. On the computer,
  the phone's Vault deletions are listed while the owner's folder is open;
  they are put back from the phone.

## Built differently, or not built

- Waiting's backup has **no pause**: the engine has none to call.
- A person's folder is mounted **read-only** (decision 0083).
- The desktop's Activity entry shows the first few files of a batch, the
  names the engine keeps (V30), not every file.

## The redesign in mockups, 2026-11-03 (evening)

The owner was not happy with the built UI and asked for mockups "as refined
as figma", copied exactly into the apps. The foundation
([system/foundation.html](system/foundation.html), its tokens in
`system/tokens.css` and `system/components.css`) and every desktop page at
real size ([system/desktop.html](system/desktop.html)). Asked and answered:

- **Waiting** is option A on both apps ([system/waiting.html](system/waiting.html)):
  one picture with the other device's face on its corner, a title whose verb
  says the way, one quiet line -- amber only when it waits on you -- and how
  far it has got.
- **Remove, Switch device and Quit** are the phone's solid buttons: red and
  blue fills, white words.
- **Recently deleted** redesigned as Photos keeps its own: the files in a
  grid by where they were deleted, days left on the picture in the last
  week, Restore all and Delete all, Select to choose some (D13, D13b).
- **Removing a device ends it on both sides, and its files here must be its
  again when the same phone comes back** (the owner). Today removing forgets
  the device entirely, so the same phone paired again gets a new record and
  what was kept for it is locked away with nothing pointing to it. **To build
  in the engine:** Remove ends the trust and remembers the identity; a phone
  presenting that identity again (decision 0079) is answered with its old
  device id. Drawn as D21 (asked before removing), D22 (Home after), D23
  ("Galaxy S23 is back").

