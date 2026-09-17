package com.qurb

import android.database.Cursor
import android.database.MatrixCursor
import android.graphics.Point
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.webkit.MimeTypeMap
import java.io.File

/**
 * The synced files, visible to the rest of the phone.
 *
 * Without this, everything lives in the app's private directory and only the
 * app's own screen can see it — which makes a file sync product that no other
 * app can open. This puts qurb in the system file picker and the Files app.
 *
 * ## Why this is the shape it is
 *
 * A `DocumentsProvider` is queried by *other* processes, often while this app
 * is not otherwise running, and its calls arrive on binder threads rather than
 * the main thread. So:
 *
 *  - it opens its own engine handle lazily and holds it, rather than assuming
 *    an activity did so;
 *  - it must never block for long. The file picker calls `queryChildDocuments`
 *    while a user is looking at a spinner, so listing reads the index and does
 *    not sync.
 *
 * ## The memory rule, restated where it bites hardest
 *
 * `openDocument` is the reason
 * [decision 0018](../../../../../docs/decisions/0018-file-contents-never-cross-the-ffi.md)
 * exists. The system hands back a file descriptor and the reading app may pull
 * gigabytes through it. Content is exported to a cache file a chunk at a time —
 * peak memory is one chunk, at most 2 MiB — and the descriptor is opened onto
 * that. Assembling a whole file in memory here is what gets an iOS FileProvider
 * extension killed, and would get this process killed too on a phone under
 * pressure.
 */
class QurbDocumentsProvider : DocumentsProvider() {

    override fun onCreate(): Boolean = true

    // -- roots ---------------------------------------------------------------

    override fun queryRoots(projection: Array<out String>?): Cursor {
        val cursor = MatrixCursor(projection ?: ROOT_COLUMNS)

        // A device that has not been set up offers no root at all, rather than
        // an empty one. An empty root in the picker invites someone to try to
        // save into a store that does not exist yet.
        val context = context ?: return cursor
        if (!Engine.isSetUp(context)) return cursor

        cursor.newRow().apply {
            add(Root.COLUMN_ROOT_ID, ROOT_ID)
            add(Root.COLUMN_DOCUMENT_ID, ROOT_ID)
            add(Root.COLUMN_TITLE, "qurb")
            add(Root.COLUMN_SUMMARY, "Synced across your devices")
            add(Root.COLUMN_MIME_TYPES, "*/*")
            add(Root.COLUMN_ICON, R.mipmap.ic_launcher)
            // LOCAL_ONLY is deliberately absent: these files come from other
            // devices, and a picker filtering for local-only content is asking
            // for something this cannot promise.
            add(
                Root.COLUMN_FLAGS,
                Root.FLAG_SUPPORTS_CREATE or Root.FLAG_SUPPORTS_IS_CHILD or
                    Root.FLAG_SUPPORTS_SEARCH,
            )
        }
        return cursor
    }

    // -- listing -------------------------------------------------------------

    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor {
        val cursor = MatrixCursor(projection ?: DOCUMENT_COLUMNS)
        if (documentId == ROOT_ID) {
            cursor.addDirectory(ROOT_ID, "qurb")
            return cursor
        }

        val path = documentId.removePrefix("$ROOT_ID/")
        val file = File(Engine.root(context!!), path)
        if (file.isDirectory) {
            cursor.addDirectory(documentId, file.name)
        } else {
            cursor.addFile(documentId, path, file.length(), file.lastModified())
        }
        return cursor
    }

    /**
     * The files directly under `parentDocumentId`.
     *
     * Read from the filesystem rather than the index, because the index is
     * flat: it stores `album/photo.jpg` as one path, and a picker needs to see
     * `album` as a folder it can open. The two agree — the engine writes what
     * it indexes — and the tree is what a person expects to navigate.
     */
    override fun queryChildDocuments(
        parentDocumentId: String,
        projection: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val cursor = MatrixCursor(projection ?: DOCUMENT_COLUMNS)
        val context = context ?: return cursor

        val root = Engine.root(context)
        val dir = if (parentDocumentId == ROOT_ID) {
            root
        } else {
            File(root, parentDocumentId.removePrefix("$ROOT_ID/"))
        }

        dir.listFiles()
            // The store lives inside the synced root and is not a document.
            // Showing it would invite someone to open, move or delete the index
            // and the encrypted chunks from a file manager.
            ?.filter { it.name != ".qurb" }
            ?.sortedWith(compareBy({ !it.isDirectory }, { it.name.lowercase() }))
            ?.forEach { child ->
                val id = documentId(root, child)
                if (child.isDirectory) {
                    cursor.addDirectory(id, child.name)
                } else {
                    cursor.addFile(id, child.name, child.length(), child.lastModified())
                }
            }
        return cursor
    }

    override fun querySearchDocuments(
        rootId: String,
        query: String,
        projection: Array<out String>?,
    ): Cursor {
        val cursor = MatrixCursor(projection ?: DOCUMENT_COLUMNS)
        val context = context ?: return cursor
        val root = Engine.root(context)

        // Searched against the index rather than by walking the tree: the index
        // already holds every live path and a walk would stat the whole library
        // while someone waits.
        val engine = runCatching { blocking { Engine.open(context) } }.getOrNull() ?: return cursor
        runCatching { engine.list() }.getOrDefault(emptyList())
            .filter { it.path.contains(query, ignoreCase = true) }
            .take(SEARCH_LIMIT)
            .forEach { entry ->
                val file = File(root, entry.path)
                cursor.addFile("$ROOT_ID/${entry.path}", entry.path, entry.size.toLong(), file.lastModified())
            }
        return cursor
    }

