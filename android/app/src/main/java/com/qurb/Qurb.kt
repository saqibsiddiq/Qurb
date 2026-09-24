package com.qurb

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.Qurb
import uniffi.qurb_mobile.Settings
import uniffi.qurb_mobile.createProtected
import uniffi.qurb_mobile.isSetUp
import uniffi.qurb_mobile.restoreProtected
import java.io.File

/**
 * The one handle on the engine, and the rules about how to call it.
 *
 * Every function here is `suspend` on `Dispatchers.IO`, because every call into
 * the library blocks: the engine takes `&mut self` behind a lock, and reading a
 * file or reaching a peer happens on the calling thread. Calling any of it from
 * the main thread would freeze the interface for as long as a sync takes.
 */
object Engine {

    @Volatile
    private var handle: Qurb? = null

    /**
     * Where the synced files live.
     *
     * `filesDir` rather than external storage: it is private to this app,
     * enforced by the kernel, and included in the device's own encryption. The
     * cost is that a file manager cannot see it, which a FileProvider would
     * eventually solve.
     */
    fun root(context: Context): File = File(context.filesDir, "qurb")

    fun isSetUp(context: Context): Boolean = isSetUp(root(context).absolutePath)

    /** Set up a new identity. Returns the 24 words, which are shown exactly once. */
    suspend fun create(context: Context): String = withContext(Dispatchers.IO) {
        root(context).mkdirs()
        createProtected(root(context).absolutePath, AndroidKeyStore(context)).recoveryPhrase
    }

    /** Set up from another device's words. */
    suspend fun restore(context: Context, phrase: String) = withContext(Dispatchers.IO) {
        root(context).mkdirs()
        restoreProtected(root(context).absolutePath, phrase, AndroidKeyStore(context))
    }

    /**
     * The open engine, opened on first use.
     *
     * Kept for the life of the process rather than reopened per call: opening
     * takes a keystore round trip and a SQLite connection, and two handles on
     * one store would contend on the database lock.
     */
    suspend fun open(context: Context): Qurb = withContext(Dispatchers.IO) {
        // Fetched before the lock, because it is a network round trip and the
        // lock is held while the store opens. Null whenever push is not set
        // up, which is the ordinary case for a build with no Firebase project:
        // the phone then syncs on its schedule instead of being poked.
        val wake = runCatching { Push.token(context) }.getOrNull()

        handle ?: synchronized(this@Engine) {
            handle ?: Qurb.openProtected(
                root(context).absolutePath,
                AndroidKeyStore(context),
                Settings(
                    deviceName = android.os.Build.MODEL ?: "phone",
                    signalUrl = signalUrl(context),
                    relay = null,
                    port = 0u,
                    discover = true,
                    wakeToken = wake,
                ),
            ).also { handle = it }
        }
    }

    /**
     * Take a file from anywhere on the phone into the synced folder.
     *
     * The one path by which content enters qurb on Android, shared by the file
     * picker and the share sheet so the two cannot drift apart.
     *
     * Staged through the cache rather than streamed straight in: `importFile`
     * takes a path, a `content://` URI is not one, and the resolver's stream is
     * the only way to read what another app is handing over. The staging copy
     * is deleted whether or not the import works — a failed share must not
     * leave the cache holding a copy of someone's photo.
     *
     * Needs no network and does not wait for one. The file is written and
     * indexed here and now; reaching another device is a separate matter that
     * happens whenever one is next reachable.
     */
    suspend fun importUri(context: Context, uri: android.net.Uri): String =
        withContext(Dispatchers.IO) {
            val name = freeName(context, safeName(displayName(context, uri)))
            val staging = File(context.cacheDir, name)
            try {
                context.contentResolver.openInputStream(uri).use { input ->
                    staging.outputStream().use { output ->
                        requireNotNull(input) { "could not read that file" }.copyTo(output)
                    }
                }
                open(context).importFile(staging.absolutePath, name)
                name
            } finally {
                staging.delete()
            }
        }

