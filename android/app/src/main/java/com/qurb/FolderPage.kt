package com.qurb

import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.text.TextUtils
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.Available
import uniffi.qurb_mobile.FileEntry
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.Progress

/**
 * One folder page for the Vault, a computer's *Shared* and this phone's
 * backup folder on a computer (the owner's review of 2026-11-02, V-5),
 * modelled on Samsung's My Files: a top bar of search, new folder, *+* and ⋮;
 * the folder's name and a path to tap back along; its folders and files as a
 * list or a grid; a long press selects, and a bar of what to do with the
 * selection; copy and move by going to the folder and *Copy here*. Every
 * difference between the three is the [Place] behind it.
 */
class FolderPage(
    app: MainActivity,
    private val place: Place,
    /** The page it was opened from, which the way back names at the top. */
    private val up: String,
    private val start: String = "",
) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    private var dir = start
    private var items: List<FolderItem>? = null
    private var query: String? = null
    private val selected = LinkedHashSet<String>()
    private var unreachable = false
    private var searchBox: View? = null
    private var searching: kotlinx.coroutines.Job? = null

    /** What stays above the tab bar: the selection's actions, *Copy here*, a transfer's progress. */
    private val dock = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
    override val dockBar: View = dock

    init {
        // The page draws its own top: the binding's title is not used.
        views.back.visibility = View.GONE
        views.title.visibility = View.GONE
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { load() }
    }

    override fun refresh() {
        draw()
        load()
    }

    override fun back(): Boolean = when {
        selected.isNotEmpty() -> { selected.clear(); draw(); true }
        query != null -> { query = null; draw(); load(); true }
        dir != start && dir.isNotEmpty() -> { open(dir.substringBeforeLast('/', "").let { if (it.length < start.length) start else it }); true }
        else -> false
    }

    override fun left() {
        app.hideTabs(false)
        place.left()
    }

    private fun open(to: String) {
        dir = to
        selected.clear()
        items = null
        draw()
        load()
    }

    /** The listing asked again; what was there last shows at once (point 9). */
    private fun load() {
        val q = query
        val at = dir
        if (items == null && q == null) items = Seen.of["${place.key}|$at"]
        scope.launch {
            val got = runCatching {
                withContext(Dispatchers.IO) { if (q != null) place.search(app, q) else place.list(app, at) }
            }
            if (at != dir || q != query) return@launch
            got.onSuccess {
                items = it
                unreachable = false
                if (q == null) Seen.of["${place.key}|$at"] = it
            }.onFailure {
                unreachable = true
                android.util.Log.w("qurb", "could not list ${place.title}", it)
            }
            views.refresh.isRefreshing = false
            draw()
        }
    }

    // ------------------------------------------------------------ drawing

    private fun draw() {
        val page = views.sections
        page.removeAllViews()
        if (selected.isNotEmpty()) selectionHead(page) else head(page)
        val shown = items?.let { sorted(it) }
        when {
            shown == null && unreachable -> kit.empty(page, R.drawable.ic_cloud_off, "${place.where} isn't reachable", "Try again") { load() }
            shown == null -> skeleton(page)
            else -> {
                if (unreachable) offBanner(page)
                if (shown.isEmpty()) kit.empty(page, if (query != null) R.drawable.ic_search else R.drawable.ic_folder_open,
                    if (query != null) "Nothing called “$query”" else "This folder is empty")
                else if (view() == "grid") grid(page, shown) else list(page, shown)
                if (unreachable) page.getChildAt(page.childCount - 1)?.alpha = 0.5f
            }
        }
        page.addView(View(app), LinearLayout.LayoutParams(1, kit.dp(96)))
        dockDraw()
    }

    /** The top: the way back, the four icons; the name; the path. */
    private fun head(page: LinearLayout) {
        val bar = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        val backTo = when {
            query != null -> place.title
            dir == start || dir.isEmpty() -> up
            else -> dir.substringBeforeLast('/', "").let { if (it.length < start.length || it.isEmpty()) place.title else it.substringAfterLast('/') }
        }
        bar.addView(TextView(app).apply {
            text = backTo
            setTextAppearance(R.style.Text_Quiet)
            setCompoundDrawablesRelativeWithIntrinsicBounds(icon(R.drawable.ic_arrow_left, 20), null, null, null)
            compoundDrawablePadding = kit.dp(8)
            setPadding(0, kit.dp(10), kit.dp(10), kit.dp(10))
            setOnClickListener { if (!back()) app.onBackPressedDispatcher.onBackPressed() }
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        bar.addView(iconButton(R.drawable.ic_search, "Search") {
            query = if (query == null) "" else null
            items = if (query == null) Seen.of["${place.key}|$dir"] else emptyList()
            draw()
        })
        if (place.canAdd(dir)) {
            bar.addView(iconButton(R.drawable.ic_folder_plus, "New folder") { newFolder() })
            bar.addView(iconButton(R.drawable.ic_plus, "Add", green = true) { addSheet() })
        }
        bar.addView(iconButton(R.drawable.ic_ellipsis_vertical, "Sort and view") { menu(it) })
        page.addView(bar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(4) })

        if (query != null) {
            // One field for the whole search, kept across redraws, asked once
            // the typing pauses: redrawn with each answer, it lost the caret.
            val box = searchBox ?: kit.searchField("Search ${place.title}") { typed ->
                query = typed
                searching?.cancel()
                if (typed.isBlank()) { items = emptyList(); draw(); return@searchField }
                searching = scope.launch {
                    kotlinx.coroutines.delay(300)
                    load()
                }
            }.let { (box, field) ->
                searchBox = box
                field.post { field.requestFocus() }
                box
            }
            (box.parent as? ViewGroup)?.removeView(box)
            page.addView(box, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)).apply { topMargin = kit.dp(10) })
            return
        }
        searchBox = null
        page.addView(TextView(app).apply {
            text = if (dir == start || dir.isEmpty()) place.title else dir.substringAfterLast('/')
            setTextAppearance(R.style.Text_Display)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(6) })
        page.addView(path(), LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(2) })
    }

    /** Where this is, each step tappable back to it (S3). */
    private fun path(): View {
        val row = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        val rest = dir.removePrefix(start).trim('/').split('/').filter { it.isNotEmpty() }
        val steps = listOf(place.title to start) + rest.mapIndexed { i, name ->
            name to listOf(start.trim('/')).plus(rest.take(i + 1)).filter { it.isNotEmpty() }.joinToString("/")
        }
        val scroller = android.widget.HorizontalScrollView(app).apply { isHorizontalScrollBarEnabled = false }
        for ((i, step) in steps.withIndex()) {
            if (i > 0) row.addView(ImageView(app).apply {
                setImageResource(R.drawable.ic_chevron_right)
                imageTintList = ColorStateList.valueOf(kit.color(R.color.text_3))
            }, LinearLayout.LayoutParams(kit.dp(12), kit.dp(12)).apply { marginStart = kit.dp(4); marginEnd = kit.dp(4) })
            val last = i == steps.lastIndex
            row.addView(TextView(app).apply {
                text = step.first
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(if (last) R.color.green else R.color.text_3))
                if (!last) setOnClickListener { open(step.second) }
                setPadding(0, kit.dp(4), 0, kit.dp(4))
            })
        }
        scroller.addView(row)
        scroller.post { scroller.fullScroll(View.FOCUS_RIGHT) }
        return scroller
    }

    /** Selecting (S1): how many, All, and the way out. */
    private fun selectionHead(page: LinearLayout) {
        val bar = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        bar.addView(iconButton(R.drawable.ic_x, "Stop selecting") { selected.clear(); draw() })
        bar.addView(TextView(app).apply {
            text = "${selected.size} selected"
            setTextAppearance(R.style.Text_Title)
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(8) })
        val all = items.orEmpty()
        val everything = all.isNotEmpty() && all.all { it.path in selected }
        bar.addView(LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            addView(tick(everything).apply {
                layoutParams = LinearLayout.LayoutParams(kit.dp(22), kit.dp(22)).apply { gravity = Gravity.CENTER_HORIZONTAL }
            })
            addView(TextView(app).apply {
                text = "All"
                setTextAppearance(R.style.Text_Meta)
                gravity = Gravity.CENTER
                includeFontPadding = false
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(3) })
            setOnClickListener {
                if (everything) selected.clear() else all.forEach { selected += it.path }
                draw()
            }
            setPadding(kit.dp(6), kit.dp(4), kit.dp(6), kit.dp(4))
        }, LinearLayout.LayoutParams(kit.dp(48), ViewGroup.LayoutParams.WRAP_CONTENT))
        page.addView(bar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(56)).apply { topMargin = kit.dp(4) })
        page.addView(path())
    }

    private fun offBanner(page: LinearLayout) {
        page.addView(LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = GradientDrawable().apply { cornerRadius = kit.dp(14).toFloat(); setColor(kit.color(R.color.error_bg)) }
            setPadding(kit.dp(12), kit.dp(10), kit.dp(12), kit.dp(10))
            addView(ImageView(app).apply {
                setImageResource(R.drawable.ic_cloud_off)
                imageTintList = ColorStateList.valueOf(kit.color(R.color.error))
            }, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)))
            addView(TextView(app).apply {
                text = "${place.where} is off"
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.error))
            }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(8) })
            addView(TextView(app).apply {
                text = "Try again"
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.error))
                setOnClickListener { load() }
            })
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(10) })
    }

    /** Grey rows where the list will be, the first time (point 5). */
    private fun skeleton(page: LinearLayout) {
        repeat(7) { i ->
            page.addView(LinearLayout(app).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                addView(View(app).apply { background = rounded(R.color.neutral_bg, 12) }, LinearLayout.LayoutParams(kit.dp(44), kit.dp(44)))
                addView(View(app).apply { background = rounded(R.color.neutral_bg, 6) },
                    LinearLayout.LayoutParams(kit.dp(120 + (i * 37) % 90), kit.dp(12)).apply { marginStart = kit.dp(12) })
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(62)).apply { topMargin = if (i == 0) kit.dp(12) else 0 })
        }
    }

    // --------------------------------------------------------- the listing

    private fun list(page: LinearLayout, shown: List<FolderItem>) {
        val rows = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        page.addView(rows, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(8) })
        // A folder of thousands: a hundred drawn at once, the rest as asked (point 9).
        fun more(from: Int) {
            val upTo = minOf(shown.size, from + PAGE)
            for (item in shown.subList(from, upTo)) rows.addView(row(item, shown))
            if (upTo < shown.size) {
                var link: View? = null
                link = kit.row(rows, R.drawable.ic_ellipsis, "Show ${minOf(PAGE, shown.size - upTo)} more",
                    "${Words.count(upTo.toLong())} of ${Words.files(shown.size)}") {
                    link?.let { rows.removeView(it) }
                    more(upTo)
                }.root
            }
        }
        more(0)
    }

    private fun row(item: FolderItem, shown: List<FolderItem>): View {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = kit.dp(62)
            setPadding(kit.dp(4), kit.dp(6), kit.dp(4), kit.dp(6))
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            contentDescription = item.name
        }
        val tile = FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.file_tile)
            clipToOutline = true
        }
        val picture = ImageView(app).apply {
            setImageResource(if (item.folder) R.drawable.ic_folder else States.icon(item.name))
            imageTintList = ColorStateList.valueOf(kit.color(if (item.folder) R.color.green else R.color.text_2))
        }
        tile.addView(picture, FrameLayout.LayoutParams(kit.dp(22), kit.dp(22), Gravity.CENTER))
        row.addView(tile, LinearLayout.LayoutParams(kit.dp(44), kit.dp(44)))
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = item.name
            setTextAppearance(R.style.Text_Name)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.MIDDLE
        })
        val under = if (query != null) item.path.substringBeforeLast('/', "").ifEmpty { place.title }.replace("/", " › ") else null
        if (under != null) words.addView(TextView(app).apply {
            text = under
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.START
        })
        row.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(12) })
        if (selected.isNotEmpty()) {
            row.addView(tick(item.path in selected))
        } else if (item.folder) {
            row.addView(ImageView(app).apply {
                setImageResource(R.drawable.ic_chevron_right)
                imageTintList = ColorStateList.valueOf(kit.color(R.color.text_3))
            }, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)))
        } else {
            row.addView(TextView(app).apply {
                text = Words.size(item.size)
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.text_2))
                // A Vault file kept only on a computer: a small cloud (point 6).
                if (!item.here) {
                    setCompoundDrawablesRelativeWithIntrinsicBounds(null, null, icon(R.drawable.ic_cloud, 14), null)
                    compoundDrawablePadding = kit.dp(4)
                }
            })
        }
        if (!item.folder) place.thumb(app, scope, picture, item)
        row.setOnClickListener { tap(item, shown) }
        row.setOnLongClickListener { selected += item.path; draw(); true }
        return row
    }

    /** Grid view (S5): folders and files alike as tiles, three to a row. */
    private fun grid(page: LinearLayout, shown: List<FolderItem>) {
        val gap = kit.dp(10)
        for (start in shown.indices step 3) {
            val line = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
            for (i in start until start + 3) {
                val cell = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
                line.addView(cell, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { if (i % 3 != 0) marginStart = gap })
                val item = shown.getOrNull(i) ?: continue
                val square = object : FrameLayout(app) {
                    override fun onMeasure(w: Int, h: Int) = super.onMeasure(w, w)
                }.apply {
                    background = ContextCompat.getDrawable(app, R.drawable.file_tile)
                    clipToOutline = true
                    foreground = ContextCompat.getDrawable(app, R.drawable.row_bg)
                    if (item.path in selected) foreground = rounded(R.color.green_soft, 14).apply { alpha = 150 }
                }
                val picture = ImageView(app).apply {
                    setImageResource(if (item.folder) R.drawable.ic_folder else States.icon(item.name))
                    imageTintList = ColorStateList.valueOf(kit.color(if (item.folder) R.color.green else R.color.text_2))
                }
                square.addView(picture, FrameLayout.LayoutParams(kit.dp(26), kit.dp(26), Gravity.CENTER))
                if (selected.isNotEmpty()) square.addView(tick(item.path in selected),
                    FrameLayout.LayoutParams(kit.dp(22), kit.dp(22), Gravity.END or Gravity.TOP).apply { setMargins(0, kit.dp(6), kit.dp(6), 0) })
                cell.addView(square)
                cell.addView(TextView(app).apply {
                    text = item.name
                    setTextAppearance(R.style.Text_Meta)
                    gravity = Gravity.CENTER
                    maxLines = 1
                    ellipsize = TextUtils.TruncateAt.MIDDLE
                    setPadding(0, kit.dp(6), 0, 0)
                })
                if (!item.folder) place.thumb(app, scope, picture, item)
                square.setOnClickListener { tap(item, shown) }
                square.setOnLongClickListener { selected += item.path; draw(); true }
            }
            views.sections.addView(line, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { topMargin = gap })
        }
    }

    private fun tap(item: FolderItem, shown: List<FolderItem>) {
        if (selected.isNotEmpty()) {
            if (!selected.remove(item.path)) selected += item.path
            draw()
            return
        }
        if (item.folder) {
            query = null
            open(item.path)
            return
        }
        if (!item.here) {
            keepHere(item)
            return
        }
        val files = shown.filter { !it.folder && it.here }
        val looks = files.map { f ->
            place.peek(app, scope, f).apply {
                actions = buildList {
                    add((R.drawable.ic_share_2 to "Share") to { share(listOf(f)) })
                    add((R.drawable.ic_copy to "Copy") to { clip(listOf(f), move = false) })
                    add((R.drawable.ic_info to "Details") to { details(f) })
                    if (place.canChange(dir) || place.restores) add((R.drawable.ic_trash_2 to "Delete") to { delete(listOf(f)) })
                }
            }
        }
        Peek.show(app, scope, looks, files.indexOf(item))
    }

    // ------------------------------------------------------- sort and view

    private val prefs get() = app.getSharedPreferences("folders", android.content.Context.MODE_PRIVATE)
    // Remembered for every folder, as My Files does (the owner, 2026-11-02:
    // grid chosen, then a folder opened, and it was a list again).
    private fun sort() = prefs.getString("sort", "name") ?: "name"
    private fun descending() = prefs.getBoolean("desc", false)
    private fun view() = prefs.getString("view", "list") ?: "list"

    private fun sorted(all: List<FolderItem>): List<FolderItem> {
        val (folders, files) = all.partition { it.folder }
        val byName = compareBy<FolderItem> { it.name.lowercase() }
        val order = when (sort()) {
            "date" -> compareBy<FolderItem> { it.modified }.then(byName)
            "size" -> compareBy<FolderItem> { it.size }.then(byName)
            "type" -> compareBy<FolderItem> { it.name.substringAfterLast('.', "").lowercase() }.then(byName)
            else -> byName
        }
        val d = descending()
        return folders.sortedWith(if (d && sort() == "name") byName.reversed() else byName) +
            files.sortedWith(if (d) order.reversed() else order)
    }

    /** ⋮ (S4, S5): how this folder is sorted and shown, remembered for it,
     *  in three sections as the mockup has them -- Sort by, Order, View. */
    private fun menu(anchor: View) {
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(R.color.surface, 18)
            elevation = kit.dp(12).toFloat()
            setPadding(0, kit.dp(8), 0, kit.dp(8))
        }
        val popup = android.widget.PopupWindow(box, kit.dp(220), ViewGroup.LayoutParams.WRAP_CONTENT, true).apply {
            isOutsideTouchable = true
            elevation = kit.dp(12).toFloat()
        }
        fun heading(words: String) = box.addView(TextView(app).apply {
            text = words.uppercase()
            setTextAppearance(R.style.Text_Label)
            setPadding(kit.dp(16), kit.dp(8), kit.dp(16), kit.dp(4))
        })
        fun line() = box.addView(View(app).apply { setBackgroundColor(kit.color(R.color.border)) },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(1)).apply { setMargins(0, kit.dp(6), 0, kit.dp(6)) })
        fun option(icon: Int?, words: String, on: Boolean, choose: () -> Unit) = box.addView(LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            setPadding(kit.dp(16), kit.dp(10), kit.dp(16), kit.dp(10))
            if (icon != null) addView(ImageView(app).apply {
                setImageResource(icon)
                imageTintList = ColorStateList.valueOf(kit.color(R.color.text_2))
            }, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)).apply { marginEnd = kit.dp(10) })
            addView(TextView(app).apply { text = words; setTextAppearance(R.style.Text_Body) },
                LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            if (on) addView(ImageView(app).apply {
                setImageResource(R.drawable.ic_check)
                imageTintList = ColorStateList.valueOf(kit.color(R.color.green))
            }, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)))
            setOnClickListener { choose(); popup.dismiss(); draw() }
        })
        heading("Sort by")
        for ((k, words) in listOf("name" to "Name", "date" to "Date", "size" to "Size", "type" to "Type"))
            option(null, words, sort() == k) { prefs.edit().putString("sort", k).apply() }
        line()
        heading("Order")
        option(null, "Ascending", !descending()) { prefs.edit().putBoolean("desc", false).apply() }
        option(null, "Descending", descending()) { prefs.edit().putBoolean("desc", true).apply() }
        line()
        heading("View")
        option(R.drawable.ic_list, "List", view() != "grid") { prefs.edit().putString("view", "list").apply() }
        option(R.drawable.ic_grid_four, "Grid", view() == "grid") { prefs.edit().putString("view", "grid").apply() }
        popup.showAsDropDown(anchor, -kit.dp(180), 0)
    }

    // --------------------------------------------------------- the dock

    private fun dockDraw() {
        dock.removeAllViews()
        Transfer.now?.let { dock.addView(progressCard(it)) }
        val pending = Clipboard.pending
        // Selecting: the actions take the tab bar's place, looking as it does.
        app.hideTabs(selected.isNotEmpty())
        when {
            selected.isNotEmpty() -> dock.addView(actionBar(), LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(68)))
            pending != null && place.canAdd(dir) -> dock.addView(pasteBar(pending))
        }
    }

    /** The selection's actions (S1): what this place allows, More for the rest. */
    private fun actionBar(): View {
        val chosen = items.orEmpty().filter { it.path in selected }
        // As the tab bar it stands in for: the same glass, height and labels.
        val bar = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.nav_glass)
            elevation = kit.dp(8).toFloat()
            setPadding(kit.dp(4), 0, kit.dp(4), 0)
        }
        fun act(icon: Int, words: String, danger: Boolean = false, enabled: Boolean = true, go: () -> Unit) {
            bar.addView(LinearLayout(app).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER
                alpha = if (enabled) 1f else 0.35f
                background = ContextCompat.getDrawable(app, R.drawable.row_bg)
                setPadding(0, kit.dp(8), 0, kit.dp(8))
                // Each icon over its word, both on the item's centre line: the
                // word as wide as the item and centred in it, on one line.
                addView(ImageView(app).apply {
                    setImageResource(icon)
                    scaleType = ImageView.ScaleType.FIT_CENTER
                    imageTintList = ColorStateList.valueOf(kit.color(if (danger) R.color.error else R.color.text_2))
                }, LinearLayout.LayoutParams(kit.dp(22), kit.dp(22)).apply { gravity = Gravity.CENTER_HORIZONTAL })
                addView(TextView(app).apply {
                    text = words
                    textSize = 12f
                    gravity = Gravity.CENTER
                    textAlignment = View.TEXT_ALIGNMENT_CENTER
                    maxLines = 1
                    includeFontPadding = false
                    setTextColor(kit.color(if (danger) R.color.error else R.color.text_2))
                }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(4) })
                contentDescription = words
                if (enabled) setOnClickListener { go() }
            }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f))
        }
        if (place.restores) act(R.drawable.ic_rotate_ccw, "Restore") { place.restore(app, chosen); done() }
        if (place.canChange(dir)) act(R.drawable.ic_folder_input, "Move") { clip(chosen, move = true) }
        act(R.drawable.ic_copy, "Copy") { clip(chosen, move = false) }
        act(R.drawable.ic_share_2, "Share", enabled = chosen.none { it.folder }) { share(chosen) }
        if (place.canChange(dir) || place.restores) act(R.drawable.ic_trash_2, "Delete", danger = true) { delete(chosen) }
        act(R.drawable.ic_ellipsis, "More") { more(chosen) }
        return bar
    }

    private fun done() {
        selected.clear()
        draw()
    }

    /** Copying or moving: go to a folder, then here (S2). */
    private fun pasteBar(pending: Clipboard.Pending): View {
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.chosen_bar)
            setPadding(kit.dp(16), kit.dp(12), kit.dp(16), kit.dp(14))
        }
        box.addView(TextView(app).apply {
            text = "${if (pending.move) "Moving" else "Copying"} ${Words.count(pending.clips.size.toLong())} ${if (pending.clips.size == 1) "item" else "items"} from ${pending.from}"
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_2))
        })
        val buttons = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
        buttons.addView(kit.button("Cancel", Kit.Style.SECONDARY) { Clipboard.pending = null; draw() },
            LinearLayout.LayoutParams(0, kit.dp(46), 1f))
        buttons.addView(kit.button(if (pending.move) "Move here" else "Copy here") { paste(pending) },
            LinearLayout.LayoutParams(0, kit.dp(46), 1f).apply { marginStart = kit.dp(10) })
        box.addView(buttons, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(10) })
        return box
    }

    /** One card for whatever is crossing between the phone and a computer (points 2, 8). */
    private fun progressCard(t: Transfer): View {
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.card)
            setPadding(kit.dp(14), kit.dp(12), kit.dp(14), kit.dp(12))
        }
        val top = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        top.addView(TextView(app).apply {
            text = "${t.what} ${t.done + 1} of ${t.count}"
            setTextAppearance(R.style.Text_Name)
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        top.addView(TextView(app).apply {
            text = "Cancel"
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.green))
            setOnClickListener { t.cancelled = true }
        })
        box.addView(top)
        box.addView(com.google.android.material.progressindicator.LinearProgressIndicator(app).apply {
            max = 1000
            progress = if (t.count > 0) (t.done * 1000 / t.count) else 0
            setIndicatorColor(kit.color(R.color.green))
            trackColor = kit.color(R.color.neutral_bg)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(8) })
        return LinearLayout(app).apply {
            addView(box, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { setMargins(kit.dp(4), 0, kit.dp(4), kit.dp(8)) })
        }
    }

    // --------------------------------------------------------- doing

    private fun clip(chosen: List<FolderItem>, move: Boolean) {
        val source = place
        Clipboard.pending = Clipboard.Pending(chosen.map { place.clip(it) }, move, place.title, place.key,
            chosen.map { it.path }) { a, i -> source.delete(a, chosen[i]) }
        app.say(if (move) "Go to a folder, then Move here" else "Go to a folder, then Copy here")
        done()
    }

    private fun paste(pending: Clipboard.Pending) {
        if (!Clipboard.begin()) return
        Clipboard.pending = null
        val here = dir
        val samePlace = pending.placeKey == place.key
        scope.launch {
            val t = Transfer(if (pending.move) "Moving" else "Copying", pending.clips.size)
            Transfer.now = t
            draw()
            try {
                for ((i, clip) in pending.clips.withIndex()) {
                    if (t.cancelled) break
                    t.done = i
                    dockDraw()
                    withContext(Dispatchers.IO) {
                        if (samePlace && pending.move) place.moveWithin(app, pending.paths[i], here)
                        else {
                            Clipboard.clip = clip
                            place.pasteHere(app, here)
                        }
                    }
                }
                // A move between places: the originals deleted once each is copied.
                if (pending.move && !samePlace && !t.cancelled) {
                    withContext(Dispatchers.IO) { Clipboard.deleteSources(app, pending) }
                }
                app.say(if (pending.move) "Moved" else "Copied")
            } catch (e: Exception) {
                app.fail(if (pending.move) "Not all moved" else "Not all copied", e)
            } finally {
                Clipboard.done()
                Clipboard.clip = null
                Transfer.now = null
                app.madeChange()
                load()
            }
        }
    }

    private fun share(chosen: List<FolderItem>) {
        scope.launch {
            try {
                val uris = withContext(Dispatchers.IO) { chosen.mapNotNull { place.shareUri(app, it) } }
                app.share(uris, if (chosen.size == 1) app.mimeType(chosen[0].name) else "*/*")
            } catch (e: Exception) {
                app.fail("Could not share that", e)
            }
            done()
        }
    }

    private fun delete(chosen: List<FolderItem>) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2)
            .title(if (chosen.size == 1) "Delete ${chosen[0].name}?" else "Delete ${chosen.size} items?", centred = true)
            .line(place.deletedTo)
            .buttons("Delete", danger = true) {
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) { for (item in chosen) place.delete(app, item) }
                        app.say("Deleted")
                    } catch (e: Exception) {
                        app.fail("Not deleted", e)
                    }
                    done()
                    app.madeChange()
                    load()
                }
            }
            .show()
    }

    /** What is not in the bar (S1, S6): Rename, Details, Save, Keep here. */
    private fun more(chosen: List<FolderItem>) {
        val sheet = kit.sheet()
        val one = chosen.singleOrNull()
        if (one != null && place.canChange(dir)) sheet.action(R.drawable.ic_pencil, "Rename") { rename(one) }
        if (one != null) sheet.action(R.drawable.ic_info, "Details") { details(one) }
        if (chosen.none { it.folder }) sheet.action(R.drawable.ic_download, "Save to this phone") {
            scope.launch {
                try {
                    val n = withContext(Dispatchers.IO) { chosen.count { place.save(app, it) } }
                    app.say("Saved ${Words.files(n)} to Downloads")
                } catch (e: Exception) {
                    app.fail("Not saved", e)
                }
                done()
            }
        }
        if (one != null && !one.here) sheet.action(R.drawable.ic_cloud, "Keep here") { keepHere(one); done() }
        sheet.show()
    }

    private fun details(item: FolderItem) {
        val facts = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        if (!item.folder) kit.setting(facts, R.drawable.ic_hard_drive, "Size", Words.size(item.size))
        if (item.modified > 0) kit.setting(facts, R.drawable.ic_clock, "Changed",
            java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT).format(java.util.Date(item.modified * 1000)))
        kit.setting(facts, R.drawable.ic_folder, "Where", (listOf(place.title) + item.path.removePrefix(start).trim('/').split('/').dropLast(1)).filter { it.isNotEmpty() }.joinToString(" › "))
        kit.setting(facts, R.drawable.ic_monitor_smartphone, "On", place.where)
        kit.sheet().title(item.name).view(facts, top = 8).show()
        done()
    }

    private fun rename(item: FolderItem) = kit.askName("Rename", item.name, "Rename") { to ->
        scope.launch {
            try {
                withContext(Dispatchers.IO) { place.rename(app, item, to) }
            } catch (e: Exception) {
                app.fail("Not renamed", e)
            }
            done()
            app.madeChange()
            load()
        }
    }

    private fun newFolder() = kit.askName("New folder", "", "Create") { name ->
        scope.launch {
            try {
                val path = if (dir.isEmpty()) name else "$dir/$name"
                withContext(Dispatchers.IO) { place.makeFolder(app, path) }
                open(path)
            } catch (e: Exception) {
                app.fail("Could not make that folder", e)
            }
        }
    }

    /** + (V-2, point 2): files, or a whole folder, from this phone. */
    private fun addSheet() {
        kit.sheet().title("Add to ${if (dir == start || dir.isEmpty()) place.title else dir.substringAfterLast('/')}")
            .action(R.drawable.ic_file, "Files") { app.pickFiles { uris -> add(uris.map { it to "" }) } }
            .action(R.drawable.ic_folder, "A folder") {
                app.pickFolder { tree ->
                    scope.launch {
                        val found = runCatching { withContext(Dispatchers.IO) { TreeFiles.read(app, tree) } }
                            .getOrElse { app.fail("Could not read that folder", it); return@launch }
                        add(found.second.map { it.uri to "${found.first}/${it.relative.substringBeforeLast('/', "")}".trimEnd('/') })
                    }
                }
            }
            .show()
    }

    private fun add(files: List<Pair<Uri, String>>) {
        if (files.isEmpty()) return
        val here = dir
        scope.launch {
            val t = Transfer("Adding", files.size)
            Transfer.now = t
            draw()
            var added = 0
            try {
                for ((i, f) in files.withIndex()) {
                    if (t.cancelled) break
                    t.done = i
                    dockDraw()
                    val into = listOf(here, f.second).filter { it.isNotEmpty() }.joinToString("/")
                    place.add(app, f.first, into)
                    added++
                }
                app.say(if (added == 1) "Added" else "Added ${Words.files(added)}")
            } catch (e: Exception) {
                app.fail(if (added == 0) "Not added" else "Added $added, then stopped", e)
            } finally {
                Transfer.now = null
                app.madeChange()
                load()
            }
        }
    }

    private fun keepHere(item: FolderItem) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { place.keepHere(app, item) }
                app.say("Fetching ${item.name}…")
                app.sync()
            } catch (e: Exception) {
                app.fail("Could not fetch that", e)
            }
        }
    }

    // --------------------------------------------------------- small parts

    private fun icon(res: Int, size: Int) = ContextCompat.getDrawable(app, res)?.mutate()?.apply {
        setBounds(0, 0, kit.dp(size), kit.dp(size))
        setTint(kit.color(R.color.text_2))
    }

    private fun rounded(color: Int, radius: Int) = GradientDrawable().apply {
        cornerRadius = kit.dp(radius).toFloat()
        setColor(kit.color(color))
    }

    private fun iconButton(res: Int, said: String, green: Boolean = false, go: (View) -> Unit): View =
        ImageView(app).apply {
            setImageResource(res)
            imageTintList = ColorStateList.valueOf(kit.color(if (green) R.color.green else R.color.text))
            if (green) background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(kit.color(R.color.green_soft)) }
            setPadding(kit.dp(9), kit.dp(9), kit.dp(9), kit.dp(9))
            contentDescription = said
            setOnClickListener { go(it) }
            layoutParams = LinearLayout.LayoutParams(kit.dp(40), kit.dp(40)).apply { marginStart = kit.dp(2) }
        }

    private fun tick(on: Boolean): View = FrameLayout(app).apply {
        background = ContextCompat.getDrawable(app, if (on) R.drawable.checkbox_on else R.drawable.checkbox_off)
        if (on) addView(ImageView(app).apply { setImageResource(R.drawable.ic_checkbox_tick) },
            FrameLayout.LayoutParams(kit.dp(18), kit.dp(18), Gravity.CENTER))
        layoutParams = LinearLayout.LayoutParams(kit.dp(22), kit.dp(22))
    }

    /** Each folder's listing when last seen, this run: shown at once when opened again. */
    private object Seen {
        val of = java.util.concurrent.ConcurrentHashMap<String, List<FolderItem>>()
    }

    private companion object {
        const val PAGE = 100
    }
}

