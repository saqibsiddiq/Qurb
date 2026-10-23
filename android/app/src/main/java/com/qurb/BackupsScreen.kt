package com.qurb

import android.view.View
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.BackupState
import uniffi.qurb_mobile.PeerInfo

/**
 * Backups, reached from the button at the top of Home (the owner's item 5,
 * 2026-10-23): WhatsApp's and this phone's folders, and where they go.
 *
 * **Interim.** The redesign's Backups (docs/design/redesign.md, *Backups and
 * Activity, draft 1*) is cards with ticks, bars and switches, for the device
 * chosen on Home. A backup set for each device is engine work not done yet
 * (decision 0073): today the backups go to every computer keeping this
 * phone's Private Vault, so this page names those, as chips, and keeps the
 * rows Home had until the redesign.
 */
class BackupsScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    private val backups = Backups(app) { refresh() }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Home"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Backups"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(
        val holders: List<PeerInfo>,
        val whatsApp: BackupState,
        val chosen: Int,
        val folders: BackupState,
    )

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    State(engine.holders(), engine.whatsappBackup(), engine.chosenFolders().size, engine.foldersBackup())
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
        val keeper = state.holders.joinToString(" and ") { it.name }.ifEmpty { null }
        if (state.holders.isNotEmpty()) {
            val chips = android.widget.LinearLayout(app).apply {
                orientation = android.widget.LinearLayout.HORIZONTAL
                setPadding(0, kit.dp(8), 0, kit.dp(4))
            }
            for (h in state.holders) {
                chips.addView(kit.deviceChip(h, Engine.isOff(app, h)), android.widget.LinearLayout.LayoutParams(
                    android.view.ViewGroup.LayoutParams.WRAP_CONTENT, android.view.ViewGroup.LayoutParams.WRAP_CONTENT,
                ).apply { marginEnd = kit.dp(8) })
            }
            page.addView(chips)
        }
        val group = kit.group(page)
        val backing = WhatsApp.tree(app) != null
        kit.item(group, "WhatsApp", backups.whatsAppLine(state.whatsApp, keeper, backing)) {
            backups.whatsApp(state.whatsApp, keeper, backing)
        }
        kit.item(group, "Your folders", backups.foldersLine(state.chosen, state.folders, keeper)) {
            app.push(FoldersScreen(app, "Backups", R.id.tab_home))
        }
    }
}
