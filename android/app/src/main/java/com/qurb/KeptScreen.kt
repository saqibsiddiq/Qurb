package com.qurb

import android.os.Environment
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
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
 * is on. Any file can be downloaded to this phone, or restored where it was,
 * and the whole folder searched, by the computer (decision 0073, item 4).
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
    /** This phone's own folder there, as the top lists it: not an earlier self's. */
    private var thisPhone: String? = null
    private var query = ""
    private var searching: Job? = null
    private lateinit var search: EditText

    /** What the folder shows below the search field, redrawn on each listing. */
    private val listed = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Files"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "On ${computer.name}"
        views.subtitle.visibility = View.VISIBLE
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
        scope.launch(Dispatchers.IO) { Opened.tidy(app) }

        val (field, box) = kit.searchField("Search ${computer.name}") { typed ->
            query = typed
            searching?.cancel()
            searching = scope.launch {
                delay(300)
                refresh()
            }
        }
        search = box
        views.sections.addView(field, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)))
        views.sections.addView(listed, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(14) })
    }

    override fun back(): Boolean {
        if (query.isNotEmpty()) {
            search.setText("")
            return true
        }
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
            listed.removeAllViews()
            kit.empty(listed, R.drawable.ic_monitor_smartphone, "Reaching ${computer.name}…")
        }
        val asked = query
        scope.launch {
            try {
                val listing = withContext(Dispatchers.IO) {
                    val open = folder ?: Engine.hearingTheNetwork(app) {
                        engine().openKeptFolder(computer.fingerprint, 20u)
                    }.also { folder = it }
                    if (asked.isEmpty()) open.list(dir) else KeptListing(emptyList(), open.search(false, asked))
                }
                // A listing for words since changed is not shown.
                if (asked == query) show(listing)
            } catch (e: Exception) {
                left()
                listed.removeAllViews()
                // Both ends, since either may be the one: on 2026-10-19 the S23's
                // mobile data stopped carrying anything while Android still
                // called it connected, and this screen blamed the laptop.
                kit.empty(listed, R.drawable.ic_cloud_off,
                    "${computer.name} isn't reachable. It has to be on with Qurb running, and this phone " +
                        "online.", "Try again") { refresh() }
                android.util.Log.w("qurb", "could not reach the kept folder", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(listing: KeptListing) {
        val page = listed
        page.removeAllViews()
        if (dir.isEmpty() && query.isEmpty()) thisPhone = listing.folders.firstOrNull { !it.endsWith(", earlier") }
        // Files added from here go into this phone's Private Vault, where it
        // is looked at (decision 0073, item 3); the computer keeps them at
        // the next sync.
        val vault = thisPhone?.let { "$it/Private Vault" }
        val inVault = vault != null && (dir == vault || dir.startsWith("$vault/"))
        views.action.apply {
            visibility = if (inVault && query.isEmpty()) View.VISIBLE else View.GONE
            text = "Add"
            setIconResource(R.drawable.ic_plus)
            setOnClickListener {
                val into = dir.removePrefix(vault.orEmpty()).trim('/')
                app.pickFiles { uris ->
                    change(if (uris.size == 1) "Added" else "Added ${uris.size} files") {
                        for (uri in uris) Engine.importUri(app, uri, into, true)
                    }
                }
            }
        }
        if (listing.folders.isEmpty() && listing.files.isEmpty()) {
            kit.empty(page, if (query.isEmpty()) R.drawable.ic_folder_open else R.drawable.ic_search, when {
                query.isNotEmpty() -> "Nothing called “$query”."
                dir.isEmpty() -> "${computer.name} keeps nothing for this phone yet."
                else -> "This folder is empty."
            })
            return
        }
        val rows = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        page.addView(rows)
        // Something copied, to paste in this folder of the Private Vault
        // (the owner's list of 2026-10-26, item 13); the cross forgets it.
        val clip = VaultClip.copied
        if (clip != null && inVault && query.isEmpty()) {
            val clipName = clip.substringAfterLast('/')
            kit.row(rows, R.drawable.ic_clipboard, "Paste $clipName", "",
                trail = kit.iconButton(R.drawable.ic_x, "Forget $clipName") { VaultClip.copied = null; refresh() }) {
                val into = dir.removePrefix(vault.orEmpty()).trim('/')
                change("Pasted $clipName") { it.copyKept(clip, if (into.isEmpty()) clipName else "$into/$clipName") }
            }
        }
        // As Figma draws a folder's contents: folders as cards, two to a
        // row, files as rows, no headings. A folder of the Private Vault,
        // held, gives the sheet a tap on a file gives: what a computer's file
        // manager does with one, done by this phone (item 13).
        kit.folderGrid(rows, listing.folders.map { name ->
            val icon = if (dir.isEmpty()) R.drawable.ic_smartphone else R.drawable.ic_folder
            val path = "${dir.removePrefix(vault.orEmpty()).trim('/')}/$name".trim('/')
            kit.folderCard(name, icon,
                count = if (dir.isEmpty() && name.endsWith(", earlier")) "Before it was set up again" else null,
                onHold = if (inVault) ({ folderActions(path, name) }) else null) {
                dir = if (dir.isEmpty()) name else "$dir/$name"
                refresh()
            }
        })
        if (listing.folders.isNotEmpty() && listing.files.isNotEmpty()) {
            rows.addView(View(app), LinearLayout.LayoutParams(1, kit.dp(8)))
        }
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
            kit.fileRow(rows, States.icon(file.name), file.name, Words.size(file.size)) { choose(file) }
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

    /** One file: opened here, or where it would go, either way. This
     *  phone's own can be changed too (decision 0073, item 3): one of its
     *  Private Vault renamed, moved or deleted; a backup deleted, which the
     *  engine refuses while its original is still on this phone. */
    private fun choose(file: KeptFile) {
        // Its own name: a search names each file by its whole place.
        val name = file.path.substringAfterLast('/')
        val sheet = kit.sheet()
            .file(States.icon(name), name, where())
            .action(R.drawable.ic_external_link, "Open", meta = "In an app on this phone, without saving a copy") {
                open(file)
            }
            .action(R.drawable.ic_save, "Save", meta = "A copy in this phone's Downloads, in Qurb") {
                fetch(file, downloads(file), "Saved to Downloads")
            }
            .action(R.drawable.ic_rotate_ccw, "Restore", meta = restoredTo(file)) { restore(file) }
        if (file.mine) {
            if (!backup(file)) {
                sheet.action(R.drawable.ic_pencil, "Rename") {
                    kit.askName("Rename", name, "Rename") { to ->
                        val parent = file.path.substringBeforeLast('/', "")
                        change("Renamed") { it.rename(file.path, if (parent.isEmpty()) to else "$parent/$to") }
                    }
                }
                sheet.action(R.drawable.ic_copy, "Copy") {
                    VaultClip.copied = file.path
                    app.say("Copied $name")
                    refresh()
                }
                sheet.action(R.drawable.ic_folder_input, "Move") {
                    kit.askName("Move to folder", file.path.substringBeforeLast('/', ""), "Move") { folder ->
                        val to = folder.trim('/')
                        change("Moved") { it.rename(file.path, if (to.isEmpty()) name else "$to/$name") }
                    }
                }
            }
            sheet.action(R.drawable.ic_trash_2, "Delete", danger = true) {
                kit.sheet()
                    .mark(R.drawable.ic_trash_2)
                    .title("Delete $name?", centred = true)
                    // In this phone's Recently deleted, whose restore puts it
                    // back from the copy the computer keeps as long.
                    .line("In Recently deleted for 30 days")
                    .buttons("Delete", danger = true) { change("Deleted $name") { it.deleteKept(file.path) } }
                    .show()
            }
        }
        sheet.show()
    }

    /** Where these are: the computer, its dot blue on and red off. */
    private fun where(): View = kit.chip(R.drawable.ic_laptop, computer.name,
        if (Engine.isOff(app, computer)) R.color.offline else R.color.online)

    private fun backup(file: KeptFile) = file.path.startsWith(WHATSAPP) || file.path.startsWith(STORAGE)

    /**
     * A folder of this phone's Private Vault there, as a computer's file
     * manager has one (the owner's list of 2026-10-26, item 13): renamed,
     * copied, moved or deleted whole, each file of it as one would be, by
     * this phone's own vault changing and the computer following.
     */
    private fun folderActions(path: String, name: String) {
        kit.sheet()
            .file(R.drawable.ic_folder, name, where(), iconTint = R.color.green)
            .action(R.drawable.ic_pencil, "Rename") {
                kit.askName("Rename", name, "Rename") { to ->
                    val parent = path.substringBeforeLast('/', "")
                    change("Renamed") { it.moveKept(path, if (parent.isEmpty()) to else "$parent/$to") }
                }
            }
            .action(R.drawable.ic_copy, "Copy") {
                VaultClip.copied = path
                app.say("Copied $name")
                refresh()
            }
            .action(R.drawable.ic_folder_input, "Move") {
                kit.askName("Move to folder", path.substringBeforeLast('/', ""), "Move") { folder ->
                    val to = folder.trim('/')
                    change("Moved") { it.moveKept(path, if (to.isEmpty()) name else "$to/$name") }
                }
            }
            .action(R.drawable.ic_trash_2, "Delete", danger = true) {
                kit.sheet()
                    .mark(R.drawable.ic_trash_2)
                    .title("Delete $name?", centred = true)
                    .line("Everything in it, in Recently deleted for 30 days")
                    .buttons("Delete", danger = true) { change("Deleted $name") { it.deleteKept(path) } }
                    .show()
            }
            .show()
    }

    /**
     * A change to this phone's own vault, then a sync, which takes it to the
     * computer: said done, or why not, and the folder read again once the
     * computer has it.
     */
    private fun change(done: String, work: suspend (uniffi.qurb_mobile.Qurb) -> Unit) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val engine = engine()
                    work(engine)
                    Engine.hearingTheNetwork(app) { runCatching { engine.syncWithin(15u) } }
                }
                app.say(done)
                app.changed()
            } catch (e: Exception) {
                app.fail("Not done", e)
            }
            refresh()
        }
    }

    /** Shown or played here, as Files opens a file ([Opened]). */
    private fun open(file: KeptFile) = Opened.open(app, scope, folder, file)

    private fun downloads(file: KeptFile): String =
        java.io.File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
            "Qurb/${file.path.substringAfterLast('/')}").path

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
                    app.fail("Could not restore ${file.path.substringAfterLast('/')}", e)
                }
            }
        }
    }

    private fun fetch(file: KeptFile, to: String, done: String) {
        app.say("Fetching ${file.path.substringAfterLast('/')}…")
        scope.launch {
            try {
                val went = withContext(Dispatchers.IO) {
                    val open = folder ?: error("the folder is closed")
                    open.download(file, to)
                }
                app.say("$done: ${went.substringAfterLast('/')}")
            } catch (e: Exception) {
                app.fail("Could not fetch ${file.path.substringAfterLast('/')}", e)
            }
        }
    }

    companion object {
        /** Rows drawn at a time; a folder of WhatsApp images can hold thousands. */
        private const val PAGE = 100
        private const val WHATSAPP = ".qurb-outside/whatsapp/"
        private const val STORAGE = ".qurb-outside/storage/"
    }
}
