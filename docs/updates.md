# Updates, during the beta

Installed QurbSpace apps update themselves from the latest release on
GitHub ([decision 0078](decisions/0078-the-beta-updates-itself.md)).

**Desktop (Linux):** install the **AppImage** from the latest release — make
it executable and run it, or put it where you keep programs. It looks for a
new version a minute after starting and every six hours, puts it in place,
and *Restart to update* appears at the foot of the sidebar; the next login
starts the new one anyway. A .deb install does not update itself.

**Desktop (Windows):** the installer from the latest release. A new version
is fetched in the background; *Install update* in the sidebar installs it.
Not yet run on Windows.

**Phone:** the APK from the latest release. The first time an update is
ready, Android asks to allow QurbSpace to install apps — allow it once; after
that, updates install by themselves while the app is closed. Settings →
Version says where it stands, and a tap looks now.

## Making a release

```bash
git tag v0.2.0-beta.1
git push origin v0.2.0-beta.1
```

`.github/workflows/release.yml` builds everything, signs it, and publishes it
as the latest release; every installed app picks it up within six hours.
Bump the beta number for each release; a version never goes backwards.

The workflow needs these repository secrets (Settings → Secrets and
variables → Actions): `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the updater key, in
`~/.config/qurb/updater.key` and `updater.password`); `ANDROID_KEYSTORE_BASE64`,
`ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_PASSWORD` (the release key);
optionally `GOOGLE_SERVICES_JSON_BASE64` for waking phones.

**Keep both keys backed up somewhere other than the laptop.** Losing the
updater key means no installed desktop app accepts another update; losing
the release key means every phone has to uninstall to move on, losing its
app data.