/** A file or folder as the page shows it. */
class FolderItem(
    val name: String,
    /** Its path in its place: what the place is asked about it by. */
    val path: String,
    val folder: Boolean,
    val size: ULong = 0uL,
    /** Unix seconds; 0 when not known. */
    val modified: Long = 0,
    /** Its bytes on this phone, or where the place reads them; false for a
     *  Vault file kept only on a computer. */
    val here: Boolean = true,
    val entry: FileEntry? = null,
    val kept: KeptFile? = null,
)

/** Something crossing between the phone and a computer, shown above the tab bar. */
class Transfer(val what: String, val count: Int) {
    @Volatile var done = 0
    @Volatile var cancelled = false

    companion object {
        @Volatile var now: Transfer? = null
    }
}

/**
 * What is behind a folder page: where its listing comes from and what may be
 * done in it. Called off the main thread unless said otherwise.
 */
abstract class Place {
    /** The name at its top: Vault, Shared, the phone's name. */
    abstract val title: String
    /** The device it is on, as said when it is off. */
    abstract val where: String
    /** Unique to it, for remembered listings and sort. */
    abstract val key: String
    /** What the delete sheet says becomes of what is deleted. */
    abstract val deletedTo: String
    open val restores = false

    abstract fun list(app: MainActivity, dir: String): List<FolderItem>
    abstract fun search(app: MainActivity, query: String): List<FolderItem>
    /** Whether files may be added, folders made and things pasted, here. */
    abstract fun canAdd(dir: String): Boolean
    /** Whether what is here may be renamed, moved and deleted. */
    open fun canChange(dir: String) = canAdd(dir)

