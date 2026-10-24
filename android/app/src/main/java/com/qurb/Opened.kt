package com.qurb

import android.content.Context
import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder

/**
 * A file of a computer's folder, opened on this phone without keeping it:
 * this phone's own folder there (decision 0067) and the computer's shared
 * folder (decision 0073) open their files the same way.
 */
object Opened {

    /**
     * Shown or played here, as Files opens a file (the owner's item of
     * 2026-10-15: a photo or a song in this folder could only be downloaded
     * or restored).
     *
     * A large file is read as the app showing it asks (`StreamProvider`):
     * a video starts at once and seeks where it likes, and nothing of it is
     * kept. The first way, fetching it whole first, kept the S23 waiting on
     * a 1.6 GB episode and filled its cache. A small one is fetched whole and
     * checked into the cache under its content's hash -- so a photo opened
     * again is not fetched again -- and handed over through a FileProvider,
     * readable through that one URI only; the cache is held under
     * [OPENED_MOST] and let go of a day later ([tidy]).
     */
    fun open(app: MainActivity, scope: CoroutineScope, folder: KeptFolder?, file: KeptFile) {
        val type = MimeTypeMap.getSingleton()
            .getMimeTypeFromExtension(file.name.substringAfterLast('.', "").lowercase()) ?: "*/*"
        if (file.size > STREAM_FROM) {
            scope.launch {
                try {
                    val uri = withContext(Dispatchers.IO) {
                        val open = folder ?: error("the folder is closed")
                        StreamProvider.add(app, open.stream(file), file.name, type)
                    }
                    val view = Intent(Intent.ACTION_VIEW).apply {
                        setDataAndType(uri, type)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }
                    app.startActivity(Intent.createChooser(view, "Open with"))
                } catch (e: android.content.ActivityNotFoundException) {
                    app.fail("Nothing on this phone can open ${file.name}", e)
                } catch (e: Exception) {
                    app.fail("Could not open ${file.name}", e)
                }
            }
            return
        }
        val dir = java.io.File(app.cacheDir, "$OPENED/${file.content.take(16)}")
        val name = file.name.substringAfterLast('/')
        val here = java.io.File(dir, name)
        if (!here.exists()) app.say("Fetching $name…")
        scope.launch {
            try {
                val local = withContext(Dispatchers.IO) {
                    if (here.exists()) return@withContext here
                    val open = folder ?: error("the folder is closed")
                    dir.mkdirs()
                    java.io.File(open.download(file, here.path))
                }
                val uri = FileProvider.getUriForFile(app, "${app.packageName}.opened", local)
                scope.launch(Dispatchers.IO) { tidy(app) }
                val view = Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, type)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                app.startActivity(Intent.createChooser(view, "Open with"))
            } catch (e: android.content.ActivityNotFoundException) {
                app.fail("Nothing on this phone can open $name", e)
            } catch (e: Exception) {
                app.fail("Could not open $name", e)
            }
        }
    }

    /** Files opened more than a day ago let go of, and the oldest beyond
     *  [OPENED_MOST] in all. */
    fun tidy(context: Context) {
        val opened = java.io.File(context.cacheDir, OPENED)
        val old = System.currentTimeMillis() - 24L * 60 * 60 * 1000
        val kept = opened.listFiles()?.sortedByDescending { it.lastModified() } ?: return
        var total = 0L
        for (dir in kept) {
            total += dir.walkTopDown().filter { it.isFile }.sumOf { it.length() }
            if (dir.lastModified() < old || total > OPENED_MOST) dir.deleteRecursively()
        }
    }

    /** Under the app's cache: files fetched to be opened (`opened_paths.xml`). */
    private const val OPENED = "opened"
    /** Larger than this, a file is read as it is shown rather than fetched first. */
    private const val STREAM_FROM: ULong = 16_777_216uL
    /** The most the cache of opened files holds. */
    private const val OPENED_MOST = 200L * 1024 * 1024
}
