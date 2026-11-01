package com.qurb

import android.app.Dialog
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.pdf.PdfRenderer
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.style.ForegroundColorSpan
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.ImageButton
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.MediaController
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.TableLayout
import android.widget.TableRow
import android.widget.TextView
import android.widget.VideoView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder

/**
 * Quick preview of every kind of file (the owner's review of 2026-11-02),
 * full screen over the folder, a swipe to the next and the one before:
 * a photo; a video or a song played here, from the computer as it plays; a
 * PDF by its pages; text and code with line numbers; a CSV as a table; Word,
 * a sheet or slides by their pages, turned into a PDF by the computer
 * (LibreOffice there); anything else, its kind and size and *Open in an app*.
 * At the top, close, the name, where it is in the folder, *Open*; at the
 * foot, what the folder page gives -- Share, Copy, Details, Delete.
 */
object Peek {

    /** One file, as the preview sees it. */
    class Item(
        val name: String,
        val size: ULong = 0uL,
        /** Its bytes, here: read from this phone, or fetched from a computer. */
        val local: suspend () -> java.io.File?,
        /** Handed to an app. */
        val open: () -> Unit,
        /** Played as it arrives: a video or a song on a computer. */
        val stream: (suspend () -> Uri?)? = null,
        /** An Office document as a PDF, made by the computer. */
        val convert: (suspend () -> java.io.File?)? = null,
    ) {
        /** The folder page's actions on it, at the foot of the preview. */
        var actions: List<Pair<Pair<Int, String>, () -> Unit>> = emptyList()
    }

    fun shows(name: String) = true

    private val texts = setOf("txt", "md", "log", "json", "xml", "yml", "yaml", "toml", "ini", "conf", "html", "htm", "css",
        "js", "ts", "kt", "java", "rs", "py", "sh", "c", "h", "cpp", "go", "sql", "gradle", "properties", "csv", "tsv")
    private val videos = setOf("mp4", "mkv", "webm", "mov", "3gp", "avi", "m4v")
    private val songs = setOf("mp3", "m4a", "aac", "flac", "ogg", "wav", "opus")
    private val office = setOf("doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp")

    fun kind(name: String): String {
        val e = name.substringAfterLast('.', "").lowercase()
        return when {
            Thumbs.isPicture(name) -> "picture"
            e in videos -> "video"
            e in songs -> "song"
            e == "pdf" -> "pdf"
            e == "csv" || e == "tsv" -> "table"
            e in texts -> "text"
            e in office -> "office"
            else -> "other"
        }
    }

    fun show(app: MainActivity, scope: CoroutineScope, items: List<Item>, at: Int) {
        if (items.getOrNull(at) == null) return
        Viewer(app, scope, items, at).dialog.show()
    }

    /** A Vault or Received file, here or fetched for the preview. */
    fun local(app: MainActivity, entry: uniffi.qurb_mobile.FileEntry) = Item(
        entry.path.substringAfterLast('/'), entry.size,
        local = { java.io.File(Engine.root(app), entry.path).takeIf { it.isFile } },
        open = { app.open(entry) },
    )

    /** A file of a computer's folder, fetched into the cache of opened files. */
    fun remote(app: MainActivity, scope: CoroutineScope, folder: () -> KeptFolder?, file: KeptFile) = Item(
        file.name.substringAfterLast('/'), file.size,
        local = { withContext(Dispatchers.IO) { folder()?.let { Opened.fetched(app, it, file) } } },
        open = { Opened.open(app, scope, folder(), file) },
        stream = {
            withContext(Dispatchers.IO) {
                val open = folder() ?: return@withContext null
                val type = android.webkit.MimeTypeMap.getSingleton()
                    .getMimeTypeFromExtension(file.name.substringAfterLast('.', "").lowercase()) ?: "*/*"
                StreamProvider.add(app, open.stream(file), file.name, type)
            }
        },
        convert = {
            withContext(Dispatchers.IO) {
                val open = folder() ?: return@withContext null
                val to = java.io.File(app.cacheDir, "converted/${file.content.take(16)}.pdf")
                if (to.exists()) return@withContext to
                to.parentFile?.mkdirs()
                if (open.convert(file, to.path)) to else null
            }
        },
    )