    abstract fun makeFolder(app: MainActivity, path: String)
    abstract fun rename(app: MainActivity, item: FolderItem, to: String)
    abstract fun delete(app: MainActivity, item: FolderItem)
    abstract fun moveWithin(app: MainActivity, from: String, toDir: String)
    abstract fun clip(item: FolderItem): Clip
    abstract fun pasteHere(app: MainActivity, dir: String)
    abstract suspend fun add(app: MainActivity, uri: Uri, into: String)
    abstract fun peek(app: MainActivity, scope: kotlinx.coroutines.CoroutineScope, item: FolderItem): Peek.Item
    abstract fun thumb(app: MainActivity, scope: kotlinx.coroutines.CoroutineScope, icon: ImageView, item: FolderItem)
    /** A file handed to another app: a URI it may read. */
    abstract fun shareUri(app: MainActivity, item: FolderItem): Uri?
    /** A copy into the phone's Downloads, in Qurb; whether one was made. */
    abstract fun save(app: MainActivity, item: FolderItem): Boolean
    open fun keepHere(app: MainActivity, item: FolderItem) {}
    open fun restore(app: MainActivity, items: List<FolderItem>) {}
    /** Gone from the screen: what it holds open, let go. */
    open fun left() {}

    protected fun downloads(name: String) = java.io.File(
        android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_DOWNLOADS), "Qurb/$name").path
}

