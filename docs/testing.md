# Testing the whole product

What has been tested end to end, how, under what conditions, and what it
found. Newest first. Each run says what it could not reach and why: a
function not listed under *Tested* has not been.

## 2026-11-09: every function, from start to end

Asked by the owner: "lets check if the entire product is working as
intended ... testing each and every function of the application from start to
end". Four passes, the first three on the laptop alone, the fourth with the
owner's Galaxy S23.

**Conditions.** The owner's laptop (Ryzen, CachyOS, Linux 7.2), release
builds of `qurb`, `qurb-desktop` and the APK at commit `628b20d` and the few
before it. The desktop window driven through WebKit's WebDriver on GTK's
Broadway backend, in a home, config, runtime directory and display of its own,
so that nothing reached the owner's `~/qurb` or running QurbSpace. The phone
app on the Android 14 x86_64 emulator `qurb-34g` (Google APIs, no Google
account), 0.2.0-beta.8.

### Pass 1: the automated suites

| suite | how | result |
|---|---|---|
| every crate but the desktop's | `cargo test --workspace --exclude qurb-desktop` | 962 passed, 0 failed, 5 ignored |
| the desktop's | `cargo test -p qurb-desktop` | 31 passed (27 and 4), 0 failed |
| the engine on Android | `scripts/android-test.sh x86_64`, 86 test binaries on the emulator | 866 passed, 0 failed, 3 ignored |
| the real window, run through | `scripts/desktop-smoke.sh` | passes |
| the real window, set up by joining | `SMOKE_MODE=join` | passes |
| the real window following the desktop's theme | `SMOKE_MODE=theme` | passes |

The smoke test first failed: see *Found and fixed*, 1. It also needed
bringing up to the window as it is now (the theme sheet's pills, the asking
sheet's six boxes, Waiting's rows by device, the device page's Remove, the
synced folder at `~/qurb/Shared` since decision 0083), and its cleanup could
not remove the read-only person folder it leaves. `android-test.sh` filled the
emulator's disk with cargo's hash-named copies of its helpers; it now pushes
only the helpers the tests run.

### Pass 2: every function of the window that needs no phone

`SMOKE_MODE=full ./scripts/desktop-smoke.sh` (`scripts/desktop_full.py`):
the real window against a second device, the command line, that runs and
syncs, both on a meeting point of the test's own. Twelve steps, all passing,
no command failing:

1. Set up with Start; renamed; notifications off and on; start at login off
   and on (the autostart file written); a picture set and removed; the space
   limit set and read back by Storage, a typed size read, nonsense refused;
   the 24 words shown.
2. A second device paired by its six digits, approved in the window, run,
   and seen online.
3. A file made here reaches the other device; one made there arrives here;
   its details name the other device as holding it.
4. A local copy freed (*Available elsewhere*), and fetched back.
5. A file deleted: gone here and there, in Recently deleted (drawn as the
   grid), restored on both; deleted again and deleted for good; another
   deleted and Recently deleted emptied.
6. A file moved into the Vault leaves the other device; moved out, it
   returns.
7. The same file changed on both while apart: two versions found, drawn,
   and settled by keeping both.
8. A folder shared with this computer only: a new file in it does not reach
   the other device; shared with everyone again, it does. A folder kept
   remotely, its file freed; kept here again, fetched.
9. A file sent to the running device and collected; sending it again is
   said to have gone before; a send to the device while off waits, shows
   under Waiting and on its page, and is cancelled; a file sent from the
   other device arrives and is listed in Received; *Show* and *Open folder*
   called.
10. Activity, Home's summary, Sync now, Storage and its parts, the update
    state, a preview.
11. Joining a device of the same person by entering its code; *the same
    device* and *not the same*; a device removed, remembered as removed, and
    what it kept deleted.
12. Quit; a fresh folder set up with the 24 words.

Commands not reached by either window test, and why: those that need a phone
(a phone's folders and choosing them, its backed-up previews, people's
folders asked for, opened, locked and their files, a phone's Recently
deleted, previews of what a phone keeps); *update now* and *apply update*,
which download and install; and the ones that hand a file or folder to
another program (open a file, show it, show the folder, open a place), whose
programs are not there to look at.

