package com.qurb

import android.Manifest
import android.annotation.SuppressLint
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
 * reasons, in the words of the redesign's Notifications draft 1, which the
 * desktop's are not yet.
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
    @SuppressLint("MissingPermission") // permitted() is asked first
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
        for ((h, said) in worth.takeLast(AT_ONCE)) {
            val id = NEWS_BASE + (h.id % 100_000).toInt()
            val builder = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.qurb_glyph)
                .setColor(ContextCompat.getColor(context, R.color.green))
                .setContentTitle(said.title)
                .setContentText(said.words)
                .setStyle(NotificationCompat.BigTextStyle().bigText(said.words))
                .setAutoCancel(true)
                .setContentIntent(opening(context, id, null))
            when (said.action) {
                Action.OPEN -> builder.addAction(0, "Open", opening(context, id, Intent().putExtra(OPEN, h.path)))
                Action.TRY_AGAIN -> builder.addAction(0, "Try again", opening(context, id, Intent().putExtra(AGAIN, true)))
                null -> {}
            }
            NotificationManagerCompat.from(context).notify(id, builder.build())
        }
    }

    /**
     * The app, opened from notification [id]: by a tap on it, or with
     * [extras] by one of its buttons, which the app acts on and then clears
     * the notification ([answered]), since Android leaves it up after a
     * button.
     */
    private fun opening(context: Context, id: Int, extras: Intent?): PendingIntent {
        val intent = Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        if (extras != null) intent.putExtras(extras).putExtra(NOTICE, id)
        // One request code per notification and button, or Android hands back
        // one intent for them all, with the first one's extras.
        val code = id * 4 + when {
            extras == null -> 0
            extras.hasExtra(OPEN) -> 1
            else -> 2
        }
        return PendingIntent.getActivity(context, code, intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** A notification's button acted on: the notification goes with it. */
    fun answered(context: Context, intent: Intent) {
        val id = intent.getIntExtra(NOTICE, -1)
        if (id >= 0) NotificationManagerCompat.from(context).cancel(id)
        intent.removeExtra(NOTICE)
    }

    /**
     * A computer asking to open this person's folder there (decisions 0060
     * and 0067), heard in the background: the person is at that computer and
     * waiting, so it is worth interrupting for. Tapped, the app opens and asks
     * the question behind the screen lock; nothing is approved from here.
     */
    @SuppressLint("MissingPermission") // permitted() is asked first
    fun asked(context: Context, engine: Qurb) {
        if (onScreen || !permitted(context)) return
        val ask = runCatching { engine.openAsks() }.getOrNull()?.firstOrNull() ?: return
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        // Once an ask: a phone syncing every quarter of an hour would
        // otherwise say it again each time.
        if (prefs.getString(ASKED, null) == ask.fingerprint + ask.name) return
        prefs.edit().putString(ASKED, ask.fingerprint + ask.name).apply()
        channel(context)
        // Answer opens the app, as a tap does, where the question waits
        // behind the screen lock.
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.qurb_glyph)
            .setColor(ContextCompat.getColor(context, R.color.green))
            .setContentTitle("${ask.name} asks to open your folder")
            .setContentText("Only if you asked there")
            .setAutoCancel(true)
            .setContentIntent(opening(context, ASKED_ID, null))
            .addAction(0, "Answer", opening(context, ASKED_ID, Intent().putExtra(ANSWER, true)))
            .build()
        NotificationManagerCompat.from(context).notify(ASKED_ID, notification)
    }

    /** A notification's one button, when it has one (Notifications draft 1). */
    enum class Action { OPEN, TRY_AGAIN }

    /** What a notification says: its title, a line, and a button. */
    data class Said(val title: String, val words: String, val action: Action? = null)

    /**
     * The words for an entry worth a notification, or null: who and what in
     * the title, the file or the reason under it, as Notifications draft 1
     * has them.
     */
    fun say(h: Happening): Said? {
        val what = h.path?.substringAfterLast('/') ?: ""
        val detail = h.detail
        return when {
            // Sent to this phone, rather than arriving because it is shared:
            // the engine writes that in the detail when it files it. Opened
            // from the notification when it is in the store, as it is unless
            // it was saved out to a folder ("saved to ...").
            h.kind == "received" && detail?.startsWith("sent to this device") == true ->
                Said(h.device?.let { "$it sent you a file" } ?: "Somebody sent you a file", what,
                    if (h.path != null && "saved to" !in detail) Action.OPEN else null)
            h.kind == "collected" ->
                Said(h.device?.let { "Delivered to $it" } ?: "Delivered", what)
            // A send called off: the file changed or went before that device
            // collected it. Sending it again is choosing it again, so no
            // button.
            h.kind == "failed" && detail?.startsWith("not sent: ") == true ->
                Said("Could not send $what", detail.removePrefix("not sent: ").replaceFirstChar { it.uppercase() })
            // Anything else is tried again by the next sync, which the button
            // starts now.
            h.kind == "failed" ->
                Said(if (what.isEmpty()) "Qurb could not finish something" else "Could not finish $what",
                    detail?.replaceFirstChar { it.uppercase() } ?: "", Action.TRY_AGAIN)
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
    /** The one notification for an ask to open the folder. */
    private const val ASKED_ID = 9_000
    private const val ASKED = "asked"

    /** Set on the intent a notification's button opens the app with. */
    const val OPEN = "com.qurb.OPEN_RECEIVED"
    const val AGAIN = "com.qurb.TRY_AGAIN"
    const val ANSWER = "com.qurb.ANSWER"
    private const val NOTICE = "com.qurb.NOTICE"
}
