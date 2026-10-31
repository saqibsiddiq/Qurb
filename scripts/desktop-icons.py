#!/usr/bin/env python3
"""Write crates/desktop/ui/icons.js from the Android app's icons.

The window draws the phone's icons (Phosphor, written into the
app's drawables by scripts/android-icons.py), so the two show the same
marks. Each drawable's paths are read as they are, in their 256 box; only
the icons named below are written.

    ./scripts/desktop-icons.py
"""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DRAWABLES = ROOT / "android/app/src/main/res/drawable"
OUT = ROOT / "crates/desktop/ui/icons.js"

# Every icon the window draws, by the app's names (dashes for underscores).
ICONS = """
    house house-fill folder folder-fill folder-open folder-input folder-plus
    monitor-smartphone settings settings-fill send check circle-check plus
    clock laptop smartphone lock-keyhole arrow-up arrow-down arrow-up-down
    search image file file-image file-video file-text file-audio
    file-archive file-code trash-2 history users user-circle cloud cloud-off
    shield-check download upload x chevron-right chevron-down arrow-left
    refresh-cw rotate-ccw circle-alert triangle-alert hard-drive whatsapp
    ellipsis qr-code keyboard wifi bell palette sliders-horizontal key-round
    info external-link git-compare camera copy pencil link save
""".split()


def main() -> None:
    lines = []
    for name in ICONS:
        xml = (DRAWABLES / f"ic_{name.replace('-', '_')}.xml").read_text()
        paths = re.findall(r'android:pathData="([^"]+)"', xml)
        lines.append(f'  "{name}": [{", ".join(f"{p!r}".replace("\'", chr(34)) for p in paths)}],')
    OUT.write_text(
        "// The icons, from Phosphor (MIT), as the Android app draws them: written\n"
        "// by scripts/desktop-icons.py from its drawables, so the two apps\n"
        "// show the same marks. Each is a list of paths in a 256 box. Do not edit;\n"
        "// add the name to that script's list and run it again.\n"
        "const ICONS = {\n" + "\n".join(lines) + "\n};\n"
    )
    print(f"wrote {OUT} with {len(ICONS)} icons")


if __name__ == "__main__":
    main()