### Pass 3: the phone app on the emulator

**Tested, alone:** setting up with *Start on this phone*, the notification
permission asked; Home with no device; *Add a device* and its four ways;
*Show my code* (the QR, the six digits, the countdown, *Copy code*, *Stop*);
Settings; Theme, Dark and Auto applied; Activity and its filters; Background
sync; Advanced (identity, key, meeting point, relay, version); Devices;
Recently deleted; Files; the scanner with the camera refused (*Camera not
allowed*, *Type 6 digits*) and allowed.

**Not tested here: the phone with a computer.** A phone pairs with a computer
by scanning its code, or by six digits on the same Wi-Fi. Neither can be done
on this emulator:

- The scanner's barcode reader is Google Play services', fetched on first
  use; this emulator has no Google account, Play services never delivers it,
  and no frame can be read. A test build with the reader bundled was not made:
  it would mean downloading a library, and testing a build that does not ship.
- The six digits find the computer by a multicast on the local network, which
  the emulator's own network address translation does not carry.

So pairing, syncing, sending, backups, people's folders, the Vault kept on
the computer, removing and adding again, between the phone app and a
computer, are pass 4's.

### Pass 4: the Galaxy S23 and the laptop

With the owner, 2026-11-09 to 2026-11-10. The laptop on the day's build
(`target/release`, replacing the installed AppImage with the owner's yes);
the S23 on 0.2.0-beta.8 builds, installed over wireless debugging with its
data kept. Background sync on the S23 is set to *Off*. The phone's side was
driven over adb where the owner asked, and stopped whenever he was using
the phone.

1. **Home on both** -- each sees the other. Passed; found *Found and fixed*,
   4.
2. **The laptop sends to the phone** -- `qurb send` from the laptop. The
   first try was misread: the laptop's *sent* means queued (*collected* is
   the event for a send taken), and it was never collected -- the open app
   did not sync (*Found and fixed*, 6), and the file, in a temporary folder,
   went with the reboot; the phone's Activity then showed the raw error of
   *Found and fixed*, 9. Done again on 2026-11-10: collected twenty seconds
   after it was sent, nothing tapped; in Received; its text as sent. Passed.
3. **The phone sends to the laptop** -- a photo, in the laptop's
   Downloads/qurb at 18:08. Passed.
4. **A phone folder backed up** -- a screenshot taken: not read until the
   night's run, by design (decision 0063), and with no way to start one by
   hand (*Found and fixed*, 8).
5. **The owner's folder opened on the laptop** -- asked, approved on the
   phone, opened. Found *Found and fixed*, 5 to 7 and 10 to 11.
6. **The space limit on the phone** -- *28.1 GB used, 10.9 GB free of 39.0
   GB*, the laptop's own figures. Passed.
7. **Recently deleted on the phone** -- a received file deleted, shown in
   Recently deleted, restored alone, back in Received. Passed.
8. **The scanner on the S23** -- *Point at the code*, the torch offered: the
   reader is there. Passed.
9. **The phone's Activity** -- the day's sends, the arrival, the deletion,
   the earlier failure. Passed.

**2026-11-12, after the fixes.** The S23 on the build of 2026-11-12 (with
*Back up now*), installed with its data kept; the laptop on `7b4e553`'s
build. The owner asked for *sakib* in the laptop's Files with QurbSpace open
on the phone, and approved; the folder was mounted 31 s after the phone app
was opened. Read through the mount from the laptop with Python's `os.walk`,
warm page cache, everything kept locally: 74,745 entries in 2.90 s the first
walk and 2.00 s the second; the folder's top level (*Android*, *DCIM*,
*Download*, *Movies*, *Music*, *Pictures*, *Private Vault*, *WhatsApp*,
*log*) in 0.1 ms. No figure was taken before the fix to compare against.

Not seen yet: the fix of *Found and fixed*, 4, live, which needs WhatsApp to
rewrite its backups after a night's run.

### Found and fixed

