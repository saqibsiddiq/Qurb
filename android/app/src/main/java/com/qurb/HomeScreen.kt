package com.qurb

import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import com.qurb.databinding.ScreenHomeBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import uniffi.qurb_mobile.BackupWaiting
import uniffi.qurb_mobile.ConflictInfo
import uniffi.qurb_mobile.ConflictSide
import uniffi.qurb_mobile.Outstanding
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.Waiting

/**
 * Home, as redesigned (docs/design/redesign.md, Home draft 3; decision 0073,
 * item 5): is the device I chose up to date?
 *
 * One device is shown, the one chosen here: its state as a large mark in the
 * middle of the screen and two or three words, the device as a chip that
 * opens the list to choose another, then Sync and Send, of one size. Sync
 * syncs every device; Send goes to the chosen one. What is still on its way
 * shows under them, and only while something is (the owner's item 6,
 * 2026-10-23). Backups and Activity are buttons at the top. Pulling down
 * syncs too.
 *
 * Words are few (the owner, 2026-10-23: "i want to convey the information
 * while keeping minimum texts"): which device and whether it is on is the
 * chip and its dot, a count is a badge, how far a backup has got is a bar.
 *
 * A phone has no daemon running to ask whether it is up to date, so the state
 * is built from what it does know: whether it is syncing now, two versions of
 * a file, whether the chosen device answered the last pass that tried it
 * ([Engine.isOff]), and whether anything is still on its way.
 */
class HomeScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenHomeBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    /** The state last drawn, to notice the moment everything becomes synced. */
    private var lastTitle: String? = null

    /** The last reading, and the redraw every two seconds while a backup is
     *  going, so that its bar moves as the computer collects. */
    private var last: State? = null
    private var ticking: Job? = null

    init {
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener {
            views.refresh.isRefreshing = false
            app.sync()
        }
        views.sync.setOnClickListener { app.sync() }
        views.mark.setOnClickListener { app.sync() }
        views.add.setOnClickListener { app.pair() }
        views.backups.setOnClickListener { app.push(BackupsScreen(app)) }
        views.activity.setOnClickListener { app.push(ActivityScreen(app)) }
    }

    /** Everything the screen draws, read in one go off the main thread. */
    private class State(
        val peers: List<PeerInfo>,
        val holders: List<PeerInfo>,
        val outstanding: Outstanding,
        val conflicts: List<ConflictInfo>,
        /** Sent from here, not collected yet. */
        val sends: List<Waiting>,
        /** Both backups' files the computer does not have yet. */
        val backup: BackupWaiting,
        /** Bytes this phone has handed to collecting devices, for the bar. */
        val served: ULong,
        /** The file a collecting device is taking now, by name. */
        val going: String?,
    ) {
        fun with(backup: BackupWaiting, served: ULong, going: String?) =
            State(peers, holders, outstanding, conflicts, sends, backup, served, going)
    }

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    State(
                        engine.peers(),
                        engine.holders(),
                        engine.outstanding(),
                        engine.conflicts(),
                        runCatching { engine.waiting() }.getOrDefault(emptyList()),
                        engine.backupWaiting(WAITING_ROWS.toUInt()),
                        runCatching { engine.serving().bytes }.getOrDefault(0uL),
                        runCatching { engine.servingFile() }.getOrNull(),
                    )
                }
                show(state)
            } catch (e: Exception) {
                hero(StatusMark.Look.ERROR, R.drawable.mark_circle_alert, "Can't read this phone's files")
                app.fail("Qurb can't read this phone's files", e)
            }
        }
    }

    private fun show(state: State) {
        last = state
        // Every two seconds while Home is on screen, the backup read again
        // and drawn again when it has moved: a backup that starts after Home
        // was drawn shows, and its bar moves as the computer collects.
        if (ticking == null) ticking = scope.launch {
            while (view.isAttachedToWindow) {
                delay(2_000)
                val now = last ?: continue
                val fresh = runCatching {
                    withContext(Dispatchers.IO) {
                        val engine = engine()
                        now.with(engine.backupWaiting(WAITING_ROWS.toUInt()), engine.serving().bytes,
                            runCatching { engine.servingFile() }.getOrNull())
                    }
                }.getOrNull() ?: continue
                val moved = fresh.backup.filesLeft != now.backup.filesLeft ||
                    fresh.backup.bytesLeft != now.backup.bytesLeft || fresh.going != now.going ||
                    (backupsLeft(fresh) > 0 && fresh.served != now.served)
                if (moved) show(fresh)
            }
            ticking = null
        }
        val chosen = Engine.chosen(app, state.peers, state.holders)
        val off = chosen != null && Engine.isOff(app, chosen)
        // The later of the app's own note and the engine's record of when each
        // device was last reached: the note began on 2026-10-14, the engine's
        // record on 2026-09-27.
        val synced = listOfNotNull(Engine.lastSynced(app), state.peers.mapNotNull { it.lastSeen }.maxOrNull())
            .maxOrNull()
        views.markBadge.removeAllViews()
        views.review.visibility = View.GONE
        when {
            chosen == null -> hero(StatusMark.Look.FIRST, R.drawable.mark_plus, "Add a device")
            app.syncing -> hero(StatusMark.Look.SYNCING, R.drawable.mark_refresh_cw, "Syncing")
            state.conflicts.isNotEmpty() -> {
                hero(StatusMark.Look.NEEDS_YOU, R.drawable.mark_git_compare, "Two versions")
                views.markBadge.addView(kit.badge(state.conflicts.size, Kit.Tone.ATTENTION))
                views.review.visibility = View.VISIBLE
                views.review.setOnClickListener {
                    if (state.conflicts.size == 1) review(state.conflicts[0]) else choose(state.conflicts)
                }
            }
            off -> hero(StatusMark.Look.OFF, R.drawable.mark_cloud_off, "${chosen.name} is off")
            // Backups and sends on their way are not "all synced": on
            // 2026-10-15 the S23 said so above 130 WhatsApp files still to go.
            backupsLeft(state) > 0 -> hero(StatusMark.Look.WAITING, R.drawable.mark_hard_drive, "Backing up")
            state.outstanding.files.isNotEmpty() || synced == null ->
                hero(StatusMark.Look.WAITING, R.drawable.mark_upload, "Not synced yet")
            state.sends.isNotEmpty() ->
                hero(StatusMark.Look.WAITING, R.drawable.mark_upload, "Waiting for ${Words.list(state.sends.map { it.to }.distinct())}")
            else -> hero(StatusMark.Look.SYNCED, R.drawable.mark_check, SYNCED)
        }

        views.device.removeAllViews()
        if (chosen != null) views.device.addView(kit.deviceChip(chosen, off) { kit.chooseDevice(state.peers, chosen) { refresh() } })
        views.device.visibility = if (chosen == null) View.GONE else View.VISIBLE
        views.actions.visibility = if (chosen == null) View.GONE else View.VISIBLE
        views.add.visibility = if (chosen == null) View.VISIBLE else View.GONE
        views.sync.isEnabled = !app.syncing
        views.send.setOnClickListener { if (chosen != null) app.pickFilesToSend(chosen) }
        views.send.contentDescription = chosen?.let { "Send to ${it.name}" }

        val queued = chosen != null && showWaiting(state)
        // In the middle of the screen while it is the only thing on it; near
        // the top, and smaller, while a queue shows under it.
        val size = kit.dp(if (queued) 128 else 168)
        views.markBox.layoutParams = views.markBox.layoutParams.apply { width = size; height = size }
        for (space in listOf(views.above, views.below)) {
            space.layoutParams = (space.layoutParams as LinearLayout.LayoutParams).apply {
                weight = if (queued) 0f else 1f
                height = if (queued && space === views.above) kit.dp(16) else 0
            }
        }
    }

    /**
     * What is still on its way (the owner's item 6): files made here that no
     * other device has, sends not collected, and backups still going, each a
     * row with its size, a backup with a bar of how far it has got. Returns
     * whether there was anything; with nothing, the section is not shown.
     */
    private fun showWaiting(state: State): Boolean {
        val box = views.waiting
        box.removeAllViews()
        val files = state.outstanding.files
        val backupLeft = backupsLeft(state)
        val count = files.size + state.sends.size + backupLeft
        box.visibility = if (count == 0) View.GONE else View.VISIBLE
        if (count == 0) return false

        val heading = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = android.view.Gravity.CENTER_VERTICAL
        }
        heading.addView(TextView(app).apply {
            text = "Waiting"
            setTextAppearance(R.style.Text_Label)
            androidx.core.view.ViewCompat.setAccessibilityHeading(this, true)
        })
        heading.addView(kit.badge(count), LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { marginStart = kit.dp(8) })
        box.addView(heading)

        val queue = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = androidx.core.content.ContextCompat.getDrawable(app, R.drawable.card)
            clipToOutline = true
        }
        box.addView(queue, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = kit.dp(8) })

        for (f in files.take(WAITING_ROWS)) {
            kit.queueRow(queue, States.icon(f.path), f.path.substringAfterLast('/'), Words.size(f.size))
        }
        if (files.size > WAITING_ROWS) {
            kit.queueRow(queue, R.drawable.ic_upload, "${files.size - WAITING_ROWS} more",
                Words.size(files.drop(WAITING_ROWS).sumOf { it.size }))
        }
        for (w in state.sends.take(WAITING_ROWS)) {
            kit.queueRow(queue, R.drawable.ic_send, w.path.substringAfterLast('/'), Words.size(w.size))
        }
        if (state.sends.size > WAITING_ROWS) {
            kit.queueRow(queue, R.drawable.ic_send, "${state.sends.size - WAITING_ROWS} more",
                Words.size(state.sends.drop(WAITING_ROWS).sumOf { it.size }))
        }
        // A backup going: where to, how far this round has got and how much
        // it is, then the files themselves, each with its folder and size
        // (the owner's list of 2026-10-25, which had only *Folders, 18 files*
        // and a bar measured against the whole backup, always full).
        if (backupLeft > 0) {
            val round = BackingUp.round(app, state.backup, state.served, state.going)
            val to = state.holders.firstOrNull()?.name
            // The title and what is going given the row's whole width; how
            // much under the bar, where a size at the end cut both short.
            kit.queueRow(queue, R.drawable.ic_hard_drive, if (to != null) "Backing up to $to" else "Backing up",
                "", round.fraction, detail = round.what, after = round.sizes)
            val named = state.backup.files.take(WAITING_ROWS)
            for (f in named) {
                kit.queueRow(queue, States.icon(f.name), f.name, Words.size(f.size), detail = f.folder.ifEmpty { null })
            }
            val rest = backupLeft - named.size
            if (rest > 0) {
                kit.queueRow(queue, R.drawable.ic_folder, "${Words.count(rest.toLong())} more",
                    Words.size(state.backup.bytesLeft - named.sumOf { it.size }))
            }
        }
        return true
    }

    /** Files of the WhatsApp and folder backups still to go, when a computer keeps them. */
    private fun backupsLeft(state: State): Int =
        if (state.holders.isEmpty()) 0 else state.backup.filesLeft.toInt()

    /** The state: its mark, and its two or three words. */
    private fun hero(look: StatusMark.Look, icon: Int, title: String) {
        views.mark.show(look, icon)
        views.mark.contentDescription = "$title. Sync"
        views.state.text = title

        // §33: when everything becomes synced, the mark settles with one soft
        // pulse. Then nothing moves.
        if (title == SYNCED && lastTitle != null && lastTitle != title && !Kit.calm()) {
            views.markBox.scaleX = 0.92f
            views.markBox.scaleY = 0.92f
            views.markBox.animate().scaleX(1f).scaleY(1f).setDuration(600)
                .setInterpolator(android.view.animation.OvershootInterpolator(2f)).start()
        }
        lastTitle = title
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
            "${s.by}, ${Words.ago(s.changedAt)}\n${Words.size(s.size)}" +
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
        // Keeping the other one, or both, needs its bytes here. Said, where
        // the desktop greys the two choices out, rather than left missing.
        if (!c.other.here) sheet.text("The other version isn't on this phone yet, so only this one can be kept for now.")
        sheet.action(R.drawable.ic_check, "Keep this version") { settle(c, "this") }
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
        /** Files listed one by one under Waiting; the rest are counted. */
        const val WAITING_ROWS = 3
        const val SYNCED = "All synced"
    }
}
