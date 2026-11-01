package com.qurb

import android.view.View
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.FileEntry

/**
 * Received: the files other devices sent to this phone (Files draft 3), the
 * newest first, each with who sent it. Read from the history, which records
 * each one as it arrives ("sent to this device"), and shown as long as the
 * file is still here; it lives in the Vault, which is where a send to a
 * phone goes (decision 0037).
 */
class ReceivedScreen(app: MainActivity, private val up: String = "Files") : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = up
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Received"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        scope.launch {
            try {
                val received = withContext(Dispatchers.IO) {
                    val engine = engine()
                    val here = engine.list().associateBy { it.path }
                    val history = engine.history(HISTORY.toUInt(), null)
                    // Renamed since, from here: followed to its name now.
                    val renamed = history.filter { it.kind == "moved" && it.detail?.startsWith(RENAMED_FROM) == true }
                        .mapNotNull { h -> h.path?.let { h.detail!!.removePrefix(RENAMED_FROM) to it } }
                        .reversed().toMap()
                    fun now(path: String): String {
                        var at = path
                        repeat(renamed.size) { at = renamed[at] ?: return at }
                        return at
                    }
                    history
                        .filter { it.kind == "received" && it.detail?.startsWith("sent to this device") == true }
                        .mapNotNull { h -> h.path?.let { here[now(it)] }?.let { it to h } }
                        .distinctBy { it.first.path }
                }
                show(received)
            } catch (e: Exception) {
                app.fail("Could not read what was received", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(received: List<Pair<FileEntry, uniffi.qurb_mobile.Happening>>) {
        val page = views.sections
        page.removeAllViews()
        if (received.isEmpty()) {
            kit.empty(page, R.drawable.ic_download, "Nothing sent to this phone yet.")
            return
        }
        // As Figma's Received: rows, each with who sent it as a badge on
        // its tile; a tap opens, held, every option (the owner's list of
        // 2026-11-01, item 5).
        val rows = android.widget.LinearLayout(app).apply { orientation = android.widget.LinearLayout.VERTICAL }
        page.addView(rows)
        for ((file, h) in received) {
            val name = file.path.substringAfterLast('/')
            val from = h.device
            kit.fileRow(rows, States.icon(name), name, Words.size(file.size),
                badge = from?.let { Kit.Badge(States.device(it), R.color.green) },
                said = from?.let { "from $it" } ?: Words.ago(h.at), onHold = { choose(file, from) }) { app.open(file) }
        }
    }

    /** A file received: open it, save a copy, send it on, rename, copy or
     *  delete it. It is in the Vault, so a copy is the Vault's, pasted
     *  anywhere; nothing is pasted into Received, which is what arrived. */
    private fun choose(file: FileEntry, from: String?) {
        val name = file.path.substringAfterLast('/')
        kit.sheet()
            .file(States.icon(name), name, from?.let { kit.chip(States.device(it), it) })
            .action(R.drawable.ic_external_link, "Open") { app.open(file) }
            .action(R.drawable.ic_save, "Save") { app.saveCopy(file) }
            .action(R.drawable.ic_send, "Send") {
                app.chooseDevice("Send $name to") { to -> app.send(file, to) }
            }
            .action(R.drawable.ic_pencil, "Rename") {
                kit.askName("Rename", name, "Rename") { to ->
                    val parent = file.path.substringBeforeLast('/', "")
                    scope.launch {
                        try {
                            withContext(Dispatchers.IO) { engine().rename(file.path, if (parent.isEmpty()) to else "$parent/$to") }
                            app.madeChange()
                        } catch (e: Exception) {
                            app.fail("Not renamed", e)
                        }
                        refresh()
                    }
                }
            }
            .action(R.drawable.ic_copy, "Copy") { Clipboard.copy(app, Clip.Vault(file.path)) }
            .action(R.drawable.ic_trash_2, "Delete", danger = true) {
                kit.sheet()
                    .mark(R.drawable.ic_trash_2)
                    .title("Delete $name?", centred = true)
                    .buttons("Delete", danger = true) {
                        scope.launch {
                            try {
                                withContext(Dispatchers.IO) { engine().remove(file.path) }
                                app.madeChange()
                            } catch (e: Exception) {
                                app.fail("Could not delete that", e)
                            }
                            refresh()
                        }
                    }
                    .show()
            }
            .show()
    }

    companion object {
        /** How far back the history is read for sends. */
        private const val HISTORY = 500

        /** How the engine's history says a file was renamed here. */
        private const val RENAMED_FROM = "renamed from "
    }
}
