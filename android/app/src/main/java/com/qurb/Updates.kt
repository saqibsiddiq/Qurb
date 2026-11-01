package com.qurb

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/**
 * Updating the app wherever the phone is, during the beta (decision 0078):
 * what the latest release offers is read from a small file published with
 * it, the APK it names fetched and checked against the SHA-256 that file
 * gives, and installed over this one. Android itself refuses an APK signed
 * with any other key, so a file changed on its way here cannot install.
 *
 * The check says nothing about the person: one request for a public file,
 * which tells GitHub an address asked, and nothing of what this phone holds.
 */
object Updates {

    /** Where the latest release says what it offers the phone. */
    const val CHANNEL = "https://github.com/saqibsiddiq/Qurb/releases/latest/download/android.json"

    /** How often it is asked, at most: twice a day is ample for a beta. */
    private const val EVERY_MS = 6 * 60 * 60 * 1000L

    private const val TAG = "QurbUpdates"
    private const val PREFS = "updates"

    /** A newer build, as the release describes it. */
    data class Offer(val versionCode: Long, val versionName: String, val url: String, val sha256: String, val size: Long)

    /** Where updating stands, for Settings. */
    sealed class State {
        object UpToDate : State()
        data class Ready(val versionName: String) : State()
        data class Failed(val why: String) : State()
    }

    fun installedCode(context: Context): Long {
        val info = context.packageManager.getPackageInfo(context.packageName, 0)
        return if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else @Suppress("DEPRECATION") info.versionCode.toLong()
    }

    fun installedName(context: Context): String =
        context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: ""

    fun state(context: Context): State {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        prefs.getString("failed", null)?.let { return State.Failed(it) }
        val ready = prefs.getString("ready_name", null)
        val code = prefs.getLong("ready_code", 0)
        return if (ready != null && code > installedCode(context) && apk(context, code).exists()) State.Ready(ready) else State.UpToDate
    }

    /** Whether the person has let this app install updates (once, in the system's page). */
    fun allowed(context: Context): Boolean =
        Build.VERSION.SDK_INT < 26 || context.packageManager.canRequestPackageInstalls()

    /** The system's page where installing from this app is allowed. */
    fun allowIntent(context: Context): Intent =
        Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, android.net.Uri.parse("package:${context.packageName}"))

    private fun apk(context: Context, code: Long) = File(File(context.cacheDir, "updates"), "qurb-$code.apk")

    /** What the latest release offers, if it is newer than this build. */
    suspend fun check(context: Context): Offer? = withContext(Dispatchers.IO) {
        val text = fetch(CHANNEL).toString(Charsets.UTF_8)
        val json = JSONObject(text)
        val offer = Offer(
            versionCode = json.getLong("versionCode"),
            versionName = json.getString("versionName"),
            url = json.getString("url"),
            sha256 = json.getString("sha256").lowercase(),
            size = json.optLong("size", 0),
        )
        offer.takeIf { it.versionCode > installedCode(context) }
    }

    /**
     * Look for a newer build, at most every [EVERY_MS] unless [now], fetch it
     * if there is one, and install it when [install] -- from the background
     * worker, so that an update lands while nobody is using the app. Returns
     * where it stands. Never throws: an update that fails is said in Settings,
     * and the sync it ran beside is not touched.
     */
    suspend fun run(context: Context, now: Boolean = false, install: Boolean = false): State {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (!now && System.currentTimeMillis() - prefs.getLong("checked_at", 0) < EVERY_MS) {
            val held = state(context)
            if (install && held is State.Ready) installReady(context)
            return held
        }
        return try {
            val offer = check(context)
            prefs.edit().putLong("checked_at", System.currentTimeMillis()).remove("failed").apply()
            if (offer == null) {
                File(context.cacheDir, "updates").deleteRecursively()
                prefs.edit().remove("ready_name").remove("ready_code").apply()
                return State.UpToDate
            }
            val file = apk(context, offer.versionCode)
            if (!file.exists()) download(offer, file)
            prefs.edit().putString("ready_name", offer.versionName).putLong("ready_code", offer.versionCode).apply()
            if (install) installReady(context)
            State.Ready(offer.versionName)
        } catch (e: Exception) {
            Log.w(TAG, "update check failed", e)
            val why = e.message ?: e.javaClass.simpleName
            prefs.edit().putLong("checked_at", System.currentTimeMillis()).putString("failed", why).apply()
            State.Failed(why)
        }
    }

    private suspend fun download(offer: Offer, to: File) = withContext(Dispatchers.IO) {
        to.parentFile?.mkdirs()
        val part = File(to.path + ".part")
        val digest = MessageDigest.getInstance("SHA-256")
        open(offer.url).inputStream.use { input ->
            part.outputStream().use { out ->
                val buf = ByteArray(1 shl 16)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    digest.update(buf, 0, n)
                    out.write(buf, 0, n)
                }
            }
        }
        val got = digest.digest().joinToString("") { "%02x".format(it) }
        if (got != offer.sha256) {
            part.delete()
            error("the download was not the release's: its checksum differs")
        }
        part.renameTo(to)
    }

    /** Install the build fetched, if there is one newer than this. */
    fun installReady(context: Context) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val code = prefs.getLong("ready_code", 0)
        val file = apk(context, code)
        if (code <= installedCode(context) || !file.exists()) return
        if (!allowed(context)) return
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(context.packageName)
            // Android 12 and later let an app update itself without asking
            // each time, once it is allowed to install at all.
            if (Build.VERSION.SDK_INT >= 31) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            file.inputStream().use { input ->
                session.openWrite("qurb.apk", 0, file.length()).use { out ->
                    input.copyTo(out)
                    session.fsync(out)
                }
            }
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
            val done = PendingIntent.getBroadcast(context, id, Intent(context, UpdateReceiver::class.java), flags)
            session.commit(done.intentSender)
        }
    }

    private fun open(url: String): HttpURLConnection {
        var at = URL(url)
        // GitHub answers a release file with a redirect to where it is kept.
        repeat(5) {
            val c = (at.openConnection() as HttpURLConnection).apply {
                connectTimeout = 15_000
                readTimeout = 30_000
                instanceFollowRedirects = false
                setRequestProperty("User-Agent", "QurbSpace-updater")
            }
            if (c.responseCode in 300..399) {
                at = URL(at, c.getHeaderField("Location"))
                c.disconnect()
            } else {
                if (c.responseCode != 200) error("the release answered ${c.responseCode}")
                return c
            }
        }
        error("the release redirected too often")
    }

    private fun fetch(url: String): ByteArray = open(url).inputStream.use { it.readBytes() }
}

/** What the installer says of an update: asked for a yes, or done. */
class UpdateReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            // Android wants the person to confirm: the first time, or before 12.
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirm = intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT) ?: return
                context.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
            PackageInstaller.STATUS_SUCCESS -> Unit
            else -> {
                val why = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "the install did not finish"
                context.getSharedPreferences("updates", Context.MODE_PRIVATE).edit().putString("failed", why).apply()
            }
        }
    }
}
