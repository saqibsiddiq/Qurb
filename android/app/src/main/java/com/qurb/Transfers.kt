package com.qurb

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.NotificationCompat
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

    /** The worker's foreground notice: sent so far, of how much. */
    fun foreground(context: Context, sent: ULong, total: ULong): ForegroundInfo {
        channel(context)
        val percent = if (total > 0uL) (sent.toDouble() / total.toDouble() * 100).toInt().coerceIn(0, 100) else 0
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_arrow_up_down)
            .setColor(ContextCompat.getColor(context, R.color.green))
            .setContentTitle("Sending to your devices")
            .setContentText(
                if (sent == 0uL) "Waiting for a device to collect ${Words.size(total)}"
                else "${Words.size(sent)} of ${Words.size(total)}"
            )
            .setProgress(100, percent, sent == 0uL)
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
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ForegroundInfo(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            ForegroundInfo(NOTIFICATION, notification)
        }
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
