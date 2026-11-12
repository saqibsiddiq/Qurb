package com.qurb

import android.content.Context
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import android.util.Log
import android.webkit.MimeTypeMap
import uniffi.qurb_mobile.Qurb
import java.io.File

/**
 * A file sent to this phone, saved where the person chose as well as kept in
 * QurbSpace (decision 0086): Downloads/Qurb, or a folder they picked. After
 * each sync, each file received since the last look; the first look only
 * notes where the history ends, so turning it on does not save last week's.
 */
object ReceivedOut {
    private const val TAG = "QurbReceived"
    private const val PREFS = "received_out"
    private const val SEEN = "seen"

    fun save(context: Context, engine: Qurb) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val history = runCatching { engine.history(50u, null) }.getOrNull() ?: return
        val newest = history.maxOfOrNull { it.id } ?: return
        val seen = prefs.getLong(SEEN, -1)
        prefs.edit().putLong(SEEN, maxOf(newest, seen)).apply()
        val mode = Controls.received(context)
        if (seen < 0 || mode == Controls.RECEIVED_KEEP) return
        // Sent to this phone, not arriving because it is shared: the engine
        // says so in the detail (as Notices reads it).
        val sent = history.filter { it.id > seen && it.kind == "received" && it.detail?.startsWith("sent to this device") == true }
        for (h in sent.sortedBy { it.id }) {
            val path = h.path ?: continue
            val name = path.substringAfterLast('/')
            runCatching {
                when (mode) {
                    Controls.RECEIVED_DOWNLOADS -> {
                        val dir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Qurb")
                        dir.mkdirs()
                        engine.export(path, free(dir, name).path)
                    }
                    Controls.RECEIVED_FOLDER -> toFolder(context, engine, path, name)
                }
            }.onFailure { Log.w(TAG, "could not save $name where chosen", it) }
        }
    }

    /** `name` in `dir`, or `name (2)` and so on where it is taken. */
    private fun free(dir: File, name: String): File {
        var at = File(dir, name)
        var n = 2
        while (at.exists()) {
            val dot = name.lastIndexOf('.').takeIf { it > 0 } ?: name.length
            at = File(dir, "${name.substring(0, dot)} ($n)${name.substring(dot)}")
            n++
        }
        return at
    }

    /** Into the folder the person picked: written to the cache, then copied in. */
    private fun toFolder(context: Context, engine: Qurb, path: String, name: String) {
        val tree = Uri.parse(Controls.receivedFolder(context) ?: return)
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase())
            ?: "application/octet-stream"
        val staging = File(context.cacheDir, "received-out").apply { mkdirs() }.let { File(it, name) }
        try {
            engine.export(path, staging.path)
            val made = DocumentsContract.createDocument(context.contentResolver, parent, mime, name) ?: return
            context.contentResolver.openOutputStream(made)?.use { out -> staging.inputStream().use { it.copyTo(out) } }
        } finally {
            staging.delete()
        }
    }
}
