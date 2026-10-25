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
                val history = withContext(Dispatchers.IO) { engine().history(shown.toUInt(), null) }
                show(history)
            } catch (e: Exception) {
                app.fail("Could not read what happened", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(history: List<Happening>) {
        val page = views.sections
        page.removeAllViews()
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
            val device = when {
                h.kind == "failed" && !h.detail.isNullOrBlank() -> h.detail!!
                local -> "This phone"
                else -> h.device!!
            }
            val deviceIcon = when {
                h.kind == "failed" -> null
                local -> R.drawable.ic_smartphone
                else -> States.device(h.device!!)
            }
            val name = h.path?.substringAfterLast('/') ?: h.device ?: h.kind
            kit.activityRow(page, icon, what, name, deviceIcon, device, time(h.at))
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

    /** Today, Yesterday, or the day's date. */
    private fun day(at: Long): String {
        val then = Calendar.getInstance().apply { timeInMillis = at * 1000 }
        val now = Calendar.getInstance()
        fun same(a: Calendar, b: Calendar) =
            a.get(Calendar.YEAR) == b.get(Calendar.YEAR) && a.get(Calendar.DAY_OF_YEAR) == b.get(Calendar.DAY_OF_YEAR)
        if (same(then, now)) return "Today"
        now.add(Calendar.DAY_OF_YEAR, -1)
        if (same(then, now)) return "Yesterday"
        return DateUtils.formatDateTime(app, at * 1000,
            DateUtils.FORMAT_SHOW_WEEKDAY or DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_ABBREV_ALL)
    }

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
        const val PAGE = 60
    }
}
