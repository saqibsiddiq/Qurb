# 0078 — The beta updates itself, wherever it is

**Status:** Accepted — built 2026-11-01. Supersedes the "no automatic
updater" part of [0047](0047-versions-and-upgrades.md); its versions, index
migrations and refusals stand. **Not yet run end to end:** the first release
needs the keys added to the repository's secrets, and the owner's S23 runs a
build signed with the debug key, which no release can update.
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

- **The first release** waits for the secrets.
- **Windows** builds in CI; it has never run on Windows.
- **macOS** is not built.
- **A phone on the debug key** — the owner's S23 — has to move to a
  release-signed build once, by reinstalling, which clears the app's data.
  Its files stay on the laptop.
- **Rolling back** an update is by hand: install the earlier release.
