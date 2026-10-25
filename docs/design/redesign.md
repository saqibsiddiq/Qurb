# The redesign of 2026-10

**Status:** started 2026-10-20. Home is at draft 3; Devices is at draft 4;
Settings is at draft 2; Backups and Activity are at draft 1; Folders to back up is a tree; Setup, Sharing into Qurb, Pairing and Notifications are at draft 1; the desktop is at draft 1; Files is at draft 4, on a simpler model ([0073](../decisions/0073-the-phone-works-with-its-computer.md)). **Building began
2026-10-24 with Android's Home, Devices, Settings, Backups, Activity, Setup, Pairing, Sharing into Qurb, Notifications, the QR scanner, Files and, the next day, the folders to back up as a tree** ([below](#built-android-home-2026-10-24)); the desktop is
still a design in Figma.

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
and notification in both apps, with its words. Until 2026-10-24 it missed the words
written into Android's layout files (27 of them, among them the whole welcome
screen); the script reads those too since.

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
| 4 | Devices in the phone app to select one device at a time, each with its own files backed up | Decided 2026-10-23: (b), below. To be designed with Devices; needs engine work |
| 5 | Backups like the Activity button at the top | Draft 1: a Backups button beside the clock |
| 6 | The sync circle in the centre; Waiting only when something is queued | Draft 1 |
| 7 | Why a conflict at all, if the phone and the laptop carry their own files? | Answered (see below) |

**3, as the code has it.** A phone's pass reaches every paired device at once
and syncs each one that answers (`crates/mobile-ffi`, `Qurb::sync_within`); the
computer's daemon does the same (`sync_all`). *Files* is one set, the same on
every device. The Private Vault, and with it the WhatsApp and folder backups,
goes to each device chosen to keep it, and a folder can be limited to chosen
devices ([0044](../decisions/0044-sharing-with-chosen-devices.md)).

**4.** Asked: either the phone works with only the device selected, and the
others wait; or every device goes on syncing, and selecting one is for
choosing what it keeps and where Send goes. **The owner chose the second.** A
backup set for each device is not in the engine today, where every device
keeping the vault keeps all of it: building it is engine work, and gets a
decision record when it is built.

**7.** Files are not separate copies: after a sync both devices hold the same
files. A conflict is the same file changed on two devices before they meet,
and Qurb keeps both versions rather than lose one. The four on the S23 were
made on purpose by the network tests of 2026-10-19, one for each network.
At the owner's word the laptop's `qurb-tests` folder (27 test files, 915 MB)
went to its Trash on 2026-10-23, so Recently deleted and the Trash both have
them. The phone's own private test files under the same name are deleted on
the phone, and were not yet.

## The owner's second list, 2026-10-23

1. Item 4 above: the second way. Clear the test folder: done, above.
2. "i dont like '.' that we are using, i want to convey the information while
   keeping minimum texts since i want the app to be so intuitive that the user
   understands everything without alot of texts". This is now a rule for every
   surface: no ` · ` separators and no full stops on short labels; titles of
   two or three words; a device as a chip (icon, name, a dot for on or off); a
   count as a badge; progress as a bar. A sentence stays only where a choice
   needs explaining. Home draft 2 applies it.
3. "i like the redesign".

## Devices, draft 1 (2026-10-23)

Asked for by the owner ("lets do devices section now") after choosing the
second way for item 4. Six screens, below Home on the Android page:

- **Devices**: each device a card with its icon, name, an on/off dot, how long
  since it was seen when it is off, and small icons for what it keeps (a chat
  bubble for WhatsApp, a folder with how many, a lock for Private Vault). *This
  phone* is listed apart. A second computer, *Office PC*, is made up, to show
  two devices keeping different things.
- **A device** opens to its own page rather than a sheet: Send, what it backs up
  (WhatsApp and Private Vault as switches, Folders opening a list), its
  identity, and Remove.
- **Its folders**: the phone's folders as switches with their sizes, and *Wi-Fi
  only* beneath.
- **Add a device**: four choices with icons and no descriptions.
- **Show my code**: the QR code, the six digits, the time left, *Copy code* and
  *Stop*. The long typed code moved behind *Copy code*. The QR code drawn is a
  pattern, not a working code.
- **Remove saqib?**: the one place a few words stay, because removing can lose
  files.

What the engine does not do yet: a different backup set for each device, and
WhatsApp and the folders kept by one device and not another. Today every
device keeping the vault keeps all of it.

## Devices, draft 2 (2026-10-23)

The owner on draft 1:

1. "can the whatsapp icon look similar to whatsapp logo?" A new icon in the
   app's line style: a speech bubble with a solid handset in it. It is drawn
   for qurb, not WhatsApp's logo; before a release uses it, WhatsApp's brand
   guidelines on showing their mark are to be checked.
2. Send on each device's page: kept.
3. Identity: behind an info button on the device's page, with when it was
   paired; and in Settings, when Settings is redesigned.
4. "remove saqib? looks very messy and asymmetric and ugly". Rebuilt: a mark in
   a pale red circle, the title and one line centred, one full-width warning,
   one full-width option, and Cancel and Remove of equal width.

A colour was added to the foundations for it: `error-bg`, #F7E9E7, as in the
app's colours.

## Files, draft 1 (2026-10-24)

Asked for by the owner ("lets do files"). Nine screens, below Devices:

- **Files**: a search icon and a menu beside the title, no sentence under it.
  Private Vault, the phone's folder on the computer, and Recently deleted as
  three tiles. Folders as tiles with how many files. Each file is its name and
  size; where it is, a badge on its icon: a green tick on this phone, a cloud
  on another device, an amber mark on none. Add is a round plus.
- **A folder**, **Search** with its results counted in a badge.
- **A file**: which devices have it as chips, and what can be done as a grid of
  icons with one word each (Open, Send, Save, Rename, Move, Vault, Free up,
  Delete).
- **Delete**, in the centred form of Remove, with *30 days in Deleted*.
- **Deleted**: how many days each has left as a badge, and a restore icon.
- **Vault**, with the device it is backed up on as a chip.
- **On saqib**: the phone's folders kept there, as tiles.
- **The menu**: sorting as a three-way switch, then New folder and Save all here.

File names in these screens are examples. The folder tiles were first a
single line and cut names short at two to a row; they are now an icon, the
name and the count, stacked.

## Files, draft 2: simpler (2026-10-24)

The owner, after the explanation of Files as it is: "this page is too
complicated". His list, and his three answers, are in
[0073](../decisions/0073-the-phone-works-with-its-computer.md). Eight screens,
below draft 1:

- **Files**: the selected computer's folder as one large card, and Vault,
  Received and Deleted as three tiles.
- **The computer's folder**: whether it is on, a search box, its folders as
  tiles.
- **A folder there**: its files, a new-folder button, and a round plus to add
  files from the phone.
- **A file there**: six actions, three and three: Open, Save, Send, Rename,
  Move, Delete.
- **Rename**, which says the change is made on the computer at once; **Delete**,
  which says it waits 30 days in the computer's Deleted.
- **The computer off**: its folder opens when it is on.
- **Received**: a small device mark on each file for which device sent it.

## Devices, draft 3 (2026-10-24)

