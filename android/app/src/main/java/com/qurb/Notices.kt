package com.qurb

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import uniffi.qurb_mobile.Happening
import uniffi.qurb_mobile.Qurb

/**
 * The few things worth interrupting somebody about, on the phone: the same
 * three the desktop raises (crates/desktop/src/notify.rs), for the same
 * reasons.
 *
 * - **somebody sent you a file** -- a thing another person did, on purpose,
 *   for you;
 * - **a device collected what you sent it** -- closing the loop on something
 *   you did;
 * - **something failed** -- because the alternative is finding out later.
 *
 * Ordinary syncing is silent, and so is everything else. Read from the
 * history after each sync, so nothing new has to be kept: the last entry
 * looked at is remembered, and only what is newer is considered. Nothing is
 * raised while the app is on screen, where Home already shows it. The first
 * look only notes where the history ends, so a phone updated to this build is
 * not told about last week.
 */
object Notices {
    private const val CHANNEL = "news"
    private const val PREFS = "notices"
    private const val SEEN = "seen"
    private const val ON = "on"
    /** The most at once: a phone back after a week has a great deal of history,
     *  and Activity is where a backlog belongs (the desktop's limit too). */
    private const val AT_ONCE = 3

    /** Whether the app is on screen now, set by MainActivity. */
    @Volatile var onScreen = false

    fun enabled(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(ON, true)

    fun setEnabled(context: Context, on: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(ON, on).apply()
    }

    /** Whether Android lets Qurb notify at all. */
    fun permitted(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

    /** After a sync: say what is worth saying since the last look. */
    fun tell(context: Context, engine: Qurb) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val history = runCatching { engine.history(50u, null) }.getOrNull() ?: return
        val newest = history.maxOfOrNull { it.id } ?: return
        val seen = prefs.getLong(SEEN, -1)
        if (newest > seen) prefs.edit().putLong(SEEN, newest).apply()
        if (seen < 0 || onScreen || !enabled(context) || !permitted(context)) return

        val worth = history.filter { it.id > seen }.sortedBy { it.id }.mapNotNull { h -> say(h)?.let { h to it } }
        if (worth.isEmpty()) return
        channel(context)
        for ((h, words) in worth.takeLast(AT_ONCE)) {
            val notification = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_bell)
                .setColor(ContextCompat.getColor(context, R.color.green))
                .setContentTitle(words.first)
                .setContentText(words.second)
                .setStyle(NotificationCompat.BigTextStyle().bigText(words.second))
                .setAutoCancel(true)
                .setContentIntent(
                    PendingIntent.getActivity(
                        context, h.id.toInt(),
                        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                    )
                )
                .build()
            NotificationManagerCompat.from(context).notify(NEWS_BASE + (h.id % 100_000).toInt(), notification)
        }
    }

    /** The title and words for an entry worth a notification, or null. */
    fun say(h: Happening): Pair<String, String>? {
        val what = h.path?.substringAfterLast('/') ?: ""
        return when {
            // Sent to this phone, rather than arriving because it is shared:
            // the engine writes that in the detail when it files it.
            h.kind == "received" && h.detail?.startsWith("sent to this device") == true ->
                (h.device?.let { "$it sent you a file" } ?: "Somebody sent you a file") to what
            h.kind == "collected" ->
                "Delivered" to (h.device?.let { "$it has $what" } ?: "$what was collected")
            h.kind == "failed" ->
                "Qurb could not finish something" to listOfNotNull(what.ifEmpty { null }, h.detail).joinToString(": ")
            else -> null
        }
    }

    private fun channel(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(CHANNEL) != null) return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Sent, delivered, and what failed", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "A file somebody sent you, a file you sent arriving, and anything Qurb could not finish"
            }
        )
    }

    /** Clear of the Transfers notifications' ids. */
    private const val NEWS_BASE = 10_000
}
