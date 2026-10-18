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
            return done
        }

        override fun onRelease() {}
    }

    companion object {
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

        /** A URI another app can open `stream` by, as `name`, of `type`. */
        @Synchronized
        fun add(context: Context, stream: KeptStream, name: String, type: String): Uri {
            val token = UUID.randomUUID().toString()
            open[token] = Open(stream, name, type)
            while (open.size > KEPT) open.remove(open.keys.first())
            return Uri.Builder().scheme("content").authority("${context.packageName}.stream")
                .appendPath(token).appendPath(name).build()
        }

        /** Every stream let go of: the folder they read from is closing. */
        @Synchronized
        fun clear() = open.clear()
    }
}
