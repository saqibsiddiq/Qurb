package com.qurb

import android.text.format.DateUtils
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
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.Happening
import java.util.Calendar

/**
 * Activity, from the clock at the top of Home: everything this phone did,
 * newest first. The answer to "why is my file not here?", which a log cannot
 * give once the process that wrote it has gone (decision 0031).
 *
 * As redesigned (docs/design/redesign.md, Backups and Activity, draft 1): by
 * day, each entry the file's icon with a badge for what happened, its name,
 * the device involved by its icon and name, and the time. Why something
 * failed stays in words, under its name: that is the one thing here a person
 * needs to read.
 */
class ActivityScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    /** How far back the history goes on screen. Grows with "Show older". */
    private var shown = PAGE

    /** The chip chosen (the owner's review of 2026-11-02): all, backups, transfers, problems. */
    private var filter = "all"

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Settings"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Activity"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        refreshing {
            try {
                val history = withContext(Dispatchers.IO) {
                    if (filter == "all") engine().history(shown.toUInt(), null)
                    else engine().historyOf(filter, shown.toUInt(), null)
                }
                show(history)
            } catch (e: Exception) {
                app.fail("Could not read what happened", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    /** All, Backups, Transfers, Problems: each a question to the database. */
    private fun chips(page: LinearLayout) {
        val row = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
        for ((key, words) in listOf("all" to "All", "backups" to "Backups", "transfers" to "Transfers", "problems" to "Problems")) {
            val on = key == filter
            row.addView(TextView(app).apply {
                text = words
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(if (on) R.color.surface else R.color.text_2))
                background = android.graphics.drawable.GradientDrawable().apply {
                    cornerRadius = kit.dp(16).toFloat()
                    setColor(kit.color(if (on) R.color.green else R.color.surface))
                    if (!on) setStroke(kit.dp(1), kit.color(R.color.border))
                }
                setPadding(kit.dp(12), kit.dp(7), kit.dp(12), kit.dp(7))
                setOnClickListener { filter = key; shown = PAGE; refresh() }
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginEnd = kit.dp(6) })
        }
        page.addView(android.widget.HorizontalScrollView(app).apply { isHorizontalScrollBarEnabled = false; addView(row) },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(10) })
    }

    private fun show(history: List<Happening>) {
        val page = views.sections
        page.removeAllViews()
        chips(page)
        if (history.isEmpty()) {
            nothingYet(page)
            return
        }
        var day: String? = null
        for (h in history) {
            val label = day(h.at)
            if (label != day) {
                kit.label(page, label, top = if (day == null) 16 else 20)
                day = label
            }
            val (what, icon) = what(h)
            val local = h.device == null
            val many = h.count > 1uL
            val backup = h.path?.startsWith(".qurb-outside/") == true
            // One standing for many (a backup, a sync, a failure again): what
            // it was and how many, not one of its names.
            val name = when {
                many && backup -> if (h.device != null) "Backed up to ${h.device}" else "Backed up"
                many && h.kind == "failed" -> h.path?.substringAfterLast('/') ?: "Failed"
                many -> "${Words.files(h.count.toInt())} ${what.words}"
                else -> h.path?.substringAfterLast('/') ?: h.device ?: h.kind
            }
            val device = when {
                h.kind == "failed" && !h.detail.isNullOrBlank() -> if (many) "${h.detail}, ${h.count} times" else h.detail!!
                many && h.size != null -> "${Words.files(h.count.toInt())}, ${Words.size(h.size!!)}"
                local -> "This phone"
                else -> h.device!!
            }
            val deviceIcon = when {
                h.kind == "failed" -> null
                local -> R.drawable.ic_smartphone
                else -> States.device(h.device!!)
            }
            val row = kit.activityRow(page, icon, what, name, deviceIcon, device, time(h.at))
            if (many || h.kind == "failed") {
                row.background = ContextCompat.getDrawable(app, R.drawable.row_bg)
                row.setOnClickListener { app.push(ActivityEntryScreen(app, h, name)) }
            }
        }
        if (history.size == shown) {
            page.addView(kit.button("Show older", Kit.Style.TEXT) {
                shown += PAGE
                refresh()
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { topMargin = kit.dp(6); marginStart = -kit.dp(12) })
        }
    }

    /** What happened, as its badge, and the icon of what it happened to. */
    private fun what(h: Happening): Pair<Kit.What, Int> {
        val file = h.path?.let { States.icon(it) } ?: R.drawable.ic_file
        return when (h.kind) {
            "received" -> Kit.What.ARRIVED to file
            "sent", "collected" -> Kit.What.SENT to file
            "deleted" -> Kit.What.DELETED to file
            "stored" -> Kit.What.CHANGED to file
            "conflicted" -> Kit.What.TWO_VERSIONS to file
            "evicted" -> Kit.What.FREED to file
            "restored" -> Kit.What.RESTORED to file
            // True of a guest or a computer this phone visits as well as of
            // this person's own devices.
            "paired" -> Kit.What.PAIRED to States.device(h.device ?: "")
            "removed" -> Kit.What.STOPPED to States.device(h.device ?: "")
            "cancelled" -> Kit.What.STOPPED to file
            "failed" -> Kit.What.FAILED to file
            else -> Kit.What.CHANGED to file
        }
    }

    private fun day(at: Long): String = Words.day(at)

    /** The time of day, as the phone is set: 19:24, or 7:24 PM. */
    private fun time(at: Long): String = android.text.format.DateFormat.getTimeFormat(app).format(java.util.Date(at * 1000))

    /** Nothing has happened yet: the clock, and two words. */
    private fun nothingYet(page: LinearLayout) {
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
        }
        val disc = FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.chip)
        }
        disc.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_history)
            imageTintList = android.content.res.ColorStateList.valueOf(kit.color(R.color.text_3))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(kit.dp(40), kit.dp(40), Gravity.CENTER))
        box.addView(disc, LinearLayout.LayoutParams(kit.dp(80), kit.dp(80)))
        box.addView(TextView(app).apply {
            text = "Nothing yet"
            setTextAppearance(R.style.Text_Title)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(16) })
        page.addView(box, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(150) })
    }

    private companion object {
        /** Read at a time, as the list is scrolled to its end. */
        const val PAGE = 50
    }
}

