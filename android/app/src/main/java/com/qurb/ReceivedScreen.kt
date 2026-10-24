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
        val card = kit.card(page)
        for ((file, h) in received) {
            val name = file.path.substringAfterLast('/')
            // Who sent it; when, for a sender the history did not name.
            kit.row(card, States.icon(name), name, h.device ?: Words.ago(h.at)) { app.open(file) }
        }
    }

    companion object {
        /** How far back the history is read for sends. */
        private const val HISTORY = 500
    }
}
