package com.qurb

import android.view.View
import android.widget.LinearLayout
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.BackupState
import uniffi.qurb_mobile.OwnFolder

/**
 * This phone's folders, and which are backed up to the computer that keeps
 * its Private Vault (decision 0063): chosen here as well as on the computer,
 * the later choice winning wherever it was made (the owner's item 10,
 * 2026-10-14). Reached from Home and from Settings.
 *
 * Each top-level folder is a group, its folders inside it. A folder inside a
 * chosen one is backed up with it, so its switch is fixed and says so, as on
 * the computer.
 */
class FoldersScreen(app: MainActivity, from: String, override val tab: Int) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = from
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Your folders"
        views.subtitle.visibility = View.VISIBLE
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(val folders: List<OwnFolder>, val keeper: String?, val backup: BackupState)

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    // Never listed yet, with the access there: list now, so that
                    // the first look is not empty.
                    if (Folders.allowed() && engine.ownFolders().isEmpty()) {
                        runCatching { Folders.run(app, engine, 0u, listNow = true) }
                    }
                    State(
                        engine.ownFolders(),
                        engine.holders().joinToString(" and ") { it.name }.ifEmpty { null },
                        engine.foldersBackup(),
                    )
                }
                show(state)
            } catch (e: Exception) {
                app.fail("Could not read this phone's folders", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(state: State) {
        val page = views.sections
        page.removeAllViews()
        val keeper = state.keeper
        views.subtitle.text = if (keeper == null) "Backed up to the computer that keeps your Private Vault."
            else "Backed up to $keeper over Wi-Fi. What you delete here stays there."

        if (keeper == null) {
            kit.empty(page, R.drawable.ic_hard_drive,
                "No computer keeps this phone's Private Vault yet. Choose one in Devices, and its folders back up to it.",
                "Devices") { app.go(R.id.tab_devices) }
            return
        }
        if (!Folders.allowed()) {
            kit.empty(page, R.drawable.ic_hard_drive,
                "To back up its folders, Qurb needs to read this phone's files. It reads only the folders you " +
                    "choose, and lists the rest's names and sizes for $keeper.",
                "Allow") { app.startActivity(Folders.allowing(app)) }
            return
        }

        val chosen = state.folders.filter { it.chosen }
        if (chosen.isNotEmpty()) {
            page.addView(View(app), LinearLayout.LayoutParams(1, kit.dp(16)))
            val facts = kit.group(page)
            kit.item(facts, "On $keeper",
                if (state.backup.files == 0uL) "Nothing yet. It goes over Wi-Fi."
                else "${Words.count(state.backup.heldFiles)} of ${Words.files(state.backup.files.toInt())}, " +
                    "${Words.size(state.backup.heldBytes)} of ${Words.size(state.backup.bytes)}")
            if (state.backup.onlyKept > 0uL) {
                kit.item(facts, "Deleted on this phone", "${Words.files(state.backup.onlyKept.toInt())}, kept on $keeper")
            }
            kit.item(facts, "Back up now", "On Wi-Fi", chevron = false) {
                WhatsApp.runNow(app)
                app.say("Backing up on Wi-Fi")
            }
        }

        if (state.folders.isEmpty()) {
            kit.empty(page, R.drawable.ic_folder, "Looking at this phone's folders. Pull down in a moment.")
            return
        }
        val top = state.folders.filter { !it.path.contains('/') }
        // A group a top-level folder, itself first, then what is in it: no
        // title over it, which would only say its first row's name again.
        for (folder in top) {
            page.addView(View(app), LinearLayout.LayoutParams(1, kit.dp(16)))
            val group = kit.group(page)
            addSwitch(group, folder, folder.path, state, keeper)
            for (inner in state.folders.filter { it.path.startsWith("${folder.path}/") }) {
                addSwitch(group, inner, inner.path.substringAfter('/'), state, keeper)
            }
        }
    }

    private fun addSwitch(group: LinearLayout, folder: OwnFolder, label: String, state: State, keeper: String) {
        // Backed up with a chosen folder it is inside: fixed, and saying so.
        val outer = state.folders.firstOrNull { it.chosen && folder.path.startsWith("${it.path}/") }
        val facts = "${Words.files(folder.files.toInt())}, ${Words.size(folder.bytes)}" +
            if (folder.keptFiles > 0uL) " · ${folder.keptFiles} on $keeper" else ""
        val row = kit.toggle(group, label, if (outer != null) "Backed up with ${outer.path}" else facts,
            outer != null || folder.chosen) { on -> choose(folder.path, on, keeper) }
        if (outer != null) {
            row.toggle.isEnabled = false
            row.root.isClickable = false
            row.root.alpha = 0.6f
        }
    }

    private fun choose(path: String, on: Boolean, keeper: String) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { engine().chooseFolder(path, on) }
                // Started now rather than at the next background pass, and the
                // computer told at the next sync.
                if (on) WhatsApp.runNow(app)
                SyncWorker.runNow(app)
                app.say(if (on) "$path backs up to $keeper on Wi-Fi" else "$path backs up nothing new")
                refresh()
            } catch (e: Exception) {
                app.fail("Could not choose $path", e)
                refresh()
            }
        }
    }
}
