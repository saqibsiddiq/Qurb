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

Under way, with the owner. The laptop on the day's build (`target/release`,
replacing the installed AppImage of 2026-11-01 with the owner's yes); the S23
on 0.2.0-beta.8, installed over wireless debugging with its data kept.
Background sync on the S23 is set to *Off*, so it syncs when asked or when
it notices a change, and only while QurbSpace is on screen.

1. **Home on both** -- each sees the other. Passed, and found *Found and
   fixed*, 4: four WhatsApp backups waiting at 99%.
2. **The laptop sends to the phone** -- `qurb send` from the laptop's folder;
   collected by the phone while QurbSpace was open (the laptop's activity
   says *sent*). Passed.
3. to 9. **Not run yet** (paused at 23:00 for the owner): the phone sending
   to the laptop; a phone folder backed up after choosing it on the laptop;
   the owner's folder asked for and opened on the laptop; the space limit
   seen on the phone; the phone's Recently deleted; the scanner; the phone's
   Activity.

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

### Found, and left

- **Scanning needs Google Play services.** A phone without them (de-Googled,
  some Huawei) can pair only by the six digits, on the same Wi-Fi. Bundling
  the reader would fix it at the cost of the APK's size (decision 0039 left
  the scanner's weight unmeasured): the owner's call.
- **There is no way to start a backup by hand.** *Back up now* is on the
  phone's Backups screen, which nothing opens any more; WhatsApp's folder is
  listed only at night, while charging, on Wi-Fi.
- **With background sync off, opening the app does not sync.** The phone
  syncs when the circle is tapped, when pulled down, or after a change of
  its own; a send waiting on the laptop is collected only then.
- **A camera refused once cannot be allowed from the scanner.** It says
  *Camera not allowed* and offers the six digits; allowing it means the
  system's settings.
- The test harnesses themselves: the window tests read the window's state
  rather than looking at it (Broadway's screenshots are not worth reading);
  how every screen looks was checked separately, in the fixtures, light and
  dark (docs/design/redesign.md).
