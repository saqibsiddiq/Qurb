# The redesign of 2026-10

**Status:** started 2026-10-20. Home is at draft 3; Devices is at draft 4;
Settings is at draft 2; Backups and Activity are at draft 1; Setup, Sharing into Qurb, Pairing and Notifications are at draft 1; the desktop is at draft 1; Files is at draft 3, on a simpler model ([0073](../decisions/0073-the-phone-works-with-its-computer.md)). **Building began
2026-10-24 with Android's Home, Devices, Settings, Backups, Activity and Setup** ([below](#built-android-home-2026-10-24)); everything else here is
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
   engine work.
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
   and the widening of what a guest can reach, still to be bounded (which
   guests; read only or not).
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
- **Recently deleted stays in Storage** until Files has it.
- **The shared files' two settings** (*Who has each folder*, *Keep new files
  private*) **are under Advanced**, labelled *Shared files*. The design drops
  them with the shared files, but the phone holds shared files until 0073's
  engine work, and until then they still do something.
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
- **Waiting**: *Approve on your other device*, the number large, *It shows
  this number too*, and Home's status mark at 96 dp with its ring travelling
  round a devices icon. A join that fails goes back to the step it came from.
- *Join* and *Restore* are enabled only once six digits or 24 words are in.
  Before, tapped short, they did nothing and said nothing.

**Where it differs, and why:**

- **"Your other device", not its name.** The phone learns the other device's
  name only once the join is done: the code carries no name. Saying *Approve
  on saqib* needs the engine to say the name earlier.
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

