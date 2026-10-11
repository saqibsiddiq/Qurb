# WhatsApp access

Throwaway. Can an app reach WhatsApp's folder on a phone well enough to back
it up, and to put a backup back? Asked on 2026-10-11, when the owner wanted
WhatsApp backed up somewhere other than his Google account, whose free 15 GB
WhatsApp's backups have counted against since December 2023. What came of it
is [decision 0062](../../docs/decisions/0062-backing-up-whatsapp.md).

## What it is

One Android activity, `src/main/java/app/qurb/waspike/Spike.java`, in Java
with no libraries and no storage permission. Five buttons:

1. **Pick WhatsApp's folder**: the system's folder picker, opened at
   `Android/media/com.whatsapp/WhatsApp`, and the grant kept
   (`takePersistableUriPermission`).
2. **Read it**: walk the whole tree through that grant, count files and
   bytes under each top folder, list `Databases` and `Backups` by name, and
   read the newest chat backup through, hashing it.
3. **Write test**: make a file in the folder, read it back, delete it.
4. **Pick Android/media**: for a phone WhatsApp has not run on yet.
5. **Make `com.whatsapp/WhatsApp/Databases`** inside it.

Everything goes to the screen and to logcat under `wa-spike`.

```bash
cd experiments/whatsapp-access
echo "sdk.dir=$HOME/Android/Sdk" > local.properties
JAVA_HOME=~/Android/jdk-17* $(find ~/.gradle/wrapper/dists -name gradle -path '*/bin/*' | head -1) --offline assembleDebug
adb install -r build/outputs/apk/debug/whatsapp-access-debug.apk
adb logcat -s wa-spike
```

## Corners cut

- One screen, Java, every walk on a bare thread, no error handling past
  printing it. Nothing here is meant to be kept.
- "Newest backup" is by modification time, so it found a 110 KB increment
  rather than the 498 MB full backup; read speed on a large file through the
  grant was measured on the emulator only.
- It takes write permission when it picks, which it uses only for button 3
  and 5. On the owner's phone neither was pressed.

## What it found, 2026-10-11

**Emulator** (`qurb-test`, Android 14, x86_64), against a stand-in folder
made with `adb`: a 30 MB and a 28.6 MB `msgstore` file, 1,900 small images
and 100 voice notes.

| | |
|---|---|
| picker | opens at `Android › media › com.whatsapp › WhatsApp`, *Use this folder* offered |
| grant | kept across a restart of the app |
| walk | 2,003 files, 62.7 MB, 9 folder queries, 1.1 s |
| read | the 28.6 MB file at 211 MB/s |
| write | a file made, read back the same, deleted |
| a phone with no WhatsApp folder | `Android/media` itself can be picked, and `com.whatsapp/WhatsApp/Databases` made in it |

**The owner's Galaxy S23** (SM-S911B, Android 16, One UI 8.5), his real
WhatsApp, read only:

| | |
|---|---|
| picker | the same: opens at WhatsApp's folder, *Use this folder*, then *Allow access for 'WhatsApp'* |
| the folder | 37,722 files, 9.65 GB |
| chat backups (`Databases`) | 9 files, 499 MB: a full `msgstore.db.crypt14` of 498 MB written 2026-10-05, and daily `msgstore-increment-N.db.crypt14` of 58 to 154 KB |
| other backups (`Backups`) | 157 files, 12.5 MB: settings, stickers, wallpapers, `wa.db` |
| media | images 3.93 GB in 27,823 files, video 2.44 GB in 504, documents 2.23 GB in 2,002, stickers 373 MB in 3,750, voice notes 36.5 MB in 1,452, and caches (`.Statuses`, `.Links`, `.Shared`, …) |
| walk | 209 folder queries, **53.7 s** |
| encryption | `.crypt14` throughout: *end-to-end encrypted backup* is off (it writes `.crypt15`) |

The spike was uninstalled from the phone afterwards, so nothing kept its
grant.

Two things this changes:

- **WhatsApp backs up incrementally now.** A full chat backup about weekly,
  and an increment of around 100 KB each day. Once a computer holds the full
  one, a day's chat backup costs about 100 KB to carry.
- **Walking the whole folder takes nearly a minute** on the phone, almost all
  of it the 27,823-file images folder. That suits once a day, not every
  sync.

## Not answered

- **Whether WhatsApp restores from a backup put back this way.** The chat
  backup is encrypted, and reports differ on whether a new phone can open a
  `.crypt14` one copied into place (the key may come from WhatsApp's servers
  after the number is verified, or may stay on the old phone). Testing it
  needs WhatsApp registered on the emulator -- whose official APK carries
  x86_64 code and starts there -- with a number that is not anybody's active
  WhatsApp. There is none yet.
- **Writing into WhatsApp's folder on a Samsung phone.** Shown on the
  emulator; not tried on the S23.
