package com.qurb

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract

/**
 * Every file inside a folder the person chose in the system's picker, with
 * its place under that folder: what *+ › A folder* adds (the owner's review
 * of 2026-11-02, V-2). Read through the documents provider, a level at a
 * time, without a library for it.
 */
object TreeFiles {
    class Found(val relative: String, val uri: Uri)

    /** The folder's own name, and its files. Called off the main thread. */
    fun read(context: Context, tree: Uri): Pair<String, List<Found>> {
        val rootId = DocumentsContract.getTreeDocumentId(tree)
        val name = context.contentResolver.query(DocumentsContract.buildDocumentUriUsingTree(tree, rootId),
            arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        } ?: "Folder"
        val found = ArrayList<Found>()
        fun walk(id: String, under: String) {
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
            context.contentResolver.query(children, arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            ), null, null, null)?.use { c ->
                while (c.moveToNext()) {
                    val child = c.getString(0)
                    val childName = c.getString(1)
                    val path = if (under.isEmpty()) childName else "$under/$childName"
                    if (c.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR) walk(child, path)
                    else found += Found(path, DocumentsContract.buildDocumentUriUsingTree(tree, child))
                }
            }
        }
        walk(rootId, "")
        return name to found
    }
}
