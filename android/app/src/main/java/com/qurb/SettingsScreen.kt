package com.qurb

import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import androidx.core.content.ContextCompat
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.DeletedFile
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.ShareTarget
import uniffi.qurb_mobile.SharedFolder
import uniffi.qurb_mobile.Usage

/**
 * Settings (direction §23): grouped lists, quieter than everything else, in
 * the direction's order -- this phone, devices, storage, privacy,
 * notifications, recovery, appearance, advanced.
 */
class SettingsScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.title.text = "Settings"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(
        val usage: Usage,
        val background: String,
        val deleted: List<DeletedFile>,
        val peers: List<PeerInfo>,
        val sentCopies: ULong,
    )

    override fun refresh() {
        scope.launch {
            try {
                val state = withContext(Dispatchers.IO) {
                    // `state` waits on WorkManager's own database; off the main
                    // thread like everything else.
                    val engine = engine()
                    State(engine.usage(), SyncWorker.state(app), engine.recentlyDeleted(), engine.peers(),
                        engine.sentCopies())
                }
                show(state)
            } catch (e: Exception) {
                app.fail("Could not read the settings", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(state: State) {
        val page = views.sections
        page.removeAllViews()

        kit.groupTitle(page, "This phone")
        var group = kit.group(page)
        kit.item(group, "Name", "${android.os.Build.MODEL ?: "Phone"} — what your other devices call it")
        // Said as it is: the Keystore key is not tied to unlocking the phone
        // (AndroidKeyStore.kt says why), so the screen lock guards the app,
        // not the key.
        kit.item(group, "Key protection",
            "In the Android Keystore: only Qurb on this phone can use it. It works while the phone " +
                "is locked, so syncing in the background can too — your screen lock keeps others out of the app.")

        kit.groupTitle(page, "Devices")
        group = kit.group(page)
        kit.item(group, "Your devices",
            if (state.peers.isEmpty()) "None yet" else state.peers.joinToString(", ") { it.name }) {
            app.go(R.id.tab_devices)
        }

        kit.groupTitle(page, "Storage")
        group = kit.group(page)
        kit.item(group, "Your files", Words.size(state.usage.logical))
        kit.item(group, "Qurb on this phone", Words.size(state.usage.onDisk))
        kit.item(group, "Who has each folder",
            "Which devices each folder is on, and whether this phone keeps it") { chooseFolder() }
        kit.item(group, "Recently deleted",
            if (state.deleted.isEmpty()) "Nothing. Files deleted on any device are kept 30 days"
            else "${Words.files(state.deleted.size)}, ${Words.size(state.deleted.sumOf { it.size })} — restorable for 30 days") {
            app.push(DeletedScreen(app))
        }
        kit.item(group, "Free unused space", "Clears what nothing needs any more", chevron = false) { tidy() }
        // Its own line, never part of the one above: the devices sent to have
        // these, but one may have lost its copy since (decision 0030).
        if (state.sentCopies > 0uL) {
            kit.item(group, "Copies of files you sent",
                "${Words.size(state.sentCopies)}, kept here after they arrived") { letGoOfSentCopies(state.sentCopies) }
        }

        kit.groupTitle(page, "Privacy")
        group = kit.group(page)
        val private = Engine.ownFilesPrivate(app)
        kit.toggle(group, "Keep new files private",
            if (private) "Files that arrive on this phone from other apps go to your Private Vault"
            else "Files that arrive on this phone from other apps go to all your devices",
            private) { setPrivate(it) }

        kit.groupTitle(page, "Notifications")
        group = kit.group(page)
        kit.item(group, "On this phone",
            "Qurb doesn't raise notifications here yet. What was sent to you is in Activity, from Home.")

        kit.groupTitle(page, "Recovery")
        group = kit.group(page)
        // Where the key is safe, said plainly (decision 0053): Block Store
        // backs it up end to end encrypted only when the phone has a screen
        // lock, and a phone without one should know its key stays here.
        val safe = kit.item(group, "Where your key is safe", "Checking…")
        scope.launch {
            safe.value.text = if (Backup.leavesThePhone(app)) {
                "On this phone, and in your Google backup, end-to-end encrypted with your screen lock. " +
                    "Any of your devices can also give it to a new one with a code."
            } else {
                "On this phone only: with no screen lock, it is not backed up. Set one, or keep your " +
                    "computer paired: it can give the key to a new phone with a code."
            }
        }
        kit.item(group, "Recovery phrase", "The 24 words that spell your key, if you want them. Nobody needs to write them down.") {
            warnThenShowPhrase()
        }

        kit.groupTitle(page, "Appearance")
        group = kit.group(page)
        kit.item(group, "Theme", "Light. A dark theme comes after this one is settled.")
        kit.item(group, "Motion", if (Kit.calm()) "Reduced, as this phone's settings ask" else "Full; follows this phone's animation settings")

        kit.groupTitle(page, "Advanced")
        group = kit.group(page)
        kit.item(group, "Background sync", state.background) { explainBackground() }
        kit.item(group, "Rendezvous service", Engine.signalUrl(app)) { editSignal() }
        kit.item(group, "Relay", Engine.relayAddress(app) ?: "None — devices must reach each other directly") { editRelay() }
        val version = runCatching {
            app.packageManager.getPackageInfo(app.packageName, 0).versionName
        }.getOrNull() ?: "unknown"
        kit.item(group, "Version", "$version\n${uniffi.qurb_mobile.engineVersion()}")
    }

    /**
     * The words again, for a new paper copy before the old one is lost.
     *
     * Not a secret kept from the person holding the phone: anyone who can
     * open this app can read every file already, so the words give away
     * nothing new (decision 0033). But they are the key, so the screen says
     * so first, and the sheet is kept out of screenshots while they are on it.
     */
    private fun warnThenShowPhrase() {
        kit.sheet()
            .header(R.drawable.ic_key_round, "Show your recovery phrase?")
            .text("Anyone who sees these 24 words can read every file you keep in Qurb, on any device. " +
                "Make sure nobody is looking.")
            .buttons("Show") {
                scope.launch {
                    try {
                        val phrase = withContext(Dispatchers.IO) { engine().recoveryPhrase() }
                        kit.sheet()
                            .header(R.drawable.ic_key_round, "Your recovery phrase", "Numbered, in order")
                            .view(Words.phraseView(app, phrase), top = 16)
                            .buttons("Hide", secondary = "Close") {}
                            .secure()
                            .show()
                    } catch (e: Exception) {
                        app.fail("Could not show the words", e)
                    }
                }
            }
            .show()
    }

    private fun setPrivate(on: Boolean) {
        scope.launch {
            try {
                Engine.setOwnFilesPrivate(app, on)
                app.say(if (on) "New files stay private" else "New files go to all your devices")
            } catch (e: Exception) {
                app.fail("Could not change that", e)
            } finally {
                refresh()
            }
        }
    }

    /**
     * Which devices a folder is on (decision 0044), and whether this phone
     * keeps it or downloads each file when opened (0045).
     */
    private fun chooseFolder() {
        scope.launch {
            val (folders, devices) = try {
                withContext(Dispatchers.IO) { engine().sharing() to engine().shareTargets() }
            } catch (e: Exception) {
                app.fail("Could not read the folders", e)
                return@launch
            }
            val sheet = kit.sheet().header(R.drawable.ic_folder, "Who has each folder",
                "A folder is on every device unless you choose")
            if (folders.isEmpty()) sheet.text("No folders yet. Folders appear here once they hold a file.")
            for (f in folders) {
                val who = if (f.everyone) "Every device" else f.members.joinToString(", ") { id ->
                    devices.find { it.id == id }?.name ?: "a removed device"
                }
                sheet.action(R.drawable.ic_folder, f.folder + "  ·  " + who +
                    if (f.remote) "  ·  downloaded when opened" else "") { folderSheet(f, devices) }
            }
            sheet.show()
        }
    }

    private fun folderSheet(folder: SharedFolder, devices: List<ShareTarget>) {
        val sheet = kit.sheet().header(R.drawable.ic_folder, folder.folder,
            "A device you leave out keeps what it has, and gets nothing new")
        val chosen = devices.associate { it.id to (folder.everyone || it.id in folder.members) }.toMutableMap()
        val box = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        val group = kit.group(box)
        for (d in devices) {
            kit.toggle(group, d.name, if (d.here) "This phone" else "", chosen[d.id] == true) { chosen[d.id] = it }
        }
        sheet.view(box, top = 16)
        sheet.action(
            if (folder.remote) R.drawable.ic_download else R.drawable.ic_cloud_off,
            if (folder.remote) "Keep this folder on this phone" else "Free local space: download files when opened",
        ) { if (folder.remote) keepHere(folder.folder) else keepRemotely(folder.folder) }
        sheet.buttons("Save") {
            val members = chosen.filterValues { it }.keys.toList()
            // All ticked is no rule at all, so a device paired later is in too.
            setSharing(folder.folder, if (members.size == devices.size) emptyList() else members)
        }
        sheet.show()
    }

    /** Decision 0045: listed here, fetched when opened, never the only copy freed. */
    private fun keepRemotely(folder: String) {
        scope.launch {
            try {
                val r = withContext(Dispatchers.IO) { engine().keepRemotely(folder) }
                val kept = if (r.kept.isEmpty()) "" else
                    " ${Words.files(r.kept.size)} stayed: this phone has the only copy."
                app.say("Freed ${Words.size(r.bytes)}. Nothing was deleted.$kept")
            } catch (e: Exception) {
                app.fail("Could not free that folder", e)
            } finally {
                app.changed()
            }
        }
    }

    private fun keepHere(folder: String) {
        scope.launch {
            try {
                val asked = withContext(Dispatchers.IO) { engine().keepLocally(folder) }
                app.say(if (asked > 0u) "${Words.files(asked.toInt())} on their way back" else "Kept here")
                SyncWorker.runNow(app)
            } catch (e: Exception) {
                app.fail("Could not change that", e)
            } finally {
                app.changed()
            }
        }
    }

    private fun setSharing(folder: String, members: List<String>) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { engine().setSharing(folder, members) }
                app.say(
                    if (members.isEmpty()) "$folder is on all your devices"
                    else "Saved. A device left out keeps what it has, and gets nothing new."
                )
                SyncWorker.runNow(app)
            } catch (e: Exception) {
                app.fail("Could not change that", e)
            }
        }
    }

    private fun tidy() {
        scope.launch {
            try {
                val tidied = withContext(Dispatchers.IO) { engine().housekeep() }
                app.say(if (tidied.freed > 0uL) "Freed ${Words.size(tidied.freed)}" else "Nothing to free")
            } catch (e: Exception) {
                app.fail("Could not free space", e)
            } finally {
                refresh()
            }
        }
    }

    /**
     * This phone's copies of files it sent that have arrived (decision 0030).
     * Kept until asked for by name, and the asking says what it can cost: on
     * 2026-10-07 the S23 held the last copy of a video the laptop had taken
     * and since lost.
     */
    private fun letGoOfSentCopies(bytes: ULong) {
        kit.sheet()
            .header(R.drawable.ic_send, "Let go of ${Words.size(bytes)}?")
            .text(SentCopies.COST)
            .buttons("Let go", danger = true, secondary = "Keep") {
                scope.launch {
                    try {
                        val freed = withContext(Dispatchers.IO) { engine().releaseSentCopies() }
                        app.say("Freed ${Words.size(freed)}")
                    } catch (e: Exception) {
                        app.fail("Could not free that", e)
                    } finally {
                        refresh()
                    }
                }
            }
            .show()
    }

    /**
     * What the background scheduler does, in plain words. The honest answer is
     * "roughly every fifteen minutes, when Android allows", and an app that
     * quietly does nothing for hours while claiming to sync is worse than one
     * that says so.
     */
    private fun explainBackground() {
        kit.sheet()
            .header(R.drawable.ic_refresh_cw, "Background sync")
            .text("Android decides when this runs. Fifteen minutes is the shortest period it accepts, " +
                "and an idle phone may go much longer between attempts. With push, another device " +
                "wakes this phone when it has something.\n\nBoth devices have to be on at the same " +
                "moment for a sync to happen, so a computer that is off is missed until next time.")
            .buttons("Run one now", secondary = "Close") {
                // Through the scheduler rather than directly, so this exercises
                // the same path the periodic schedule uses.
                SyncWorker.runNow(app)
                app.say("Queued. It runs when Android allows.")
            }
            .show()
    }

    private fun field(start: String, hint: String): EditText = EditText(app).apply {
        setText(start)
        this.hint = hint
        setPadding(kit.dp(16), kit.dp(14), kit.dp(16), kit.dp(14))
        background = ContextCompat.getDrawable(app, R.drawable.glass_group)
    }

    /**
     * The relay, for when two devices cannot reach each other directly --
     * which on mobile data is often. Checked by the engine's own rule as it is
     * saved, so a mistyped address is refused here rather than found out by
     * every sync after.
     */
    private fun editRelay() {
        val input = field(Engine.relayAddress(app).orEmpty(), "relay.example.com:9001")
        val says = android.widget.TextView(app).apply {
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.error))
            visibility = View.GONE
        }
        val sheet = kit.sheet()
            .header(R.drawable.ic_arrow_up_down, "Relay")
            .text("When this phone and another device can't reach each other directly — common on " +
                "mobile data — their encrypted traffic goes through a relay instead. It can't read it.\n\n" +
                "Run `qurb relay` on a server of your own and enter its address and port. Leave it " +
                "empty for none.")
            .view(input, top = 14)
            .view(says, top = 6)
        val save = kit.button("Save") {
            val text = input.text.toString().trim()
            val problem = if (text.isEmpty()) null else uniffi.qurb_mobile.relayAddressProblem(text)
            if (problem != null) {
                says.text = problem
                says.visibility = View.VISIBLE
                return@button
            }
            Engine.setRelayAddress(app, text.ifEmpty { null })
            sheet.dismiss()
            app.say(if (text.isEmpty()) "No relay" else "Saved")
            refresh()
        }
        sheet.view(save, top = 16).show()
    }

    /**
     * Where the rendezvous service is: on a server of your own, run
     * `qurb signal` and point the phone at it.
     */
    private fun editSignal() {
        val input = field(Engine.signalUrl(app), "wss://…")
        kit.sheet()
            .header(R.drawable.ic_link, "Rendezvous service")
            .text("Two devices find each other through this when they're not on the same network. " +
                "Run `qurb signal` on a server of your own and enter the address it prints — wss://… " +
                "for one reachable from anywhere. It never sees your files.")
            .view(input, top = 14)
            .buttons("Save") {
                Engine.setSignalUrl(app, input.text.toString().trim())
                app.say("Saved")
                refresh()
            }
            .show()
    }
}
