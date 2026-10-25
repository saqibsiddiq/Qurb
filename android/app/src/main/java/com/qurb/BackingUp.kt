package com.qurb

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.qurb_mobile.BackupWaiting
import uniffi.qurb_mobile.Qurb

/**
 * A backup on its way to the computer keeping it, as Home and the notice show
 * it: which files, how far, how much (the owner's list of 2026-10-25: "it only
 * says folder instead it should be informing exactly what is backing up,
 * there is no progress bar, no file size, no notifixation").
 *
 * A round runs from the first file waiting to the last one the computer has,
 * and its bar is how much of that round is there -- not of the whole backup,
 * which on the S23 was 38,057 files of 38,075 and drew a bar already full.
 * Files added while it runs make the round larger rather than move the bar
 * back. Between the computer's confirmations, the bytes this phone has handed
 * over move it, so that a long video does not hold it still.
 */
object BackingUp {

    class Round(val done: ULong, val total: ULong, val waiting: BackupWaiting, val going: String? = null) {
        val fraction: Float get() = if (total == 0uL) 0f else (done.toDouble() / total.toDouble()).toFloat()
        val sizes: String get() = "${Words.size(done)} of ${Words.size(total)}"
        val percent: Int get() = (fraction * 100).toInt().coerceIn(0, 100)

        /** The file going now, and how many more: `IMG_2041.jpg and 17 more`;
         *  before one is going, how many are left. */
        val what: String get() {
            val first = going ?: return "${Words.files(waiting.filesLeft.toInt())} left"
            val more = waiting.filesLeft - 1uL
            return if (more == 0uL) first else "$first and ${Words.count(more)} more"
        }
    }

    private const val PREFS = "backing_up"
    private const val TOTAL = "total"
    private const val LEFT = "left"

    /** What this process had handed over when it first saw the round. */
    @Volatile private var servedAtStart: ULong? = null

    /** Whether [watch] has put its notice up: only then does [finish] take
     *  it down, which shares its place with a long transfer's. */
    @Volatile private var posted = false

    /** The round as it stands, given what is waiting and the bytes served so far. */
    fun round(context: Context, waiting: BackupWaiting, served: ULong, going: String? = null): Round {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val left = waiting.bytesLeft
        if (waiting.filesLeft == 0uL) {
            prefs.edit().remove(TOTAL).remove(LEFT).apply()
            servedAtStart = null
            return Round(0uL, 0uL, waiting)
        }
        var total = prefs.getLong(TOTAL, 0L).toULong()
        val before = prefs.getLong(LEFT, 0L).toULong()
        total = when {
            total == 0uL -> left
            left > before -> total + (left - before)
            else -> total
        }
        prefs.edit().putLong(TOTAL, total.toLong()).putLong(LEFT, left.toLong()).apply()
        val start = servedAtStart ?: served.also { servedAtStart = it }
        val confirmed = total - left
        val moving = if (served > start) served - start else 0uL
        return Round(minOf(maxOf(confirmed, moving), total), total, waiting, going)
    }

    /** The computer keeping this phone's backups, for naming on the notice. */
    fun keeper(engine: Qurb): String? = runCatching { engine.holders().firstOrNull()?.name }.getOrNull()

    /**
     * Keep the notice up to date while a pass runs, posting it once the
     * computer is collecting and only when what it says would change. Returns
     * the job, and [finish] says how it ended.
     */
    fun watch(context: Context, engine: Qurb, scope: CoroutineScope, to: String?): Job = scope.launch(Dispatchers.IO) {
        var shown: Pair<Int, String>? = null
        while (true) {
            delay(2_000)
            val waiting = runCatching { engine.backupWaiting(1u) }.getOrNull() ?: continue
            val serving = runCatching { engine.serving() }.getOrNull() ?: continue
            if (waiting.filesLeft == 0uL || !serving.collecting) continue
            val round = round(context, waiting, serving.bytes, runCatching { engine.servingFile() }.getOrNull())
            val now = round.percent to round.what
            if (now == shown) continue
            shown = now
            posted = true
            NotificationManagerCompat.from(context).cancel(Transfers.BACKED_UP)
            notify(context, Transfers.NOTIFICATION, Transfers.backingUp(context, round, to))
        }
    }

    /**
     * After a pass: *Backed up* if it ended with nothing left of a round it
     * had begun, the notice gone if it left some for the next.
     */
    fun finish(context: Context, engine: Qurb, before: BackupWaiting?, watching: Job, to: String?) {
        watching.cancel()
        val after = runCatching { engine.backupWaiting(1u) }.getOrNull() ?: return
        val served = runCatching { engine.serving().bytes }.getOrDefault(0uL)
        if (before != null && before.filesLeft > 0uL && after.filesLeft == 0uL) {
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val total = prefs.getLong(TOTAL, before.bytesLeft.toLong()).toULong()
            round(context, after, served)
            if (posted) NotificationManagerCompat.from(context).cancel(Transfers.NOTIFICATION)
            notify(context, Transfers.BACKED_UP,
                Transfers.backedUp(context, before.filesLeft, maxOf(total, before.bytesLeft), to))
        } else if (posted) {
            NotificationManagerCompat.from(context).cancel(Transfers.NOTIFICATION)
        }
        posted = false
    }

    private fun notify(context: Context, id: Int, notification: android.app.Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) return
        NotificationManagerCompat.from(context).notify(id, notification)
    }
}
