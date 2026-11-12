package com.qurb

import android.content.Context

/**
 * The person's controls over the app (decision 0086), kept in one place:
 * pausing, mobile data, when backups run, which notifications, updating,
 * locking the app, and where received files are saved.
 *
 * Pause and mobile data are said to the engine before every sync
 * ([apply]), so a pause made here holds in the background worker too.
 */
object Controls {
    private const val PREFS = "controls"

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    // ---------- Pause ----------

    /** Paused with no end. */
    const val UNTIL_RESUMED = Long.MAX_VALUE

    /** When the pause ends, in Unix seconds; null while syncing. */
    fun pausedUntil(context: Context): Long? =
        prefs(context).getLong("paused_until", 0L).takeIf { it > System.currentTimeMillis() / 1000 }

    /** When a pause ends, in a few words. */
    fun pausedWords(until: Long?): String = when {
        until == null -> ""
        until == UNTIL_RESUMED -> "Until you resume"
        else -> {
            val at = java.util.Calendar.getInstance().apply { timeInMillis = until * 1000 }
            val today = java.util.Calendar.getInstance()
            val time = java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(at.time)
            if (at.get(java.util.Calendar.DAY_OF_YEAR) == today.get(java.util.Calendar.DAY_OF_YEAR)) "Until $time" else "Until $time tomorrow"
        }
    }

    fun pause(context: Context, until: Long) {
        prefs(context).edit().putLong("paused_until", until).apply()
        Engine.current()?.setPaused(until)
    }

    // ---------- Mobile data ----------

    /** Nothing synced on mobile data. */
    const val MOBILE_OFF = "off"
    /** Everything but the backups of WhatsApp and chosen folders: as before. */
    const val MOBILE_NO_BACKUPS = "no_backups"
    /** Everything, the backups too. */
    const val MOBILE_ALL = "all"

    fun mobileData(context: Context): String = prefs(context).getString("mobile", MOBILE_NO_BACKUPS) ?: MOBILE_NO_BACKUPS

    fun setMobileData(context: Context, to: String) {
        prefs(context).edit().putString("mobile", to).apply()
        WhatsApp.schedule(context, changed = true)
    }

    /** Whether backups may run on the network the phone is on now. */
    fun backupsAllowedNow(context: Context): Boolean =
        Engine.unmetered(context) || mobileData(context) == MOBILE_ALL

    // ---------- Backup timing ----------

    /** Each night, while charging: as before. */
    const val BACKUP_NIGHTLY = "nightly"
    /** Every six hours, charging or not. */
    const val BACKUP_OFTEN = "often"

    fun backupTiming(context: Context): String = prefs(context).getString("backup", BACKUP_NIGHTLY) ?: BACKUP_NIGHTLY

    fun setBackupTiming(context: Context, to: String) {
        prefs(context).edit().putString("backup", to).apply()
        WhatsApp.schedule(context, changed = true)
    }

    // ---------- Notifications ----------

    /** The kinds of notification, each of which can be turned off alone. */
    val KINDS = listOf("received", "delivered", "problems", "asks")

    fun quiet(context: Context): Set<String> = prefs(context).getStringSet("quiet", emptySet()) ?: emptySet()

    fun setQuiet(context: Context, kind: String, quiet: Boolean) {
        val now = quiet(context).toMutableSet()
        if (quiet) now += kind else now -= kind
        prefs(context).edit().putStringSet("quiet", now).apply()
    }

    fun wants(context: Context, kind: String): Boolean = kind !in quiet(context)

    // ---------- Updates ----------

    /** Whether an update installs itself, or waits for a tap in Settings. */
    fun autoUpdate(context: Context): Boolean = prefs(context).getBoolean("auto_update", true)

    fun setAutoUpdate(context: Context, on: Boolean) = prefs(context).edit().putBoolean("auto_update", on).apply()

    // ---------- App lock ----------

    /** Whether opening the app asks for the screen lock. */
    fun appLock(context: Context): Boolean = prefs(context).getBoolean("app_lock", false)

    fun setAppLock(context: Context, on: Boolean) = prefs(context).edit().putBoolean("app_lock", on).apply()

    // ---------- Received files ----------

    /** Kept in QurbSpace only, as before: saved out by hand. */
    const val RECEIVED_KEEP = "keep"
    /** Also saved to Downloads/Qurb. */
    const val RECEIVED_DOWNLOADS = "downloads"
    /** Also saved to a folder the person chose. */
    const val RECEIVED_FOLDER = "folder"

    fun received(context: Context): String = prefs(context).getString("received", RECEIVED_KEEP) ?: RECEIVED_KEEP

    /** The chosen folder, as the system's tree address. */
    fun receivedFolder(context: Context): String? = prefs(context).getString("received_folder", null)

    fun setReceived(context: Context, to: String, folder: String? = null) {
        prefs(context).edit().putString("received", to).putString("received_folder", folder).apply()
    }

    // ---------- Said to the engine ----------

    /**
     * Pause and mobile data as the engine is to act on them now: before each
     * sync, and when the engine opens.
     */
    fun apply(context: Context, engine: uniffi.qurb_mobile.Qurb) {
        val unmetered = Engine.unmetered(context)
        val mobile = mobileData(context)
        runCatching { engine.setPaused(pausedUntil(context) ?: 0L) }
        runCatching { engine.setHeld(!unmetered && mobile == MOBILE_OFF) }
        runCatching { engine.setUnmetered(unmetered || mobile == MOBILE_ALL) }
    }
}