    // -- content -------------------------------------------------------------

    /**
     * Hand back a descriptor onto the file's contents.
     *
     * Read-only. Writing through the picker would mean deciding what a partial
     * write means to a sync engine mid-transfer, and "read `r` only" is a
     * smaller promise that can actually be kept.
     */
    override fun openDocument(
        documentId: String,
        mode: String,
        signal: CancellationSignal?,
    ): ParcelFileDescriptor {
        require(mode == "r") { "qurb documents are read-only" }
        val context = context ?: throw IllegalStateException("no context")

        val path = documentId.removePrefix("$ROOT_ID/")
        val direct = File(Engine.root(context), path)

        // The common case: the file is materialised on disk exactly where the
        // engine put it, so the descriptor points straight at it and nothing is
        // copied at all.
        if (direct.isFile) {
            return ParcelFileDescriptor.open(direct, ParcelFileDescriptor.MODE_READ_ONLY)
        }

        // The fallback, for content the index holds without a file beside it.
        // Exported a chunk at a time -- see the class comment about memory.
        val staging = File(context.cacheDir, "open-${path.hashCode()}-${direct.name}")
        val engine = blocking { Engine.open(context) }
        engine.export(path, staging.absolutePath)
        return ParcelFileDescriptor.open(staging, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun openDocumentThumbnail(
        documentId: String,
        sizeHint: Point?,
        signal: CancellationSignal?,
    ): android.content.res.AssetFileDescriptor? = null

    // -- writing -------------------------------------------------------------

    /**
     * Save a file into the synced tree from another app.
     *
     * The only write this provider allows. `deleteDocument` and `renameDocument`
     * are deliberately absent: a deletion here becomes a tombstone that
     * propagates to every device, and letting a file manager do that by accident
     * is not a risk worth taking before there is any undo.
     */
    override fun createDocument(parentDocumentId: String, mimeType: String, displayName: String): String {
        val context = context ?: throw IllegalStateException("no context")
        val parent = if (parentDocumentId == ROOT_ID) "" else
            parentDocumentId.removePrefix("$ROOT_ID/") + "/"

        val path = parent + displayName
        val file = File(Engine.root(context), path)
        file.parentFile?.mkdirs()

        if (mimeType == Document.MIME_TYPE_DIR) {
            file.mkdirs()
        } else {
            file.createNewFile()
            // Empty for now. The writing app opens it next and fills it in;
            // the engine notices at the next scan.
        }
        return "$ROOT_ID/$path"
    }

    override fun isChildDocument(parentDocumentId: String, documentId: String): Boolean =
        documentId.startsWith(if (parentDocumentId == ROOT_ID) ROOT_ID else "$parentDocumentId/")

    // -- helpers -------------------------------------------------------------

    private fun documentId(root: File, file: File): String =
        "$ROOT_ID/${file.absolutePath.removePrefix(root.absolutePath).trimStart('/')}"

    private fun MatrixCursor.addDirectory(id: String, name: String) {
        newRow().apply {
            add(Document.COLUMN_DOCUMENT_ID, id)
            add(Document.COLUMN_DISPLAY_NAME, name)
            add(Document.COLUMN_MIME_TYPE, Document.MIME_TYPE_DIR)
            add(Document.COLUMN_FLAGS, Document.FLAG_DIR_SUPPORTS_CREATE)
        }
    }

    private fun MatrixCursor.addFile(id: String, name: String, size: Long, modified: Long) {
        newRow().apply {
            add(Document.COLUMN_DOCUMENT_ID, id)
            add(Document.COLUMN_DISPLAY_NAME, name.substringAfterLast('/'))
            add(Document.COLUMN_MIME_TYPE, mimeType(name))
            add(Document.COLUMN_SIZE, size)
            add(Document.COLUMN_LAST_MODIFIED, modified)
            add(Document.COLUMN_FLAGS, 0)
        }
    }

    private fun mimeType(name: String): String {
        val extension = name.substringAfterLast('.', "").lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
            ?: "application/octet-stream"
    }

    /**
     * Run a suspending call from a binder thread.
     *
     * A `DocumentsProvider` method has no coroutine scope and must return a
     * value, so there is nothing to do but block. The caller is a binder thread
     * that the system expects to block, which is why the engine's calls are
     * blocking in the first place.
     */
    private fun <T> blocking(block: suspend () -> T): T =
        kotlinx.coroutines.runBlocking { block() }

    private companion object {
        const val ROOT_ID = "qurb"
        const val SEARCH_LIMIT = 200

        val ROOT_COLUMNS = arrayOf(
            Root.COLUMN_ROOT_ID, Root.COLUMN_DOCUMENT_ID, Root.COLUMN_TITLE,
            Root.COLUMN_SUMMARY, Root.COLUMN_MIME_TYPES, Root.COLUMN_ICON, Root.COLUMN_FLAGS,
        )
        val DOCUMENT_COLUMNS = arrayOf(
            Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE,
            Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED, Document.COLUMN_FLAGS,
        )
    }
}
