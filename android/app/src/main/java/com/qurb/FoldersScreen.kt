package com.qurb

import android.content.res.ColorStateList
import android.text.TextUtils
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.FolderTick
import uniffi.qurb_mobile.OwnFolder
import uniffi.qurb_mobile.PeerInfo

/**
 * Which of this phone's folders back up (decision 0063), as a file explorer
 * shows them (the owner's review of 2026-11-02, F-1 to F-4): one page for each
 * folder, its folders as rows -- name, size, a tick, and an arrow to open it
 * -- a path at the top, a search through every folder, and *Select all* for
 * the folder on screen. A tick is all of a folder, a dash some of what is
 * inside it, empty none; ticking a folder takes everything inside it, folders
 * added later too. What is chosen, and that it goes over Wi-Fi only, stays
 * above the tab bar. Reached from a computer's page (Home's circle).
 */
class FoldersScreen(
    app: MainActivity,
    private val from: String,
    override val tab: Int,
    /** The folder on screen, from the root of the phone's storage; "" for the top. */
    private val dir: String = "",
) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root

    private var query = ""
    private var last: State? = null

    private val chosenText = TextView(app).apply { setTextAppearance(R.style.Text_Name) }
    override val dockBar: View = LinearLayout(app).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        background = ContextCompat.getDrawable(app, R.drawable.chosen_bar)
        setPadding(kit.dp(16), kit.dp(12), kit.dp(16), kit.dp(12))
        visibility = View.GONE
        addView(chosenText, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_wifi)
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(16), kit.dp(16)).apply { marginStart = kit.dp(10) })
        addView(TextView(app).apply {
            text = "Wi-Fi only"
            setTextAppearance(R.style.Text_Meta)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = kit.dp(8) })
        layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { setMargins(kit.dp(4), 0, kit.dp(4), kit.dp(24)) }
    }

    /** The search field and the rows under it, kept apart so typing does not redraw the field. */
    private val listed = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
    private var searchAdded = false

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = when {
            dir.isEmpty() -> from
            dir.contains('/') -> dir.substringBeforeLast('/').substringAfterLast('/')
            else -> "Folders"
        }
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = if (dir.isEmpty()) "Folders" else dir.substringAfterLast('/')
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(val folders: List<OwnFolder>, val keeper: PeerInfo?)

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    // Never listed yet, with the access there: list now, so that
                    // the first look is not empty.
                    if (Folders.allowed(app) && engine.ownFolders().isEmpty()) {
                        runCatching { Folders.run(app, engine, 0u, listNow = true) }
                    }
                    State(engine.ownFolders(), engine.holders().firstOrNull())
                }
                show(state)
            } catch (e: Exception) {
                app.fail("Could not read this phone's folders", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(state: State) {
        last = state
        val page = views.sections
        dockBar.visibility = View.GONE
        val keeper = state.keeper
        if (keeper == null || !Folders.allowed(app) || state.folders.isEmpty()) {
            page.removeAllViews()
            searchAdded = false
            when {
                keeper == null -> kit.empty(page, R.drawable.ic_hard_drive, "Choose a computer to back up to first",
                    "Choose") { app.openDevice() }
                !Folders.allowed(app) -> kit.empty(page, R.drawable.ic_hard_drive,
                    "QurbSpace needs to read this phone's files to back up its folders. It reads only the folders you choose.",
                    "Allow") { app.allowFiles() }
                else -> kit.empty(page, R.drawable.ic_folder, "Looking at this phone's folders. Pull down in a moment.")
            }
            return
        }
        if (!searchAdded) {
            page.removeAllViews()
            // Where this is, from the phone's top (F-3).
            page.addView(TextView(app).apply {
                text = (listOf("Phone") + dir.split('/').filter { it.isNotEmpty() }).joinToString("  ›  ")
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.text_3))
            })
            val (box, _) = kit.searchField("Search folders") { typed ->
                query = typed.trim()
                last?.let { draw(it, keeper) }
            }
            page.addView(box, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)).apply { topMargin = kit.dp(10) })
            page.addView(listed, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { topMargin = kit.dp(10) })
            // Room at the foot for the bar kept above the tab bar.
            page.addView(View(app), LinearLayout.LayoutParams(1, kit.dp(72)))
            searchAdded = true
        }
        draw(state, keeper)
        val chosen = state.folders.filter { !it.path.contains('/') }.sumOf { it.chosenBytes }
        chosenText.text = if (chosen > 0uL) "${Words.size(chosen)} chosen" else "Nothing chosen"
        dockBar.visibility = View.VISIBLE
    }

    /** The folders here, or those whose names match the search at any depth. */
    private fun draw(state: State, keeper: PeerInfo) {
        listed.removeAllViews()
        val parentOf = { path: String -> path.substringBeforeLast('/', "") }
        val hasInside = state.folders.mapTo(HashSet()) { parentOf(it.path) }
        val here = if (query.isEmpty()) state.folders.filter { parentOf(it.path) == dir }
            else state.folders.filter { it.path.substringAfterLast('/').contains(query, ignoreCase = true) }
        if (here.isEmpty()) {
            kit.empty(listed, if (query.isEmpty()) R.drawable.ic_folder_open else R.drawable.ic_search,
                if (query.isEmpty()) "No folders in here" else "Nothing called “$query”")
            return
        }
        val card = kit.card(listed).apply { setPadding(0, kit.dp(4), 0, kit.dp(4)) }
        // Select all, for the folders on screen (F-2).
        if (query.isEmpty()) {
            val all = when {
                here.all { it.tick == FolderTick.ALL } -> FolderTick.ALL
                here.any { it.tick != FolderTick.NONE } -> FolderTick.SOME
                else -> FolderTick.NONE
            }
            card.addView(line(null, "Select all", null, all, opens = false, keeper = keeper) {
                chooseAll(here.map { it.path }, all != FolderTick.ALL, keeper)
            })
        }
        for (folder in here.sortedBy { it.path.substringAfterLast('/').lowercase() }) {
            val opens = folder.path in hasInside
            card.addView(line(folder, folder.path.substringAfterLast('/'),
                if (query.isEmpty()) Words.size(folder.bytes) else "${folder.path.substringBeforeLast('/', "Phone").replace("/", " › ")}  ${Words.size(folder.bytes)}",
                folder.tick, opens, keeper) { choose(folder.path, folder.tick != FolderTick.ALL, keeper) })
        }
    }

    /** A row: the folder's icon, name and size, its tick, and an arrow when
     *  there are folders inside it -- the row then opens it, the tick chooses. */
    private fun line(folder: OwnFolder?, name: String, under: String?, tick: FolderTick, opens: Boolean,
                     keeper: PeerInfo, onTick: () -> Unit): View {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = kit.dp(60)
            setPadding(kit.dp(14), kit.dp(6), kit.dp(10), kit.dp(6))
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            contentDescription = "$name, " + when (tick) {
                FolderTick.ALL -> "backed up"
                FolderTick.SOME -> "partly backed up"
                FolderTick.NONE -> "not backed up"
            }
        }
        row.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_folder)
            imageTintList = ColorStateList.valueOf(kit.color(if (folder == null) R.color.text_3 else R.color.green))
            visibility = if (folder == null) View.INVISIBLE else View.VISIBLE
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(22), kit.dp(22)))
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = name
            setTextAppearance(R.style.Text_Name)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        })
        if (under != null) words.addView(TextView(app).apply {
            text = under
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.START
        })
        row.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(14) })
        val box = checkbox(tick, 22).apply {
            contentDescription = if (tick == FolderTick.ALL) "Stop backing up $name" else "Back up all of $name"
            setOnClickListener { onTick() }
        }
        row.addView(box, LinearLayout.LayoutParams(kit.dp(22), kit.dp(22)).apply { marginStart = kit.dp(10) })
        row.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_chevron_right)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.text_3))
            visibility = if (opens) View.VISIBLE else View.INVISIBLE
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)).apply { marginStart = kit.dp(8) })
        row.setOnClickListener {
            if (opens && folder != null) app.push(FoldersScreen(app, from, tab, folder.path)) else onTick()
        }
        return row
    }

    private fun chooseAll(paths: List<String>, on: Boolean, keeper: PeerInfo) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { val engine = engine(); for (p in paths) engine.chooseFolder(p, on) }
                if (on) WhatsApp.runNow(app)
                SyncWorker.runNow(app)
                app.say(if (on) "${Words.count(paths.size.toLong())} folders back up to ${keeper.name} on Wi-Fi" else "Nothing here backs up now")
            } catch (e: Exception) {
                app.fail("Could not choose those", e)
            }
            refresh()
        }
    }

    /** The tree's checkbox: ticked, a dash, or empty (the redesign's Checkbox). */
    private fun checkbox(tick: FolderTick, size: Int) = FrameLayout(app).apply {
        layoutParams = LinearLayout.LayoutParams(kit.dp(size), kit.dp(size))
        addView(ImageView(app).apply { importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO },
            FrameLayout.LayoutParams(kit.dp(size * 18 / 22), kit.dp(size * 18 / 22), Gravity.CENTER))
        setTick(tick)
    }

    private fun FrameLayout.setTick(tick: FolderTick) {
        background = ContextCompat.getDrawable(app,
            if (tick == FolderTick.NONE) R.drawable.checkbox_off else R.drawable.checkbox_on)
        (getChildAt(0) as ImageView).setImageResource(when (tick) {
            FolderTick.ALL -> R.drawable.ic_checkbox_tick
            FolderTick.SOME -> R.drawable.ic_checkbox_dash
            FolderTick.NONE -> 0
        })
    }

    private fun choose(path: String, on: Boolean, keeper: PeerInfo) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { engine().chooseFolder(path, on) }
                // Started now rather than at the next background pass, and the
                // computer told at the next sync.
                if (on) WhatsApp.runNow(app)
                SyncWorker.runNow(app)
                app.say(if (on) "${path.substringAfterLast('/')} backs up to ${keeper.name} on Wi-Fi"
                    else "${path.substringAfterLast('/')} backs up nothing new")
                refresh()
            } catch (e: Exception) {
                app.fail("Could not choose $path", e)
                refresh()
            }
        }
    }
}