/**
 * One entry of Activity opened (the owner's review of 2026-11-02): a big job
 * -- when, how many, how much, the names kept, then how many more -- or a
 * failure -- what, why, how many tries, when, and *Try again*.
 */
class ActivityEntryScreen(app: MainActivity, private val h: Happening, private val name: String) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Activity"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = name
        views.refresh.isEnabled = false
    }

    override fun refresh() {
        val page = views.sections
        page.removeAllViews()
        val card = kit.card(page, top = 12)
        val time = { at: Long -> android.text.format.DateFormat.getTimeFormat(app).format(java.util.Date(at * 1000)) }
        val started = h.started
        kit.setting(card, R.drawable.ic_clock, "When",
            "${Words.day(h.at)} ${if (started != null && started != h.at) "${time(started)} to ${time(h.at)}" else time(h.at)}")
        if (h.kind == "failed") {
            h.detail?.let { kit.setting(card, R.drawable.ic_info, "Why", it) }
            if (h.count > 1uL) kit.setting(card, R.drawable.ic_history, "Tried", "${h.count} times")
            page.addView(kit.button("Try again") { app.sync(); app.say("Trying again") },
                LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)).apply { topMargin = kit.dp(18) })
            return
        }
        kit.setting(card, R.drawable.ic_file, "Files", Words.count(h.count.toLong()))
        h.size?.let { kit.setting(card, R.drawable.ic_hard_drive, "Size", Words.size(it)) }
        h.device?.let { kit.setting(card, R.drawable.ic_monitor_smartphone, "With", it) }
        if (h.names.isNotEmpty()) {
            kit.label(page, "Files")
            val list = kit.card(page)
            for (n in h.names) kit.setting(list, States.icon(n), n.removePrefix(".qurb-outside/").substringAfterLast('/'))
            val more = h.count.toLong() - h.names.size
            if (more > 0) page.addView(TextView(app).apply {
                text = "and ${Words.count(more)} more"
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.text_3))
                gravity = Gravity.CENTER
                setPadding(0, kit.dp(10), 0, 0)
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
    }
}
