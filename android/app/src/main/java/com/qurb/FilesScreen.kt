package com.qurb

import android.text.Editable
import android.text.TextWatcher
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.recyclerview.widget.GridLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.qurb.databinding.RowItemBinding
import com.qurb.databinding.ScreenFilesBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.Available
import uniffi.qurb_mobile.FileEntry

/**
 * Files, or Private Vault (direction §18, §15): the same browser over the two
 * areas a phone has -- the shared space every device sees, and this phone's
 * own.
 *
 * Every file says where its bytes are -- *On this phone*, *Available
 * elsewhere*, *Only copy here* -- because that decides what a person can
 * safely do with it, and the actions offered follow from it. *Free local
 * space* never looks like deleting (§12), and is never offered for the only
 * copy (§13).
 *
 * Browsed a folder at a time from the engine's index, the same way the system
 * file picker lists Qurb, so the two cannot disagree. Search looks at every
 * folder at once. Back goes up a folder before it leaves.
 */
class FilesScreen(app: MainActivity, private val private: Boolean) : Screen(app) {

    private val views = ScreenFilesBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    private val items = ItemAdapter()

    /** The folder being looked at; the empty string is the top. */
    private var dir = ""
    private var query = ""
    private var sort = Sort.NAME
    private var searching: Job? = null

    /** The files in the current list, for "save everything here". */
    private var shown: List<FileEntry> = emptyList()

    private enum class Sort(val label: String) { NAME("Name"), NEWEST("Newest first"), LARGEST("Largest first") }

    private sealed class Item {
        data class Label(val text: String) : Item()
        data class Folder(val name: String, val path: String) : Item()
        data class File(val entry: FileEntry) : Item()
        data class Link(val icon: Int, val title: String, val meta: String, val go: () -> Unit) : Item()
    }

