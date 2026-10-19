package com.qurb

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Handler
import android.os.HandlerThread
import android.os.ParcelFileDescriptor
import android.os.ProxyFileDescriptorCallback
import android.os.storage.StorageManager
import android.provider.OpenableColumns
import android.system.ErrnoException
import android.system.OsConstants
import uniffi.qurb_mobile.KeptStream
import java.util.UUID

/**
 * A file of this phone's folder on a computer, opened in another app while it
 * arrives (the owner's report of 2026-10-15: the S23 struggled to play large
 * videos from the laptop, each fetched whole into the cache first).
 *
 * The app that opens it is handed an ordinary file descriptor, seekable, made
 * by Android from callbacks (`openProxyFileDescriptor`): each read it makes is
 * answered from the computer, a chunk at a time, by the engine
 * ([KeptStream]). A player starts at once and seeks where it likes; the
 * phone keeps only the few chunks being read.
 *
 * Reached only through a URI handed with the intent, carrying a token nobody
 * else knows; not exported.
 */
class StreamProvider : ContentProvider() {

    private class Open(val stream: KeptStream, val name: String, val type: String)

    override fun onCreate() = true

    override fun getType(uri: Uri): String? = opened(uri)?.type

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor? {
        val open = opened(uri) ?: throw java.io.FileNotFoundException("no longer open")
        if (mode != "r") throw SecurityException("read only")
        val storage = context!!.getSystemService(StorageManager::class.java)
        return storage.openProxyFileDescriptor(ParcelFileDescriptor.MODE_READ_ONLY, Reading(open.stream), handler())
    }

    override fun query(uri: Uri, projection: Array<String>?, selection: String?, args: Array<String>?, sort: String?): Cursor? {
        val open = opened(uri) ?: return null
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        return MatrixCursor(columns).apply {
            addRow(columns.map {
                when (it) {
                    OpenableColumns.DISPLAY_NAME -> open.name
                    OpenableColumns.SIZE -> open.stream.size().toLong()
                    else -> null
                }
            })
        }
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, args: Array<String>?) = 0
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<String>?) = 0

    /** Each read the other app makes, answered from the computer. */
    private class Reading(private val stream: KeptStream) : ProxyFileDescriptorCallback() {
        override fun onGetSize(): Long = stream.size().toLong()

        override fun onRead(offset: Long, size: Int, data: ByteArray): Int {
            // A short answer before the end reads as the end to some players,
            // so the whole of what was asked is gathered, a megabyte at a time.
            val started = android.os.SystemClock.elapsedRealtime()
            var done = 0
            try {
                while (done < size) {
                    val part = stream.readAt((offset + done).toULong(), (size - done).coerceAtMost(MOST).toUInt())
                    if (part.isEmpty()) break
                    System.arraycopy(part, 0, data, done, part.size)
                    done += part.size
                }
            } catch (e: Exception) {
                android.util.Log.w("qurb", "could not read a file from the computer", e)
                if (done == 0) throw ErrnoException("read", OsConstants.EIO)
            }
            // What a player waits on, when it waits: where the time to start
            // playing goes (2026-10-19, 37 s on mobile data).
            val took = android.os.SystemClock.elapsedRealtime() - started
            if (took >= SLOW_READ_MS) android.util.Log.i("qurb", "read $done bytes at $offset in $took ms")
            return done
        }

        override fun onRelease() {}
    }

    companion object {
        /** A read worth a line in the log: a player waiting this long stutters. */
        private const val SLOW_READ_MS = 500L

        /** What one read asks the engine for at most: its own bound. */
        private const val MOST = 1 shl 20

        /** Streams open at once: each holds a few chunks and the folder's
         *  connection, so the oldest goes when more are opened. */
        private const val KEPT = 3

        private val open = LinkedHashMap<String, Open>()

        private var thread: HandlerThread? = null

        @Synchronized
        private fun handler(): Handler {
            val running = thread ?: HandlerThread("qurb-stream").also { it.start(); thread = it }
            return Handler(running.looper)
        }

        @Synchronized
        private fun opened(uri: Uri): Open? = uri.pathSegments.firstOrNull()?.let { open[it] }

        /** The network the open streams were read over, while any are. */
        private var reading: android.net.Network? = null
        private var watching = false

        /**
         * When the phone's network changes -- Wi-Fi gone, mobile data taking
         * over -- the connection an open file is read over is gone with it,
         * and a read waiting on it would wait until it timed out: on the S23
         * on 2026-10-19 the player was declared not responding first. So
         * every open stream is closed then, and its reads fail at once; the
         * player says it cannot read, and opening the file again reads it
         * over the network there is now.
         */
        private fun watchNetwork(context: Context) {
            if (watching) return
            watching = true
            val connectivity = context.getSystemService(android.net.ConnectivityManager::class.java)
            reading = connectivity.activeNetwork
            connectivity.registerDefaultNetworkCallback(object : android.net.ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: android.net.Network) = changed(network)
                override fun onLost(network: android.net.Network) = changed(null)
            })
        }

        @Synchronized
        private fun changed(now: android.net.Network?) {
            if (now != null && now == reading) return
            reading = now
            if (open.isEmpty()) return
            android.util.Log.i("qurb", "the network changed; closing ${open.size} open file(s)")
            open.values.forEach { runCatching { it.stream.stopReading() } }
            open.clear()
        }

        /** A URI another app can open `stream` by, as `name`, of `type`. */
        @Synchronized
        fun add(context: Context, stream: KeptStream, name: String, type: String): Uri {
            watchNetwork(context.applicationContext)
            val token = UUID.randomUUID().toString()
            open[token] = Open(stream, name, type)
            // Let go of, not closed: a file's stream shares its folder's
            // connection, which the newer ones are read over too.
            while (open.size > KEPT) open.remove(open.keys.first())
            return Uri.Builder().scheme("content").authority("${context.packageName}.stream")
                .appendPath(token).appendPath(name).build()
        }

        /** Every stream let go of: the folder they read from is closing. */
        @Synchronized
        fun clear() {
            open.values.forEach { runCatching { it.stream.stopReading() } }
            open.clear()
        }
    }
}