    /** What the other app calls this file, if it will say. */
    fun displayName(context: Context, uri: android.net.Uri): String {
        context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val column = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (column >= 0 && cursor.moveToFirst()) {
                cursor.getString(column)?.let { return it }
            }
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "file"
    }

    /**
     * A name that stays where it is put.
     *
     * The name comes from another application, which makes it untrusted input:
     * separators and `..` in it would place the file somewhere other than the
     * folder — outside it, given enough of them. Reduced to its last component
     * with the remaining awkward characters replaced, and never empty.
     */
    private fun safeName(candidate: String): String {
        val leaf = candidate.substringAfterLast('/').substringAfterLast('\\')
        val cleaned = leaf.replace(Regex("""[\x00-\x1f]"""), "").trim().trimStart('.')
        return cleaned.ifEmpty { "shared-${System.currentTimeMillis()}" }
    }

    /**
     * A name nothing is using yet.
     *
     * Two apps produce files called `IMG_0001.jpg` without either being wrong,
     * and importing over an existing path does not merge them — it records a
     * new version of that file, and since content is stored once, the old
     * version's bytes go with it. So a share would quietly destroy an unrelated
     * file that happened to share a name.
     *
     * A suffix rather than a comparison. Checking whether the bytes are
     * identical means hashing both files, which on a phone sharing a long video
     * is two full reads to decide something that costs little to get wrong the
     * safe way: re-sharing the same photo leaves a second copy, which someone
     * can delete. The other way round they cannot.
     */
    private fun freeName(context: Context, name: String): String {
        val folder = root(context)
        if (!File(folder, name).exists()) return name

        val stem = name.substringBeforeLast('.', name)
        val extension = name.substringAfterLast('.', "")
        val dot = if (extension.isEmpty()) "" else "."
        for (n in 2..999) {
            val candidate = "$stem ($n)$dot$extension"
            if (!File(folder, candidate).exists()) return candidate
        }
        return "$stem-${System.currentTimeMillis()}$dot$extension"
    }

    /**
     * Where the rendezvous service is.
     *
     * Editable because there is no hosted one yet: to try this you run
     * `qurb signal` on a computer and point the phone at it. `10.0.2.2` is the
     * emulator's route to its host; a real phone needs the machine's address on
     * the local network.
     */
    /**
     * Run something with the phone able to *hear* the local network.
     *
     * Android drops multicast before it reaches an application unless a
     * MulticastLock is held: the radio would otherwise wake for every packet
     * on the network, which on a phone is a battery decision rather than a
     * networking one.
     *
     * The consequence is one-directional and was invisible until it ran on
     * hardware. Sending needs no lock, so the phone announced itself perfectly
     * and the laptop saw it every time; receiving needs one, so the phone never
     * heard the laptop answer and concluded no device was there.
     *
     * Held for the length of a sync and released immediately after, in a
     * `finally` so that a failed sync does not leave the radio awake. Acquiring
     * it is best effort: a device with no Wi-Fi service, or a manufacturer that
     * refuses, still syncs over the rendezvous service.
     */
    suspend fun <T> hearingTheNetwork(context: Context, work: suspend () -> T): T {
        val wifi = context.applicationContext
            .getSystemService(Context.WIFI_SERVICE) as? android.net.wifi.WifiManager
        val lock = runCatching {
            wifi?.createMulticastLock("qurb-discovery")?.apply {
                setReferenceCounted(false)
                acquire()
            }
        }.getOrNull()

        return try {
            work()
        } finally {
            runCatching { lock?.release() }
        }
    }

    fun signalUrl(context: Context): String =
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE)
            .getString("signal", DEFAULT_SIGNAL) ?: DEFAULT_SIGNAL

    fun setSignalUrl(context: Context, url: String) {
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE)
            .edit().putString("signal", url).commit()
        // Dropped so the next call picks up the new address. A connector holds
        // the URL it was built with.
        handle = null
    }

    const val DEFAULT_SIGNAL = "ws://10.0.2.2:9000"
}
