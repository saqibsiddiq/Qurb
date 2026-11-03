package com.qurb

import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
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
import uniffi.qurb_mobile.DeletedFile

/**
 * Recently deleted (decision 0042, direction §22): files deleted on this
 * phone or on another device, kept here for thirty days. Restoring one puts
 * it back where it was: a shared file on every device, the way any change
 * travels, and a Private Vault file in the vault. Reached from Settings →
 * Manage storage.
 *
 * Drawn as the desktop's, as Photos keeps its own (the owner's mockups of
 * 2026-11-03, P-RD1 to P-RD3): the files themselves in a grid, by where they
 * were; days left only in the last week, on the picture, in amber. Restore
 * all and Delete all in a bar at the foot; Select, or a tap on a file, to
 * choose some -- the title then says how many, with × to stop and All, and
 * the bar holds Restore, green as Send is, and Delete, red.
 */
class DeletedScreen(app: MainActivity, fromSettings: Boolean = false) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    private val bar = LinearLayout(app)
    private val frame = FrameLayout(app).apply {
        addView(views.root)
        addView(bar, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
            Gravity.BOTTOM).apply {
            val side = app.resources.getDimensionPixelSize(R.dimen.gutter)
            setMargins(side, 0, side, app.resources.getDimensionPixelSize(R.dimen.nav_clearance) - app.kit.dp(64))
        })
    }
    override val view: View get() = frame
    override val tab = if (fromSettings) R.id.tab_settings else R.id.tab_files
    private val backWords = if (fromSettings) "Settings" else "Files"

    private var deleted: List<DeletedFile> = emptyList()
    /** Where each one's bytes are on this phone, for its picture. */
    private var here: Map<Long, String> = emptyMap()
    private var selecting = false
    private val chosen = mutableSetOf<Long>()

    init {
        views.back.visibility = View.VISIBLE
        // Named for where it goes back to, as every other page's is.
        views.back.text = backWords
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Recently deleted"
        views.subtitle.text = "Kept for 30 days, then deleted for good"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
        bar.apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(kit.dp(8), kit.dp(8), kit.dp(8), kit.dp(8))
            background = GradientDrawable().apply {
                cornerRadius = kit.dp(28).toFloat()
                setColor(kit.color(R.color.surface))
                setStroke(kit.dp(1), kit.color(R.color.border))
            }
            elevation = kit.dp(6).toFloat()
            visibility = View.GONE
        }
    }

    override fun refresh() {
        refreshing {
            try {
                withContext(Dispatchers.IO) {
                    val engine = engine()
                    deleted = engine.recentlyDeleted()
                    here = deleted.mapNotNull { d -> engine.deletedFile(d.id)?.let { d.id to it } }.toMap()
                }
                chosen.retainAll(deleted.map { it.id }.toSet())
                if (deleted.isEmpty()) selecting = false
                draw()
            } catch (e: Exception) {
                app.fail("Could not read Recently deleted", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun draw() {
        val page = views.sections
        page.removeAllViews()
        drawHead()
        drawBar()
        if (deleted.isEmpty()) {
            // P-RD3: nothing, said once, with how long things wait here.
            views.subtitle.visibility = View.GONE
            kit.empty(page, R.drawable.ic_trash_2, "Nothing deleted lately")
            // Its words the title, and how long things wait the quiet line
            // under them, in the same block (P-RD3).
            (page.getChildAt(page.childCount - 1) as? LinearLayout)?.let { box ->
                (box.getChildAt(1) as? TextView)?.apply {
                    setTextAppearance(R.style.Text_Name)
                    setTextColor(kit.color(R.color.text))
                }
                box.addView(TextView(app).apply {
                    text = "Deleted files wait here for 30 days"
                    setTextAppearance(R.style.Text_Meta)
                    setTextColor(kit.color(R.color.text_3))
                    gravity = Gravity.CENTER
                    setPadding(0, kit.dp(4), 0, 0)
                })
            }
            return
        }
        views.subtitle.visibility = View.VISIBLE
        // By where they were: the Vault, then the shared folder.
        for ((words, files) in listOf("Vault" to deleted.filter { it.private }, "Shared" to deleted.filter { !it.private })) {
            if (files.isEmpty()) continue
            kit.label(page, words, top = 18)
            grid(page, files)
        }
    }

    /** The title, and what is beside it: Select; or, choosing, × and All. */
    private fun drawHead() {
        val action = views.action
        if (selecting) {
            views.back.visibility = View.GONE
            views.title.text = "${chosen.size} selected"
            views.title.setCompoundDrawablesRelativeWithIntrinsicBounds(R.drawable.ic_x, 0, 0, 0)
            views.title.compoundDrawablePadding = kit.dp(14)
            views.title.compoundDrawableTintList = ColorStateList.valueOf(kit.color(R.color.text_2))
            views.title.setOnClickListener { stopChoosing() }
            views.title.contentDescription = "${chosen.size} selected. Stop choosing"
        } else {
            views.back.visibility = View.VISIBLE
            views.title.text = "Recently deleted"
            views.title.setCompoundDrawablesRelativeWithIntrinsicBounds(0, 0, 0, 0)
            views.title.setOnClickListener(null)
            views.title.isClickable = false
            views.title.contentDescription = null
        }
        action.visibility = if (deleted.isEmpty()) View.GONE else View.VISIBLE
        action.text = if (selecting) "All" else "Select"
        action.icon = null
        action.setTextColor(kit.color(R.color.text))
        action.setOnClickListener {
            if (selecting) chosen.addAll(deleted.map { it.id }) else selecting = true
            draw()
        }
    }

    private fun stopChoosing() {
        selecting = false
        chosen.clear()
        draw()
    }

    /** The bar at the foot: Restore all and Delete all; choosing, Restore
     *  and Delete for what is chosen. */
    private fun drawBar() {
        bar.removeAllViews()
        if (deleted.isEmpty()) { bar.visibility = View.GONE; return }
        bar.visibility = View.VISIBLE
        val half = { v: View ->
            bar.addView(v, LinearLayout.LayoutParams(0, kit.dp(48), 1f).apply { marginStart = if (bar.childCount > 0) kit.dp(8) else 0 })
        }
        if (selecting) {
            val some = chosen.isNotEmpty()
            half(kit.button("Restore", Kit.Style.PRIMARY) { restore(deleted.filter { it.id in chosen }) }.apply {
                setIconResource(R.drawable.ic_rotate_ccw); iconSize = kit.dp(16); isEnabled = some
            })
            half(kit.button("Delete", Kit.Style.DANGER) { forget(deleted.filter { it.id in chosen }) }.apply {
                setIconResource(R.drawable.ic_trash_2); iconSize = kit.dp(16); isEnabled = some
            })
        } else {
            half(kit.button("Restore all", Kit.Style.PRIMARY) { restore(deleted) })
            half(kit.button("Delete all", Kit.Style.SECONDARY) { empty(deleted.size) }.apply {
                setTextColor(kit.color(R.color.error))
            })
        }
    }

    /** The files, three to a row: each its picture, its name under it. */
    private fun grid(page: LinearLayout, files: List<DeletedFile>) {
        val gap = kit.dp(10)
        for (start in files.indices step 3) {
            val row = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
            for (i in start until start + 3) {
                val params = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                    if (i % 3 != 0) marginStart = gap
                }
                val d = files.getOrNull(i)
                if (d == null) { row.addView(View(app), params); continue }
                row.addView(cell(d), params)
            }
            page.addView(row, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { topMargin = gap })
        }
    }

    private fun cell(d: DeletedFile): View {
        val name = d.path.substringAfterLast('/')
        val on = d.id in chosen
        val square = Square(app).apply {
            background = GradientDrawable().apply {
                cornerRadius = kit.dp(14).toFloat()
                setColor(kit.color(R.color.surface))
                setStroke(kit.dp(if (on) 3 else 1), kit.color(if (on) R.color.green else R.color.border))
            }
            clipToOutline = true
        }
        val picture = ImageView(app).apply { importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO }
        square.addView(picture, FrameLayout.LayoutParams(kit.dp(30), kit.dp(30), Gravity.CENTER))
        val bytes = here[d.id]
        if (bytes != null) Thumbs.file(scope, picture, java.io.File(bytes)) else Thumbs.kind(scope, picture, name)
        // The days it has left, only in its last week: the last day counts
        // as one, since a 0 would read as gone.
        val days = (RETENTION_DAYS - (System.currentTimeMillis() / 1000 - d.deletedAt) / 86400).coerceAtLeast(1)
        if (days <= LAST_WEEK) square.addView(TextView(app).apply {
            text = if (days == 1L) "1 day" else "$days days"
            setTextColor(0xFFFFFFFF.toInt())
            textSize = 11.5f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT, 600, false)
            gravity = Gravity.CENTER
            setPadding(kit.dp(8), 0, kit.dp(8), 0)
            background = GradientDrawable().apply { cornerRadius = kit.dp(11).toFloat(); setColor(kit.color(R.color.attention)) }
        }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, kit.dp(22), Gravity.START or Gravity.BOTTOM)
            .apply { setMargins(kit.dp(8), 0, 0, kit.dp(8)) })
        if (selecting) square.addView(ImageView(app).apply {
            if (on) setImageResource(R.drawable.ic_check)
            imageTintList = ColorStateList.valueOf(0xFFFFFFFF.toInt())
            setPadding(kit.dp(5), kit.dp(5), kit.dp(5), kit.dp(5))
            background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                setColor(if (on) kit.color(R.color.green) else 0x33000000)
                setStroke(kit.dp(2), 0xFFFFFFFF.toInt())
            }
        }, FrameLayout.LayoutParams(kit.dp(24), kit.dp(24), Gravity.END or Gravity.TOP).apply { setMargins(0, kit.dp(8), kit.dp(8), 0) })
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            addView(square)
            addView(TextView(app).apply {
                text = name
                setTextAppearance(R.style.Text_Meta)
                textSize = 12.5f
                setTextColor(kit.color(R.color.text_2))
                maxLines = 1
                ellipsize = android.text.TextUtils.TruncateAt.END
                setPadding(kit.dp(2), kit.dp(7), kit.dp(2), 0)
            })
            contentDescription = listOfNotNull(name, if (days <= LAST_WEEK) "$days days left" else null,
                if (selecting) (if (on) "selected" else "not selected") else null).joinToString(", ")
            foreground = ContextCompat.getDrawable(app, R.drawable.row_bg)
            // A tap chooses, as a long press does in Photos; a tap while not
            // choosing starts it.
            setOnClickListener {
                selecting = true
                if (!chosen.remove(d.id)) chosen.add(d.id)
                draw()
            }
            setOnLongClickListener { choose(d); true }
        }
        return box
    }

    private fun empty(count: Int) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2)
            .title("Delete all for good?", centred = true)
            .line("${Words.files(count)}, gone for good")
            .buttons("Delete all", danger = true) {
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) { engine().emptyRecentlyDeleted() }
                        app.say("Deleted")
                    } catch (e: Exception) {
                        app.fail("Not deleted", e)
                    } finally {
                        app.changed()
                    }
                }
            }
            .show()
    }

    /** One file's sheet, from a long press: what restoring does, and both. */
    private fun choose(d: DeletedFile) {
        kit.sheet()
            // No size for one whose bytes are kept on a computer, not here.
            .header(States.icon(d.path), d.path.substringAfterLast('/'), if (d.size > 0uL) Words.size(d.size) else "")
            .text((if (d.private) "Restoring puts it back in your Private Vault."
                else "Restoring puts it back in Qurb, and it returns on your other devices at their next sync.") +
                (d.why?.let { "\n\n${it.replaceFirstChar(Char::uppercase)}." } ?: ""))
            .action(R.drawable.ic_rotate_ccw, "Restore") { restore(listOf(d)) }
            .action(R.drawable.ic_trash_2, "Delete for good", danger = true) { forget(listOf(d)) }
            .show()
    }

    private fun restore(files: List<DeletedFile>) {
        scope.launch {
            var renamed = 0
            var failed: Exception? = null
            for (d in files) {
                try {
                    val at = withContext(Dispatchers.IO) { engine().restoreDeleted(d.id) }
                    if (at != d.path) renamed++
                } catch (e: Exception) {
                    failed = e
                }
            }
            if (failed != null) app.fail("Could not restore ${if (files.size == 1) "it" else "them all"}", failed)
            else app.say(when {
                renamed > 0 -> "Restored, $renamed under a new name"
                files.all { it.private } -> "Restored to your Private Vault"
                else -> "Restored, on all your devices"
            })
            selecting = false
            chosen.clear()
            SyncWorker.runNow(app)
            app.changed()
        }
    }

    private fun forget(files: List<DeletedFile>) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2)
            .title(if (files.size == 1) "Delete for good?" else "Delete ${files.size} for good?", centred = true)
            .line("They can't be restored")
            .buttons("Delete", danger = true) {
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) { files.forEach { engine().forgetDeleted(it.id) } }
                        app.say("Deleted for good")
                    } catch (e: Exception) {
                        app.fail("Could not delete it", e)
                    } finally {
                        selecting = false
                        chosen.clear()
                        app.changed()
                    }
                }
            }
            .show()
    }

    /** As tall as it is wide. */
    private class Square(context: android.content.Context) : FrameLayout(context) {
        override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) = super.onMeasure(widthMeasureSpec, widthMeasureSpec)
    }

    private companion object {
        const val RETENTION_DAYS = 30L
        /** Days left are shown from here down. */
        const val LAST_WEEK = 7L
    }
}