    init {
        if (private) {
            // As Figma's Vault: its name, and no paragraph under it.
            views.title.text = "Vault"
            views.searchBox.hint = "Search Vault"
            views.search.hint = "Search Vault"
            views.back.visibility = View.VISIBLE
            views.back.text = "Files"
            views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        } else {
            views.subtitle.text = "Everything in your Qurb space, and where it is."
        }
        views.subtitle.visibility = if (private) View.GONE else View.VISIBLE
        views.action.setOnClickListener { app.pickFilesToAdd(dir, private) }
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
        views.more.setOnClickListener { more() }

        val grid = GridLayoutManager(app, 2)
        grid.spanSizeLookup = object : GridLayoutManager.SpanSizeLookup() {
            override fun getSpanSize(position: Int) = if (items.at(position) is Item.Folder) 1 else 2
        }
        views.list.layoutManager = grid
        views.list.adapter = items

        views.search.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                query = s?.toString()?.trim() ?: ""
                // A quarter of a second after the last key, not on every one.
                searching?.cancel()
                searching = scope.launch {
                    delay(250)
                    refresh()
                }
            }
        })
    }

    override fun back(): Boolean = when {
        query.isNotEmpty() -> {
            views.search.setText("")
            true
        }
        dir.isNotEmpty() -> {
            dir = dir.substringBeforeLast('/', "")
            refresh()
            true
        }
        else -> false
    }

    private fun open(folder: String) {
        dir = folder
        refresh()
    }

    /** A computer of another person keeping this phone's vault, if one does
     *  (decision 0060): the vault is then "your folder" there. */
    private var keptOn: String? = null

    /** This person's computers keeping this phone's folder, reached from the
     *  top of Files (decision 0067, step 4). */
    private var keepers: List<uniffi.qurb_mobile.PeerInfo> = emptyList()
    private var computers: List<uniffi.qurb_mobile.PeerInfo> = emptyList()

    override fun refresh() {
        drawCrumbs()
        refreshing {
            try {
                val (folders, files) = withContext(Dispatchers.IO) {
                    val engine = engine()
                    if (private) {
                        val keepers = engine.holders().map { it.fingerprint }.toSet()
                        keptOn = engine.peers().firstOrNull { it.relation == "host" && it.fingerprint in keepers }?.name
                    } else if (dir.isEmpty()) {
                        keepers = engine.holders().filter { it.relation == "own" }
                        computers = engine.peers().filter { it.relation == "host" || (it.relation == "own" && it.kind == "computer") }
                    }
                    if (query.isNotEmpty()) {
                        emptyList<String>() to engine.searchIn(query, SEARCH_LIMIT.toUInt(), private)
                    } else {
                        val listing = engine.browseIn(dir, private)
                        // With the folders on disk that hold nothing yet: the
                        // index knows files, and a folder just made is empty.
                        // Not one holding Private Vault's files: both areas
                        // share the disk, and that folder is the vault's.
                        val vaults = if (private) emptySet() else engine.browseIn(dir, true).folders.toSet()
                        val made = if (private) emptyList() else java.io.File(Engine.root(app), dir).listFiles()
                            ?.filter { it.isDirectory && !it.name.startsWith(".") && it.name !in vaults }
                            ?.map { it.name }
                            .orEmpty()
                        (listing.folders + made).distinct().sorted() to listing.files
                    }
                }
                shown = sorted(files)
                val prefix = if (dir.isEmpty()) "" else "$dir/"
                val list = buildList {
                    if (!private && dir.isEmpty() && query.isEmpty()) {
                        add(Item.Link(R.drawable.ic_lock_keyhole, "Private Vault",
                            "Private to this phone") { app.push(FilesScreen(app, private = true)) })
                        // The owner's item 5: the folder a computer keeps for
                        // this phone, to restore or download from it.
                        for (keeper in keepers) {
                            add(Item.Link(R.drawable.ic_hard_drive, "Your folder on ${keeper.name}",
                                "Backups, WhatsApp, deleted files · while it's on") { app.push(KeptScreen(app, keeper)) })
                        }
                        // Each computer's shared folder, which this phone and
                        // its guests' phones reach live (decision 0073): the
                        // owner's computers, and any this phone visits.
                        for (computer in computers) {
                            add(Item.Link(R.drawable.ic_users, "Shared", "On ${computer.name}") {
                                app.push(SharedScreen(app, computer))
                            })
                        }
                    }
                    // Something copied in the Vault, to paste in this folder
                    // (as Private on a computer has it); held, forgotten.
                    val clip = VaultClip.copied
                    if (private && clip != null && query.isEmpty()) {
                        val clipName = clip.substringAfterLast('/')
                        add(Item.Link(R.drawable.ic_clipboard, "Paste $clipName", "") { paste(clip) })
                    }
                    // As Figma draws a folder: cards, then rows, no headings.
                    addAll(folders.map { Item.Folder(it, prefix + it) })
                    addAll(shown.map { Item.File(it) })
                    if (!private && dir.isEmpty() && query.isEmpty()) {
                        add(Item.Link(R.drawable.ic_trash_2, "Recently deleted",
                            "Kept for 30 days") { app.push(DeletedScreen(app)) })
                    }
                }
                items.submit(list)
                showEmpty(folders.isEmpty() && files.isEmpty())
            } catch (e: Exception) {
                app.fail("Could not read this phone's files", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun showEmpty(empty: Boolean) {
        views.empty.removeAllViews()
        views.empty.visibility = if (empty) View.VISIBLE else View.GONE
        if (!empty) return
        when {
            query.isNotEmpty() -> kit.empty(views.empty, R.drawable.ic_search, "Nothing called “$query”.")
            dir.isNotEmpty() -> kit.empty(views.empty, R.drawable.ic_folder_open, "This folder is empty.")
            private -> kit.empty(views.empty, R.drawable.ic_lock_keyhole,
                "Nothing here yet. Files you add here stay on this phone.")
            else -> kit.empty(views.empty, R.drawable.ic_folder_open,
                "Nothing here yet. Add files, or add a device and sync to see what it shares.")
        }
    }

    /** Where you are, as a trail back to the top. */
    private fun drawCrumbs() {
        val crumbs = views.crumbs
        crumbs.removeAllViews()
        if (private) {
            // As Figma's folder page: the folder's name as the title, the
            // way back naming where it goes.
            crumbs.visibility = View.GONE
            views.title.text = if (dir.isEmpty()) "Vault" else dir.substringAfterLast('/')
            views.back.text = when {
                dir.isEmpty() -> "Files"
                dir.contains('/') -> dir.substringBeforeLast('/').substringAfterLast('/')
                else -> "Vault"
            }
            return
        }
        if (query.isNotEmpty()) return
        val parts = if (dir.isEmpty()) emptyList() else dir.split('/')
        val crumb = { label: String, to: String, last: Boolean ->
            crumbs.addView(TextView(app).apply {
                text = label
                setTextAppearance(if (last) R.style.Text_Name else R.style.Text_Quiet)
                textSize = 14f
                setPadding(kit.dp(6), kit.dp(6), kit.dp(6), kit.dp(6))
                background = ContextCompat.getDrawable(app, R.drawable.row_bg)
                if (!last) setOnClickListener { open(to) }
            })
        }
        crumb(if (private) "Private Vault" else "Qurb", "", parts.isEmpty())
        parts.forEachIndexed { i, part ->
            crumbs.addView(android.widget.ImageView(app).apply {
                setImageResource(R.drawable.ic_chevron_right)
                imageTintList = android.content.res.ColorStateList.valueOf(kit.color(R.color.text_3))
            }, ViewGroup.LayoutParams(kit.dp(14), kit.dp(14)))
            crumb(part, parts.take(i + 1).joinToString("/"), i == parts.lastIndex)
        }
    }

    private fun sorted(files: List<FileEntry>): List<FileEntry> = when (sort) {
        Sort.NAME -> files.sortedBy { it.path.lowercase() }
        Sort.NEWEST -> files.sortedByDescending { it.modifiedAt }
        Sort.LARGEST -> files.sortedByDescending { it.size }
    }

    /** What applies to the whole list rather than one file. */
    private fun more() {
        val sheet = kit.sheet().header(if (private) R.drawable.ic_lock_keyhole else R.drawable.ic_folder,
            if (dir.isEmpty()) views.title.text.toString() else dir.substringAfterLast('/'))
        for (s in Sort.entries) {
            sheet.action(if (s == sort) R.drawable.ic_check else R.drawable.ic_sliders_horizontal, "Sort: ${s.label}") {
                sort = s
                refresh()
            }
        }
        if (query.isEmpty() && !private) sheet.action(R.drawable.ic_folder_plus, "New folder…") { newFolder() }
        val here = shown.filter { States.here(it) }
        if (here.isNotEmpty()) {
            sheet.action(R.drawable.ic_save, "Save ${Words.files(here.size)} to this phone…") { app.saveAll(here) }
        }
        if (!private) sheet.action(R.drawable.ic_trash_2, "Recently deleted") { app.push(DeletedScreen(app)) }
        sheet.show()
    }

    /** A file's details and what can be done with it (§19), in a sheet. */
    private fun choose(entry: FileEntry) {
        val name = entry.path.substringAfterLast('/')
        val folder = entry.path.substringBeforeLast('/', "")
        val downloading = Downloads.wanted(entry)
        val state = States.of(entry, downloading, keptOn)
        // As Figma's file sheet: its kind, its name, where it is as a chip,
        // then what can be done as round buttons.
        val sheet = kit.sheet().file(States.icon(entry.path), name, kit.chip(state.icon, state.words))

        // A file on no device can be read about and deleted, and nothing
        // else: there is nothing to open and nowhere to fetch it from.
        val here = States.here(entry)
        if (here) {
            sheet.action(R.drawable.ic_external_link, "Open") { app.open(entry) }
        } else if (entry.available == Available.ELSEWHERE && !downloading) {
            sheet.action(R.drawable.ic_download, "Keep here", meta = "Kept on this phone") { fetch(entry) }
        }
        if (entry.available == Available.HERE) {
            sheet.action(R.drawable.ic_cloud_off, "Free up", meta = "Free local space; it stays in Qurb") { free(entry) }
        }
        if (here) {
            sheet.action(R.drawable.ic_send, "Send") {
                app.chooseDevice("Send $name to") { to -> app.send(entry, to) }
            }
            sheet.action(R.drawable.ic_save, "Save", meta = "A copy to this phone") { app.saveCopy(entry) }
            sheet.action(R.drawable.ic_pencil, "Rename") { rename(entry) }
            if (private) sheet.action(R.drawable.ic_copy, "Copy") { copy(entry.path) }
            sheet.action(R.drawable.ic_folder_input, "Move") { move(entry) }
            // No moving between the shared area and the Vault (decision
            // 0057) since a phone keeps no shared files (decision 0073).
        }
        sheet.action(R.drawable.ic_trash_2, "Delete", danger = true) { delete(entry) }
        sheet.show()
    }

    /**
     * A folder of the Vault, held: renamed, copied, moved or deleted whole,
     * as a computer's file manager does (the owner, 2026-10-26), in the
     * sheet a file's tap gives.
     */
    private fun folderActions(path: String) {
        val name = path.substringAfterLast('/')
        kit.sheet()
            .file(R.drawable.ic_folder, name, iconTint = R.color.green)
            .action(R.drawable.ic_pencil, "Rename") {
                ask("Rename", name, "Rename") { to ->
                    val parent = path.substringBeforeLast('/', "")
                    vault("Renamed") { it.moveKept(path, if (parent.isEmpty()) to else "$parent/$to") }
                }
            }
            .action(R.drawable.ic_copy, "Copy") { copy(path) }
            .action(R.drawable.ic_folder_input, "Move") {
                ask("Move to folder", path.substringBeforeLast('/', ""), "Move") { folder ->
                    val to = folder.trim('/')
                    vault("Moved") { it.moveKept(path, if (to.isEmpty()) name else "$to/$name") }
                }
            }
            .action(R.drawable.ic_trash_2, "Delete", danger = true) {
                kit.sheet()
                    .mark(R.drawable.ic_trash_2)
                    .title("Delete $name?", centred = true)
                    .line("Everything in it, in Recently deleted for 30 days")
                    .buttons("Delete", danger = true) { vault("Deleted $name") { it.deleteKept(path) } }
                    .show()
            }
            .show()
    }

    private fun copy(path: String) {
        VaultClip.copied = path
        app.say("Copied ${path.substringAfterLast('/')}")
        refresh()
    }

    private fun paste(clip: String) {
        val name = clip.substringAfterLast('/')
        vault("Pasted $name") { it.copyKept(clip, if (dir.isEmpty()) name else "$dir/$name") }
    }

    /** A change to the Vault, said done or why not, and the folder read again. */
    private fun vault(done: String, work: (uniffi.qurb_mobile.Qurb) -> Unit) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { work(engine()) }
                app.say(done)
                app.madeChange()
            } catch (e: Exception) {
                app.fail("Not done", e)
                app.changed()
            }
        }
    }

    /** Ask for a name, with a starting text, then act on it. */
    private fun ask(title: String, start: String, action: String, then: (String) -> Unit) {
        val input = EditText(app).apply {
            setText(start)
            setSelection(start.substringBeforeLast('.').length.coerceAtMost(start.length))
            setPadding(kit.dp(16), kit.dp(14), kit.dp(16), kit.dp(14))
            background = ContextCompat.getDrawable(app, R.drawable.glass_group)
        }
        kit.sheet().header(null, title).view(input, top = 16).buttons(action) {
            val text = input.text.toString().trim().trim('/')
            if (text.isNotEmpty()) then(text)
        }.show()
        input.requestFocus()
    }

    private fun rename(entry: FileEntry) {
        val folder = entry.path.substringBeforeLast('/', "")
        ask("Rename", entry.path.substringAfterLast('/'), "Rename") { name ->
            moveTo(entry, if (folder.isEmpty()) name else "$folder/$name")
        }
    }

    /** By folder path, typed: the top is an empty box. */
    private fun move(entry: FileEntry) {
        ask("Move to folder", entry.path.substringBeforeLast('/', ""), "Move") { folder ->
            moveTo(entry, "$folder/${entry.path.substringAfterLast('/')}")
        }
    }

    private fun moveTo(entry: FileEntry, to: String) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { engine().rename(entry.path, to) }
                app.say("Now ${to.substringAfterLast('/')}")
                app.madeChange()
            } catch (e: Exception) {
                app.fail("Could not do that", e)
                app.changed()
            }
        }
    }

    private fun newFolder() {
        ask("New folder", "", "Make") { name ->
            val path = if (dir.isEmpty()) name else "$dir/$name"
            scope.launch {
                try {
                    withContext(Dispatchers.IO) { engine().makeFolder(path) }
                    open(path)
                } catch (e: Exception) {
                    app.fail("Could not make that folder", e)
                }
            }
        }
    }

    /**
     * Free local space (§12, §31): this phone stops keeping the bytes; the
     * file stays in Qurb and in this list. The engine refuses when no other
     * device is known to hold it, so offering it only for files marked as held
     * elsewhere is a courtesy rather than the protection.
     */
    private fun free(entry: FileEntry) {
        scope.launch {
            try {
                val freed = withContext(Dispatchers.IO) { engine().freeLocal(entry.path) }
                app.say("Freed ${Words.size(freed)}. ${entry.path.substringAfterLast('/')} stays in Qurb.")
            } catch (e: Exception) {
                app.fail("Could not free that", e)
            } finally {
                app.changed()
            }
        }
    }

    /**
     * Into this phone's Private Vault, or out of it to every device (decision
     * 0057). Into it, the person is told first what happens to the copies on
     * their other devices, and whether anything still keeps one.
     */
    /** Keep on this phone (§30): asked for now, fetched at the next sync with
     *  a device that has it. */
    private fun fetch(entry: FileEntry) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { engine().fetch(entry.path) }
                Downloads.ask(entry.path)
                refresh()
                app.sync()
            } catch (e: Exception) {
                app.fail("Could not ask for that", e)
            }
        }
    }

    private fun delete(entry: FileEntry) {
        val where = if (entry.private) {
            "It's deleted from this phone, and from any device keeping a backup of your Private Vault."
        } else {
            "It's in your Qurb space, so it's deleted on every device at its next sync."
        }
        kit.sheet()
            .header(States.icon(entry.path), "Delete ${entry.path.substringAfterLast('/')}?")
            .text("$where Recently deleted keeps it for 30 days.")
            .buttons("Delete", danger = true) {
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) { engine().remove(entry.path) }
                        app.say("Deleted. It's in Recently deleted for 30 days.")
                        app.madeChange()
                    } catch (e: Exception) {
                        app.fail("Could not delete that", e)
                        app.changed()
                    }
                }
            }
            .show()
    }

    private inner class ItemAdapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        private var list: List<Item> = emptyList()

        fun at(position: Int): Item? = list.getOrNull(position)

        fun submit(next: List<Item>) {
            list = next
            notifyDataSetChanged()
        }

        override fun getItemViewType(position: Int) = when (list[position]) {
            is Item.Folder -> FOLDER
            is Item.Label -> LABEL
            is Item.File -> FILE
            else -> ROW
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val inflater = LayoutInflater.from(parent.context)
            return when (viewType) {
                FOLDER -> FolderHolder(android.widget.FrameLayout(app).apply {
                    layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                })
                FILE -> FileHolder(kit.fileRow().also {
                    it.root.layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(64))
                        .apply { topMargin = kit.dp(4) }
                })
                LABEL -> object : RecyclerView.ViewHolder(TextView(app).apply {
                    setTextAppearance(R.style.Text_Label)
                    setPadding(kit.dp(8), kit.dp(18), kit.dp(8), kit.dp(6))  // 12 + 8: the gutter
                }) {}
                else -> RowHolder(RowItemBinding.inflate(inflater, parent, false))
            }
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            when (val item = list[position]) {
                is Item.Label -> (holder.itemView as TextView).text = item.text
                is Item.Folder -> (holder as FolderHolder).bind(item)
                is Item.File -> (holder as FileHolder).bind(item.entry)
                is Item.Link -> (holder as RowHolder).bind(item)
            }
        }

        override fun getItemCount() = list.size
    }

    /** A folder as a card, two to a row, 12 dp apart and 12 dp over the next. */
    private inner class FolderHolder(private val slot: android.widget.FrameLayout) : RecyclerView.ViewHolder(slot) {
        fun bind(item: Item.Folder) {
            slot.removeAllViews()
            slot.addView(kit.folderCard(item.name,
                onHold = if (private) ({ folderActions(item.path) }) else null) { open(item.path) })
            val left = folderColumn(bindingAdapterPosition) == 0
            slot.setPadding(if (left) 0 else kit.dp(6), 0, if (left) kit.dp(6) else 0, kit.dp(12))
        }
    }

    /** Which of the two columns the folder at `position` is in. */
    private fun folderColumn(position: Int): Int {
        var column = 0
        for (i in 0 until position) column = if (items.at(i) is Item.Folder) (column + 1) % 2 else 0
        return column
    }

    /** A file as Figma's row, a badge for where it is; tapped or held, its sheet. */
    private inner class FileHolder(private val row: Kit.FileRow) : RecyclerView.ViewHolder(row.root) {
        fun bind(entry: FileEntry) {
            val downloading = Downloads.wanted(entry)
            // The whole path when searching, where the folder is part of the
            // answer; just the name inside a folder.
            val name = if (query.isNotEmpty()) entry.path else entry.path.substringAfterLast('/')
            val state = States.of(entry, downloading, keptOn)
            kit.bindFile(row, States.icon(entry.path), name, Words.size(entry.size), badge(entry, downloading),
                state.words) { choose(entry) }
            row.tile.alpha = if (!States.here(entry) && !downloading) 0.7f else 1f
        }
    }

    /** Where a file is, as Figma's badge: a check on green for here, a cloud on grey for elsewhere. */
    private fun badge(entry: FileEntry, downloading: Boolean): Kit.Badge = when {
        downloading && entry.available == Available.ELSEWHERE -> Kit.Badge(R.drawable.ic_download, R.color.green)
        entry.available == Available.HERE -> Kit.Badge(R.drawable.ic_circle_check, R.color.healthy)
        entry.available == Available.ONLY_HERE -> Kit.Badge(R.drawable.ic_circle_check, R.color.attention)
        entry.available == Available.ELSEWHERE -> Kit.Badge(R.drawable.ic_cloud, R.color.neutral_bg, R.color.text_2)
        else -> Kit.Badge(R.drawable.ic_triangle_alert, R.color.error)
    }

    private inner class RowHolder(private val row: RowItemBinding) : RecyclerView.ViewHolder(row.root) {
        fun bind(link: Item.Link) {
            kit.bindRow(row, link.icon, link.title, link.meta, trail = kit.chevron()) { link.go() }
            row.tile.alpha = 1f
        }
    }

    private companion object {
        const val SEARCH_LIMIT = 500
        const val ROW = 0
        const val FOLDER = 1
        const val LABEL = 2
        const val FILE = 3
    }
}

/**
 * Files asked for with *Keep on this phone* that are not here yet: shown as
 * *Downloading* until a listing finds them here (§30). Shared by both
 * browsers, since a file asked for in one may be looked at in the other.
 */
object Downloads {
    private val asked = mutableSetOf<String>()

    fun ask(path: String) {
        synchronized(asked) { asked += path }
    }

    fun wanted(entry: FileEntry): Boolean = synchronized(asked) {
        if (entry.available != Available.ELSEWHERE) asked -= entry.path
        entry.path in asked
    }
}

/**
 * A file or folder of the Vault copied, by its path there, to paste in
 * another folder: from the phone's own Vault or from Private on a computer,
 * which are the same vault. Kept while the app is.
 */
object VaultClip {
    var copied: String? = null
}