/** This phone's Private Vault. */
class VaultPlace : Place() {
    override val title = "Vault"
    override val where = "This phone"
    override val key = "vault"
    override val deletedTo = "Deleted from this phone, and from the computers keeping your Vault"
    /** Folders made here, empty: the index knows files, not folders. */
    private val made = java.util.Collections.synchronizedSet(HashSet<String>())

    private fun item(e: FileEntry) = FolderItem(e.path.substringAfterLast('/'), e.path, false, e.size, e.modifiedAt / 1_000_000_000,
        here = States.here(e), entry = e)

    override fun list(app: MainActivity, dir: String): List<FolderItem> {
        val engine = Engine.openNow(app)
        val listing = engine.browseIn(dir, true)
        val prefix = if (dir.isEmpty()) "" else "$dir/"
        val folders = (listing.folders + made.filter { it.startsWith(prefix) && !it.removePrefix(prefix).contains('/') }
            .map { it.removePrefix(prefix) }).distinct()
        return folders.map { FolderItem(it, prefix + it, true) } + listing.files.map { item(it) }
    }

    override fun search(app: MainActivity, query: String) =
        Engine.openNow(app).searchIn(query, 500u, true).map { item(it) }

    override fun canAdd(dir: String) = true
    override fun makeFolder(app: MainActivity, path: String) {
        Engine.openNow(app).makeFolder(path)
        made += path
    }

