package com.qurb

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import android.util.Log
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.BackupRun
import uniffi.qurb_mobile.OutsideFile
import java.util.concurrent.TimeUnit

/**
 * WhatsApp's folder, kept by a computer in this phone's Private Vault instead of
 * in the person's Google account (decision 0062).
 *
 * Android lets an app read another app's folder under `Android/media` only
 * through a folder the person picks, once, in the system's own picker -- which
 * opens right at WhatsApp's. The grant is kept, and each night the folder is
 * listed through it and handed to the engine, which keeps every file in the
 * vault where it lies, copying nothing, and offers it to the computer keeping
 * the vault while the phone is on Wi-Fi.
 *
 * Restoring WhatsApp from it has not been watched working yet; the screens say
 * so, and that WhatsApp's own Google backup should stay on until it has.
 */
object WhatsApp {

    private const val TAG = "qurb-whatsapp"
    private const val PREFS = "whatsapp"
    private const val NIGHTLY = "whatsapp-nightly"
    private const val NOW = "whatsapp-now"
    private const val MORE = "whatsapp-more"

    /** WhatsApp's folder, as the external storage provider names it. */
    private const val FOLDER = "primary:Android/media/com.whatsapp/WhatsApp"
    private const val STORAGE = "com.android.externalstorage.documents"

    /** Measured on the S23: a first run reads 9.65 GB, about a minute and a
     *  half; listing takes another. Inside a worker's ten minutes, and what is
     *  left over is carried on with next time. */
    private const val RUN_SECONDS = 420u

    /** The picker, opened at WhatsApp's folder. */
    fun pickIntent(): Intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
        putExtra(DocumentsContract.EXTRA_INITIAL_URI, DocumentsContract.buildDocumentUri(STORAGE, FOLDER))
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    }

    /**
     * The folder the person picked. Kept only if it is WhatsApp's: any other
     * would be backed up under WhatsApp's name. Read only -- writing into it
     * is for restoring, which asks again.
     */
    fun accept(context: Context, picked: Uri): Boolean {
        if (DocumentsContract.getTreeDocumentId(picked) != FOLDER) return false
        context.contentResolver.takePersistableUriPermission(picked, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        prefs(context).edit().putString("tree", picked.toString()).apply()
        schedule(context)
        runNow(context)
        return true
    }

    /** The grant, while Android still holds it for this app. */
    fun tree(context: Context): Uri? {
        val kept = prefs(context).getString("tree", null)?.let(Uri::parse) ?: return null
        val held = context.contentResolver.persistedUriPermissions.any { it.uri == kept && it.isReadPermission }
        return kept.takeIf { held }
    }

    /** Stop backing up. What the computer keeps stays there. */
    fun stop(context: Context) {
        tree(context)?.let {
            runCatching {
                context.contentResolver.releasePersistableUriPermission(it, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
        }
        prefs(context).edit().clear().apply()
        WorkManager.getInstance(context).cancelUniqueWork(NIGHTLY)
    }

    /** When it last ran, in unix seconds, and what it did, in words. */
    fun lastRun(context: Context): Pair<Long, String>? {
        val at = prefs(context).getLong("at", 0)
        return if (at == 0L) null else at to (prefs(context).getString("said", "") ?: "")
    }

    /**
     * List WhatsApp's folder through the grant and hand it to the engine. Its
     * caches -- the folders whose names start with a dot -- are not walked.
     */
    suspend fun run(context: Context): BackupRun? = withContext(Dispatchers.IO) {
        val tree = tree(context) ?: return@withContext null
        val started = System.currentTimeMillis()
        val files = list(context, tree)
        val listed = System.currentTimeMillis() - started
        val run = Engine.open(context).backUpWhatsapp(files, RUN_SECONDS)
        val took = System.currentTimeMillis() - started - listed
        val said = buildList {
            add("${run.added + run.changed + run.unchanged} files")
            if (run.added > 0u) add("${run.added} new")
            if (run.changed > 0u) add("${run.changed} changed")
            if (run.kept > 0u) add("${run.kept} deleted in WhatsApp, kept")
            if (run.remaining > 0u) add("${run.remaining} left for next time")
            if (run.failed > 0u) add("${run.failed} could not be read")
        }.joinToString(", ")
        prefs(context).edit()
            .putLong("at", System.currentTimeMillis() / 1000)
            .putString("said", said)
            .apply()
        Log.i(TAG, "listed ${files.size} in ${listed} ms; read ${run.bytesRead} bytes of " +
            "${run.added + run.changed} files in ${took} ms; $run")
        run
    }

    private fun list(context: Context, tree: Uri): List<OutsideFile> {
        val out = ArrayList<OutsideFile>(40_000)
        val columns = arrayOf(
            Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE,
            Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED,
        )
        fun walk(id: String, path: String) {
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
            context.contentResolver.query(children, columns, null, null, null)?.use { c ->
                while (c.moveToNext()) {
                    val child = c.getString(0)
                    val name = c.getString(1) ?: continue
                    if (name.startsWith(".")) continue
                    val here = if (path.isEmpty()) name else "$path/$name"
                    if (c.getString(2) == Document.MIME_TYPE_DIR) {
                        walk(child, here)
                    } else {
                        out += OutsideFile(
                            path = here,
                            uri = DocumentsContract.buildDocumentUriUsingTree(tree, child).toString(),
                            size = if (c.isNull(3)) 0uL else c.getLong(3).toULong(),
                            modifiedMs = if (c.isNull(4)) 0L else c.getLong(4),
                        )
                    }
                }
            }
        }
        walk(DocumentsContract.getTreeDocumentId(tree), "")
        return out
    }

    /**
     * Each night, charging and on Wi-Fi: after WhatsApp's own backup. Not at
     * once -- a periodic request runs straight away, beside the first run
     * [runNow] starts, and two ran together on the emulator.
     */
    private fun schedule(context: Context) {
        val request = PeriodicWorkRequestBuilder<Nightly>(24, TimeUnit.HOURS)
            .setInitialDelay(12, TimeUnit.HOURS)
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.UNMETERED)
                    .setRequiresCharging(true)
                    .build()
            )
            .build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(NIGHTLY, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    /** Once, now, on Wi-Fi: *Back up now*, and the first run after setting up. */
    fun runNow(context: Context) {
        val request = OneTimeWorkRequestBuilder<Nightly>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.UNMETERED).build())
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(NOW, ExistingWorkPolicy.KEEP, request)
    }

    private fun carryOn(context: Context) {
        val request = OneTimeWorkRequestBuilder<Nightly>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.UNMETERED).build())
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(MORE, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    /**
     * The run, in the background, then a sync: the computer collects while
     * this phone serves, which on Wi-Fi with this much waiting it does in the
     * foreground, where Android lets it carry on (decision 0050).
     */
    class Nightly(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
        override suspend fun doWork(): Result {
            val run = runCatching { run(applicationContext) }
                .onFailure { Log.w(TAG, "the backup did not run", it) }
                .getOrNull() ?: return Result.success()
            // Always: what an earlier run found may still be on its way.
            SyncWorker.runNow(applicationContext)
            // A first run reads more than one window allows -- 3.5 GB of the
            // S23's 9.65 in its seven minutes. The rest carries on now, on
            // Wi-Fi, rather than over the nights that follow.
            if (run.remaining > 0u && run.failed == 0u) carryOn(applicationContext)
            return Result.success()
        }
    }
}
