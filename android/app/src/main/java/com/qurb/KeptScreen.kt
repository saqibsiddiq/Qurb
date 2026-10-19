package com.qurb

import android.content.Intent
import android.os.Environment
import android.view.View
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
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
        scope.launch(Dispatchers.IO) { tidyOpened() }
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
        StreamProvider.clear()
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
        showFiles(rows, listing.files, 0)
    }

    /**
     * Files from `from`, a page at a time, and a row for the next page. A
     * folder of WhatsApp images holds thousands, and drawn 500 at once the
     * S23 stopped for 0.8 s (2026-10-15); Files' own list recycles its rows,
     * this one draws what is asked for.
     */
    private fun showFiles(rows: LinearLayout, files: List<KeptFile>, from: Int) {
        val upTo = minOf(files.size, from + PAGE)
        for (file in files.subList(from, upTo)) {
            kit.row(rows, States.icon(file.name), file.name,
                facts(file)) { choose(file) }
        }
        if (upTo < files.size) {
            var more: View? = null
            more = kit.row(rows, R.drawable.ic_ellipsis, "Show ${minOf(PAGE, files.size - upTo)} more",
                "${Words.count(upTo.toLong())} of ${Words.files(files.size)} shown") {
                more?.let { rows.removeView(it) }
                showFiles(rows, files, upTo)
            }.root
        }
    }

    /** A file's size and when it changed -- or its size alone, when the
     *  file never said when: a time of 0 read "20742 days ago" on the S23
     *  for a video a download manager left undated (2026-10-15). */
    private fun facts(file: KeptFile): String =
        if (file.modifiedAt <= 0) Words.size(file.size) else "${Words.size(file.size)}  ·  ${Words.ago(file.modifiedAt)}"

    /** One file: opened here, or where it would go, either way. */
    private fun choose(file: KeptFile) {
        kit.sheet()
            .header(States.icon(file.name), file.name, facts(file))
            .action(R.drawable.ic_external_link, "Open", meta = "In an app on this phone, without saving a copy") {
                open(file)
            }
            .action(R.drawable.ic_download, "Download", meta = "A copy in this phone's Downloads, in Qurb") {
                fetch(file, downloads(file), "Saved to Downloads")
            }
            .action(R.drawable.ic_rotate_ccw, "Restore", meta = restoredTo(file)) { restore(file) }
            .show()
    }

    /**
     * Shown or played here, as Files opens a file (the owner's item of
     * 2026-10-15: a photo or a song in this folder could only be downloaded
     * or restored).
     *
     * A large file is read as the app showing it asks (`StreamProvider`):
     * a video starts at once and seeks where it likes, and nothing of it is
     * kept. The first way, fetching it whole first, kept the S23 waiting on
     * a 1.6 GB episode and filled its cache. A small one is fetched whole and
     * checked into the cache under its content's hash -- so a photo opened
     * again is not fetched again -- and handed over through a FileProvider,
     * readable through that one URI only; the cache is held under
     * [OPENED_MOST] and let go of a day later (`tidyOpened`).
     */
    private fun open(file: KeptFile) {
        val type = MimeTypeMap.getSingleton()
            .getMimeTypeFromExtension(file.name.substringAfterLast('.', "").lowercase()) ?: "*/*"
        if (file.size > STREAM_FROM) {
            scope.launch {
                try {
                    val uri = withContext(Dispatchers.IO) {
                        val open = folder ?: error("the folder is closed")
                        StreamProvider.add(app, open.stream(file), file.name, type)
                    }
                    val view = Intent(Intent.ACTION_VIEW).apply {
                        setDataAndType(uri, type)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }
                    app.startActivity(Intent.createChooser(view, "Open with"))
                } catch (e: android.content.ActivityNotFoundException) {
                    app.fail("Nothing on this phone can open ${file.name}", e)
                } catch (e: Exception) {
                    app.fail("Could not open ${file.name}", e)
                }
            }
            return
        }
        val dir = java.io.File(app.cacheDir, "$OPENED/${file.content.take(16)}")
        val here = java.io.File(dir, file.name)
        if (!here.exists()) app.say("Fetching ${file.name}…")
        scope.launch {
            try {
                val local = withContext(Dispatchers.IO) {
                    if (here.exists()) return@withContext here
                    val open = folder ?: error("the folder is closed")
                    dir.mkdirs()
                    java.io.File(open.download(file, here.path))
                }
                val uri = FileProvider.getUriForFile(app, "${app.packageName}.opened", local)
                scope.launch(Dispatchers.IO) { tidyOpened() }
                val view = Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, type)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                app.startActivity(Intent.createChooser(view, "Open with"))
            } catch (e: android.content.ActivityNotFoundException) {
                app.fail("Nothing on this phone can open ${file.name}", e)
            } catch (e: Exception) {
                app.fail("Could not open ${file.name}", e)
            }
        }
    }

    /** Files opened from here more than a day ago let go of, and the
     *  oldest beyond [OPENED_MOST] in all. */
    private fun tidyOpened() {
        val opened = java.io.File(app.cacheDir, OPENED)
        val old = System.currentTimeMillis() - 24L * 60 * 60 * 1000
        val kept = opened.listFiles()?.sortedByDescending { it.lastModified() } ?: return
        var total = 0L
        for (dir in kept) {
            total += dir.walkTopDown().filter { it.isFile }.sumOf { it.length() }
            if (dir.lastModified() < old || total > OPENED_MOST) dir.deleteRecursively()
        }
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
                if (!Folders.allowed(app)) {
                    app.say("Allow Qurb to manage this phone's files first, in Settings → Backups")
                    return
                }
                val rest = if (file.path.startsWith(WHATSAPP)) "${WhatsApp.relative()}/" +
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
        /** Rows drawn at a time; a folder of WhatsApp images can hold thousands. */
        private const val PAGE = 100
        private const val WHATSAPP = ".qurb-outside/whatsapp/"
        /** Under the app's cache: files fetched to be opened (`opened_paths.xml`). */
        private const val OPENED = "opened"
        /** Larger than this, a file is read as it is shown rather than fetched first. */
        private const val STREAM_FROM: ULong = 16_777_216uL
        /** The most the cache of opened files holds. */
        private const val OPENED_MOST = 200L * 1024 * 1024
        private const val STORAGE = ".qurb-outside/storage/"
    }
}
