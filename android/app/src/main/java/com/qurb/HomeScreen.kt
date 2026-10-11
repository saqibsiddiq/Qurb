package com.qurb

import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.content.res.ColorStateList
import android.view.View
import android.view.animation.LinearInterpolator
import com.qurb.databinding.ScreenHomeBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.ConflictInfo
import uniffi.qurb_mobile.ConflictSide
import uniffi.qurb_mobile.Happening
import uniffi.qurb_mobile.Outstanding
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.Usage

/**
 * Home (direction §5): is my Qurb space okay?
 *
 * One state, one action -- *Send to device*, or *Add a device* until there is
 * one -- a line of secondary facts, attention only when something needs a
 * decision, and a little that is recent. A healthy Home should be almost
 * boring. Pulling down syncs, and so does *Sync now*.
 *
 * A phone has no daemon running to ask whether it is up to date, so the state
 * is built from what it does know: whether it is syncing now, whether
 * anything made here has not reached another device, and when it last
 * reached one.
 */
class HomeScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenHomeBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    private var turning: ObjectAnimator? = null

    /** The state last drawn, to notice the moment everything becomes synced. */
    private var lastTitle: String? = null

    init {
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener {
            views.refresh.isRefreshing = false
            app.sync()
        }
        views.sync.setOnClickListener { app.sync() }
        views.mark.setOnClickListener { app.sync() }
        views.seeAll.setOnClickListener { app.push(ActivityScreen(app)) }
    }

    /** Everything the screen draws, read in one go off the main thread. */
    private class State(
        val peers: List<PeerInfo>,
        val holders: List<PeerInfo>,
        val outstanding: Outstanding,
        val usage: Usage,
        val recent: List<Happening>,
        val conflicts: List<ConflictInfo>,
    )

    override fun refresh() {
        scope.launch {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    State(
                        engine.peers(),
                        engine.holders(),
                        engine.outstanding(),
                        engine.usage(),
                        engine.history(RECENT.toUInt(), null),
                        engine.conflicts(),
                    )
                }
                show(state)
            } catch (e: Exception) {
                hero("error", R.drawable.ic_circle_alert, "Qurb can't read this phone's files", e.message ?: "")
            }
        }
    }

    private fun show(state: State) {
        val waiting = state.outstanding.files
        // The later of the app's own note and the engine's record of when each
        // device was last reached. The note began with this screen, so on a
        // phone upgraded to it, it is empty until the next sync that reaches a
        // device -- and Home said "Not synced yet" of a phone that had synced
        // for weeks. The engine has recorded every device it reached since
        // 2026-09-27.
        val synced = listOfNotNull(Engine.lastSynced(app), state.peers.mapNotNull { it.lastSeen }.maxOrNull())
            .maxOrNull()
        when {
            state.peers.isEmpty() -> hero(
                "away", R.drawable.ic_monitor_smartphone, "Add your first device",
                "Qurb keeps your files on devices you own. Connect your computer or another phone to begin.",
            )
            app.syncing -> hero("syncing", R.drawable.ic_arrow_up_down, "Syncing…", "Bringing your devices up to date.")
            waiting.isNotEmpty() -> hero(
                "away", R.drawable.ic_upload,
                if (waiting.size == 1) "1 file is waiting to reach your devices"
                else "${waiting.size} files are waiting to reach your devices",
                onlyHereSays(state),
            )
            synced == null -> hero("away", R.drawable.ic_refresh_cw, "Not synced yet", "Sync to bring your devices up to date.")
            else -> hero(
                "", R.drawable.ic_check, "Everything is synced.",
                if (state.conflicts.isNotEmpty()) {
                    "Your files are safe. ${if (state.conflicts.size == 1) "One thing needs" else "${state.conflicts.size} things need"} your attention."
                } else {
                    "Your files are safe. Nothing needs your attention."
                },
            )
        }

        if (state.peers.isEmpty()) {
            views.action.text = "Add a device"
            views.action.setIconResource(R.drawable.ic_plus)
            views.action.setOnClickListener { app.pair() }
        } else {
            views.action.text = "Send to device"
            views.action.setIconResource(R.drawable.ic_send)
            views.action.setOnClickListener { sendSomething() }
        }
        views.sync.visibility = if (state.peers.isEmpty()) View.GONE else View.VISIBLE
        views.sync.isEnabled = !app.syncing

        views.facts.text = listOfNotNull(
            "${Words.size(state.usage.logical)} of files",
            if (state.peers.isNotEmpty()) Words.devices(state.peers.size) else null,
            synced?.let { "synced ${Words.ago(it)}" },
        ).joinToString("  ·  ")

        views.attention.removeAllViews()
        showConflicts(state.conflicts)

        views.recent.removeAllViews()
        if (state.recent.isEmpty()) {
            kit.empty(views.recent, R.drawable.ic_clock, "Nothing yet. What happens between your devices shows up here.")
        }
        for (h in state.recent.take(5)) {
            val (icon, subject, line) = Words.happened(h)
            kit.row(views.recent, icon, subject, line)
        }
        views.seeAll.visibility = if (state.recent.isEmpty()) View.GONE else View.VISIBLE
    }

    /**
     * What to say about files that exist only on this phone, by which of three
     * situations it is: this phone's own files with nobody chosen to keep
     * them; or a device that will take them and has not been reached yet.
     */
    private fun onlyHereSays(state: State): String {
        val ownWithNoKeeper = state.outstanding.files.any { it.private } && state.holders.isEmpty()
        return if (ownWithNoKeeper) {
            "They're in your Private Vault, and no device is keeping a backup yet. Choose one in Devices."
        } else {
            val going = (state.holders.ifEmpty { state.peers }).map { it.name }
            val to = Words.list(going)
            if (going.size == 1) "Until then they're only on this phone. They go to $to the next time both are online."
            else "Until then they're only on this phone. They go to $to, each the next time it and this phone are online."
        }
    }

    /** The state, one action's worth of context, and the mark that shows it. */
    private fun hero(mark: String, icon: Int, title: String, says: String) {
        val tint = when (mark) {
            "syncing" -> R.color.green
            "away" -> R.color.neutral
            "attention" -> R.color.attention
            "error" -> R.color.error
            else -> R.color.healthy
        }
        views.markIcon.setImageResource(icon)
        views.markIcon.imageTintList = ColorStateList.valueOf(kit.color(tint))
        views.state.text = title
        views.stateSays.text = says

        // §32: a light travels round the mark while syncing -- not a spinner.
        val moving = mark == "syncing" && !Kit.calm()
        views.markArc.visibility = if (moving) View.VISIBLE else View.GONE
        if (moving && turning == null) {
            turning = ObjectAnimator.ofFloat(views.markArc, View.ROTATION, 0f, 360f).apply {
                duration = 1600
                repeatCount = ValueAnimator.INFINITE
                interpolator = LinearInterpolator()
                start()
            }
        } else if (!moving) {
            turning?.cancel()
            turning = null
        }

        // §33: when everything becomes synced, the mark settles with one soft
        // pulse. Then nothing moves.
        if (title == "Everything is synced." && lastTitle != null && lastTitle != title && !Kit.calm()) {
            views.mark.scaleX = 0.92f
            views.mark.scaleY = 0.92f
            views.mark.animate().scaleX(1f).scaleY(1f).setDuration(600)
                .setInterpolator(android.view.animation.OvershootInterpolator(2f)).start()
        }
        lastTitle = title
    }

    /** Send to device: which files, then which device. */
    private fun sendSomething() {
        app.chooseDevice("Send to") { peer -> app.pickFilesToSend(peer) }
    }

    /** Two versions of a file (§14): attention, and a review. */
    private fun showConflicts(conflicts: List<ConflictInfo>) {
        if (conflicts.isEmpty()) return
        val title = if (conflicts.size == 1) "1 thing needs attention" else "${conflicts.size} things need attention"
        val says = if (conflicts.size == 1) {
            "${conflicts[0].path.substringAfterLast('/')} has two versions."
        } else {
            "${conflicts.size} files have two versions."
        }
        kit.attention(views.attention, R.drawable.ic_git_compare, title, says, "Review") {
            if (conflicts.size == 1) review(conflicts[0]) else choose(conflicts)
        }
    }

    private fun choose(conflicts: List<ConflictInfo>) {
        val sheet = kit.sheet().header(R.drawable.ic_git_compare, "Two versions",
            "Both are kept. Nothing is lost whichever you choose.")
        for (c in conflicts) {
            sheet.action(States.icon(c.path), c.path.substringAfterLast('/')) { review(c) }
        }
        sheet.show()
    }

    /**
     * One conflict: both versions described, three choices (§14). Whichever
     * is not kept goes to Recently deleted, so no choice here loses anything.
     */
    private fun review(c: ConflictInfo) {
        fun describe(s: ConflictSide?) = if (s == null) "Since deleted or renamed" else
            "${s.by}  ·  ${Words.ago(s.changedAt)}  ·  ${Words.size(s.size)}" +
                if (s.here) "" else "\nNot on this phone yet"
        val sheet = kit.sheet()
            .header(States.icon(c.path), c.path.substringAfterLast('/'), "Two devices changed it at the same time")
        val versions = android.widget.LinearLayout(app).apply {
            orientation = android.widget.LinearLayout.VERTICAL
        }
        val group = kit.group(versions)
        kit.item(group, "This version", describe(c.`this`))
        kit.item(group, "The other version", describe(c.other))
        sheet.view(versions, top = 16)
        // What each looks like, side by side, where it can be shown (§2).
        val looks = android.widget.LinearLayout(app).apply {
            orientation = android.widget.LinearLayout.HORIZONTAL
            visibility = android.view.View.GONE
        }
        sheet.view(looks, top = 12)
        scope.launch {
            val sides = listOf(c.`this`, c.other).map { side ->
                if (side == null || !side.here) null
                else withContext(Dispatchers.IO) { runCatching { Previews.of(app, side.path) }.getOrNull() }
            }
            if (sides.all { it == null }) return@launch
            sides.forEachIndexed { i, look ->
                val frame = android.widget.FrameLayout(app).apply {
                    background = androidx.core.content.ContextCompat.getDrawable(app, R.drawable.tile)
                    clipToOutline = true
                    if (look != null) addView(Previews.view(app, look))
                }
                looks.addView(frame, android.widget.LinearLayout.LayoutParams(0, kit.dp(130), 1f).apply {
                    if (i == 0) marginEnd = kit.dp(6) else marginStart = kit.dp(6)
                })
            }
            looks.visibility = android.view.View.VISIBLE
        }
        sheet.text("Whichever you don't keep goes to Recently deleted for 30 days.")
        sheet.action(R.drawable.ic_check, "Keep this version") { settle(c, "this") }
        // Keeping the other one, or both, needs its bytes here.
        if (c.other.here) {
            sheet.action(R.drawable.ic_git_compare, "Keep the other version") { settle(c, "other") }
            sheet.action(R.drawable.ic_copy, "Keep both") { settle(c, "both") }
        }
        sheet.show()
    }

    private fun settle(c: ConflictInfo, keep: String) {
        scope.launch {
            try {
                val kept = withContext(Dispatchers.IO) { engine().settleConflict(c.other.path, keep) }
                app.say(if (keep == "both") "Kept both" else "Kept ${kept.substringAfterLast('/')}")
                SyncWorker.runNow(app)
            } catch (e: Exception) {
                app.fail("Could not settle that", e)
            } finally {
                app.changed()
            }
        }
    }

    private companion object {
        /** How many recent events Home reads; five are shown. */
        const val RECENT = 6
    }
}