1. **No computer could be set up through the window** (`e9d582d`). Since
   the two-screen setup of 2026-11-03, *Start* sent no limit, zero, and the
   setup commands still refused it: "choose how much space qurb may use".
   The same for joining by a code or by the 24 words. Zero is no limit, as
   the config file has it. Found by the smoke test's first step.
2. **The window counted sizes in 1000s, the phone in 1024s** (`5b89e56`).
   The same file showed two sizes, and a space limit typed as "300 GB" read
   back as "322 GB". The window now counts as the phone, the config file and
   `qurb` do; the limit sheet's sizes are whole gigabytes in that unit.
   Speeds stay in 1000s, as they were saved. Found by pass 2's limit check.
3. **The phone's scanner said "Point at the code" to a code it could not
   read** (`628b20d`). Its reader comes from Play services on first use; until
   it does, every frame fails, and the failures went unheard. It now asks for
   the reader at once and says *Getting the scanner ready*, then *Point at
   the code* once a frame is read, or to type the six digits if it cannot be
   fetched. Found on the emulator. On the owner's S23, which has scanned
   before, the reader is there already.

4. **A WhatsApp backup rewritten after the night's run waited a day**
   (`0519dfe`). On the S23, Home said *Backing up 4 files, 99%* while the
   laptop said everything was synced: WhatsApp rewrote its four chat backups
   at 08:54, after the night's listing; the phone still listed their old
   contents, could not give them, and the laptop asked again every pass. The
   folder is listed only at night while charging. Now a kept file found
   changed when asked for is marked, read again by the next listing whatever
   its size and time, and the app lists the folders at once on Wi-Fi after
   any sync that finds one. The day's four were cleared by forcing the
   night's run from adb; the fix itself is shown by a test, not yet seen on
   the phone.

5. **The owner's folder was named after the laptop's login** (`ebc1076`),
   "Saqib" with a letter, while the phone calls itself "sakib" and has a
   picture. Then, at the owner's word (`999486a`), `~/qurb` holds Shared and
   a folder for each phone, named as the phone is, its picture on its tile,
   its backups at its top; *This computer* (the laptop's own vault) no longer
   shown, its files left in the vault.
6. **The phone heard the laptop's ask only when the circle was tapped**
   (`ebc1076`): with background sync off nothing ran a pass while the app
   was open. Now a pass runs as it opens and every half minute while on
   screen; the send of pass 4, 2, was collected that way.
7. **A person's folder opened a level above the phone's** (`26aeef3`), then
   made unnecessary by 5.
8. **No way to start a backup by hand** (`7b4e553`): *Back up now* is on the
   phone's device page again, with when the last run was.
9. **A file gone from the other device read as a hash** (`7b4e553`): now
   *No longer on the other device*.
10. **A folder left mounted by a QurbSpace that stopped** failed every look,
    "Transport endpoint is not connected" (`999486a`); cleared when the
    folders are laid out.
11. **Opening the phone's folder was very slow** (`1e1af78`): each look the
    file manager made read everything kept for the phone again. Read once
    into a tree, kept for thirty seconds.

### Found, and left

- **The installed app cannot open the folder.** The AppImage updated itself
  to 0.2.0-beta.5, the latest published, which knows the index only to
  version 28; the builds since 2026-11-03 moved it to 32. At login it fails
  ("could not start: opening ~/qurb/Shared"). A release from the current code
  ends it; publishing one is the owner's.
- **One yes opens all the owner's phones' folders.** Each phone opening only
  its own needs the ask to name the phone, on both sides.

- **Scanning needs Google Play services.** A phone without them (de-Googled,
  some Huawei) can pair only by the six digits, on the same Wi-Fi. Bundling
  the reader would fix it at the cost of the APK's size (decision 0039 left
  the scanner's weight unmeasured): the owner's call.
- **A camera refused once cannot be allowed from the scanner.** It says
  *Camera not allowed* and offers the six digits; allowing it means the
  system's settings.
- The test harnesses themselves: the window tests read the window's state
  rather than looking at it (Broadway's screenshots are not worth reading);
  how every screen looks was checked separately, in the fixtures, light and
  dark (docs/design/redesign.md).
