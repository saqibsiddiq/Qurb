package com.qurb

import android.database.Cursor
import android.database.MatrixCursor
import android.graphics.Point
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.webkit.MimeTypeMap
import uniffi.qurb_mobile.Available
import uniffi.qurb_mobile.FileEntry
import uniffi.qurb_mobile.Qurb
import java.io.File
import java.io.FileNotFoundException

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
 * ## The index, not the disk
 *
 * What is listed is what the engine knows, not what happens to be in the
 * directory. It used to walk the directory, which lost every file freed from
 * this phone -- its bytes are elsewhere, so there was nothing on disk to find --
 * and meant the picker and the app could disagree about what the folder held.
 * A freed file is listed, says it is not on this phone, and is downloaded when
 * opened. The one thing still read from disk is an empty directory somebody
 * just created here, which the index cannot know about until a file is in it.
 *
 * ## The memory rule, restated where it bites hardest
 *
 * `openDocument` is the reason
 * [decision 0018](../../../../../docs/decisions/0018-file-contents-never-cross-the-ffi.md)
 * exists. The system hands back a file descriptor and the reading app may pull
 * gigabytes through it. So no path here holds a file in memory: a file in the
 * folder is opened where it lies; bytes the engine holds without a copy in the
 * folder are exported to a cache file a chunk at a time — peak memory one
 * chunk, at most 2 MiB; and a freed file is brought back by a sync, which
 * writes it into the folder the same way. Assembling a whole file in memory
 * here is what gets an iOS FileProvider extension killed, and would get this
 * process killed too on a phone under pressure.
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

        val context = context ?: return cursor
        val path = pathOf(documentId)
        val engine = engine()
        engine.entry(path)?.let { entry ->
            cursor.addFile(documentId, entry, onDisk(context, path))
            return cursor
        }
        val listed = engine.browse(path)
        if (listed.folders.isNotEmpty() || listed.files.isNotEmpty() || onDisk(context, path).isDirectory) {
            cursor.addDirectory(documentId, path.substringAfterLast('/'))
            return cursor
        }
        throw FileNotFoundException("$path is not in qurb")
    }

    /**
     * The folders and files directly under `parentDocumentId`, from the index.
     *
     * The index is flat -- it stores `album/photo.jpg` as one path -- and the
     * engine turns that into the folders a person expects to open. Folders
     * first, then files, each by name.
     */
    override fun queryChildDocuments(
        parentDocumentId: String,
        projection: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val cursor = MatrixCursor(projection ?: DOCUMENT_COLUMNS)
        val context = context ?: return cursor

        val dir = if (parentDocumentId == ROOT_ID) "" else pathOf(parentDocumentId)
        val listed = engine().browse(dir)
        val prefix = if (dir.isEmpty()) "" else "$dir/"

        // Directories created here that nothing has been saved into yet: on
        // disk and not in the index, which holds files. One directory read,
        // not a walk.
        val empty = onDisk(context, dir).listFiles { f -> f.isDirectory && f.name != ".qurb" }
            ?.map { it.name }.orEmpty()
        (listed.folders + empty).distinct().sortedBy { it.lowercase() }.forEach { name ->
            cursor.addDirectory("$ROOT_ID/$prefix$name", name)
        }
        listed.files.sortedBy { it.path.substringAfterLast('/').lowercase() }.forEach { entry ->
            cursor.addFile("$ROOT_ID/${entry.path}", entry, onDisk(context, entry.path))
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
        // The index's own search: every file in the folder, freed ones
        // included, without a walk of the library while someone waits.
        runCatching { engine().search(query, SEARCH_LIMIT.toUInt()) }.getOrDefault(emptyList())
            .forEach { entry -> cursor.addFile("$ROOT_ID/${entry.path}", entry, onDisk(context, entry.path)) }
        return cursor
    }

    // -- content -------------------------------------------------------------

    /**
     * Hand back a descriptor onto the file's contents.
     *
     * A file on this phone is opened where it lies, and nothing is copied. A
     * file freed from this phone is downloaded first: asked for, and a sync
     * run for up to [OPEN_WAIT] seconds while the opening app shows its own
     * spinner. If no device that has it answers, the open fails and says so;
     * the request stands, and it arrives at the next sync.
     *
     * Writing is allowed onto a file that is on this phone -- which is what a
     * "Save to qurb" from another app needs, having just made one with
     * [createDocument]. When the writer closes it, the folder is scanned and
     * synced, so the new file is indexed and on its way without waiting for
     * the app to be opened.
     */
    override fun openDocument(
        documentId: String,
        mode: String,
        signal: CancellationSignal?,
    ): ParcelFileDescriptor {
        val context = context ?: throw IllegalStateException("no context")
        val path = pathOf(documentId)
        val file = onDisk(context, path)

        if (mode != "r") {
            if (!file.isFile) throw FileNotFoundException("$path is not on this phone to write to")
            val closed = ParcelFileDescriptor.OnCloseListener { SyncWorker.runNow(context) }
            return ParcelFileDescriptor.open(
                file,
                ParcelFileDescriptor.parseMode(mode),
                Handler(Looper.getMainLooper()),
                closed,
            )
        }

        if (!file.isFile) {
            val engine = engine()
            val entry = engine.entry(path) ?: throw FileNotFoundException("$path is not in qurb")
            if (entry.available == Available.NOWHERE) {
                throw FileNotFoundException(
                    "${path.substringAfterLast('/')} is on no device this phone syncs with any more."
                )
            }
            if (entry.available != Available.ELSEWHERE) {
                // The bytes are here without a copy in the folder -- a file
                // removed from it that the next scan has not noticed yet.
                // Exported a chunk at a time; see the class comment on memory.
                val staging = File(context.cacheDir, "open-${path.hashCode()}-${file.name}")
                engine.export(path, staging.absolutePath)
                return ParcelFileDescriptor.open(staging, ParcelFileDescriptor.MODE_READ_ONLY)
            }
            signal?.throwIfCanceled()
            engine.fetch(path)
            blocking { Engine.hearingTheNetwork(context) { engine.syncWithin(OPEN_WAIT) } }
            signal?.throwIfCanceled()
        }
        if (!file.isFile) {
            throw FileNotFoundException(
                "${path.substringAfterLast('/')} is not on this phone, and no device that has " +
                    "it answered. It will download at the next sync."
            )
        }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
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
     * With writing into what it creates, the only change this provider allows.
     * `deleteDocument` and `renameDocument` are deliberately absent: a deletion here becomes a tombstone that
     * propagates to every device, and letting a file manager do that by accident
     * is not a risk worth taking before there is any undo.
     */
    override fun createDocument(parentDocumentId: String, mimeType: String, displayName: String): String {
        val context = context ?: throw IllegalStateException("no context")
        val parent = if (parentDocumentId == ROOT_ID) "" else
            parentDocumentId.removePrefix("$ROOT_ID/") + "/"

        val path = freePath(context, parent, displayName.substringAfterLast('/'))
        val file = onDisk(context, path)
        file.parentFile?.mkdirs()

        if (mimeType == Document.MIME_TYPE_DIR) {
            file.mkdirs()
        } else {
            file.createNewFile()
            // Empty for now. The writing app opens it next and fills it in,
            // and closing it asks for a scan and a sync; see openDocument.
        }
        return "$ROOT_ID/$path"
    }

    /**
     * A name nothing in the folder is using, on disk or in the index.
     *
     * Android leaves this to the provider, and it matters more now that a
     * created file can be written: returning the name of a file that already
     * exists would have the saving app overwrite it, and a freed file with that
     * name has no bytes on disk to collide with. Suffixed the way the app's
     * own imports are, `photo (2).jpg`.
     */
    private fun freePath(context: android.content.Context, parent: String, name: String): String {
        val engine = engine()
        val taken = { candidate: String ->
            onDisk(context, parent + candidate).exists() || engine.entry(parent + candidate) != null
        }
        if (!taken(name)) return parent + name
        val stem = name.substringBeforeLast('.', name)
        val extension = name.substringAfterLast('.', "").let { if (it.isEmpty()) "" else ".$it" }
        for (n in 2..999) {
            val candidate = "$stem ($n)$extension"
            if (!taken(candidate)) return parent + candidate
        }
        return parent + "$stem-${System.currentTimeMillis()}$extension"
    }

    override fun isChildDocument(parentDocumentId: String, documentId: String): Boolean =
        documentId.startsWith(if (parentDocumentId == ROOT_ID) ROOT_ID else "$parentDocumentId/")

    // -- helpers -------------------------------------------------------------

    private fun pathOf(documentId: String): String = documentId.removePrefix("$ROOT_ID/").trim('/')

    /**
     * Where a path is on disk, refusing anything that would land outside the
     * folder or inside the store. Document IDs arrive from other processes;
     * one with `..` in it must not become a way out of the folder, nor one
     * starting `.qurb` a way into the index and the encrypted chunks.
     */
    private fun onDisk(context: android.content.Context, path: String): File {
        val root = Engine.root(context).canonicalFile
        val file = File(root, path).canonicalFile
        val inside = file == root || file.path.startsWith(root.path + File.separator)
        val store = File(root, ".qurb").path
        if (!inside || file.path == store || file.path.startsWith(store + File.separator)) {
            throw SecurityException("$path is outside qurb's folder")
        }
        return file
    }

    private fun engine(): Qurb = blocking { Engine.open(context!!) }

    private fun MatrixCursor.addDirectory(id: String, name: String) {
        newRow().apply {
            add(Document.COLUMN_DOCUMENT_ID, id)
            add(Document.COLUMN_DISPLAY_NAME, name)
            add(Document.COLUMN_MIME_TYPE, Document.MIME_TYPE_DIR)
            add(Document.COLUMN_FLAGS, Document.FLAG_DIR_SUPPORTS_CREATE)
        }
    }

    private fun MatrixCursor.addFile(id: String, entry: FileEntry, onDisk: File) {
        val here = onDisk.isFile
        newRow().apply {
            add(Document.COLUMN_DOCUMENT_ID, id)
            add(Document.COLUMN_DISPLAY_NAME, entry.path.substringAfterLast('/'))
            add(Document.COLUMN_MIME_TYPE, mimeType(entry.path))
            add(Document.COLUMN_SIZE, entry.size.toLong())
            // Nanoseconds in the index, milliseconds here.
            add(Document.COLUMN_LAST_MODIFIED, entry.modifiedAt / 1_000_000)
            add(Document.COLUMN_FLAGS, if (here) Document.FLAG_SUPPORTS_WRITE else 0)
            if (entry.available == Available.ELSEWHERE) {
                add(Document.COLUMN_SUMMARY, "Not on this phone — downloads when opened")
            }
            if (entry.available == Available.NOWHERE) {
                add(Document.COLUMN_SUMMARY, "On no device")
            }
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
            Document.COLUMN_SUMMARY,
        )

        /**
         * How long opening a freed file waits for a device that has it:
         * enough to reach one on the same network and fetch a photo, short
         * enough that the opening app is not left spinning.
         */
        val OPEN_WAIT = 25u
    }
}
