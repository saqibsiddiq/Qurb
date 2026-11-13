package com.qurb

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import android.view.ViewGroup
import android.widget.LinearLayout
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The sheets behind the person's controls (decision 0086), for Settings and
 * Home: pause, mobile data, backups, notifications, received files, a
 * report, and resetting the phone.
 */
class ControlSheets(private val app: MainActivity) {
    private val kit get() = app.kit

    /** One way to choose. */
    private class Choice(val key: String, val icon: Int, val words: String)

    /**
     * A sheet of choices, one row each with its icon, the chosen one ticked:
     * as the app's other sheets, its mark and title at the top. Three buttons
     * side by side broke their words on a phone's width.
     */
    private fun choose(mark: Int, title: String, choices: List<Choice>, chosen: String, picked: (String) -> Unit) {
        val sheet = kit.sheet().mark(mark, R.color.green_soft, R.color.green).title(title, centred = true)
        for (c in choices) sheet.choice(c.icon, c.words, c.key == chosen) { picked(c.key) }
        sheet.show()
    }

    /** Pause for an hour, until the morning, or until resumed; or resume. */
    fun pause(then: () -> Unit) {
        val until = Controls.pausedUntil(app)
        if (until != null) {
            kit.sheet()
                .mark(R.drawable.ic_pause, R.color.green_soft, R.color.green)
                .title("Sync is paused", centred = true)
                .line(Controls.pausedWords(until))
                .buttons("Resume") {
                    Controls.pause(app, 0)
                    app.sync()
                    then()
                }
                .show()
            return
        }
        val morning = java.util.Calendar.getInstance().apply {
            add(java.util.Calendar.DAY_OF_YEAR, 1)
            set(java.util.Calendar.HOUR_OF_DAY, 8)
            set(java.util.Calendar.MINUTE, 0)
            set(java.util.Calendar.SECOND, 0)
        }.timeInMillis / 1000
        val now = System.currentTimeMillis() / 1000
        choose(R.drawable.ic_pause, "Pause sync", listOf(
            Choice("hour", R.drawable.ic_clock, "1 hour"),
            Choice("morning", R.drawable.ic_history, "Until 8:00 tomorrow"),
            Choice("resume", R.drawable.ic_pause, "Until I resume"),
        ), "") { key ->
            Controls.pause(app, when (key) {
                "hour" -> now + 3600
                "morning" -> morning
                else -> Controls.UNTIL_RESUMED
            })
            then()
        }
    }

    fun mobileWords(): String = when (Controls.mobileData(app)) {
        Controls.MOBILE_OFF -> "Off"
        Controls.MOBILE_ALL -> "Everything"
        else -> "Without backups"
    }

    /** Nothing, all but backups, or everything on mobile data. */
    fun mobileData(then: () -> Unit) =
        choose(R.drawable.ic_cell_signal, "Mobile data", listOf(
            Choice(Controls.MOBILE_OFF, R.drawable.ic_cloud_off, "Off"),
            Choice(Controls.MOBILE_NO_BACKUPS, R.drawable.ic_cell_signal, "Without backups"),
            Choice(Controls.MOBILE_ALL, R.drawable.ic_hard_drive, "Everything"),
        ), Controls.mobileData(app)) {
            Controls.setMobileData(app, it)
            Engine.noteNetwork(app)
            then()
        }

    fun backupWords(): String = if (Controls.backupTiming(app) == Controls.BACKUP_OFTEN) "Every 6 hours" else "Nightly"

    /** Each night while charging, or every six hours. */
    fun backups(then: () -> Unit) =
        choose(R.drawable.ic_hard_drive, "Backups", listOf(
            Choice(Controls.BACKUP_NIGHTLY, R.drawable.ic_clock, "Nightly, while charging"),
            Choice(Controls.BACKUP_OFTEN, R.drawable.ic_refresh_cw, "Every 6 hours"),
        ), Controls.backupTiming(app)) {
            Controls.setBackupTiming(app, it)
            then()
        }

    fun noticeWords(): String {
        if (!Notices.enabled(app) || !Notices.permitted(app)) return "Off"
        val on = Controls.KINDS.count { Controls.wants(app, it) }
        return if (on == Controls.KINDS.size) "All" else "$on of ${Controls.KINDS.size}"
    }

