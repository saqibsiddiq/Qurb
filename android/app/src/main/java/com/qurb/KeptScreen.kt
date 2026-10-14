package com.qurb

import android.os.Environment
import android.view.View
import android.widget.LinearLayout
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder
import uniffi.qurb_mobile.KeptListing
import uniffi.qurb_mobile.PeerInfo

/**
 * This phone's folder on a computer (the owner's item 5, decision 0067 step
 * 4): what the computer keeps for it -- its WhatsApp, the folders it backs
 * up, its Private Vault, files deleted here included, and what it kept for
 * this phone before it was set up again -- browsed live while the computer
 * is on. Any file can be downloaded to this phone, or restored where it was.
 *
 * Reached from Files. The connection is made once and kept while the folder
 * is open, and closed when it is left.
 */
class KeptScreen(app: MainActivity, private val computer: PeerInfo) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    private var folder: KeptFolder? = null
    private var dir = ""

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Files"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "On ${computer.name}"
        views.subtitle.visibility = View.VISIBLE
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun back(): Boolean {
        if (dir.isEmpty()) return false
        dir = dir.substringBeforeLast('/', "")
        refresh()
        return true
    }

    override fun left() {
        val open = folder ?: return
        folder = null
        scope.launch(Dispatchers.IO) { runCatching { open.disconnect() } }
    }

    override fun refresh() {
        views.subtitle.text = if (dir.isEmpty()) "Your folder there: backups, WhatsApp and Private Vault, " +
            "deleted files included, while ${computer.name} is on." else dir.replace("/", "  ›  ")
        if (folder == null) {
            views.sections.removeAllViews()
            kit.empty(views.sections, R.drawable.ic_monitor_smartphone, "Reaching ${computer.name}…")
        }
        scope.launch {
            try {
                val listing = withContext(Dispatchers.IO) {
                    val open = folder ?: Engine.hearingTheNetwork(app) {
                        engine().openKeptFolder(computer.fingerprint, 20u)
                    }.also { folder = it }
                    open.list(dir)
                }
                show(listing)
            } catch (e: Exception) {
                left()
                views.sections.removeAllViews()
                kit.empty(views.sections, R.drawable.ic_cloud_off,
                    "${computer.name} isn't reachable. It has to be on, with Qurb running.", "Try again") { refresh() }
                android.util.Log.w("qurb", "could not reach the kept folder", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(listing: KeptListing) {
        val page = views.sections
        page.removeAllViews()
        if (listing.folders.isEmpty() && listing.files.isEmpty()) {
            kit.empty(page, R.drawable.ic_folder_open,
                if (dir.isEmpty()) "${computer.name} keeps nothing for this phone yet." else "This folder is empty.")
            return
        }
        val rows = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        page.addView(rows)
        if (listing.folders.isNotEmpty()) kit.section(rows, if (dir.isEmpty()) "Phones" else "Folders")
        for (name in listing.folders) {
            val icon = if (dir.isEmpty()) R.drawable.ic_smartphone else R.drawable.ic_folder
            kit.row(rows, icon, name, if (dir.isEmpty() && name.endsWith(", earlier")) "Before it was set up again" else "") {
                dir = if (dir.isEmpty()) name else "$dir/$name"
                refresh()
            }
        }
        if (listing.files.isNotEmpty()) kit.section(rows, "Files")
        for (file in listing.files.take(MOST)) {
            kit.row(rows, States.icon(file.name), file.name,
                "${Words.size(file.size)}  ·  ${Words.ago(file.modifiedAt)}") { choose(file) }
        }
        if (listing.files.size > MOST) {
            kit.text(rows, "The first $MOST of ${Words.files(listing.files.size)}.")
        }
    }

    /** One file: where it would go, either way. */
    private fun choose(file: KeptFile) {
        kit.sheet()
            .header(States.icon(file.name), file.name, "${Words.size(file.size)}  ·  ${Words.ago(file.modifiedAt)}")
            .action(R.drawable.ic_download, "Download", meta = "A copy in this phone's Downloads, in Qurb") {
                fetch(file, downloads(file), "Saved to Downloads")
            }
            .action(R.drawable.ic_rotate_ccw, "Restore", meta = restoredTo(file)) { restore(file) }
            .show()
    }

    private fun downloads(file: KeptFile): String =
        java.io.File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Qurb/${file.name}").path

    /** Where a file goes back to: WhatsApp's folder, the folder it was backed
     *  up from, or the Private Vault. */
    private fun restoredTo(file: KeptFile): String = when {
        file.path.startsWith(WHATSAPP) -> "Back into WhatsApp's folder"
        file.path.startsWith(STORAGE) -> "Back to ${file.path.removePrefix(STORAGE).substringBeforeLast('/', "this phone")}"
        else -> "Back into your Private Vault"
    }

    private fun restore(file: KeptFile) {
        when {
            file.path.startsWith(WHATSAPP) || file.path.startsWith(STORAGE) -> {
                // Writing into another app's folder, or anywhere in the phone's
                // storage, is what "All files access" allows.
                if (!Folders.allowed()) {
                    app.say("Allow Qurb to manage this phone's files first, in Settings → Backups")
                    return
                }
                val rest = if (file.path.startsWith(WHATSAPP)) "Android/media/com.whatsapp/WhatsApp/" +
                    file.path.removePrefix(WHATSAPP) else file.path.removePrefix(STORAGE)
                fetch(file, "${Folders.root()}/$rest", "Restored")
            }
            else -> scope.launch {
                try {
                    withContext(Dispatchers.IO) {
                        val staging = java.io.File(app.cacheDir, "restore-${System.nanoTime()}")
                        try {
                            val open = folder ?: error("the folder is closed")
                            open.download(file, staging.path)
                            engine().importInto(staging.path, file.path, true)
                        } finally {
                            staging.delete()
                        }
                    }
                    app.say("Restored to your Private Vault")
                    app.changed()
                } catch (e: Exception) {
                    app.fail("Could not restore ${file.name}", e)
                }
            }
        }
    }

    private fun fetch(file: KeptFile, to: String, done: String) {
        app.say("Fetching ${file.name}…")
        scope.launch {
            try {
                val went = withContext(Dispatchers.IO) {
                    val open = folder ?: error("the folder is closed")
                    open.download(file, to)
                }
                app.say("$done: ${went.substringAfterLast('/')}")
            } catch (e: Exception) {
                app.fail("Could not fetch ${file.name}", e)
            }
        }
    }

    companion object {
        /** Rows drawn at most; a folder of WhatsApp images can hold thousands. */
        private const val MOST = 500
        private const val WHATSAPP = ".qurb-outside/whatsapp/"
        private const val STORAGE = ".qurb-outside/storage/"
    }
}
