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
 * This phone's folders as a tree, and which are backed up to the computer
 * that keeps its Private Vault (decision 0063; the owner's list of
 * 2026-10-24, item 1): every folder opens, to any depth, each level indented
 * under the one it is in. A tick is all of a folder, a dash some of what is
 * inside it, empty none; ticking a folder takes everything inside it, folders
 * added later too, and a folder inside can be set apart again. What is
 * chosen, and that it goes over Wi-Fi only, stays above the tab bar.
 *
 * Chosen here as well as on the computer, the later choice winning wherever
 * it was made (item 10, 2026-10-14). Reached from Backups, Home and Settings.
 */
class FoldersScreen(app: MainActivity, private val from: String, override val tab: Int) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root

    /** Folders open in the tree, kept across readings. Null until the
     *  first: then those with some of what is inside chosen open, so that
     *  what is chosen deep down shows. */
    private var open: MutableSet<String>? = null

    /** The last reading, drawn again when a folder opens or closes. */
    private var last: State? = null

    private val chosenBox = checkbox(FolderTick.ALL, 18)
    private val chosenText = TextView(app).apply { setTextAppearance(R.style.Text_Name) }
    override val dockBar: View = LinearLayout(app).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        background = ContextCompat.getDrawable(app, R.drawable.chosen_bar)
        setPadding(kit.dp(16), kit.dp(12), kit.dp(16), kit.dp(12))
        visibility = View.GONE
        addView(chosenBox)
        addView(chosenText, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            .apply { marginStart = kit.dp(10) })
        addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_wifi)
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(16), kit.dp(16)).apply { marginStart = kit.dp(10) })
        addView(TextView(app).apply {
            text = "Wi-Fi only"
            setTextAppearance(R.style.Text_Meta)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = kit.dp(10) })
        layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { setMargins(kit.dp(4), 0, kit.dp(4), kit.dp(24)) }
    }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = from
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Folders"
        // The title as wide as its word, the computer's chip beside it.
        views.title.layoutParams = (views.title.layoutParams as LinearLayout.LayoutParams)
            .apply { width = ViewGroup.LayoutParams.WRAP_CONTENT; weight = 0f }
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
        page.removeAllViews()
        dockBar.visibility = View.GONE
        val row = views.title.parent as LinearLayout
        while (row.childCount > 2) row.removeViewAt(1)
        val keeper = state.keeper
        if (keeper != null) {
            row.addView(kit.deviceChip(keeper, Engine.isOff(app, keeper)), 1,
                LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                    .apply { marginStart = kit.dp(12) })
        }

        if (keeper == null) {
            kit.empty(page, R.drawable.ic_hard_drive,
                "No computer keeps this phone's Private Vault yet. Choose one in Devices, and its folders back up to it.",
                "Devices") { app.go(R.id.tab_devices) }
            return
        }
        if (!Folders.allowed(app)) {
            kit.empty(page, R.drawable.ic_hard_drive,
                "To back up its folders, Qurb needs to read this phone's files. It reads only the folders you " +
                    "choose, and lists the rest's names and sizes for ${keeper.name}.",
                "Allow") { app.allowFiles() }
            return
        }
        if (state.folders.isEmpty()) {
            kit.empty(page, R.drawable.ic_folder, "Looking at this phone's folders. Pull down in a moment.")
            return
        }

        val inside = state.folders.groupBy { it.path.substringBeforeLast('/', "") }
        val open = open ?: state.folders.filter { it.tick == FolderTick.SOME }.map { it.path }.toMutableSet()
            .also { open = it }
        val tree = kit.card(page, top = 16).apply { setPadding(0, kit.dp(6), 0, kit.dp(6)) }
        fun add(parent: String, depth: Int) {
            // What has something chosen first, then the rest, each by name.
            val here = (inside[parent] ?: return).sortedWith(
                compareBy<OwnFolder> { it.tick == FolderTick.NONE }.thenBy { it.path.lowercase() })
            for (folder in here) {
                val opens = inside.containsKey(folder.path)
                tree.addView(row(folder, depth, opens, folder.path in open, keeper))
                if (opens && folder.path in open) add(folder.path, depth + 1)
            }
        }
        add("", 0)
        // Room at the foot for the bar kept above the tab bar.
        page.addView(View(app), LinearLayout.LayoutParams(1, kit.dp(72)))

        val chosen = state.folders.filter { !it.path.contains('/') }.sumOf { it.chosenBytes }
        chosenBox.setTick(if (chosen > 0uL) FolderTick.ALL else FolderTick.NONE)
        chosenText.text = if (chosen > 0uL) "${Words.size(chosen)} chosen" else "Nothing chosen"
        dockBar.visibility = View.VISIBLE
    }

    /** One folder in the tree: the arrow that opens it, its checkbox, its
     *  icon, its name and size; indented a step for each folder it is in. */
    private fun row(folder: OwnFolder, depth: Int, opens: Boolean, isOpen: Boolean, keeper: PeerInfo): View {
        val name = folder.path.substringAfterLast('/')
        val line = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = kit.dp(52)
            setPadding(kit.dp(8 + 26 * depth), 0, kit.dp(14), 0)
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            contentDescription = "$name, ${Words.size(folder.bytes)}, " + when (folder.tick) {
                FolderTick.ALL -> "backed up"
                FolderTick.SOME -> "partly backed up"
                FolderTick.NONE -> "not backed up"
            }
        }
        val arrow = ImageView(app).apply {
            setImageResource(if (isOpen) R.drawable.ic_chevron_down else R.drawable.ic_chevron_right)
            visibility = if (opens) View.VISIBLE else View.INVISIBLE
            contentDescription = if (isOpen) "Close $name" else "Open $name"
        }
        line.addView(arrow, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)))
        val box = checkbox(folder.tick, 22).apply {
            contentDescription = if (folder.tick == FolderTick.ALL) "Stop backing up $name" else "Back up all of $name"
            setOnClickListener { choose(folder.path, folder.tick != FolderTick.ALL, keeper) }
        }
        line.addView(box, LinearLayout.LayoutParams(kit.dp(22), kit.dp(22)).apply { marginStart = kit.dp(10) })
        line.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_folder)
            // A folder at the top in green, one inside another quiet, as drawn.
            if (depth == 0) imageTintList = ColorStateList.valueOf(kit.color(R.color.green))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(20), kit.dp(20)).apply { marginStart = kit.dp(10) })
        line.addView(TextView(app).apply {
            text = name
            setTextAppearance(R.style.Text_Name)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(10) })
        line.addView(TextView(app).apply {
            text = Words.size(folder.bytes)
            setTextAppearance(R.style.Text_Meta)
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = kit.dp(10) })
        // The row opens a folder with folders in it, and ticks one without.
        val toggle = View.OnClickListener {
            val open = open ?: return@OnClickListener
            if (!open.remove(folder.path)) open.add(folder.path)
            last?.let { show(it) }
        }
        if (opens) {
            arrow.setOnClickListener(toggle)
            line.setOnClickListener(toggle)
        } else {
            line.setOnClickListener { box.performClick() }
        }
        return line
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
