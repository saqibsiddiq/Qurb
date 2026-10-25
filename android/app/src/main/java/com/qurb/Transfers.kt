package com.qurb

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.ForegroundInfo

/**
 * The notification a long transfer runs under (decision 0050).
 *
 * A device collecting a large file from this phone needs the phone to keep
 * answering for minutes, and Android freezes an app nobody is looking at
 * within seconds. A worker that declares itself in the foreground is not
 * frozen -- and the price Android asks is a notification for as long as it
 * runs, which is also the honest thing to show: something is being sent.
 * Only while a large collection is in progress; the ordinary background sync
 * stays silent.
 */
object Transfers {
    private const val CHANNEL = "transfers"
    const val NOTIFICATION = 7
    private const val PAUSED = 8
    /** *Backed up*: a place of its own, since the transfers' goes when a long
     *  pass's work ends. */
    const val BACKED_UP = 9

    /** Set on the intent that opens the app from [paused]'s notification. */
    const val CONTINUE = "com.qurb.CONTINUE_SENDING"

    /** What the notice says, so that a caller can tell when it would change. */
    data class Shown(val text: String, val percent: Int, val waiting: Boolean)

    fun shown(sent: ULong, total: ULong): Shown {
        val percent = if (total > 0uL) (sent.toDouble() / total.toDouble() * 100).toInt().coerceIn(0, 100) else 0
        val text = if (sent == 0uL) "${Words.size(total)} waiting"
            else "${Words.size(sent)} of ${Words.size(total)}"
        return Shown(text, percent, sent == 0uL)
    }

    /**
     * The worker's foreground notice: sent so far, of how much, for a pass that
     * started at [since]. One time for the whole pass, and not shown: given
     * the time of each update, it moved and redrew every time.
     */
    fun foreground(
        context: Context,
        sent: ULong,
        total: ULong,
        since: Long = System.currentTimeMillis(),
        to: String? = null,
    ): ForegroundInfo {
        val notification = notice(context, sent, total, since, to)
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ForegroundInfo(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            ForegroundInfo(NOTIFICATION, notification)
        }
    }

    /**
     * The notice, changed in place once the worker is in the foreground.
     *
     * Posted here rather than through `setForeground` again: WorkManager
     * answers that by posting it twice, starting the service and notifying,
     * and on the S23 each post had the shade rebuild the row (2026-10-13).
     */
    fun update(context: Context, sent: ULong, total: ULong, since: Long, to: String? = null) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) return
        NotificationManagerCompat.from(context).notify(NOTIFICATION, notice(context, sent, total, since, to))
    }

    /**
     * Who collects, when it can be said: the one other device this phone has,
     * as a phone working with its computer has (decision 0073). With more,
     * the bytes waiting may go to any of them -- a send to one, shared files
     * to all -- and the notice says *your devices*.
     */
    fun collector(engine: uniffi.qurb_mobile.Qurb): String? =
        runCatching { engine.peers().singleOrNull()?.name }.getOrNull()

    private fun notice(context: Context, sent: ULong, total: ULong, since: Long, to: String?): android.app.Notification {
        channel(context)
        val shown = shown(sent, total)
        return NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.qurb_glyph)
            .setColor(ContextCompat.getColor(context, R.color.green))
            .setContentTitle(if (to != null) "Sending to $to" else "Sending to your devices")
            .setContentText(shown.text)
            .setProgress(100, shown.percent, shown.waiting)
            .setWhen(since)
            .setShowWhen(false)
            .setOngoing(true)
            .setSilent(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    context, 0,
                    Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                    PendingIntent.FLAG_IMMUTABLE,
                )
            )
            .build()
    }

    /**
     * A backup on its way to the computer (the owner's list of 2026-10-25):
     * the file going now and how many more, how much of the round is there,
     * and a bar. In the transfers' place, so that a long pass carrying a
     * backup shows one notice, not two.
     */
    fun backingUp(context: Context, round: BackingUp.Round, to: String?): android.app.Notification {
        channel(context)
        return NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.qurb_glyph)
            .setColor(ContextCompat.getColor(context, R.color.green))
            .setContentTitle(if (to != null) "Backing up to $to" else "Backing up")
            .setContentText(round.what)
            .setSubText(round.sizes)
            .setProgress(100, round.percent, false)
            .setShowWhen(false)
            .setOngoing(true)
            .setSilent(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(open(context))
            .build()
    }

    /** The round done: how many files, how much. Quiet, and gone when tapped. */
    fun backedUp(context: Context, files: ULong, bytes: ULong, to: String?): android.app.Notification {
        channel(context)
        return NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.qurb_glyph)
            .setColor(ContextCompat.getColor(context, R.color.green))
            .setContentTitle(if (to != null) "Backed up to $to" else "Backed up")
            .setContentText("${Words.files(files.toInt())}, ${Words.size(bytes)}")
            .setSilent(true)
            .setAutoCancel(true)
            .setContentIntent(open(context))
            .build()
    }

    private fun open(context: Context) = PendingIntent.getActivity(
        context, 0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE,
    )

    /**
     * A large transfer that Android would not let run in the background.
     *
     * Starting a foreground service is refused once the app has left the
     * screen: somebody who adds a large file and leaves while it is still being
     * imported gets an ordinary pass, and then Android cuts the app off the
     * network. Measured on the S23 on 2026-10-05, it was refused 12 seconds
     * after the app was left. Only the app being on screen lets the transfer
     * start again, so this says so, and tapping it does. Posted only when a
     * device was there to collect: a send waiting for a device that is off is
     * not something opening the app would help.
     */
    fun paused(context: Context, waiting: ULong) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) return
        channel(context)
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.qurb_glyph)
            .setColor(ContextCompat.getColor(context, R.color.green))
            .setContentTitle("Tap to finish sending")
            .setContentText("${Words.size(waiting)} waiting")
            .setSilent(true)
            .setAutoCancel(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    context, PAUSED,
                    Intent(context, MainActivity::class.java)
                        .putExtra(CONTINUE, true)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
            )
            .build()
        NotificationManagerCompat.from(context).notify(PAUSED, notification)
    }

    fun clearPaused(context: Context) {
        NotificationManagerCompat.from(context).cancel(PAUSED)
    }

    /** Quiet: a progress bar, never a sound or a heads-up. */
    private fun channel(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(CHANNEL) != null) return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Sending to your devices", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Shown while a large file is being collected from this phone"
                setShowBadge(false)
            }
        )
    }
}
