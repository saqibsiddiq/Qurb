package com.qurb

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.Qurb
import uniffi.qurb_mobile.Settings
import uniffi.qurb_mobile.createProtected
import uniffi.qurb_mobile.isSetUp
import uniffi.qurb_mobile.joinNew
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

    /**
     * Set up a new identity, with nothing to write down (decision 0052): its key
     * is kept in Block Store, and another device is added with a code.
     */
    suspend fun create(context: Context) = withContext(Dispatchers.IO) {
        root(context).mkdirs()
        val phrase = createProtected(root(context).absolutePath, AndroidKeyStore(context)).recoveryPhrase
        setPhraseConfirmed(context, true)
        Backup.save(context, phrase)
    }

    /**
     * Set up as another of the person's devices, from the code one of them is
     * showing: the key comes with the code (decision 0052). Returns the other
     * device's name.
     */
    suspend fun join(context: Context, code: String): String = withContext(Dispatchers.IO) {
        root(context).mkdirs()
        val joined = joinNew(
            root(context).absolutePath,
            code,
            android.os.Build.MODEL ?: "phone",
            AndroidKeyStore(context),
        )
        setPhraseConfirmed(context, true)
        Backup.save(context, open(context).recoveryPhrase())
        joined.name
    }

    /**
     * Undo a setup that was never finished, so this phone can join the
     * person's other devices instead.
     *
     * Before decision 0052 a new key had to have its words typed back before
     * the app opened, and a phone set up as new by mistake -- after its data
     * was cleared, say -- sat at that check with a key nobody wanted. Only
     * ever that: refused once the folder holds anything, since a key with
     * files under it is one somebody is using.
     */
    suspend fun discardUnfinished(context: Context) = withContext(Dispatchers.IO) {
        val root = root(context)
        val held = root.listFiles()?.filter { it.name != ".qurb" }.orEmpty()
        check(held.isEmpty()) { "This phone already holds files, so its key stays." }
        handle = null
        File(root, ".qurb").deleteRecursively()
        setPhraseConfirmed(context, true)
    }

    /**
     * Keep the key in Block Store, once, for a phone set up before it was
     * kept there at setup (decision 0052).
     */
    suspend fun keepKeyOnce(context: Context) {
        val prefs = context.getSharedPreferences("qurb", Context.MODE_PRIVATE)
        if (prefs.getBoolean("key_kept", false)) return
        Backup.save(context, open(context).recoveryPhrase())
        prefs.edit().putBoolean("key_kept", true).apply()
    }

    /** Whether a setup was begun and its words never confirmed. */
    fun unfinished(context: Context): Boolean = isSetUp(context) && !phraseConfirmed(context)

    /**
     * Whether the 24 words have been typed back since this phone was set up.
     *
     * Recorded as not yet when a key is made, and set when the check passes,
     * so that an app closed between the two comes back to the check rather
     * than past it. A phone set up before the check existed has no record,
     * and is not asked; nor is one restored from the words, which has just
     * typed all 24.
     */
    fun phraseConfirmed(context: Context): Boolean =
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE).getBoolean("phrase_confirmed", true)

    fun setPhraseConfirmed(context: Context, confirmed: Boolean) {
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE)
            .edit().putBoolean("phrase_confirmed", confirmed).commit()
    }

    /** Set up from another device's words, or the copy kept in Block Store. */
    suspend fun restore(context: Context, phrase: String) = withContext(Dispatchers.IO) {
        root(context).mkdirs()
        restoreProtected(root(context).absolutePath, phrase, AndroidKeyStore(context))
        Backup.save(context, phrase)
    }

    /**
     * The open engine, opened on first use.
     *
     * Kept for the life of the process rather than reopened per call: opening
     * takes a keystore round trip and a SQLite connection, and two handles on
     * one store would contend on the database lock.
     */
    suspend fun open(context: Context): Qurb = withContext(Dispatchers.IO) {
        // Already open: the ordinary case, and it has to cost nothing, because
        // every screen asks. Before this returned early, every refresh fetched
        // the push token first -- a network round trip on a build with
        // Firebase -- and nothing appeared until it came back.
        handle?.let { return@withContext it }

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
                    relay = relayAddress(context),
                    port = 0u,
                    discover = true,
                    wakeToken = wake,
                    ownFilesPrivate = ownFilesPrivate(context),
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
    suspend fun importUri(
        context: Context,
        uri: android.net.Uri,
        into: String = "",
        private: Boolean? = null,
    ): String =
        withContext(Dispatchers.IO) {
            val folder = into.trim('/')
            val leaf = safeName(displayName(context, uri))
            val name = freeName(context, if (folder.isEmpty()) leaf else "$folder/$leaf")
            val staging = File(context.cacheDir, "import-${System.nanoTime()}")
            try {
                context.contentResolver.openInputStream(uri).use { input ->
                    staging.outputStream().use { output ->
                        requireNotNull(input) { "could not read that file" }.copyTo(output)
                    }
                }
                // Into the area being looked at when there is one -- Files or
                // Private Vault -- and where the privacy setting says otherwise.
                val engine = open(context)
                if (private == null) engine.importFile(staging.absolutePath, name)
                else engine.importInto(staging.absolutePath, name, private)
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

    /**
     * Where the rendezvous service is.
     *
     * Editable because there is no hosted one yet: to try this you run
     * `qurb signal` on a computer and point the phone at it. `10.0.2.2` is the
     * emulator's route to its host; a real phone needs the machine's address on
     * the local network.
     */
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

    /**
     * The relay, as `host:port`, or nothing for direct connections only.
     *
     * The thing that lets a phone on a mobile network reach a laptop at home:
     * carriers commonly put phones behind address translation that a direct
     * connection cannot get through, and then both devices' encrypted traffic
     * goes through this instead. It sees ciphertext it has no key for.
     */
    fun relayAddress(context: Context): String? =
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE)
            .getString("relay", null)?.trim()?.ifEmpty { null }

    fun setRelayAddress(context: Context, address: String?) {
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE)
            .edit().putString("relay", address?.trim().orEmpty()).commit()
        // Dropped, as for the rendezvous service: the engine holds the setting
        // it was opened with.
        handle = null
    }

    /**
     * When this phone last reached another device, in Unix seconds, or null
     * if it never has: what lets Home say "synced" honestly, since a phone
     * has no running daemon to ask. Set by a sync from the app and by the
     * background worker, whenever a device answered.
     */
    fun lastSynced(context: Context): Long? =
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE).getLong("last_reached_at", 0L)
            .takeIf { it > 0 }

    fun noteSynced(context: Context) {
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE).edit()
            .putLong("last_reached_at", System.currentTimeMillis() / 1000).apply()
    }

    /**
     * Whether a file added on this phone stays private to it rather than going
     * to every device (decision 0036). On unless the person turns it off: a
     * phone's photographs are its owner's until they send them somewhere.
     * Files already here stay where they are either way, and a file added
     * with a choice of area -- Files or Private Vault -- goes where it was
     * added (decision 0049).
     */
    fun ownFilesPrivate(context: Context): Boolean =
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE).getBoolean("own_private", true)

    suspend fun setOwnFilesPrivate(context: Context, private: Boolean) {
        context.getSharedPreferences("qurb", Context.MODE_PRIVATE)
            .edit().putBoolean("own_private", private).commit()
        withContext(Dispatchers.IO) { open(context).setOwnFilesPrivate(private) }
    }

    /** Something picked from anywhere on the phone, where the engine can read it. */
    class Staged(val file: File, val name: String)

    /**
     * Copy something picked from anywhere on the phone into the cache, to be
     * sent under the name it had. Staged for the same reason as [importUri]:
     * the engine takes a path, and a `content://` URI is not one. The caller
     * deletes it when done, whether or not anything was sent.
     */
    suspend fun stage(context: Context, uri: android.net.Uri): Staged =
        withContext(Dispatchers.IO) {
            val staging = File(context.cacheDir, "send-${System.nanoTime()}")
            try {
                context.contentResolver.openInputStream(uri).use { input ->
                    staging.outputStream().use { output ->
                        requireNotNull(input) { "could not read that file" }.copyTo(output)
                    }
                }
            } catch (e: Exception) {
                staging.delete()
                throw e
            }
            Staged(staging, safeName(displayName(context, uri)))
        }

    /** Send a staged file to one device. Returns the name it will see. */
    suspend fun send(context: Context, staged: Staged, to: String): String =
        withContext(Dispatchers.IO) {
            open(context).sendFile(staged.file.absolutePath, staged.name, to)
            staged.name
        }

    /** Which of these files went to `to` before, and when (decision 0059). */
    suspend fun sentBefore(context: Context, files: List<File>, to: String): List<uniffi.qurb_mobile.EarlierSend> =
        withContext(Dispatchers.IO) { open(context).sentBefore(files.map { it.absolutePath }, to) }
}