    /** All on or off, and each kind alone. */
    fun notifications(then: () -> Unit) {
        fun group() = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.group_quiet)
            setPadding(0, kit.dp(4), 0, kit.dp(4))
        }
        // The switch for all of them, apart from the kinds it governs.
        val box = group()
        val kinds = group()
        val names = mapOf(
            "received" to (R.drawable.ic_download to "Files received"),
            "delivered" to (R.drawable.ic_circle_check to "Delivered"),
            "problems" to (R.drawable.ic_circle_alert to "Problems"),
            "asks" to (R.drawable.ic_lock_keyhole to "Folder requests"),
        )
        for (kind in Controls.KINDS) {
            val (icon, name) = names.getValue(kind)
            kit.setting(kinds, icon, name, on = Controls.wants(app, kind), onChange = { on -> Controls.setQuiet(app, kind, !on) })
        }
        val on = Notices.enabled(app) && Notices.permitted(app)
        kinds.visibility = if (on) android.view.View.VISIBLE else android.view.View.GONE
        kit.setting(box, R.drawable.ic_bell, "Notifications", on = on, onChange = { now ->
            Notices.setEnabled(app, now)
            if (now && !Notices.permitted(app)) app.askToNotify()
            kinds.visibility = if (now) android.view.View.VISIBLE else android.view.View.GONE
        })
        kit.sheet()
            .mark(R.drawable.ic_bell, R.color.green_soft, R.color.green)
            .title("Notifications", centred = true)
            .view(box, top = 16)
            .view(kinds, top = 10)
            .show()
            .onDismiss(then)
    }

    fun receivedWords(): String = when (Controls.received(app)) {
        Controls.RECEIVED_DOWNLOADS -> "Downloads"
        // The folder's own name: "primary:Documents/Sent" is "Sent".
        Controls.RECEIVED_FOLDER -> Controls.receivedFolder(app)
            ?.let { runCatching { android.provider.DocumentsContract.getTreeDocumentId(android.net.Uri.parse(it)) }.getOrNull() }
            ?.substringAfter(':')?.substringAfterLast('/')?.ifEmpty { "Phone" } ?: "Folder"
        else -> "QurbSpace"
    }

    /** Kept in QurbSpace only, or also saved to Downloads or a chosen folder. */
    fun received(then: () -> Unit) =
        choose(R.drawable.ic_folder_input, "Received files", listOf(
            Choice(Controls.RECEIVED_KEEP, R.drawable.ic_lock_keyhole, "QurbSpace only"),
            Choice(Controls.RECEIVED_DOWNLOADS, R.drawable.ic_download, "Also in Downloads"),
            Choice(Controls.RECEIVED_FOLDER, R.drawable.ic_folder_open, "Also in a folder…"),
        ), Controls.received(app)) { key ->
            if (key == Controls.RECEIVED_FOLDER) app.pickReceivedFolder(then)
            else {
                Controls.setReceived(app, key)
                then()
            }
        }

    /**
     * A report for when something is wrong: this build, the phone, the
     * settings, the devices, the last of what happened and the app's own
     * log, saved where the person picks. Sent nowhere.
     */
    fun report() {
        kit.sheet()
            .mark(R.drawable.ic_file_text, R.color.green_soft, R.color.green)
            .title("Save a report?", centred = true)
            .line("Version, settings, devices, recent activity and the log. File names are in it. Sent nowhere.")
            .buttons("Save") {
                app.lifecycleScope.launch {
                    val text = withContext(Dispatchers.IO) { reportText() }
                    app.saveText("qurbspace-report-${System.currentTimeMillis() / 1000}.txt", text)
                }
            }
            .show()
    }

    private suspend fun reportText(): String = buildString {
        val engine = runCatching { Engine.open(app) }.getOrNull()
        appendLine("QurbSpace report, written at Unix time ${System.currentTimeMillis() / 1000}")
        appendLine("${Updates.installedName(app)} (${Updates.installedCode(app)})")
        appendLine(runCatching { uniffi.qurb_mobile.engineVersion() }.getOrDefault(""))
        appendLine("Android ${Build.VERSION.RELEASE} (SDK ${Build.VERSION.SDK_INT}), ${Build.MANUFACTURER} ${Build.MODEL}")
        appendLine("\n== Settings")
        for (prefs in listOf("controls", "qurb")) {
            app.getSharedPreferences(prefs, Context.MODE_PRIVATE).all.toSortedMap().forEach { (k, v) ->
                // Nothing secret is kept there; a token would be, so none is written.
                if (!k.contains("token", ignoreCase = true)) appendLine("$prefs.$k = $v")
            }
        }
        appendLine("background sync every ${SyncWorker.every(app)} min, last ${SyncWorker.lastAt(app)} ${SyncWorker.lastResult(app) ?: ""}")
        appendLine("\n== Devices")
        runCatching { engine?.peers() }.getOrNull()?.forEach { appendLine("${it.name} (${it.relation}, ${it.kind ?: "?"}), last seen ${it.lastSeen}") }
        appendLine("\n== Activity, newest first")
        runCatching { engine?.history(200u, null) }.getOrNull()?.forEach { appendLine("${it.at} ${it.kind} ${it.path ?: ""} ${it.detail ?: ""}") }
        appendLine("\n== Log")
        runCatching {
            val log = Runtime.getRuntime().exec(arrayOf("logcat", "-d", "-v", "time", "--pid=${android.os.Process.myPid()}"))
            append(log.inputStream.bufferedReader().readText())
        }.onFailure { appendLine("(could not read the log: ${it.message})") }
    }

    /**
     * QurbSpace leaves this phone, after a second yes: each computer told it
     * was removed, then everything the app keeps deleted, as Android's
     * *Clear data* does. Files saved out of it stay.
     */
    fun reset() {
        kit.sheet()
            .mark(R.drawable.ic_rotate_ccw)
            .title("Reset this phone?", centred = true)
            .line("Your computers are told it left. Its key, vault and settings here are deleted. Files you saved out stay.")
            .warning("Only if you have your 24 words, or a computer has the key", compact = true)
            .buttons("Reset", danger = true) {
                app.say("Resetting…")
                app.lifecycleScope.launch {
                    withContext(Dispatchers.IO) {
                        runCatching { Engine.open(app).leave(5u) }
                    }
                    (app.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager).clearApplicationUserData()
                }
            }
            .show()
    }
}