Files shows the selected computer's folder
([0073](../decisions/0073-the-phone-works-with-its-computer.md)), so Devices
needed a way to select one; the owner: "yes add it to devices". The selected
computer's card has a green outline and a tick where its arrow was; its page
has *Use for Files*, a switch just under Send. One computer is selected at a
time; switching it on for another moves the selection.

## The owner's fourth list, 2026-10-24

1. "files folder should have 2 sections i.e 1st 'this phone' which has vault,
   recieved and deleted, 2nd 'devices' which has all the device folder
   listed(i dont like the idea of selecting device for files section)". **Files,
   draft 3**: those two sections, each computer a card with its on/off dot.
2. "selecting device should be for home section, and we can change the
   selected device in the home section itself". **Home, draft 3**: under the
   status, the chosen device's name with a small arrow; tapping it opens a list
   of devices, the chosen one ticked, and *Add a device*. A frame shows Office
   PC chosen while it is off. **Devices, draft 4**: the outline, tick and *Use
   for Files* of draft 3 taken out again.

Decision 0073 is amended to match.

## Settings, draft 1 (2026-10-24)

The owner: "sync all devices is fine, lets do settings now" (Sync on Home syncs
every device; Send goes to the chosen one). Six screens, below Files draft 3:

- **Settings**, in four groups of rows, each an icon, a name and a short value:
  *This phone* (Name, Identity, Key, Recovery), *Storage* (space used, with
  Free up beside it), *App* (Notifications, Theme, Motion), *Connection*
  (Background sync, Rendezvous, Relay), and the version beneath.
- **Name**, **Theme** (Auto, Light, Dark as a three-way switch), **Recovery**
  (where the key is safe; the 24 words behind the screen lock), **Background
  sync** (when it last ran, how often, Run now).