    override fun rename(app: MainActivity, item: FolderItem, to: String) {
        val parent = item.path.substringBeforeLast('/', "")
        val target = if (parent.isEmpty()) to else "$parent/$to"
        if (item.folder) Engine.openNow(app).moveKept(item.path, target) else Engine.openNow(app).rename(item.path, target)
    }

    override fun delete(app: MainActivity, item: FolderItem) {
        if (item.folder) Engine.openNow(app).deleteKept(item.path) else Engine.openNow(app).remove(item.path)
    }

    override fun moveWithin(app: MainActivity, from: String, toDir: String) {
        val name = from.substringAfterLast('/')
        Engine.openNow(app).moveKept(from, if (toDir.isEmpty()) name else "$toDir/$name")
    }

    override fun clip(item: FolderItem) = Clip.Vault(item.path)
    override fun pasteHere(app: MainActivity, dir: String) = Clipboard.intoVault(app, Engine.openNow(app), dir)
    override suspend fun add(app: MainActivity, uri: Uri, into: String) {
        Engine.importUri(app, uri, into, true)
    }

    override fun peek(app: MainActivity, scope: kotlinx.coroutines.CoroutineScope, item: FolderItem) = Peek.local(app, item.entry!!)
    override fun thumb(app: MainActivity, scope: kotlinx.coroutines.CoroutineScope, icon: ImageView, item: FolderItem) {
        if (item.here) Thumbs.local(scope, icon, item.path, item.entry?.modifiedAt ?: 0)
    }

