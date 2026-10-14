package com.qurb

import android.content.Context
import android.content.Intent
import android.net.Uri
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
 */
object Folders {
    private const val TAG = "qurb-folders"
    private const val PREFS = "folders"
    private const val LISTED = "listed-at"
    private const val NOTICED = "noticed"

    /**
     * How often the folders are listed again for the computer. Listing walks
     * all of this phone's storage, so not on every pass; a folder made since
     * shows on the computer within this, and at once from *Back up now*.
     */
    private const val LIST_EVERY_MS = 6L * 3600 * 1000

    fun allowed(): Boolean = Environment.isExternalStorageManager()

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

    /** The root of this phone's shared storage, which folders are named from. */
    fun root(): String = Environment.getExternalStorageDirectory().path

    /** The system's own page for granting it. */
    fun allowing(context: Context): Intent =
        Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:${context.packageName}"))

    /**
     * List the folders when due, or when [listNow], and back up what was
     * chosen, within [seconds]. Blocking; called off the main thread. `null`
     * without the permission. What is left when the time runs out is carried
     * on by the next run.
     */
    fun run(context: Context, engine: Qurb, seconds: UInt, listNow: Boolean = false): BackupRun? {
        if (!allowed()) return null
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