Gone from Settings, because the new model put them elsewhere or ended them:
Backups (each device's page), *Who has each folder* and *Keep new files
private* (no shared files on the phone), Recently deleted (Files).

## Settings, draft 2 (2026-10-24)

The owner: "merge key and recovery, hide rendezvous and relay under advanced,
lets keep notification one switch only". *Key* is one row (Safe) opening *Your
key is safe*; *Connection* is Background sync and *Advanced*, a page of its
own with Rendezvous, Relay and the version; Notifications is one switch with
no words under it.

## Backups and Activity, draft 1 (2026-10-24)

The owner: "lets do backups and activity now". Both are reached from Home's
top bar. Six screens, below Settings:

- **Backups** shows the device chosen on Home, and switches it the same way.
  WhatsApp, Folders and Private Vault are cards: a tick and the size when up to
  date, a bar and how many files are left while one runs, a switch when off.
  WhatsApp opens its details (files, size, what was deleted in WhatsApp and
  kept, when it last ran, Wi-Fi only) with *Stop* and *Back up now*. Setting it
  up names the two steps Android asks for: choose WhatsApp's folder, then
  Allow.
- One sentence is kept on Backups and on setting WhatsApp up: keep WhatsApp's
  own Google backup on too. A restore from the computer has not been tried
  ([0062](../decisions/0062-backing-up-whatsapp.md)), and until it has, nothing
  should suggest turning Google's off.
- **Activity**, by day. Each file carries a badge for what happened (arrived,
  sent, deleted, changed, two versions), the device involved by its icon and
  name, and the time on the right; *Show older* at the end. When nothing has
  happened yet, an empty state.

## Setup, Sharing into Qurb, Pairing and Notifications, draft 1 (2026-10-24)

The owner: "yes, next 4 in order". Each in its own section, below Backups and
Activity:

- **Setup**: *Welcome* (the mark, one line, *Start here* or *I already use
  Qurb*); *Welcome, key found* when the Google backup holds a key (*Use my
  key*); *Join*, with where the code is shown as icons (Qurb, Devices, plus),
  then *Scan a code* or *Type 6 digits*; six digits as six boxes; 24 words as
  24 numbered boxes; and the first Home, whose one action is to add a device.
- **Sharing into Qurb** from another app: *Save to* the Vault on this phone,
  or *Send to* a device; then the file sending with a bar and *Hide*; then
  *Sent to saqib*, or *Nothing to save*. *Save to Files* is gone with the
  shared files.
- **Pairing**: *Pair with saqib?* and *Let saqib join?* (a device that takes
  this phone's key, said in a warning chip), each with the number to compare
  large; *Approve on saqib* while this phone waits; *Open your folder on
  saqib?*, then the screen lock.
- **Notifications**, as Android draws them: what is ours is the words, the
  small icon, a progress bar and the buttons. Received (*Open*), delivered,
  the folder ask (*Answer*), sending with a bar, waiting to finish, and a
  failed send. *Open*, *Answer* and *Try again* on a notification are
  proposals: today's notifications have no buttons. The failure's words are an
  example; the real ones come from what failed.

## The owner's list of 2026-10-24, last on Android

1. "in previous version, there was no proper folder hierarchy while selecting
   folders to backup". **Folders to back up, a tree**: every folder opens, to
   any depth, each level indented under its parent; a checkbox ticked for all
   of it, a dash for some of what is inside, empty for none; ticking a folder
   takes everything inside it, folders added later too; what is chosen and
   *Wi-Fi only* stay above the tab bar. Today's chooser goes two levels deep and
   fixes the inner switch of a chosen folder; choosing folders at any depth is
   engine work. (Built 2026-10-25: *Built: Android, folders to back up as a
   tree*, below.)
2. "dark mode". **Dark**: ten of the key screens again, in the app's own night
   colours (`values-night/colors.xml`). The free Figma plan allows a colour
   collection one mode only, so the night colours are a second collection,
   *Qurb colours, dark*, of the same names, and the dark screens are copies
   bound to it. A change to a light screen is not carried to its dark copy.
3. The desktop, next. 4. Building, after.

## The desktop, draft 1 (2026-10-24)

The owner chose, when asked: the same four places as the phone; Files as *This
computer*, then *Phones*; Home as on the phone. Eight windows at 1280 by 800,
on the Figma file's Desktop page, to the right of the window as it is:

- **Home**: the device chosen here, its status in the centre, Sync and Send,
  Backups and Activity at the top; files dropped anywhere on the window are sent
  to the chosen device (a second window shows the drop).
- **Files**: *This computer* (Shared, the folder the computers share; Vault;
  Received; Deleted), then *Phones*, each phone's folder kept here.
- **A phone's folder**: search, then its folders.
- **Shared**: the list, and a file's details beside it rather than over it.
- **Devices**: phones, then computers.
- **A phone's page**: what it backs up here, with its folder tree beside it.
- **Settings** in two columns: this computer, storage (with where received
  files go), the app (with *Start at login*), and Advanced.

On the way the device chip was given a swappable icon, so a chip can name a
phone with a phone; it had shown a laptop for every device.

## The owner's list of 2026-10-24, two things forgotten

1. "we forgot to include shared folder in phone ui, which can be accessed by
   all the phone users". Asked back: opened on the computer, not copied to
   phones; reachable by the owner's phones and his guests'. **Files, draft 4**:
   *Shared* between *This phone* and *Devices*, a card naming the computer it is
   on, whether that is on, and how many people can reach it; inside, search,
   folders, files, a new folder and a plus. Decision 0073 is amended: item 6,
   and the widening of what a guest can reach, bounded later the same day:
   "shared folder can be accessed by all the users of the desktop and its
   read-write" -- every guest, and each may change it.
2. "we forgot to redesign the qr code scanner". **QR scanner**: the camera
   filling the screen with a frame in the middle and the rest dimmed; close and
   the torch at the top; *Point at the code*; found turns the frame green and
   names the device; a code that is not Qurb's turns it red and says so; *Type 6
   digits* at hand. The same scanner serves *Visit a computer*.

## Built: Android Home (2026-10-24)

The owner: "start building". Home draft 3 is in the app (`HomeScreen.kt`,
`screen_home.xml`, `StatusMark.kt`):

- the status mark, 168 dp, in the middle of the screen: All synced, Syncing (a
  third of the ring lit, travelling), Two versions (with a count badge and
  *Review*), *<device> is off*, Backing up, Not synced yet, and the first Home's
  *Add a device*;
- the chosen device as a chip (icon, name, a green or grey dot, an arrow),
  opening device cards with the chosen one ticked and a dashed *Add a device*
  card; the choice is kept on the phone (`Engine.chosen`, `Engine.choose`);
- Sync and Send of one size; Send goes straight to the chosen device's
  Downloads (the list of where to send is gone);
- Waiting only while something is queued: a count badge and queue rows, a bar
  on a backup's row;
- Backups and Activity at the top (both built since, [below](#built-android-backups-and-activity-2026-10-24)).

*Off* is honest only as far as the phone knows: a pass marks each device that
answers as seen, so a device not seen since the start of the last pass that
tried someone is off (`Engine.isOff`). A device no pass has tried yet is not
called off.

The shared parts went into `Kit`: `deviceChip`, `dot`, `badge`, `deviceCard`,
`addCard`, `queueRow`. The large icons are `mark_*.xml`, written by
`scripts/android-icons.py` at 60 dp with a thinned stroke.

**Checked** on an emulator shaped like the S23 (`qurb-34g`, Android 34 with
Google APIs, 1080 by 2340 at 480 dpi), paired with a throwaway daemon run from
a scratch folder with its own config: the first Home, light and dark; Backups;
pairing; *All synced*; *Syncing*; *saqib is off* with the daemon stopped;
Waiting, with two files made while it was off; *Two versions*, from one file
changed on both sides apart, and its Review; the list of devices. Pairing it
needed a temporary, uncommitted patch to setup, because the emulator's camera
could not get the barcode model from Google services. **Not checked:**
*Backing up*, and anything on the S23.

Two things the checking found, both fixed:

- A device just paired was called off before any pass had tried it. Now a
  device is called off only once a pass has tried it.
- A computer that reached the phone through the relay, and collected from it,
  was called off, because only the phone's own pass marked a device seen. The
  engine now tells the phone who reached it (`Served::asked_by` in
  `crates/peer`, tested in `tests/served.rs`), and the phone marks them seen
  when the pass ends. On the emulator the next pass said *All synced*, but
  that pass may have reached the computer itself, so the emulator does not
  show which of the two did it; the tests do.

After a Sync, *All synced* and *no device answered* are no longer said again
in a snackbar under Home, which says them itself.

## Built: Android Devices (2026-10-24)

Devices draft 4 is in the app (`DevicesScreen.kt`, `DeviceScreen.kt`):

- **Devices**: the title and a round Add; each device a card (icon, name, dot,
  how long since it answered, an arrow), with pills for what it keeps when it
  keeps this phone's Private Vault: WhatsApp, the folders with how many, a lock;
  *Visiting* for computers this phone visits as a guest; *This phone* apart.
- **A device's page**: its icon, name, *On* or *Off*, an info button for its
  identity and when it was paired; Send; *Backs up* with WhatsApp, Folders and
  Private Vault; Remove (*Stop visiting* for another person's computer).
- **Add a device**: four options, no descriptions. **Show my code**: the QR
  code on a white card, the six digits large, a clock with the time left,
  *Copy code* and *Stop*. Copying leaves the sheet open: closing it cancels the
  code, so a copy taken by closing would already be dead. The long code moved
  behind *Copy code*.
- **Remove**: centred, the device's mark on pale red, one line, one warning
  (files only there; else the vault kept there; and sends cancelled), *Delete
  its copies here* when it has any, Cancel and Remove of one width.
- **Every sheet's two buttons are of one width now**, and every button is a
  pill; secondary buttons are the surface with a hairline edge.

**What the engine does not do, and the page says no more than it does:** a
backup set for each device. WhatsApp and the folders travel inside the Private
Vault, to every device keeping it, so their rows show only on a device that
keeps the vault, and WhatsApp's switch there turns WhatsApp's backup on or
off for all of them. *Folders on saqib* is still the old folder list: the
redesign replaced it with the folder tree, which needs engine work (folders at
any depth). Show my code is a sheet, where the design drew a page. The Private
Vault row has no size: the engine does not report one.

Checked on the same emulator: the list, a device's page, its details, Add a
device, Show my code, Remove (cancelled), keeping the vault (the rows and the
lock pill appear), Home's list of devices.


## Built: Android Settings (2026-10-24)

Settings draft 2 is in the app (`SettingsScreen.kt`, `AdvancedScreen.kt`):
*This phone* (Name, Identity, Key), *Storage* (Space used with Free up),
*App* (Notifications as one switch with no words, Theme, Motion),
*Connection* (Background sync, Advanced). Key opens where the key is kept,
checked as before, and *Show the 24 words* asks for the screen lock first
where the phone has one. Theme is three buttons of one width. Background sync
opens when it last ran and how often, and *Run now*. Advanced has Rendezvous,
Relay and Version. The engine's version line lost its `·` too.

**Where it differs from the design, and why:**

- **Name is not changeable.** A device learns this phone's name when they
  pair and keeps it, so a new name would reach no device already paired. The
  design's Name sheet waits on the engine telling devices a new name.
- **Recently deleted stays in Storage** until Files has it. (Files has it
  since the same day, *Built: Android Files* below; it is in both for now.)
- **The shared files' two settings** (*Who has each folder*, *Keep new files
  private*) **are under Advanced**, labelled *Shared files*. The design drops
  them with the shared files, but the phone holds shared files until 0073's
  engine work, and until then they still do something. (Gone with that work,
  the same day: *Built: Android Files*, below.)
- Identity needed a new engine call, `identityShort()`.

Checked on the same emulator: the page, scrolled; Advanced; the Key, Theme and
Background sync sheets. The emulator has no screen lock, so the screen-lock
path for the 24 words is not checked, and the words were not shown.

## Built: Android Backups and Activity (2026-10-24)

Backups and Activity draft 1 is in the app (`BackupsScreen.kt`,
`ActivityScreen.kt`, `Backups.kt`, `Keeping.kt`):

- **Backups**: the device chosen on Home beside the title, chosen the same way
  (the list moved into `Kit.chooseDevice`, shared with Home); WhatsApp,
  Folders and Private Vault as cards, each a tick and the size when up to date,
  a bar and how many are left while it runs, a switch or *None chosen* when it
  is off; *Keep WhatsApp's own Google backup on too* under them while
  WhatsApp's backup is on.
- **WhatsApp**: its details as a sheet of facts (files, size, what was
  deleted in WhatsApp and kept, when it last ran, Wi-Fi), with *Stop* and *Back
  up now*; setting it up names the two steps Android asks for, says the
  sentence about Google's backup, and has *Not now* and *Choose folder*.
- **Activity**: by day (*Today*, *Yesterday*, then the date); each entry the
  file's icon with a badge for what happened, the device by its icon and name,
  the time as the phone is set. A failure says why, in its device's place.
  *Nothing yet* when empty.

**Where it differs, and why:** on a device that does not keep the Private
Vault, Backups shows the vault alone, as a switch, since WhatsApp and the
folders go only where the vault goes (no backup set per device, 0073). The
Private Vault card says how many private files are still to go, not a size: the
engine reports none. *Off* for WhatsApp shows a switch only while it is off;
on, the card opens the details.

Checked on the same emulator: Backups for a device keeping the vault, the
WhatsApp setup sheet, Activity with real entries from the tests above. Not
checked: WhatsApp's details while it backs up (the emulator has no WhatsApp),
*Nothing yet*, dark.

## Built: Android Setup (2026-10-24)

Setup draft 1 is in the app (`SetupActivity.kt`, `activity_setup.xml`), with
*Waiting for saqib* from Pairing draft 1:

- **Welcome**: the mark, *Qurb* and one line in the middle; *Start here* and
  *I already use Qurb* at the foot. With a key in the Google backup, *Your key
  is in your Google backup* over *Use my key* and *Start again here*.
- **Join**: where the other device shows its code, as Qurb, Devices and the
  plus; *Scan a code* and *Type 6 digits*; *Use 24 words* at the foot.
- **Six digits** as six boxes in two threes, the next one edged green, over a
  field nobody sees that brings up the number pad. The sixth digit joins, as
  WhatsApp's linking code does; *Join* is there for trying again.
- **24 words** as 24 numbered boxes, three to a row, the one being typed in
  edged green; a phrase pasted into any box fills it and the boxes after it.
- **Waiting**: *Approve on saqib*, the number large, *saqib shows this
  number too*, and Home's status mark at 96 dp with its ring travelling round
  the other device's icon, a laptop or a phone. A join that fails goes back to
  the step it came from.
- *Join* and *Restore* are enabled only once six digits or 24 words are in.
  Before, tapped short, they did nothing and said nothing.

**Where it differs, and why:**

- **The name, since decision 0074.** Built first as *Approve on your other
  device*: the phone learned the other device's name only once the join was
  done, since the code carries no name. Now it asks who shows the code before
  joining (`pairingWith`). A device from before that, or one that does not
  answer within three seconds, is still *your other device*, with a devices
  icon.
- **No Cancel while waiting, and back does nothing there.** The engine cannot
  call a join off midway, so a Cancel would leave it running behind a page
  saying it had stopped. It ends when the other device answers, or when the
  code lapses after five minutes.
- **Start again here asks first**: a sheet, *Start again with a new key?*, with
  one warning, *The key in your Google backup is replaced*, and Cancel and
  Start again. It replaces it, as *Set up Qurb here* did beside the old text
  link without asking; the redesign gives it a full button, easier to reach.
- **Join with a code, with a key found**: a text button under the two. The
  design offered only the key or a new one, so a phone whose backed-up key was
  left from a setup nobody uses could join only after *Clear data*. Joining
  keeps the joined key in Block Store in its place.

  Both were put to the owner as questions after the first build; the answer,
  "do the recommended options and continue". Seen on the emulator with the
  key-found state forced by a temporary, uncommitted patch.
- The first Home was built with Home.

**Found on the way, and fixed:**

- **Text weights set in layouts were lost.** From Android 9, every semibold or
  medium text style named in a layout file came out regular, and so did every
  button, Kit's too. AppCompat 1.7.0 reads a view's text appearance, then the
  view's own attributes, and the second reading sets the weight back to
  unspecified, having already cleared bold, then finds Inter again in the
  theme. Text styled from code was right, which is how it was found: setup's
  *Qurb*, read regular beside the design's semibold. `QurbViewInflater`, named
  in the theme, puts the weight back for layouts' text views and buttons, and
  Kit's buttons get it too. Home's state, its buttons and every sheet's buttons
  are now semibold, as the design draws them. Android 8 was not affected.
- **A refused join read "Network problem."** *The other device said no*, an
  expired code and a code already used were filed under the network when
  setting up a phone, though pairing had been fixed on 2026-10-11. `join_new`
  now says them as refusals (`a_phone_the_desktop_declines_is_told_so_and_not_set_up`
  in `crates/mobile-ffi/tests/syncing.rs`).
- **Setup's progress bar never showed**: it sat in the layout's padding, which
  clipped it.
- **Welcome's buttons could change under a finger.** They showed at once and
  became *Use my key* and *Start again here* only when Block Store answered,
  so a tap meant for the kept key could make a new one in its place. It did,
  on the emulator. They now appear once Block Store has answered, which it must
  within ten seconds; on the emulator they were there within the first second.

**Checked** on the same emulator: Welcome, and Welcome with a key found;
Join; six digits, typed, the sixth looking them up and failing, since the
emulator is behind its own NAT and cannot hear a device offering them; 24
words typed with the keyboard's Next; waiting, against a throwaway `qurb
pair` in a scratch folder with its own config, the number matching the one the
terminal asked about; approved there, then Home; declined there, then Join
with *The other device said no.*; back while waiting; light and dark. The
long code was handed to setup by a temporary, uncommitted patch, since the
emulator's camera cannot load the barcode model. The weights were checked in
the debug build and in the R8-shrunk one, which keeps `QurbViewInflater` by
name (`proguard-rules.pro`).

**Not checked:** *Use my key* and *Start again here* acting. The emulator's
Block Store kept no key across a reinstall, though the S23's did on 2026-10-08.
Also not checked: restoring from 24 real words, scanning a code, a phrase
pasted into the boxes, and anything on the S23.


## Built: Android Pairing (2026-10-24)

Pairing draft 1 is in the app (`Approval.kt`, `waiting_for.xml`, and the folder
ask in `MainActivity.kt`), all four of its screens:

- **Pair with saqib?** and **Let saqib join?**, on the phone showing a code: a
  sheet with the device's icon in a round green mark, the title, the number
  large on the second surface, *The same number on saqib?* after a tick, and
  Decline and Approve. *Let saqib join?* is for a device that takes this phone's
  key, and says so in a chip, *It gets this phone's key*, under a key. Swiped
  away, the sheet is a no; a code that lapses while it is up takes it away.
- **Waiting for saqib**, joining from this phone: setup's page, moved to a
  layout of its own (`waiting_for.xml`) so that Devices shows the same thing,
  over the whole app. It had been a dialog, *Approve this phone on the other
  device*.
- **Open your folder on saqib?**: a lock in the round mark, the title, *It
  locks itself after 10 minutes unused*, the chip *Only if you asked at
  saqib*, and Not now and Open; then the screen lock, as before. It had been
  four sentences.

The chips are a size under the warning chip elsewhere, their words in the
middle, as the design draws them (`Sheet.warning(compact = true)`).

**Where it differs, and why:**

- **"after 10 minutes unused", not "after 10 minutes".** The computer locks
  the folder after ten minutes in which nothing used it, not ten minutes after
  opening (`crates/desktop/src/stream.rs`); the design's words would promise
  less time than there is.
- **No Join back link and no Cancel on Waiting for saqib**, as in setup: the
  engine cannot call a join off midway, and back does nothing there.
- **Own or visited is no longer said.** The old ask told the owner's computer,
  which keeps the folder unsealed, from one visited, which keeps it sealed. The
  design says neither, and the app now follows it.
- **A long name wraps the chip** onto two lines, centred, the icon at the
  start. The design's names fit on one.

**Checked** on the emulator (`qurb-34g`, light and dark), with the command line
built for x86_64 Android and run on the emulator itself, beside the app. The
emulator's NAT redirect does not reach its guest at 10.0.2.16, so a laptop
could not reach the code the phone showed.

- *Let a device join?* from a folder with no key, which told the phone's name
  (`Who`, answered by the phone): declined, and the command line was told *the
  other device said no*, with no key written; again with the same code,
  approved, and that folder took the key. The numbers matched each time.
- *Pair with Emu-Laptop?* from that folder, now holding the key: swiped away,
  and the command line was told no.
- Joining from Devices: *Approve on Emu-Laptop* over the app, back pressed and
  nothing happening; approved there, paired; declined there, the page went and
  *Could not connect: The other device said no.* followed. The long code was
  handed to `joinWith` by a temporary, uncommitted patch.
- Setup's last step from a phone cleared of its data, after the move to the
  shared layout: *Approve on Emu-Laptop*, then Home.
- *Open your folder on Scratch-Laptop?*, made up by the same patch, since no
  computer asked. Not now answered it without failing. Open, and the screen
  lock after it, were not checked here.

**On the S23**, 2026-10-24, the fast build installed over its data, dark
theme: visiting a throwaway `qurb pair --guest` on the laptop (*Scratch-PC*)
from Devices, *Approve on Scratch-PC* came up over the app, round a laptop,
with the number the terminal asked about, 247 931; approved there, *You visit
Scratch-PC as a guest now*.

**Not checked:** *Pair with …?* and *Let … join?* on the S23, which would have
given a throwaway device the S23's key; the approval sheet going when the code
lapses under it, five minutes on, which is from reading `Asker`; a computer
really asking to open the folder; the desktop window showing a code to a phone, the usual case, since the
desktop window is not redesigned yet. *Could not connect* is still the old
dialog.

## Built: Android Sharing into Qurb (2026-10-24)

Sharing into Qurb draft 1 is in the app (`ShareActivity.kt`). The activity
draws nothing of its own any more: its sheets rise over the app the person
shared from (`Theme.Qurb.Share`, translucent), where a dialog of its own stood.

- **Save to**: *This phone*, the Vault as a card with a lock; *Send to*, each
  device as Devices shows it, with its dot, how long since it answered, and an
  arrow. *Save to Files* is gone, as decision 0073 has it; so are *Save to my
  folder on …* and *Send to …'s Downloads*, which the Vault and the device's
  name now stand for. With no device paired, nothing is asked and the file is
  saved to the Vault (where *Keep new files private* said, until the phone
  left the shared area the same day).
- **Sending**: the file's kind in a quiet tile, its name, a bar, the send and
  device icons and the device's name, and Hide. The bar turns while the file is
  copied in, then fills with the bytes that device has collected.
- **Sent to saqib**, a tick, once that device has collected every file, which is
  when the engine no longer has them waiting for it. **Nothing to save** when
  the share held no file.

**Where it differs, and why:**

- **Waiting for saqib**, a clock, not drawn in the design: after 30 seconds in
  which nothing went out, the device off or out of reach. It collects the file
  when it is next on; saying *Sent* then would not be true, and a bar standing
  still says nothing.
- **Saving shows the Sending sheet**, the lock and *Vault* where the device
  would be, with no Hide, then *Saved to Vault*. The design draws no saving
  step, and a long video takes a moment to copy in.
- **Hide waits for the copy.** The share sheet's permission to read a file ends
  with this screen, so the file is copied in first, and asked about if it went
  to that device before (decision 0059). Hide, a swipe and back do nothing
  until then, usually a second; after, they leave the rest to the sync worker.
- **Its failures in the same sheet**: *Couldn't read that*, *Set up Qurb first*,
  and *Saved 3 of 4*, with the attention mark. The old explanations under each
  outcome are gone, as on every redesigned page.
- **The bar is one bar.** Material 3 cuts a gap between what is done and what is
  not and puts a dot at the end; both are turned off.
- *Sent to … before*, the question of decision 0059, is still the old dialog.

**Checked** on the emulator (`qurb-34g`), sharing a 9.5 MB picture from its
Files app, with the command line built for Android running beside the app as
*a device*. Shared from the shell instead, the app was refused the picture
(`SecurityException`), and the sheet said *Couldn't read that*, which is how
that path was seen.

- *Save to*, light and dark; *a device* sent to and collected, *Sent to a
  device*, the file in that device's Downloads, 9,494,019 bytes.
- Emu-Laptop, off: *Waiting for Emu-Laptop* after the 30 seconds.
- *Vault*: the bar turning, then *Saved to Vault*.
- The same picture to *a device* again: *Sent to a device before* asked, *Send
  it again*, then Hide; that device's log has the pass collecting it after the
  sheet had gone.
- *Nothing to save*, from text shared with no file.

**On the S23**, the same day: a photo from the Gallery shared into Qurb and
sent to *Scratch-PC*, a computer it was visiting as a guest; *Sent to
Scratch-PC*, said by the owner, and the 3.1 MB file in that computer's
Downloads, collected on the first pass. The copy was deleted after.

**Not checked:** saving to the Vault on the S23, and sending to the owner's own
laptop, both left alone so as not to put test files in real folders; *Set up
Qurb first*; a swipe or back while a file is copied in, which is from reading
the code; a share of several
files; a file large enough for the worker's notification (32 MB); a share
while another sync is running.

## Built: Android Notifications (2026-10-24)

Notifications draft 1 is in the app (`Notices.kt`, `Transfers.kt`). Android
draws them; what is the app's is the words, the small icon, the bar and the
buttons:

- **The icon**: the mark's glyph (`qurb_glyph`) on the app's green, for every
  notification. It had been a bell, a lock, or arrows.
- ***saqib* sent you a file**, its name under, and **Open**, which opens the
  app and hands the file to whatever opens its kind, as Files does.
- **Delivered to *saqib***, the file's name under. It had been *Delivered*,
  *saqib has report.pdf*.
- ***saqib* asks to open your folder**, *Only if you asked there*, and
  **Answer**, which opens the app to the question behind the screen lock, as a
  tap does.
- **Sending to *saqib***, *120 MB of 480 MB* and the bar, for the large
  transfer the sync worker runs in the foreground; *480 MB waiting* before
  anything has gone.
- **Tap to finish sending**, *1.2 GB waiting*. Its old second sentence, on why,
  is gone.
- **Could not send *report.pdf***, with the reason under it.

**Where it differs, and why:**

- **Sending names a device only when the phone has one other.** The bytes the
  worker waits on are sends to one device, shared files for any, and the Vault
  for its keepers; with two devices or more the notification cannot know which
  will collect, and says *Sending to your devices*. With one computer, as
  decision 0073 has a phone, it names it.
- **Try again only where a sync can retry it.** A send fails only when its file
  changed or went before it was collected (*Could not send*, with that reason);
  sending it again is choosing it again, so that notification has no button.
  Any other failure is retried by the next sync, which *Try again* starts now,
  under *Could not finish …*. The design's *saqib did not answer* is not a
  failure the engine has: a send to a device that is off waits for it.
- **Answer opens the app**, as tapping the notification already did: the
  question is answered behind the screen lock, never from the shade.
- The buttons are in Android's accent colour, not the app's green, and the bar
  in Android's: the system draws both.

**Checked** on the emulator (`qurb-34g`, Android 34), the phone set up anew and
paired with the command line built for Android and running beside it, the app
away: *a device sent you a file*, *report.png*, from `qurb send`; *Open* there,
which showed the picture in Photos and took the notification away; *Delivered
to a device*, *holiday.png*; and *Sending to a device* with its bar, from *193.1
MB waiting* through *4.6 MB of 193.1 MB* to *193.1 MB of 193.1 MB*, sharing a
190 MB picture and pressing Hide.

**Seen and not explained:** four *Delivered to a device* notifications for what
were three sends of `holiday.png` to that device. The words are not the
question; which history entries are written is, and that was not looked into.

**Not checked:** anything on the S23; a failure of either kind; *Try again*;
a computer's ask and *Answer*; *Tap to finish sending*, which needs Android to
refuse the worker the foreground.

## Built: Android QR scanner (2026-10-24)

The QR scanner from the owner's list of 2026-10-24 is in the app
(`ScanActivity.kt`, `ScanFrame.kt`, `activity_scan.xml`), for *Scan a code* in
setup and in Devices, and for *Visit a computer*:

- **Looking**: the camera filling the screen, black at 45% over all of it but
  a square, and four rounded corners round the square, white. Close and the
  torch at the top, round, white at 14%, with *Scan a code* between them;
  *Point at the code* and *Type 6 digits* under the frame. The torch shows only
  on a camera with a light.
- **Found *saqib***: the corners green, and in a white pill a tick, the
  device's icon and its name, which the scanner asks the device showing the
  code (`pairingWith`, decision 0074) -- a few seconds at most, *Found* until
  it answers. Then back to setup or Devices, which join.
- **Not a Qurb code**: the corners red, the words in a pill of the error's own
  colour over *Type 6 digits*, until the code has been out of sight a moment
  and a half; then looking again.
- ***Type 6 digits*** goes to the six boxes in setup, or the number pad in
  Devices.

**Where it differs, and why:**

- **Found asks who it is, and joining asks again.** Setup and Devices each ask
  the device's name and number before joining (`pairingWith`); the scanner
  asks too, to name it, rather than hand both back. One more round trip, a few
  milliseconds on a home network; three seconds more for a device that is not
  there.
- **Camera refused**: *Camera not allowed* where *Point at the code* stood, and
  *Type 6 digits* under it. *No camera* when it cannot start. The design draws
  neither.
- The words under the frame keep one height whichever show, so the frame does
  not move when a code is found; it did, on the first build.

**Found on the way, and fixed:** the scanner took only codes beginning
`qurb1-`, so a guest code, `qurbg1-`, was looked past without a word, and
*Visit a computer* by scanning could never work. It takes both now.

**Checked** on the emulator (`qurb-34g`, its emulated back camera): looking,
with the veil over the camera's picture and the torch; found, the corners
green and *Office PC* named by a throwaway `qurb pair` of the command line
built for Android, and the frame not moving; not a Qurb code, then looking
again; *Type 6 digits* to the number pad from Devices and to the six boxes in
setup; Close. Found and not a Qurb code were brought on by a temporary,
uncommitted patch: the emulator's camera cannot load the barcode model, so it
reads nothing.

**On the S23**, 2026-10-24, the owner holding it: a guest code shown on the
laptop's screen (`qurb pair --guest`, drawn by `qrencode`), scanned from
Devices, *Visit a computer*. The corners turned green and named *Scratch-PC*,
then *Approve on Scratch-PC*, and the visit was made: the guest code that the
scanner had looked past before. Screenshots were kept every second over
wireless debugging while the scanner was up.

The same code scanned again, after it had been used, said *Found* and
*Approve on your other device* -- the device showing it no longer listens, so
it cannot say who it is -- and then the old *Could not connect* dialog,
blaming a firewall. A code used once reads as a device not there. Not fixed
here: the phone cannot tell the two apart from what the network gives it.

**Not checked:** the torch lighting; *Camera not allowed* and *No camera*;
*Not a Qurb code* on the S23, which needs a code that is not Qurb's in front
of it.

## Built: Android, the Shared page (2026-10-24)

The Shared page of Files draft 4 is in the app (`SharedScreen.kt`), reached
from Files as *Shared, On saqib* -- one for each of the owner's computers and
each computer visited -- and backed by decision 0073's steps 1 to 3:

- **The page**: *Files* to go back; *Shared* with a new-folder button and the
  round plus; which computer, its laptop and its dot; *Search Shared*; the
  folders as tiles two to a row; the files with their kind in a small tile and
  their size at the end. Inside a folder, its name in the title and the folder
  above it to go back to.
- **A file**: Open, Download, Rename, Delete. **A folder held**: Rename,
  Delete. Deleting asks first, *… keeps it in Recently deleted for 30 days*.
  **New folder** and **Rename** ask for the name in a sheet, its stem chosen.
- **The plus** adds files from the phone's picker into the folder on screen,
  a sheet with each file's name and a bar while it goes.

**Where it differs, and why:**

- **No count of the people who can reach it**, on the card or the page, and
  **no count on a folder's tile**: the computer does not say either yet.
- **The plus stands in the title**, as Devices' does, not floating at the foot:
  the page's layout has no place for one that floats.
- **Files around it is not redesigned yet**: *Shared* is a row in today's
  Files, not the card of draft 4.

**Checked** on the emulator (`qurb-34g`), the command line built for Android
running on it as the phone's own computer (`qurb run`), with two folders and a
file in its shared folder: the page as drawn, the empty folder on its disk
listed; *Trips* made, there on its disk; a 6.3 MB picture added from the
picker, the same bytes there; renamed *sea.png*, the stem chosen in the field
-- which the first build did not do, the name typed after `.png`; *PASTA*
finding *Recipes/pasta.txt*; *Trips* held and deleted, and `qurb deleted` on
the computer listing it *deleted on* the phone; a text file opened.

**On the S23**, the same day, the fast build over its data, dark theme, as a
guest: it visited a throwaway `qurb pair --guest` on the laptop (*Scratch-PC*)
by its six digits, typed over wireless debugging and found on the home Wi-Fi,
with *Approve on Scratch-PC* while it waited. Then *Shared, On Scratch-PC* in
Files: the folders and a file listed; *From the S23* made; a 3.8 MB picture
added from the picker, the same bytes there; renamed, the stem chosen;
*PASTA* finding *Recipes/pasta.txt*; the folder deleted, and Scratch-PC's
`qurb deleted` listing both files *deleted on SM-S911B*.

Files also listed *Shared, On saqib*, the owner's laptop, whose Qurb is from
before this: its page can only say it is not reachable until the laptop runs
this build. Saying *update Qurb on saqib* instead is not done.

**Not checked:** a refusal shown; a large file's progress; a file opened on
the S23.

## Built: Android Files (2026-10-24)

The rest of Files draft 4, around the Shared page, with decision 0073's steps
4 and 5 under it:

- **Files** (`FilesHome.kt`): *This phone* as three tiles, Vault, Received and
  Deleted, each its icon on the soft green and its name; then *Shared*, a card
  for each computer, the people icon and a pill naming the computer; then
  *Devices*, each of the phone's own computers with its dot, when it was last
  seen, and an arrow to the phone's folder there.
- **Received** (`ReceivedScreen.kt`): what was sent to the phone, the newest
  first, each with the name of the device that sent it.
- **Deleted**: no sentence under the title; each file when and where it was
  deleted, its days left as a clock and a number, and Restore as its icon.
- **The phone's folder on a computer** (`KeptScreen.kt`): *Search*, which asks
  the computer and searches the whole folder; inside the phone's Private Vault
  there, *Add*, from the phone's picker into the folder on screen. A file of
  the phone's own has *Rename*, *Move to folder* and *Delete* below Open,
  Download and Restore; a backup's file only *Delete*. Deleting asks first, in
  the centred form, *In Recently deleted for 30 days* -- the phone's own, which
  puts it back from the copy the computer keeps as long.
- **Files' rows lost their `·` joins**: a file is its name and its size; the
  Vault's has the place icon before the size.
- **Gone with the shared area** (0073, step 5): *Who has each folder* and *Keep
  new files private* from Advanced, *Move to Files* and *Move to Private Vault*
  from a file's sheet, and the choice of area from Sharing into Qurb, which
  saves to the Vault.

**Where it differs, and why:**

- **No counts** on the tiles, the Shared cards or the folder tiles: nothing
  gives them yet.
- **No new folder in the phone's folder on a computer**: the index lists
  files, so a folder with nothing in it exists nowhere but on the screen that
  made it. *Move to folder* with a new name makes one.
- **Received misses a send older than the last 500 happenings** in the
  history, which is what it reads.
- **A page open when a sync brings something new** keeps what it showed until
  it is opened again or pulled down, as every page in the app does.
- **Recently deleted is in Storage as well** as in Files.

**Checked** on the emulator (`qurb-34g`, the debug build), set up from
nothing, the command line built for Android running on it as the phone's own
computer and keeping its Vault: Files as drawn; *diary.txt* added to the Vault
from the picker; in the phone's folder there, renamed *journal.txt*, listed
so, then deleted, the folder empty; Deleted listing it with 30 days, restored,
back in the Vault; a file sent with `qurb send` listed in Received under the
computer's name; Shared opened from its card. Then with a second computer
paired the same way: *journal* typed into Search, found; moved into *Notes*
and found there; Back clearing the search; *soup.txt* added inside *Private
Vault › Notes*, there on the computer.

That last check found a fault, fixed the same day: deleting a file from the
phone's folder whose bytes were not on the phone -- freed to the computer, as
most there are -- put nothing in Recently deleted, though the sheet promised
it, and the computer let go of its copy within two weeks. Such a file now
goes into the phone's Recently deleted costing nothing on the phone, and
restoring it puts it back from the chunks the computer still holds; the
computer keeps a vault's deletions 30 days, and lists a Private Vault one in
its own Recently deleted too, where restoring it puts it back on the phone
(0073, *Progress*). Checked again
on the same emulator with the fix: *journal.txt* freed on the phone
(*Available elsewhere*), deleted from *Private Vault › Notes* on the
computer, the folder empty; in Deleted with 30 days, its sheet with no size;
restored, back in the Vault still freed, and after a sync back in *Notes* on
the computer, where *Download* brought its 21 bytes to the phone from the
computer.

**Not checked:** a backup's file deleted from the phone's folder; any of this
on the S23, which runs it since 2026-10-25 but has not been looked at.

## Built: Android, folders to back up as a tree (2026-10-25)

Item 1 of the owner's list of 2026-10-24, as drawn in *Folders to back up, a
tree* (`FoldersScreen.kt`), with the engine work under it in decision 0063's
amendment of 2026-10-25:

- **The page**: the way back, *Folders* with the computer's chip beside it,
  then the tree on one card. Each row an arrow that opens it when there are
  folders inside, its checkbox, the folder's icon -- green at the top,
  quiet inside -- its name and size; each level 26dp further in.
- **The checkbox**, the redesign's own: ticked for all of a folder, a dash
  for some of what is inside, empty for none (`checkbox_on`, `checkbox_off`,
  and its two marks exactly as exported). Ticking takes everything inside,
  folders added later too; unticking a folder inside a ticked one sets it
  apart, and its folder takes a dash.
- **What is chosen** -- its size, the Wi-Fi mark, *Wi-Fi only* -- kept above
  the tab bar, in a slot the shell keeps for a page's own bar
  (`Screen.dockBar`), the tree scrolling under it.

**Where it differs, and why:**

- **Folders with something chosen come first**, then the rest, each by name.
  The design's order is an example and says no rule.
- **A folder with a dash opens by itself** the first time the page is shown,
  so that what is chosen deep down shows; opening and closing is the
  person's after that, kept while the page is.
- **The page lost** *On saqib* (how much the computer has), *Deleted on this
  phone*, and *Back up now*, which the design does not have: Backups' card
  shows the progress, and ticking a folder starts a run at once.
- **Nothing chosen** shows an empty box and *Nothing chosen* in the bar,
  which the design does not draw.

**Checked** on the emulator (`qurb-34g`, the debug build) with a folder tree
made on its storage after the design's second screen: the tree as drawn,
light and dark; *Download* ticked, *ADM* unticked inside it, *2025* ticked
again inside that, *DCIM* ticked -- the marks as drawn, *3.6 MB chosen*; the
run on the phone queuing five files for the computer, *old*'s left out; the
bar gone back on Backups. On the S23 since 11:34 the same day, the laptop
on the same commit, and a backup of its chosen folders carried through
(decision 0063, *On the S23 and the laptop*). **Not checked:** the page on
the S23 by anyone but the owner; *Nothing chosen* and the empty states;
many folders.

## The owner's list of 2026-10-25

> i dont like the waiting screen, it only says folder instead it should be
> informing exactly what is backing up, there is no progress bar, no file
> size, no notifixation

1. **Say exactly what is backing up.** Home's *Waiting* said *Folders, 18
   files*.
2. **A progress bar.** There was one, measured against the whole backup --
   on the S23, 38,057 of 38,075 files -- so it was always full.
3. **File sizes.**
4. **A notification.** A backup had none unless it was over 32 MB, and then
   it said *Sending*; a folder backup had none at all (decision 0063,
   amended).

Built straight away, with no Figma draft (below).

## Built: Android, a backup on its way (2026-10-25)

The owner's list of 2026-10-25, all four (`BackingUp.kt`, `HomeScreen.kt`,
`Transfers.kt`):

- **Home's *Waiting***, while a backup is going: *Backing up to* the
  computer keeping it; under it the file going now and how many more -- the
  file named from the last piece this phone handed over, which a computer
  takes in turn; a bar, and *X of Y* under it; then the files themselves,
  the first three by folder and name, each its folder and size, and *N more*
  with their size. Drawn again every two seconds while Home is on screen and
  the backup has moved.
- **The bar is this round's**: from the first file waiting to the last one
  the computer has. Files added while it runs make the round larger rather
  than move the bar back; between the computer's word on each file, the
  bytes handed over move it, so a long video does not hold it still.
- **The notice**, during any pass a backup goes in, short or long: *Backing
  up to* the computer, the file going and how many more, *X of Y*, the bar;
  then *Backed up to* it, how many files and how much, quiet, in a place of
  its own so that it stays when a long pass's notice goes.
- **The computer says *got it* for each large file as it arrives**, rather
  than every 32 or at the end, so that *N more* counts down: thirty videos
  said *29 more* for three minutes. Small files still go in batches, as for
  WhatsApp's tens of thousands. A computer-side change, `crates/peer`.

**Where it differs, and why:** no Figma draft: the owner asked for it
directly, and it is built from the parts the redesign already has (Home's
queue rows, the transfers' notice). The rows gained a second line and a line
under the bar.

**Checked** on the emulator (`qurb-34g`, the debug build), the command line
built for Android as its computer keeping the Vault, 30 videos of 20 MB in a
camera folder: the notice, read every 15 seconds, from *VID_20261025_02.mp4
and 28 more*, *40.0 MB of 600.0 MB* at 6%, to *VID_20261025_30.mp4*, *600.0
MB of 600.0 MB* at 100% in three minutes, then *Backed up to a device, 30
files, 600.0 MB*; Home mid-way, *Waiting 29*, the row and the files as above;
then *All synced*. Before the *got it* change the same run said *29 more*
throughout, and before the notice took the computer keeping the Vault it said
only *Backing up*. **Not checked:** on the S23 and the laptop; WhatsApp's
backup going this way; a round that grows mid-way; dark.

## The owner's list of 2026-10-26

His words, numbered as he gave them; the second 19 is counted as 20 here.

1. "in home section, lets remove the sync button and only use a big send
   button only"
2. "in home section, remove the qurb logo in the top left and remove the
   recent button and backup button on the top right"
3. "in home section, rename the app from qurb to QurbSpace in the top left"
4. "in home section, remove the selected device button below the big sync
   button in the centre"
5. "in home section, i want the circular icon in the top right which can be
   either pressed and hold to trigger the same action which happens after
   clicking the select device button below the big sync button in current
   design or the circular icon can be clicked to open the page like in device
   section of the page(just remove that send button and remove button should
   be in red)"
6. "i want a blue dot attached to circular icon to signify online and red
   dot to signify offline"
7. "the circular icon will either showcase the initial letter of the
   selected device name or the pfp of the selected device and i want an
   ability to change name and pfp of the device in settings"
8. "activity log must be shifted to settings, also there is some issue with
   activity log that it shows 1file 3 times, make it crisp and accurate and
   we want to only store the activity log upto last 30days"
9. "in files section, remove the recently deleted folder inside this phone"
10. "in files section, remove the shared section"
11. "in files section, there should be only device section and upon clicking
    the specific device, you should get the option to either open shared
    files or private file which is only accessible by the phone"
12. "in files, rename the devices to remote access and use the blue and red
    dot to signify online and offline"
13. "upon remote accessing in private folder through phone, it is still not
    yet designed as finalised in the figma file and since private folders can
    only be accessed by the phone and not by the laptop, all the computer
    operations like deleting a folder/file, renaming a folder/file, copying a
    folder/file, pasting, moving, etc can be performed directly by the phone"
14. "in remote access, the shared file must be the replica of the shared
    folder of the desktop, but in my desktop the shared folders nothing but
    still there are files in the shared folder of the phone"
15. "i want to remove the entire device since i dont think, it is important
    anymore cause all the operations are shifted to the circular icon in the
    home page"
16. "in settings, upon clicking on the background sync the user can select
    how often they want the background sync through a timer or they can
    choose to switch off the background at all"
17. "in settings, after clicking show the 24 words the user must have the
    option to download a text file containing the 24 words"
18. "if someone clicks remove button, it must ask for phone
    password/fingerprint/facelock"
19. "our qr code and 6 digit page is not as beautiful as designed in the
    figma file"
20. "make sure to add some beautiful animations, different icon pack and
    maintain our symmetric and beautiful design langauge throughout all the
    changes"

## Built: Android, the owner's list of 2026-10-26 (in progress)

Built straight into the app, as he chose when asked; three questions answered
first: **Phosphor** for item 20's icon pack, from four offered; **each
device's own** name and picture for item 7, set on that device and shown on
every other; and **build directly**, not Figma first. Item by item:

- **1-4, Home.** *QurbSpace* on its own at the top left; no mark beside it,
  no Backups or Activity at the right; the chip under the status mark gone;
  one Send the width of the page, 60 dp tall. The mark still syncs when
  tapped, and pulling down still syncs.
- **5-7, the circle.** The chosen device at the top right as a circle: its
  picture (none can be set yet: item 7's engine work is to come) or the first
  letter of its name, and a dot on its edge, blue when it is on and red when
  it is off -- the dot app-wide is blue and red now, as item 12 asks of
  Files. Tapped, the device's page, without Send, its Remove in red; held,
  the list of devices to choose from, with *Add a device*. The list and the
  page show each device as its circle too.
- **8, Activity.** In Settings, under App; each file told once and 30 days
  kept (`db::told_once`, `forget_old_activity`), on the desktop and the
  command line as well.
- **9-12, Files.** *This phone* is Vault and Received; Deleted is in
  Settings, and the Shared section is gone. *Remote access* lists each
  computer as the rectangular card it was -- not a circle, which the owner
  asked for when he saw the first build -- its dot blue on and red off;
  tapped, a sheet of two, *Shared* and *Private*, Private being this phone's
  own folder there. A computer this phone only visits opens to Shared.
- **13, Private.** In the Private Vault part, a file's sheet has *Copy* as
  well as Rename, Move to folder and Delete; a folder, held, has Rename,
  Copy, Move to folder and Delete, each for everything in it; anything
  copied gives every folder a *Paste* row at the top, with a cross to forget
  it. Each is this phone's own vault changing, the computer following
  (`Store::copy_in_vault`, `Store::move_in_vault`, `delete_kept` on a folder;
  a backup's files are not copied or moved, decision 0073). A copy of a file
  freed to the computer points at the same chunks: no byte crosses. **Not
  built: a new empty folder**, which the index cannot hold; moving or pasting
  into a new name makes one.
- **14, Shared.** A phone sees the files in the computer's folder on disk,
  not every row its index has (`Db::shared_entries_here`).
- **15, the Devices tab, gone.** Three tabs. What it held is the circle's:
  the list and *Add a device* on holding it, a device's page on tapping it;
  where the app sent someone to Devices it opens the chosen device's page.
- **18, Remove** asks for the phone's fingerprint, face or screen lock
  (`ScreenLock`) after its sheet; a phone with no screen lock has the sheet
  alone.
- **20, Phosphor.** Every icon the app draws, by the names it had
  (`scripts/android-icons.py`): regular, light for the large marks, filled
  for the open tab.

**Checked** on the emulator: Home, the list on holding the circle, a
device's page on tapping it, each with the circle and its red dot; Files
with Remote access and its sheet; against the command line built for
Android as the computer, in Private: *notes2.txt* moved into a new *Work*,
*notes1.txt* copied and pasted into it, *Work* renamed *Projects* with both
inside, and *Projects* deleted, both in the phone's Recently deleted.
**Not yet:** items 7 (pictures and names between devices), 16, 17 and 19,
and item 20's animations.
