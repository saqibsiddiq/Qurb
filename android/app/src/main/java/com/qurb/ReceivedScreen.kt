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
class ReceivedScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Files"
        views.back.setTextAppearance(R.style.Text_Name)
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Received"
        views.title.setTextAppearance(R.style.Text_Hero)
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        scope.launch {
            try {
                val received = withContext(Dispatchers.IO) {
                    val engine = engine()
                    val here = engine.list().associateBy { it.path }
                    engine.history(HISTORY.toUInt(), null)
                        .filter { it.kind == "received" && it.detail?.startsWith("sent to this device") == true }
                        .mapNotNull { h -> h.path?.let { here[it] }?.let { it to h } }
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
        // its tile, and the sheet a file's tap gives, held or tapped.
        val rows = android.widget.LinearLayout(app).apply { orientation = android.widget.LinearLayout.VERTICAL }
        page.addView(rows)
        for ((file, h) in received) {
            val name = file.path.substringAfterLast('/')
            val from = h.device
            kit.fileRow(rows, States.icon(name), name, Words.size(file.size),
                badge = from?.let { Kit.Badge(States.device(it), R.color.green) },
                said = from?.let { "from $it" } ?: Words.ago(h.at)) { choose(file, from) }
        }
    }

    /** A file received: open it, save a copy, send it on, delete it. */
    private fun choose(file: FileEntry, from: String?) {
        val name = file.path.substringAfterLast('/')
        kit.sheet()
            .file(States.icon(name), name, from?.let { kit.chip(States.device(it), it) })
            .action(R.drawable.ic_external_link, "Open") { app.open(file) }
            .action(R.drawable.ic_save, "Save") { app.saveCopy(file) }
            .action(R.drawable.ic_send, "Send") {
                app.chooseDevice("Send $name to") { to -> app.send(file, to) }
            }
            .action(R.drawable.ic_trash_2, "Delete", danger = true) {
                kit.sheet()
                    .mark(R.drawable.ic_trash_2)
                    .title("Delete $name?", centred = true)
                    .line("In Recently deleted for 30 days")
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
    }
}
