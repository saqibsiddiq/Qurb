package com.qurb

import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import androidx.core.content.ContextCompat
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.ShareTarget
import uniffi.qurb_mobile.SharedFolder

/**
 * Settings → Advanced (docs/design/redesign.md, Settings draft 2: "hide
 * rendezvous and relay under advanced"): the rendezvous service, the relay,
 * and the version.
 *
 * **For now also** the shared files' two settings, which the redesign takes
 * off the phone with the shared files themselves (decision 0073, item 2):
 * which devices each folder is on, and whether files arriving from other apps
 * stay private. Until the engine stops the phone holding shared files they
 * still do something, so they are kept, out of the way, rather than hidden.
 */
class AdvancedScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Settings"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Advanced"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        show()
        views.refresh.isRefreshing = false
    }

    private fun show() {
        val page = views.sections
        page.removeAllViews()

        var card = kit.card(page, top = 16)
        kit.setting(card, R.drawable.ic_link, "Rendezvous", Engine.signalUrl(app).ifEmpty { "None" }) { editSignal() }
        kit.setting(card, R.drawable.ic_cloud, "Relay", when (val relay = Engine.relayAddress(app)) {
            null -> "Auto"
            "off" -> "Off"
            else -> relay
        }) { editRelay() }

        card = kit.card(page, top = 16)
        val version = runCatching {
            app.packageManager.getPackageInfo(app.packageName, 0).versionName
        }.getOrNull() ?: "unknown"
        val engine = uniffi.qurb_mobile.engineVersion()
        kit.setting(card, R.drawable.ic_info, "Version", version).value.apply {
            // The engine's line too: protocol and index, for a report.
            text = "$version\n$engine"
            maxLines = 3
        }

        kit.label(page, "Shared files")
        card = kit.card(page)
        kit.setting(card, R.drawable.ic_folder, "Who has each folder") { chooseFolder() }
        kit.setting(card, R.drawable.ic_lock_keyhole, "Keep new files private",
            on = Engine.ownFilesPrivate(app), onChange = { setPrivate(it) })
    }

    private fun field(start: String, hint: String): EditText = EditText(app).apply {
        setText(start)
        this.hint = hint
        setPadding(kit.dp(16), kit.dp(14), kit.dp(16), kit.dp(14))
        background = ContextCompat.getDrawable(app, R.drawable.card)
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
            .title("Relay")
            .text("When this phone and another device can't reach each other directly, their encrypted " +
                "traffic goes through a relay, which can't read it. Empty uses the one beside your " +
                "rendezvous service; off uses none.")
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
            app.say(if (text.isEmpty()) "Using the relay beside your rendezvous service" else "Saved")
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
            .title("Rendezvous")
            .text("How two devices find each other when they're not on the same network. Run " +
                "`qurb signal` on a server of your own and enter the address it prints. It never sees your files.")
            .view(input, top = 14)
            .buttons("Save") {
                val url = input.text.toString().trim()
                Engine.setSignalUrl(app, url)
                // Kept with the key, so a reinstall or a new phone restored
                // from this one's backup has it back.
                scope.launch(Dispatchers.IO) { Backup.saveSignal(app, url) }
                app.say("Saved")
                refresh()
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
            val sheet = kit.sheet().title("Who has each folder")
            if (folders.isEmpty()) sheet.text("No folders yet")
            for (f in folders) {
                val who = if (f.everyone) "Every device" else f.members.joinToString(", ") { id ->
                    devices.find { it.id == id }?.name ?: "a removed device"
                }
                sheet.action(R.drawable.ic_folder, f.folder,
                    meta = if (f.remote) "$who, downloaded when opened" else who) { folderSheet(f, devices) }
            }
            sheet.show()
        }
    }

    private fun folderSheet(folder: SharedFolder, devices: List<ShareTarget>) {
        val sheet = kit.sheet().title(folder.folder)
            .text("A device you leave out keeps what it has, and gets nothing new")
        val chosen = devices.associate { it.id to (folder.everyone || it.id in folder.members) }.toMutableMap()
        val box = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        val card = kit.card(box)
        for (d in devices) {
            kit.setting(card, if (d.here) R.drawable.ic_smartphone else R.drawable.ic_laptop, d.name,
                if (d.here) "This phone" else "", on = chosen[d.id] == true, onChange = { chosen[d.id] = it })
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
}
