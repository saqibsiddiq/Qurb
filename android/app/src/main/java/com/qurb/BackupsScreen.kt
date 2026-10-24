package com.qurb

import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.BackupState
import uniffi.qurb_mobile.Outstanding
import uniffi.qurb_mobile.PeerInfo

/**
 * Backups, from the button at the top of Home (the owner's item 5,
 * 2026-10-23), as redesigned (docs/design/redesign.md, Backups and Activity,
 * draft 1): the device chosen on Home, switched the same way, and what goes
 * to it -- WhatsApp, the folders, the Private Vault -- each a card with a
 * tick and the size when up to date, a bar and how many are left while one
 * runs, a switch when it is off.
 *
 * What goes to a device is what the engine does today: WhatsApp and the
 * folders travel inside the Private Vault, to every device keeping it. So on
 * a device that keeps the vault all three show; on one that does not, the
 * vault alone, as a switch. A different set for each device (the owner's
 * item 4 of 2026-10-23) is engine work not done yet (decision 0073).
 */
class BackupsScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    private val backups = Backups(app) { refresh() }
    private val keeping = Keeping(app) { refresh() }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Home"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Backups"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(
        val peers: List<PeerInfo>,
        val holders: List<PeerInfo>,
        val whatsApp: BackupState,
        val chosen: Int,
        val folders: BackupState,
        val outstanding: Outstanding,
    )

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    State(engine.peers(), engine.holders(), engine.whatsappBackup(),
                        if (Folders.allowed(app)) engine.chosenFolders().size else 0,
                        engine.foldersBackup(), engine.outstanding())
                }
                show(state)
            } catch (e: Exception) {
                app.fail("Could not read the backups", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(state: State) {
        val page = views.sections
        page.removeAllViews()
        val device = Engine.chosen(app, state.peers, state.holders)

        // The device beside the title, as on Home, and chosen the same way.
        views.action.visibility = View.GONE
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        if (device != null) row.addView(kit.deviceChip(device, Engine.isOff(app, device)) {
            kit.chooseDevice(state.peers, device) { refresh() }
        })
        page.addView(row, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = kit.dp(4) })

        val cards = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        page.addView(cards, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = kit.dp(18) })

        if (device == null) {
            kit.addCard(cards, "Add a device") { app.pair() }
            return
        }
        val keeps = state.holders.any { it.fingerprint == device.fingerprint }
        val privateLeft = state.outstanding.files.count { it.private }
        if (keeps && device.relation != "host") {
            val backing = WhatsApp.tree(app) != null
            kit.backupCard(cards, R.drawable.ic_whatsapp, "WhatsApp", backups.progress(state.whatsApp, backing),
                on = if (backing) null else false,
                onChange = { on ->
                    if (on) backups.whatsApp(state.whatsApp, device.name, backing = false)
                    // The switch shows what is so: on once WhatsApp's folder is chosen.
                    refresh()
                },
                onTap = { backups.whatsApp(state.whatsApp, device.name, backing = true) })
            kit.backupCard(cards, R.drawable.ic_folder, "Folders",
                backups.progress(state.folders, state.chosen > 0, none = "None chosen")) {
                app.push(FoldersScreen(app, "Backups", R.id.tab_home))
            }
            kit.backupCard(cards, R.drawable.ic_lock_keyhole, "Private Vault",
                if (privateLeft > 0) Kit.Progress(R.drawable.ic_history, "${Words.files(privateLeft)} to go")
                else Kit.Progress(R.drawable.ic_circle_check, "Up to date")) {
                app.push(DeviceScreen(app, device))
            }
            // Decision 0062, answer 4: until a restore has been watched working.
            if (backing) kit.note(page, "Keep WhatsApp's own Google backup on too")
        } else {
            kit.backupCard(cards, R.drawable.ic_lock_keyhole, "Private Vault",
                Kit.Progress(R.drawable.ic_info, "Off"), on = keeps, onChange = { on ->
                    if (on) keeping.keep(device) else keeping.stop(device)
                })
        }
    }
}
