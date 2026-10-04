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
import com.qurb.databinding.ItemFolderBinding
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
            views.title.text = "Private Vault"
            views.subtitle.text = "Private to this phone. Only this phone can see these files; " +
                "a device you choose can keep a backup."
            views.searchBox.hint = "Search Private Vault"
            views.search.hint = "Search Private Vault"
            views.back.visibility = View.VISIBLE
            views.back.text = "Files"
            views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        } else {
            views.subtitle.text = "Everything in your Qurb space, and where it is."
        }
        views.subtitle.visibility = View.VISIBLE
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

    override fun refresh() {
        drawCrumbs()
        scope.launch {
            try {
                val (folders, files) = withContext(Dispatchers.IO) {
                    val engine = engine()
                    if (query.isNotEmpty()) {
                        emptyList<String>() to engine.searchIn(query, SEARCH_LIMIT.toUInt(), private)
                    } else {
                        val listing = engine.browseIn(dir, private)
                        // With the folders on disk that hold nothing yet: the
                        // index knows files, and a folder just made is empty.
                        val made = if (private) emptyList() else java.io.File(Engine.root(app), dir).listFiles()
                            ?.filter { it.isDirectory && !it.name.startsWith(".") }
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
                    }
                    if (folders.isNotEmpty()) add(Item.Label("Folders"))
                    addAll(folders.map { Item.Folder(it, prefix + it) })
                    if (shown.isNotEmpty()) add(Item.Label(if (query.isEmpty()) "Files" else "Results"))
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
        val here = shown.filter { it.available != Available.ELSEWHERE }
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
        val state = States.of(entry, downloading)
        val sheet = kit.sheet().header(
            States.icon(entry.path), name,
            (if (entry.private) "Private Vault" else "Qurb") + if (folder.isEmpty()) "" else " / $folder",
        )
        val facts = android.widget.LinearLayout(app).apply { orientation = android.widget.LinearLayout.VERTICAL }
        val group = kit.group(facts)
        kit.item(group, state.words, when (entry.available) {
            Available.HERE -> "On this phone, and another device has it too."
            Available.ONLY_HERE -> "This is the only copy currently stored in Qurb. It reaches your other devices the next time one is online."
            Available.ELSEWHERE -> if (downloading) "Coming to this phone at the next sync."
                else "Not on this phone right now. It stays in Qurb; keep it here to open it."
        })
        kit.item(group, "${States.kind(entry.path)}  ·  ${Words.size(entry.size)}",
            "Changed ${Words.ago(entry.modifiedAt / 1_000_000_000)}")
        sheet.view(facts, top = 16)

        val here = entry.available != Available.ELSEWHERE
        if (here) {
            sheet.action(R.drawable.ic_external_link, "Open") { app.open(entry) }
        } else if (!downloading) {
            sheet.action(R.drawable.ic_download, "Keep on this phone") { fetch(entry) }
        }
        if (entry.available == Available.HERE) {
            sheet.action(R.drawable.ic_cloud_off, "Free local space") { free(entry) }
        }
        if (here) {
            sheet.action(R.drawable.ic_send, "Send to device…") {
                app.chooseDevice("Send $name to") { to -> app.send(entry, to) }
            }
            sheet.action(R.drawable.ic_save, "Save a copy to this phone") { app.saveCopy(entry) }
            sheet.action(R.drawable.ic_pencil, "Rename…") { rename(entry) }
            sheet.action(R.drawable.ic_folder_input, "Move to folder…") { move(entry) }
        }
        sheet.action(R.drawable.ic_trash_2, "Delete", danger = true) { delete(entry) }
        sheet.show()
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
            else -> ROW
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val inflater = LayoutInflater.from(parent.context)
            return when (viewType) {
                FOLDER -> FolderHolder(ItemFolderBinding.inflate(inflater, parent, false))
                LABEL -> object : RecyclerView.ViewHolder(TextView(app).apply {
                    setTextAppearance(R.style.Text_Label)
                    setPadding(kit.dp(8), kit.dp(18), kit.dp(8), kit.dp(6))
                }) {}
                else -> RowHolder(RowItemBinding.inflate(inflater, parent, false))
            }
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            when (val item = list[position]) {
                is Item.Label -> (holder.itemView as TextView).text = item.text
                is Item.Folder -> (holder as FolderHolder).bind(item)
                is Item.File -> (holder as RowHolder).bind(item.entry)
                is Item.Link -> (holder as RowHolder).bind(item)
            }
        }

        override fun getItemCount() = list.size
    }

    private inner class FolderHolder(private val tile: ItemFolderBinding) : RecyclerView.ViewHolder(tile.root) {
        fun bind(item: Item.Folder) {
            tile.name.text = item.name
            tile.root.setOnClickListener { open(item.path) }
        }
    }

    private inner class RowHolder(private val row: RowItemBinding) : RecyclerView.ViewHolder(row.root) {
        fun bind(entry: FileEntry) {
            val downloading = Downloads.wanted(entry)
            // The whole path when searching, where the folder is part of the
            // answer; just the name inside a folder.
            val name = if (query.isNotEmpty()) entry.path else entry.path.substringAfterLast('/')
            val meta = "${Words.size(entry.size)}  ·  ${Words.ago(entry.modifiedAt / 1_000_000_000)}"
            kit.bindRow(row, States.icon(entry.path), name, meta, States.of(entry, downloading)) { choose(entry) }
            row.tile.alpha = if (entry.available == Available.ELSEWHERE && !downloading) 0.7f else 1f
        }

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
