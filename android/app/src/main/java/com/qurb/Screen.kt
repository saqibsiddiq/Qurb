package com.qurb

import android.view.View
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import uniffi.qurb_mobile.Qurb

/**
 * One place in the app: one of the four under the tab bar, or one reached
 * from them -- Private Vault from Files, Activity from Home, Recently deleted
 * from Files -- which lights up the tab it belongs to and goes back to it.
 *
 * Plain classes holding their views, not Fragments. The four are built the
 * first time each is shown and kept for the life of the activity, and changing
 * tab is swapping one child view for another: no transactions, no back stack
 * beyond the few places reached from others, no state to restore beyond which
 * tab was showing. Decision 0039 asks for an app that is light, and this is
 * the least machinery that does the job.
 */
abstract class Screen(protected val app: MainActivity) {

    abstract val view: View

    /** The tab this place belongs to, and lights up while it shows. */
    abstract val tab: Int

    /**
     * Read what may have changed and redraw. Called whenever the screen comes
     * into view, when the app comes back to the front, and after anything that
     * changes what the engine knows. Never blocks: the reading happens off the
     * main thread, and what is on screen stays until the new answer is in.
     */
    abstract fun refresh()

    /**
     * Back was pressed while this screen is showing. Returns whether it used
     * it -- a folder going up -- or the app should do the usual.
     */
    open fun back(): Boolean = false

    /** Gone from the screen for good: a place reached from another, popped
     *  or left behind by a change of tab. What it holds open, it closes. */
    open fun left() {}

    protected val kit get() = app.kit

    protected val scope get() = app.lifecycleScope

    protected suspend fun engine(): Qurb = Engine.open(app)

    /** Whether a refresh is running, and whether another was asked for meanwhile. */
    private var refreshing = false
    private var refreshAgain = false

    /**
     * A refresh's work, one at a time. One asked for while another runs is
     * run once after it, however many were asked. Until 2026-10-15 each ran
     * at once: a burst of changes queued that many rounds of queries behind
     * the engine's one lock -- on the S23 a screen waited up to 1.2 s behind
     * them -- and they could finish out of order, an older answer drawn over
     * a newer one.
     */
    protected fun refreshing(work: suspend () -> Unit) {
        if (refreshing) {
            refreshAgain = true
            return
        }
        refreshing = true
        scope.launch {
            try {
                do {
                    refreshAgain = false
                    work()
                } while (refreshAgain)
            } finally {
                refreshing = false
            }
        }
    }
}
