package com.qurb

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import uniffi.qurb_mobile.BackupRun
import uniffi.qurb_mobile.Qurb

/**
 * This phone's own folders, backed up to the computer that keeps its Private
 * Vault and chosen there (decision 0063): the folders are listed for the
 * computer to show, and the ones ticked on it are kept where they lie, as
 * WhatsApp's folder is.
 *
 * Reading them needs "All files access". Without it Android lets an app into
 * a folder only when the person picks it on the phone, one at a time, so the
 * computer could neither see this phone's folders nor choose any.
 *
 * "All files access" is Android 11's. Before it, Android 8 to 10, the same
 * reach is the storage permission, asked in a dialog rather than on a page of
 * Settings. Until 2026-10-18 only the newer was asked for, and asking whether
 * it was held stopped the app on those phones: lint found it, none had run it.
 */
object Folders {
    private const val TAG = "qurb-folders"
    private const val PREFS = "folders"
    private const val LISTED = "listed-at"
    private const val NOTICED = "noticed"
    private const val CHOSEN = "chosen"

    /**
     * How often the folders are listed again for the computer. Listing walks
     * all of this phone's storage, so not on every pass; a folder made since
     * shows on the computer within this, and at once from *Back up now*.
     */
    private const val LIST_EVERY_MS = 6L * 3600 * 1000

    /** The storage permission's two halves, before Android 11: reading
     *  folders, and writing into them for a restore. */
    val OLDER = arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE, Manifest.permission.WRITE_EXTERNAL_STORAGE)

    fun allowed(context: Context): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Environment.isExternalStorageManager()
        else OLDER.all { context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }

    /**
     * Whether the access has been seen granted before, and from now on it
     * has: the first time is when the folders are first listed for the
     * computer and the nightly run scheduled.
     */
    fun noticed(context: Context): Boolean {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (prefs.getBoolean(NOTICED, false)) return true
        prefs.edit().putBoolean(NOTICED, true).apply()
        return false
    }

    /**
     * After a sync: a folder newly chosen on the computer starts backing up
     * now, on Wi-Fi, rather than at the next background pass up to an hour
     * later. On the owner's first try (2026-10-14) a folder ticked and synced
     * waited for a pass that had not come.
     */
    fun startChosen(context: Context, engine: Qurb) {
        if (!allowed(context)) return
        val chosen = runCatching { engine.chosenFolders() }.getOrNull() ?: return
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val before = prefs.getStringSet(CHOSEN, emptySet()) ?: emptySet()
        if (chosen.toSet() == before) return
        prefs.edit().putStringSet(CHOSEN, chosen.toSet()).apply()
        if ((chosen.toSet() - before).isNotEmpty()) WhatsApp.runNow(context)
    }

    /** The root of this phone's shared storage, which folders are named from. */
    fun root(): String = Environment.getExternalStorageDirectory().path

    /** The system's own page for granting it, from Android 11. Before, it is
     *  asked for in a dialog ([MainActivity.allowFiles]). */
    fun allowing(context: Context): Intent =
        Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:${context.packageName}"))

    /**
     * List the folders when due, or when [listNow], and back up what was
     * chosen, within [seconds]. Blocking; called off the main thread. `null`
     * without the permission. What is left when the time runs out is carried
     * on by the next run.
     */
    fun run(context: Context, engine: Qurb, seconds: UInt, listNow: Boolean = false): BackupRun? {
        if (!allowed(context)) return null
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val now = System.currentTimeMillis()
        if (listNow || now - prefs.getLong(LISTED, 0) > LIST_EVERY_MS) {
            val started = SystemClock.elapsedRealtime()
            val folders = engine.listFolders(root())
            prefs.edit().putLong(LISTED, now).apply()
            Log.i(TAG, "listed $folders folders in ${SystemClock.elapsedRealtime() - started} ms")
        }
        if (engine.chosenFolders().isEmpty()) return null
        val started = SystemClock.elapsedRealtime()
        val run = engine.backUpChosen(root(), seconds)
        Log.i(TAG, "backed up in ${SystemClock.elapsedRealtime() - started} ms; read ${run.bytesRead} bytes; $run")
        return run
    }
}
