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
                ),
            ).also { handle = it }
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
}