    private class Viewer(val app: MainActivity, val scope: CoroutineScope, val items: List<Item>, var at: Int) {
        val dialog = Dialog(app, android.R.style.Theme_Black_NoTitleBar_Fullscreen)
        private val stage = FrameLayout(app)
        private val name = TextView(app).apply {
            setTextColor(Color.WHITE)
            textSize = 16f
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.MIDDLE
        }
        private val count = TextView(app).apply { setTextColor(0xFFBBBBBB.toInt()); textSize = 12f }
        private val foot = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(app.kit.dp(8), app.kit.dp(14), app.kit.dp(8), app.kit.dp(26))
            setBackgroundColor(0xAA000000.toInt())
        }
        private var loading: Job? = null
        private var playing: VideoView? = null
        private var pdf: PdfRenderer? = null

        init {
            val root = FrameLayout(app).apply { setBackgroundColor(Color.BLACK) }
            root.addView(stage, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            val bar = LinearLayout(app).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setPadding(app.kit.dp(8), app.kit.dp(36), app.kit.dp(8), app.kit.dp(8))
                setBackgroundColor(0xAA000000.toInt())
            }
            bar.addView(button(R.drawable.ic_x, "Close") { dialog.dismiss() })
            bar.addView(name, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = app.kit.dp(8) })
            bar.addView(count, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginEnd = app.kit.dp(4) })
            bar.addView(button(R.drawable.ic_external_link, "Open in an app") { dialog.dismiss(); items[at].open() })
            root.addView(bar, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.TOP))
            root.addView(foot, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM))
            // A swipe left is the next, right the one before.
            val swipes = GestureDetector(app, object : GestureDetector.SimpleOnGestureListener() {
                override fun onDown(e: MotionEvent) = true
                override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
                    val dx = e2.x - (e1?.x ?: e2.x)
                    if (items.size < 2 || kotlin.math.abs(dx) < app.kit.dp(60) || kotlin.math.abs(vx) < kotlin.math.abs(vy)) return false
                    step(if (dx < 0) 1 else -1)
                    return true
                }
            })
            stage.setOnTouchListener { _, e -> swipes.onTouchEvent(e) }
            dialog.setContentView(root)
            dialog.setOnDismissListener { stop() }
            show()
        }

        private fun button(icon: Int, said: String, act: () -> Unit) = ImageButton(app).apply {
            setImageResource(icon)
            imageTintList = android.content.res.ColorStateList.valueOf(Color.WHITE)
            background = null
            contentDescription = said
            setPadding(app.kit.dp(12), app.kit.dp(12), app.kit.dp(12), app.kit.dp(12))
            setOnClickListener { act() }
        }

        private fun stop() {
            loading?.cancel()
            playing?.stopPlayback()
            playing = null
            runCatching { pdf?.close() }
            pdf = null
        }

        private fun step(by: Int) {
            at = (at + by + items.size) % items.size
            show()
        }

        private fun show() {
            stop()
            val item = items[at]
            name.text = item.name
            count.text = if (items.size > 1) "${at + 1} of ${items.size}" else ""
            foot.removeAllViews()
            foot.visibility = if (item.actions.isEmpty()) View.GONE else View.VISIBLE
            for ((look, act) in item.actions) {
                foot.addView(LinearLayout(app).apply {
                    orientation = LinearLayout.VERTICAL
                    gravity = Gravity.CENTER
                    addView(ImageView(app).apply {
                        setImageResource(look.first)
                        imageTintList = android.content.res.ColorStateList.valueOf(Color.WHITE)
                    }, LinearLayout.LayoutParams(app.kit.dp(22), app.kit.dp(22)))
                    addView(TextView(app).apply { text = look.second; setTextColor(Color.WHITE); textSize = 11f; setPadding(0, app.kit.dp(4), 0, 0) })
                    setOnClickListener { dialog.dismiss(); act() }
                }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            }
            stage.removeAllViews()
            stage.addView(ProgressBar(app), FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
            val wanted = at
            loading = scope.launch {
                try {
                    when (kind(item.name)) {
                        "video", "song" -> play(item, wanted)
                        "picture" -> {
                            val picture = withContext(Dispatchers.IO) { item.local()?.let { picture(it) } }
                            if (wanted == at) put(picture?.let { image(it) })
                        }
                        "pdf" -> {
                            val file = withContext(Dispatchers.IO) { item.local() }
                            if (wanted == at) put(file?.let { pages(it) })
                        }
                        "office" -> {
                            val file = withContext(Dispatchers.IO) { item.convert?.invoke() }
                            if (wanted == at) put(file?.let { pages(it) } ?: other(item, "Can't show this here: the computer needs LibreOffice to"))
                        }
                        "text", "table" -> {
                            val text = withContext(Dispatchers.IO) { item.local()?.let { start(it) } }
                            if (wanted == at) put(text?.let { if (kind(item.name) == "table") table(it, item.name) else code(it) })
                        }
                        else -> put(other(item, null))
                    }
                } catch (e: Exception) {
                    android.util.Log.w("qurb", "could not preview ${item.name}", e)
                    if (wanted == at) put(null)
                }
            }
        }

        private fun put(view: View?) {
            stage.removeAllViews()
            stage.addView(view ?: TextView(app).apply {
                text = "Can't show this here"
                setTextColor(Color.LTGRAY)
                gravity = Gravity.CENTER
            }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        }

        private fun image(bitmap: Bitmap) = ImageView(app).apply {
            setImageBitmap(bitmap)
            scaleType = ImageView.ScaleType.FIT_CENTER
        }

        /** A video or a song, played here: from the phone, or streamed from the computer. */
        private suspend fun play(item: Item, wanted: Int) {
            val uri = item.stream?.invoke() ?: withContext(Dispatchers.IO) { item.local() }?.let { Uri.fromFile(it) }
            if (wanted != at) return
            if (uri == null) { put(null); return }
            val video = VideoView(app)
            val box = FrameLayout(app)
            if (kind(item.name) == "song") box.addView(TextView(app).apply {
                text = item.name
                setTextColor(Color.WHITE)
                textSize = 18f
                gravity = Gravity.CENTER
            }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            box.addView(video, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
            put(box)
            val controls = MediaController(app)
            controls.setAnchorView(video)
            video.setMediaController(controls)
            video.setVideoURI(uri)
            video.setOnPreparedListener { video.start(); controls.show(0) }
            video.setOnErrorListener { _, _, _ -> put(null); true }
            playing = video
        }

        /** A PDF's pages at the screen's width, scrolled down; the first 30. */
        private suspend fun pages(file: java.io.File): View {
            val renderer = withContext(Dispatchers.IO) {
                PdfRenderer(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY))
            }
            pdf = renderer
            val list = LinearLayout(app).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(app.kit.dp(12), app.kit.dp(96), app.kit.dp(12), app.kit.dp(120))
            }
            val width = app.resources.displayMetrics.widthPixels - app.kit.dp(24)
            count.text = "${renderer.pageCount} pages"
            for (i in 0 until minOf(renderer.pageCount, PAGES)) {
                val bitmap = withContext(Dispatchers.IO) {
                    synchronized(renderer) {
                        renderer.openPage(i).use { page ->
                            val b = Bitmap.createBitmap(width, width * page.height / page.width, Bitmap.Config.ARGB_8888)
                            b.eraseColor(Color.WHITE)
                            page.render(b, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                            b
                        }
                    }
                }
                list.addView(ImageView(app).apply { setImageBitmap(bitmap); adjustViewBounds = true },
                    LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { bottomMargin = app.kit.dp(10) })
            }
            return ScrollView(app).apply { addView(list) }
        }

        /** Text and code: line numbers, tags and keys in colour. */
        private fun code(text: String): View {
            val out = SpannableStringBuilder()
            val key = Regex("(<[/?!]?[A-Za-z][\\w:.-]*|\"[^\"\\n]*\"\\s*:|\\b(fun|val|var|let|const|def|fn|class|import|return|if|else|for|while)\\b)")
            for ((i, line) in text.lines().withIndex()) {
                val numberAt = out.length
                out.append("${i + 1}".padStart(4)).append("  ")
                out.setSpan(ForegroundColorSpan(0xFF6D706A.toInt()), numberAt, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                val lineAt = out.length
                out.append(line).append('\n')
                for (m in key.findAll(line)) {
                    out.setSpan(ForegroundColorSpan(0xFF9CD2B4.toInt()), lineAt + m.range.first, lineAt + m.range.last + 1,
                        Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                }
            }
            val view = TextView(app).apply {
                this.text = out
                setTextColor(0xFFE8E8E3.toInt())
                typeface = Typeface.MONOSPACE
                textSize = 12f
                setPadding(app.kit.dp(12), app.kit.dp(100), app.kit.dp(16), app.kit.dp(120))
                setTextIsSelectable(true)
            }
            return ScrollView(app).apply {
                setBackgroundColor(0xFF1C1D1B.toInt())
                addView(HorizontalScrollView(app).apply { addView(view) })
            }
        }

        /** A CSV as a table, scrolled both ways; its first rows. */
        private fun table(text: String, name: String): View {
            val sep = if (name.endsWith(".tsv", true)) '\t' else ','
            val table = TableLayout(app).apply {
                setBackgroundColor(Color.WHITE)
                setPadding(0, 0, 0, 0)
            }
            for ((i, line) in text.lines().filter { it.isNotEmpty() }.take(ROWS).withIndex()) {
                val row = TableRow(app)
                for (cell in line.split(sep)) row.addView(TextView(app).apply {
                    this.text = cell.trim().trim('"')
                    textSize = 12f
                    setTextColor(Color.BLACK)
                    if (i == 0) { setTypeface(typeface, Typeface.BOLD); setBackgroundColor(0xFFE8F0EC.toInt()) }
                    setPadding(app.kit.dp(10), app.kit.dp(7), app.kit.dp(10), app.kit.dp(7))
                    background = background ?: null
                })
                table.addView(row)
            }
            return ScrollView(app).apply {
                setBackgroundColor(0xFF2B2C2A.toInt())
                setPadding(app.kit.dp(10), app.kit.dp(96), app.kit.dp(10), app.kit.dp(120))
                clipToPadding = false
                addView(HorizontalScrollView(app).apply { addView(table) })
            }
        }

        /** What cannot be shown here: its kind, size, and *Open in an app*. */
        private fun other(item: Item, why: String?): View = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            addView(ImageView(app).apply {
                setImageResource(States.icon(item.name))
                imageTintList = android.content.res.ColorStateList.valueOf(Color.WHITE)
            }, LinearLayout.LayoutParams(app.kit.dp(56), app.kit.dp(56)))
            addView(TextView(app).apply { text = item.name; setTextColor(Color.WHITE); textSize = 17f; gravity = Gravity.CENTER; setPadding(app.kit.dp(24), app.kit.dp(14), app.kit.dp(24), 0) })
            addView(TextView(app).apply {
                text = listOfNotNull(why, "${item.name.substringAfterLast('.', "").uppercase()} ${Words.size(item.size)}").joinToString("\n")
                setTextColor(0xFFAAAAAA.toInt()); textSize = 13f; gravity = Gravity.CENTER; setPadding(app.kit.dp(24), app.kit.dp(6), app.kit.dp(24), 0)
            })
            addView(app.kit.button("Open in an app") { dialog.dismiss(); item.open() },
                LinearLayout.LayoutParams(app.kit.dp(220), app.kit.dp(48)).apply { topMargin = app.kit.dp(18) })
        }

        /** Decoded no larger than the screen. */
        private fun picture(file: java.io.File): Bitmap? {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.path, bounds)
            if (bounds.outWidth <= 0) return null
            val side = maxOf(app.resources.displayMetrics.widthPixels, app.resources.displayMetrics.heightPixels)
            var n = 1
            while (bounds.outWidth / (n * 2) >= side || bounds.outHeight / (n * 2) >= side) n *= 2
            return BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = n })
        }

        /** A text's start: what is shown of it. */
        private fun start(file: java.io.File): String = java.io.FileInputStream(file).use { input ->
            val bytes = ByteArray(TEXT_BYTES)
            val n = input.read(bytes).coerceAtLeast(0)
            String(bytes, 0, n, Charsets.UTF_8)
        }
    }

    /** What of a text is shown: its first 256 KB. */
    private const val TEXT_BYTES = 256 * 1024
    /** A PDF's pages drawn at most. */
    private const val PAGES = 30
    /** A table's rows shown at most. */
    private const val ROWS = 500
}