    override fun shareUri(app: MainActivity, item: FolderItem): Uri? = if (item.here) app.documentUri(item.path) else null
    override fun save(app: MainActivity, item: FolderItem): Boolean {
        if (!item.here) return false
        val to = java.io.File(downloads(item.name))
        to.parentFile?.mkdirs()
        Engine.openNow(app).export(item.path, to.path)
        return true
    }

    override fun keepHere(app: MainActivity, item: FolderItem) {
        Engine.openNow(app).fetch(item.path)
        Downloads.ask(item.path)
    }
}

/** A computer's folder, over the connection kept for it: its *Shared*, or
 *  this phone's backup folder there. */
abstract class ComputerPlace(protected val computer: PeerInfo) : Place() {
    override val where get() = computer.name
    @Volatile private var open: KeptFolder? = null

    protected fun folder(app: MainActivity): KeptFolder =
        open?.takeIf { true } ?: Engine.openNow(app).openKeptFolder(computer.fingerprint, 20u).also { open = it }

    protected fun item(f: KeptFile, path: String = f.path) =
        FolderItem(f.name.substringAfterLast('/'), path, false, f.size, f.modifiedAt, kept = f)

    override fun peek(app: MainActivity, scope: kotlinx.coroutines.CoroutineScope, item: FolderItem) =
        Peek.remote(app, scope, { runCatching { folder(app) }.getOrNull() }, item.kept!!)

