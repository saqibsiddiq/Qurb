package com.qurb

import android.content.res.ColorStateList
import android.net.Uri
import android.os.Environment
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.google.android.material.progressindicator.LinearProgressIndicator
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
import uniffi.qurb_mobile.Progress

/**
 * A computer's shared folder, reached live (decision 0073, item 6; Files
 * draft 4): the folder its own person's phones and its guests' phones can
 * all reach, opened on the computer -- nothing of it is kept on this phone,
 * and it needs the computer on. Search, folders, files; a new folder, and the
 * plus to add files from this phone. Every change is made by the computer,
 * which says why when it will not.
 *
 * Reached from Files. The connection is made once and kept while the folder
 * is open, and closed when it is left, as this phone's own folder there is
 * ([KeptScreen]).
 */
class SharedScreen(app: MainActivity, private val computer: PeerInfo) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    private var folder: KeptFolder? = null
    private var dir = ""
    private var query = ""
    private var searching: Job? = null
    private lateinit var search: EditText

    /** What the folder shows below the search field, redrawn on each listing. */
    private val listed = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Files"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Shared"
        // The plus adds files from this phone; the redesign floats it, and
        // here it stands at the end of the title, as Devices' does.
        views.action.apply {
            text = ""
            contentDescription = "Add files"
            setIconResource(R.drawable.ic_plus)
            iconSize = kit.dp(22)
            iconPadding = 0
            iconGravity = com.google.android.material.button.MaterialButton.ICON_GRAVITY_TEXT_START
            iconTint = ColorStateList.valueOf(kit.color(R.color.green))
            backgroundTintList = ColorStateList.valueOf(kit.color(R.color.green_soft))
            minWidth = kit.dp(44)
            minimumWidth = kit.dp(44)
            setPadding(0, 0, 0, 0)
            layoutParams = layoutParams.apply { width = kit.dp(44); height = kit.dp(44) }
            visibility = View.VISIBLE
            setOnClickListener { app.pickFiles { uris -> add(uris) } }
        }
        val row = views.title.parent as LinearLayout
        row.addView(kit.iconButton(R.drawable.ic_folder_plus, "New folder") { newFolder() },
            row.indexOfChild(views.action),
            LinearLayout.LayoutParams(kit.dp(44), kit.dp(44)).apply { marginEnd = kit.dp(6) })
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }

        val page = views.sections
        page.addView(on(), LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        page.addView(searchField(), LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48))
            .apply { topMargin = kit.dp(14) })
        page.addView(listed, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(14) })
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

    /** Which computer the folder is on, and whether it answered: a laptop, its
     *  name, its dot. */
    private fun on(): View {
        val line = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            contentDescription = "On ${computer.name}"
        }
        line.addView(ImageView(app).apply {
            setImageResource(States.device(computer.name, computer.kind))
            imageTintList = ColorStateList.valueOf(kit.color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(14), kit.dp(14)).apply { marginEnd = kit.dp(6) })
        line.addView(TextView(app).apply {
            text = computer.name
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        })
        line.addView(kit.dot(Engine.isOff(app, computer)), LinearLayout.LayoutParams(kit.dp(8), kit.dp(8))
            .apply { marginStart = kit.dp(6) })
        return line
    }

    /** *Search Shared*: the computer looks, as the words are typed. */
    private fun searchField(): View {
        val (field, box) = kit.searchField("Search Shared") { typed ->
            query = typed
            searching?.cancel()
            searching = scope.launch {
                delay(300)
                refresh()
            }
        }
        search = box
        return field
    }

    override fun refresh() {
        views.back.text = if (dir.isEmpty()) "Files" else dir.substringBeforeLast('/', "Shared")
        views.title.text = if (dir.isEmpty()) "Shared" else dir.substringAfterLast('/')
        if (folder == null) {
            listed.removeAllViews()
            kit.empty(listed, R.drawable.ic_users, "Reaching ${computer.name}…")
        }
        val asked = query
        scope.launch {
            try {
                val listing = withContext(Dispatchers.IO) {
                    val open = opened()
                    if (asked.isEmpty()) open.shared(dir) else KeptListing(emptyList(), open.search(true, asked))
                }
                if (asked == query) show(listing)
            } catch (e: Exception) {
                left()
                listed.removeAllViews()
                kit.empty(listed, R.drawable.ic_cloud_off,
                    "${computer.name} isn't reachable. It has to be on with Qurb running, and this phone online.",
                    "Try again") { refresh() }
                android.util.Log.w("qurb", "could not reach the shared folder", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    /** The connection, made the first time. Called off the main thread. */
    private suspend fun opened(): KeptFolder =
        folder ?: Engine.hearingTheNetwork(app) { engine().openKeptFolder(computer.fingerprint, 20u) }.also { folder = it }

    private fun show(listing: KeptListing) {
        listed.removeAllViews()
        if (listing.folders.isEmpty() && listing.files.isEmpty()) {
            kit.empty(listed, if (query.isEmpty()) R.drawable.ic_folder_open else R.drawable.ic_search,
                if (query.isNotEmpty()) "Nothing called “$query”." else if (dir.isEmpty()) "Nothing shared yet." else "This folder is empty.")
            return
        }
        // As Figma draws a folder's contents: folders as cards, two to a
        // row; files as rows. Held, either gives the sheet a tap on a file
        // gives (the owner, 2026-10-26).
        kit.folderGrid(listed, listing.folders.map { name ->
            kit.folderCard(name, onHold = { folderSheet(place(name), name) }) {
                dir = place(name)
                refresh()
            }
        })
        for (file in listing.files.sortedBy { it.name.lowercase() }) {
            kit.fileRow(listed, States.icon(file.name), file.name, Words.size(file.size)) { choose(file) }
        }
    }

    private fun place(name: String) = if (dir.isEmpty()) name else "$dir/$name"

    /** Where these are: the computer, its dot blue on and red off. */
    private fun where(): View = kit.chip(R.drawable.ic_laptop, computer.name,
        if (Engine.isOff(app, computer)) R.color.offline else R.color.online)

    /** One file: open it, keep a copy, rename it, delete it. */
    private fun choose(file: KeptFile) {
        val name = file.path.substringAfterLast('/')
        kit.sheet()
            .file(States.icon(name), name, where())
            .action(R.drawable.ic_external_link, "Open") { Opened.open(app, scope, folder, file) }
            .action(R.drawable.ic_download, "Save") { download(file) }
            .action(R.drawable.ic_pencil, "Rename") { rename(file.path, name) }
            .action(R.drawable.ic_trash_2, "Delete", danger = true) { delete(file.path, name) }
            .show()
    }

    /** A folder held: rename it, delete it. */
    private fun folderSheet(path: String, name: String) {
        kit.sheet()
            .file(R.drawable.ic_folder, name, where(), iconTint = R.color.green)
            .action(R.drawable.ic_pencil, "Rename") { rename(path, name) }
            .action(R.drawable.ic_trash_2, "Delete", danger = true) { delete(path, name) }
            .show()
    }

    private fun newFolder() = kit.askName("New folder", "", "Make") { name ->
        change("Made $name") { it.makeFolder(place(name)) }
    }

    private fun rename(path: String, name: String) = kit.askName("Rename", name, "Rename") { to ->
        val parent = path.substringBeforeLast('/', "")
        change("Renamed") { it.rename(path, if (parent.isEmpty()) to else "$parent/$to") }
    }

    private fun delete(path: String, name: String) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2)
            .title("Delete $name?", centred = true)
            .line("${computer.name} keeps it in Recently deleted for 30 days")
            .buttons("Delete", danger = true) { change("Deleted $name") { it.delete(path) } }
            .show()
    }

    /** A change, made by the computer: said done, or why not, and the folder
     *  read again. */
    private fun change(done: String, work: (KeptFolder) -> Unit) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { work(opened()) }
                app.say(done)
            } catch (e: Exception) {
                app.fail("Not done", e)
            }
            refresh()
        }
    }

    private fun download(file: KeptFile) {
        val name = file.path.substringAfterLast('/')
        val to = java.io.File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Qurb/$name").path
        app.say("Fetching $name…")
        scope.launch {
            try {
                val went = withContext(Dispatchers.IO) { opened().download(file, to) }
                app.say("Saved to Downloads: ${went.substringAfterLast('/')}")
            } catch (e: Exception) {
                app.fail("Could not fetch $name", e)
            }
        }
    }

    /**
     * Files from this phone, into the folder on screen: each copied in from
     * where the picker lent it, then sent to the computer in pieces, a sheet
     * showing how far it has got.
     */
    private fun add(uris: List<Uri>) {
        val bar = LinearProgressIndicator(app).apply {
            isIndeterminate = true
            max = 1000
            trackThickness = kit.dp(6)
            trackCornerRadius = kit.dp(3)
            setIndicatorColor(kit.color(R.color.green))
            trackColor = kit.color(R.color.neutral_bg)
            indicatorTrackGapSize = 0
            trackStopIndicatorSize = 0
        }
        val words = TextView(app).apply { setTextAppearance(R.style.Text_Name) }
        val sheet = kit.sheet().view(words, top = 6).view(bar, top = 12)
        sheet.dialog.setCancelable(false)
        sheet.show()
        scope.launch {
            var added = 0
            try {
                for ((i, uri) in uris.withIndex()) {
                    val staged = Engine.stage(app, uri)
                    words.text = if (uris.size == 1) staged.name else "${staged.name}  (${i + 1} of ${uris.size})"
                    val total = staged.file.length().coerceAtLeast(1L)
                    bar.setProgressCompat(0, false)
                    try {
                        withContext(Dispatchers.IO) {
                            opened().addFile(staged.file.path, place(staged.name), object : Progress {
                                override fun sent(bytes: ULong) {
                                    val done = (bytes.toDouble() / total * 1000).toInt().coerceIn(0, 1000)
                                    bar.post { bar.setProgressCompat(done, true) }
                                }
                            })
                        }
                        added++
                    } finally {
                        staged.file.delete()
                    }
                }
                app.say(if (added == 1) "Added" else "Added $added files")
            } catch (e: Exception) {
                app.fail(if (added == 0) "Not added" else "Added $added, then stopped", e)
            } finally {
                sheet.dismiss()
                refresh()
            }
        }
    }
}
