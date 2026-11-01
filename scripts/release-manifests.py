#!/usr/bin/env python3
"""Write the two files installed apps read to find a release (decision 0078).

    scripts/release-manifests.py 0.2.0-beta.1 assets https://github.com/<repo>/releases/download/v0.2.0-beta.1

Into `assets`, beside the builds it describes:

- `latest.json`, for the desktop's updater (Tauri's format): each platform's
  build and its signature, made with the updater key when it was built;
- `android.json`, for the phone's: the APK, its version code, SHA-256 and
  size. Android itself refuses an APK signed with another key; the checksum
  catches one damaged on its way.

Run by .github/workflows/release.yml; runnable here on a folder of builds.
"""

import datetime
import hashlib
import json
import sys
from pathlib import Path


def version_code(name: str) -> int:
    """As android/app/build.gradle.kts counts it: 0.2.0-beta.1 is 20001."""
    core, _, beta = name.partition("-beta.")
    major, minor, patch = (int(x) for x in core.split("."))
    if major == 0 and minor <= 1 and not beta:
        return 1
    return ((major * 100 + minor) * 100 + patch) * 100 + (int(beta) if beta else 99)


def main() -> None:
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    version, folder, base = sys.argv[1], Path(sys.argv[2]), sys.argv[3].rstrip("/")

    platforms = {}
    for platform, pattern in (("linux-x86_64", "*.AppImage"), ("windows-x86_64", "*-setup.exe")):
        for build in folder.glob(pattern):
            sig = build.with_name(build.name + ".sig")
            if sig.exists():
                platforms[platform] = {"signature": sig.read_text().strip(), "url": f"{base}/{build.name}"}
    if platforms:
        (folder / "latest.json").write_text(json.dumps({
            "version": version,
            "notes": f"QurbSpace {version}",
            "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
            "platforms": platforms,
        }, indent=2) + "\n")

    apks = sorted(folder.glob("*-android.apk"))
    if apks:
        apk = apks[0]
        data = apk.read_bytes()
        (folder / "android.json").write_text(json.dumps({
            "versionCode": version_code(version),
            "versionName": version,
            "url": f"{base}/{apk.name}",
            "sha256": hashlib.sha256(data).hexdigest(),
            "size": len(data),
        }, indent=2) + "\n")

    print("platforms:", ", ".join(platforms) or "none", "| android:", "yes" if apks else "no")


if __name__ == "__main__":
    main()
