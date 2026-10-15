package com.qurb

import android.view.View
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Activity, reached from Home's *See all*: everything this phone did, newest
 * first. The answer to "why is my file not here?", which a log cannot give
 * once the process that wrote it has gone (decision 0031).
 */
class ActivityScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    /** How far back the history goes on screen. Grows with "Show older". */
    private var shown = PAGE

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Home"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Activity"
        views.subtitle.text = "Everything this phone did, newest first."
        views.subtitle.visibility = View.VISIBLE
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        refreshing {
            try {
                val history = withContext(Dispatchers.IO) { engine().history(shown.toUInt(), null) }
                val page = views.sections
                page.removeAllViews()
                if (history.isEmpty()) kit.empty(page, R.drawable.ic_history, "Nothing has happened yet.")
                for (h in history) {
                    val (icon, subject, line) = Words.happened(h)
                    kit.row(page, icon, subject, line, iconTint = if (h.kind == "failed") R.color.error else R.color.text_2)
                }
                if (history.size == shown) {
                    kit.row(page, R.drawable.ic_clock, "Show older", trail = kit.chevron()) {
                        shown += PAGE
                        refresh()
                    }
                }
            } catch (e: Exception) {
                app.fail("Could not read what happened", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private companion object {
        const val PAGE = 60
    }
}