    override fun thumb(app: MainActivity, scope: kotlinx.coroutines.CoroutineScope, icon: ImageView, item: FolderItem) {
        item.kept?.let { Thumbs.remote(scope, icon, { runCatching { folder(app) }.getOrNull() }, it) }
    }

    override fun shareUri(app: MainActivity, item: FolderItem): Uri? {
        val file = Opened.fetched(app, folder(app), item.kept ?: return null) ?: return null
        return FileProvider.getUriForFile(app, "${app.packageName}.opened", file)
    }

    override fun save(app: MainActivity, item: FolderItem): Boolean {
        folder(app).download(item.kept ?: return false, downloads(item.name))
        return true
    }

    override fun left() {
        open = null
    }
}

/** A computer's *Shared*: changed there, as a guest or the owner's phone may (decision 0073). */
class SharedPlace(computer: PeerInfo) : ComputerPlace(computer) {
    override val title = "Shared"
    override val key = "shared:${computer.fingerprint}"
    override val deletedTo = "${computer.name} keeps it in Recently deleted for 30 days"

    override fun list(app: MainActivity, dir: String): List<FolderItem> {
        val listing = folder(app).shared(dir)
        val prefix = if (dir.isEmpty()) "" else "$dir/"
        return listing.folders.map { FolderItem(it, prefix + it, true) } + listing.files.map { item(it) }
    }

