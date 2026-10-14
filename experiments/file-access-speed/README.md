# File access speed

Throwaway. Is reading a phone's folder with Android's "All files access"
(`MANAGE_EXTERNAL_STORAGE`) much faster than reading it through a folder the
person picked, which is how Qurb reads WhatsApp's folder now
([decision 0062](../../docs/decisions/0062-backing-up-whatsapp.md))? Asked on
2026-10-14. Two of the owner's numbered items hang on it: making sync faster
(item 1), and backing up other folders chosen on the desktop (item 3), which a
per-folder pick on the phone cannot do.

## What it is

One Android activity, `src/main/java/app/qurb/accessspeed/Spike.java`, in Java
with no libraries. *Pick a folder* opens the system picker at WhatsApp's folder
and keeps the grant; *All files access* opens the system setting; *Measure*:

1. lists the folder both ways and times each: through the picked folder
   (`DocumentsContract`), and directly (`File.listFiles`), skipping names
   starting with a dot as Qurb does;
2. takes the first 2,000 files of the direct listing, splits them into two
   interleaved halves, and reads every byte of one half through the picked
   folder (`ContentResolver.openFileDescriptor`) and of the other directly
   by path, four threads at a time as Qurb does.

Interleaving gives both ways files of the same kind without one warming the
cache for the other. `swap` gives each way the other half, to see that the
result is not the halves' sizes. Results go to the screen and to logcat under
`access-speed`. From ADB, with the folder picked once:

```bash
cd experiments/file-access-speed
echo "sdk.dir=$HOME/Android/Sdk" > local.properties
JAVA_HOME=~/Android/jdk-17* $(find ~/.gradle/wrapper/dists -name gradle -path '*/bin/*' | head -1) --offline assembleDebug
adb install -r build/outputs/apk/debug/file-access-speed-debug.apk
adb shell appops set app.qurb.accessspeed MANAGE_EXTERNAL_STORAGE allow
adb shell am start -n app.qurb.accessspeed/.Spike --es run measure --ei files 2000 [--ez swap true]
adb logcat -s access-speed
```

## Results

**Emulator, 2026-10-14** (`qurb-test`, Android 16, x86_64, on this laptop),
against the 304-file stand-in WhatsApp folder from 0062. Warm: these files had
been read before.

| | through the picked folder | directly by path |
|---|---|---|
| listing 304 files | 231 ms | 103 ms |
| reading, halves as split | 7.49 ms a file (152 files, 27.0 MB) | 0.34 ms a file (152 files, 11.0 MB) |
| reading, halves swapped | 6.37 ms a file (11.0 MB) | 0.42 ms a file (27.0 MB) |
| reading, as split again | 6.49 ms a file (27.0 MB) | 0.27 ms a file (11.0 MB) |

About 6.4 ms a file through the picked folder whatever the files' size, and
0.3 to 0.4 directly: 15 to 24 times less a file. With "All files access" the
app reads `Android/media/com.whatsapp` directly, which the 0062 spike's
manifest called impossible; it is, without that permission.

**Galaxy S23, 2026-10-14** (Android 16, One UI 8.5, on battery at 72%, the
screen on and nothing else running), against WhatsApp's real folder, 36,055
files. The first 2,000 files of the listing, about 1.2 MB each, last read
the evening before for the WhatsApp backup:

| | through the picked folder | directly by path |
|---|---|---|
| listing 36,055 files | 57.5 s, then 39.6 s | **5.3 s, then 4.3 s** |
| reading, halves as split | 5.85 ms a file, 196 MB/s (1,000 files, 1.15 GB) | **3.03 ms a file**, 435 MB/s (1,000 files, 1.32 GB) |
| reading, halves swapped | 5.75 ms a file, 229 MB/s (1.32 GB) | **2.58 ms a file**, 444 MB/s (1.15 GB) |

Listing is about ten times faster directly, and reading about twice as fast
a file, the same in both rounds. Not the twentyfold of the emulator, whose
files were small and warm. Both read far faster than Qurb's backup did
(23 MB/s, 0062): reading the bytes is not where that time goes. The spike
was uninstalled afterwards, taking its permission and grant with it.

## Corners cut

- Reads only: no hashing, no index, nothing written, so the numbers are the
  cost of getting the bytes, not of a backup.
- The sample is the first files of the listing, not a random one.
- Logs only failures' count, not which.
