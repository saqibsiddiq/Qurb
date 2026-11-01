# 0078 — The beta updates itself, wherever it is

**Status:** Accepted — built 2026-11-01, and the first beta,
v0.2.0-beta.1, published by the workflow the same day (its first run failed
on the phone build, fixed). Its APK checked: the SHA-256 in `android.json`,
the release key's signature, version code 20001. The owner's laptop runs its
AppImage, the S23 its APK. Supersedes the "no automatic updater" part of
[0047](0047-versions-and-upgrades.md); its versions, index migrations and
refusals stand. **The desktop's update proven** with v0.2.0-beta.2 the same
afternoon: the laptop's AppImage, restarted at 15:07:39, found it at
15:08:42, fetched and verified it, and had it in place at 15:08:56 —
byte for byte the release's — and, started again, ran as it and found
nothing newer. **The phone's proven the same afternoon**, after a gap was found
and fixed: the S23, reinstalled and not yet set up, sat on the setup screen,
where nothing looked for updates — so a phone that cannot get past setting
up would never receive the build that fixes it. The setup screen now looks
every time it shows, and the worker looks before asking whether the phone is
set up. With that build on it (0.2.0-beta.1, release key), the S23 found
beta.2 at 15:44:27, fetched and checked it, asked once to be allowed to
install, and at 15:45:40 was 0.2.0-beta.2 (version code 20002), Android
recording QurbSpace itself as its installer.

**Google Play Protect warned** before that install: its verdict
`POTENTIALLY_UNWANTED`, "This app may be harmful", threat type
`generic_malware` — a heuristic, about an app from a developer it does not
know, outside Play, able to install apps and to read all of the phone's
files: the pattern of a dropper. The owner chose to install anyway. Ways out
are in the left-undone list; disguising the app from the scan is not one.
**Date:** 2026-11-01

## What was asked

The owner, preparing the beta: "since this is a beta version, we want a way
to seamlessly update desktop and app anywhere in the world, make it happen."

## Decision

0047 listed what an updater would need before one was safe to build. Each,
and how it is met:

- **Somewhere to publish, and a signing key separate from the build
  machine.** GitHub Releases on the public repository; builds made by
  `.github/workflows/release.yml` from a version tag, with the keys as the
  repository's secrets. The desktop's builds are signed with an updater key
  (minisign, through Tauri's updater) whose public half is in
  `tauri.conf.json`: a build not signed with it is refused. The phone's APK is
  signed with the release key, and Android refuses an update signed with any
  other; the phone also checks its SHA-256 against the release's
  `android.json`.
- **A check that tells the publisher nothing about the person.** Each app
  asks for one public file — `latest.json` or `android.json` — at most every
  six hours, after a minute on the desktop. GitHub sees an address asked;
  nothing about devices or files is sent.
- **Staging, switching at a restart, a way back.** The desktop on Linux is an
  AppImage: the new one is put in place while the old runs, and takes over at
  the next start ("Restart to update" in the sidebar, or the next login). On
  Windows the update is fetched and waits for *Install update*, since
  installing closes the window. The phone installs an update from its
  background worker while the app is not on screen; Android 12 and later
  allow that without asking each time, once the person has allowed the app
  to install. The index is copied before a migration, as 0047 has it.
- **Devices updated together when the protocol changes.** Every device looks
  at the same latest release, so within hours of one all have it.

Builds are made on Ubuntu 22.04, so the Linux ones run on distributions that
old and newer — which a build on the owner's laptop, with glibc 2.44, does
not. A desktop installed some other way (a .deb, a binary placed by hand)
cannot replace itself, and says a newer version exists.

Versions: `v0.2.0-beta.1` and on; Android's version code counts them so each
installs over the last (`scripts/release-manifests.py`, `versionCodeOf` in
`android/app/build.gradle.kts`).

## Left undone

- **Windows** builds in CI; it has never run on Windows.
- **macOS** is not built.
- **A phone on the debug key** — the owner's S23 — has to move to a
  release-signed build once, by reinstalling, which clears the app's data.
  Its files stay on the laptop.
- **Rolling back** an update is by hand: install the earlier release.
- **Proven twice more on 2026-11-01 with v0.2.0-beta.3**: the laptop found
  it 62 s after a restart and had it in place 12 s later; the S23 took it at
  16:38:15, QurbSpace its recorded installer. The S23 needed a hand once:
  beta.2 was published before the setup screen looked for updates, and the
  phone sat there. From beta.3 a phone at setup updates itself.
- **The move to the release key cost the S23 its link to its own backups**,
  as staging.md foresaw: the reinstalled app called itself "SM-S911B", and
  a new signing key gave it a new `ANDROID_ID`, so neither name nor install
  tied it to "skib phone", under which its backups are kept. Linked by the
  owner's word (`qurb same "skib phone" 6385f8cc`), the chain of three
  identities is one phone again; nothing was lost.
- **Play Protect's warning** on the APK from GitHub — shown again at the
  beta.3 update: Google scans each new version from outside Play, so a
  phone's update is not yet without a tap. The way out that keeps
  updates seamless is Google Play's testing tracks: Play installs and updates
  the app, and Play's own policy then forbids the app updating itself, so a
  Play build would leave the updater out. Short of that, the APK can be
  submitted to Google for review as a false positive, and the developer
  registered under Android's developer verification.