    override fun search(app: MainActivity, query: String) = folder(app).search(true, query).map { item(it) }
    override fun canAdd(dir: String) = true
    override fun makeFolder(app: MainActivity, path: String) = folder(app).makeFolder(path)
    override fun rename(app: MainActivity, item: FolderItem, to: String) {
        val parent = item.path.substringBeforeLast('/', "")
        folder(app).rename(item.path, if (parent.isEmpty()) to else "$parent/$to")
    }

    override fun delete(app: MainActivity, item: FolderItem) = folder(app).delete(item.path)
    override fun moveWithin(app: MainActivity, from: String, toDir: String) {
        val name = from.substringAfterLast('/')
        folder(app).rename(from, if (toDir.isEmpty()) name else "$toDir/$name")
    }

    override fun clip(item: FolderItem) = Clip.Remote(computer, true, item.path, item.kept)
    override fun pasteHere(app: MainActivity, dir: String) =
        Clipboard.intoShared(app, Engine.openNow(app), folder(app), computer, dir)

    override suspend fun add(app: MainActivity, uri: Uri, into: String) {
        val staged = Engine.stage(app, uri)
        try {
            withContext(Dispatchers.IO) {
                folder(app).addFile(staged.file.path, if (into.isEmpty()) staged.name else "$into/${staged.name}",
                    object : Progress { override fun sent(bytes: ULong) {} })
            }
        } finally {
            staged.file.delete()
        }
    }
}

/**
 * This phone's backup folder on a computer: what it keeps of the phone's
 * folders and WhatsApp, read and restored, and its *Private Vault*, which is
 * the Vault and is changed as the Vault is (point 4).
 */
class BackupPlace(computer: PeerInfo, private val top: String) : ComputerPlace(computer) {
    override val title = top
    override val key = "backup:${computer.fingerprint}"
    override val deletedTo = "In this phone's Recently deleted for 30 days"
    override val restores = true
    private val vault = "$top/Private Vault"

    private fun inVault(dir: String) = dir == vault || dir.startsWith("$vault/")

    override fun list(app: MainActivity, dir: String): List<FolderItem> {
        val at = dir.ifEmpty { top }
        val listing = folder(app).list(at)
        return listing.folders.map { FolderItem(it, "$at/$it", true) } + listing.files.map { item(it, "$at/${it.name}") }
    }

    override fun search(app: MainActivity, query: String) = folder(app).search(false, query).map { item(it) }
    override fun canAdd(dir: String) = inVault(dir)
    override fun makeFolder(app: MainActivity, path: String) = Engine.openNow(app).makeFolder(vaultPath(path))
    private fun vaultPath(path: String) = path.removePrefix(vault).trim('/')

    override fun rename(app: MainActivity, item: FolderItem, to: String) {
        val from = item.kept?.path ?: vaultPath(item.path)
        val parent = from.substringBeforeLast('/', "")
        val target = if (parent.isEmpty()) to else "$parent/$to"
        if (item.folder) Engine.openNow(app).moveKept(from, target) else Engine.openNow(app).rename(from, target)
    }

    override fun delete(app: MainActivity, item: FolderItem) {
        Engine.openNow(app).deleteKept(item.kept?.path ?: vaultPath(item.path))
    }

    override fun moveWithin(app: MainActivity, from: String, toDir: String) {
        val name = from.substringAfterLast('/')
        val to = vaultPath(toDir)
        Engine.openNow(app).moveKept(vaultPath(from), if (to.isEmpty()) name else "$to/$name")
    }

    override fun clip(item: FolderItem): Clip =
        if (inVault(item.path)) Clip.Vault(item.kept?.path ?: vaultPath(item.path))
        else Clip.Remote(computer, false, item.path, item.kept)

    override fun pasteHere(app: MainActivity, dir: String) = Clipboard.intoVault(app, Engine.openNow(app), vaultPath(dir))
    override suspend fun add(app: MainActivity, uri: Uri, into: String) {
        Engine.importUri(app, uri, vaultPath(into), true)
    }

    override fun restore(app: MainActivity, items: List<FolderItem>) {
        val files = items.filter { !it.folder }.mapNotNull { it.kept }
        val folders = items.filter { it.folder }
        if (folders.isNotEmpty() && files.isEmpty() && folders.size == 1) {
            Restore.start(app, computer, Restore.What.Folder(folders[0].path.removePrefix("$top/")))
        } else {
            Restore.start(app, computer, Restore.What.Files(files))
        }
    }
}
