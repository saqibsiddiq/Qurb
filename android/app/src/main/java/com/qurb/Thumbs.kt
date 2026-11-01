package com.qurb

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.LruCache
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder

/**
 * The small pictures in the place of a file's icon, as the desktop's gallery
 * has them (the owner's list of 2026-11-01, item 6). A photo of this phone's
 * Vault or Received is decoded from the phone, small. One on a computer is
 * read in part: a camera's JPEG carries a thumbnail in its first bytes, and
 * only those are asked for (`KeptStream::read_at`). Any other picture there
 * is fetched whole only when it is small. Held in memory for this run, by
 * the file's content or its path and time.
 */
object Thumbs {
    private val held = object : LruCache<String, Bitmap>(24 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap) = value.byteCount
    }
    /** Known to have none: not asked again this run. */
    private val none = java.util.Collections.newSetFromMap(java.util.concurrent.ConcurrentHashMap<String, Boolean>())

    /** Three at a time, as the desktop's gallery asks: what is in view first. */
    @OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
    private val reading = Dispatchers.IO.limitedParallelism(3)

    private val pictures = setOf("jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif")

    fun isPicture(name: String) = name.substringAfterLast('.', "").lowercase() in pictures

    /** The side a thumbnail is decoded for, in pixels. */
    private const val SIDE = 192
    /** What of a JPEG on a computer is read for the thumbnail it carries. */
    private const val EXIF_BYTES = 96 * 1024
    /** Past this, a picture on a computer with no thumbnail of its own is not fetched for one. */
    private const val WHOLE_MOST = 3L * 1024 * 1024
    /** Past this, a PDF on a computer is shown by its kind, not fetched for its first page. */
    private const val PDF_MOST = 8L * 1024 * 1024
    /** What of a text is read for its first lines. */
    private const val TEXT_BYTES = 1536

    /** Into `icon`, once it is read: a file of this phone, at `path` in its
     *  folder. Every kind has one (the owner's review of 2026-11-02): a
     *  photo; a video's frame and length; a song's cover; a PDF's first page;
     *  text's first lines; an app's icon; else its kind written on a tile. */
    fun local(scope: CoroutineScope, icon: ImageView, path: String, modifiedAt: Long) {
        val key = "local:$path:$modifiedAt"
        val context = icon.context
        show(scope, icon, key) {
            val file = java.io.File(Engine.root(context), path)
            if (!file.isFile) return@show null
            when (Peek.kind(path)) {
                "picture" -> decode(file.path)
                "video" -> media(context) { it.setDataSource(file.path) }?.let { framed(it.first, it.second) }
                "song" -> media(context) { it.setDataSource(file.path) }?.first
                "pdf" -> page(file)
                "text", "table" -> text(java.io.FileInputStream(file).use { input ->
                    val buf = ByteArray(TEXT_BYTES); val n = input.read(buf).coerceAtLeast(0); buf.copyOf(n) })
                else -> if (path.endsWith(".apk", true)) app(context, file) ?: tile(path) else tile(path)
            }
        }
    }

    /** Into `icon`, once it is read: a file of a computer's folder, read in
     *  part where that is enough -- a JPEG's own thumbnail, a video's frame
     *  read as a player reads it -- else whole when small. */
    fun remote(scope: CoroutineScope, icon: ImageView, folder: () -> KeptFolder?, file: KeptFile) {
        val context = icon.context
        show(scope, icon, "kept:${file.content}") {
            val open = folder() ?: return@show null
            when (Peek.kind(file.name)) {
                "picture" -> {
                    val stream = open.stream(file)
                    val jpeg = file.name.substringAfterLast('.').lowercase().let { it == "jpg" || it == "jpeg" }
                    if (jpeg) {
                        val start = stream.readAt(0uL, minOf(file.size, EXIF_BYTES.toULong()).toUInt())
                        exifThumb(start)?.let { return@show it }
                    }
                    if (file.size.toLong() > WHOLE_MOST) return@show null
                    decode(stream.readAt(0uL, file.size.toUInt()))
                }
                "video" -> media(context) { it.setDataSource(StreamSource(open.stream(file))) }?.let { framed(it.first, it.second) }
                "song" -> media(context) { it.setDataSource(StreamSource(open.stream(file))) }?.first
                "pdf" -> if (file.size.toLong() > PDF_MOST) tile(file.name) else {
                    val local = java.io.File(context.cacheDir, "thumbs/${file.content.take(16)}.pdf")
                    local.parentFile?.mkdirs()
                    if (!local.exists()) local.writeBytes(open.stream(file).readAt(0uL, file.size.toUInt()))
                    page(local)
                }
                "office" -> {
                    val local = java.io.File(context.cacheDir, "converted/${file.content.take(16)}.pdf")
                    local.parentFile?.mkdirs()
                    if (local.exists() || open.convert(file, local.path)) page(local) else tile(file.name)
                }
                "text", "table" -> text(open.stream(file).readAt(0uL, minOf(file.size, TEXT_BYTES.toULong()).toUInt()))
                else -> tile(file.name)
            }
        }
    }

    /** A video's or a song's: its frame or cover, and its length. */
    private fun media(context: android.content.Context, source: (android.media.MediaMetadataRetriever) -> Unit): Pair<Bitmap, Long>? {
        val r = android.media.MediaMetadataRetriever()
        return try {
            source(r)
            val length = r.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0L
            val picture = r.embeddedPicture?.let { decode(it) }
                ?: r.getScaledFrameAtTime(minOf(1_000_000L, length * 500L), android.media.MediaMetadataRetriever.OPTION_CLOSEST_SYNC, SIDE, SIDE)
            picture?.let { it to length }
        } catch (_: Exception) {
            null
        } finally {
            runCatching { r.release() }
        }
    }

    /** A frame with its length in its corner: 41:30. */
    private fun framed(frame: Bitmap, millis: Long): Bitmap {
        val side = minOf(frame.width, frame.height)
        val square = Bitmap.createBitmap(frame, (frame.width - side) / 2, (frame.height - side) / 2, side, side)
        val out = Bitmap.createScaledBitmap(square, SIDE, SIDE, true).copy(Bitmap.Config.ARGB_8888, true)
        if (millis <= 0) return out
        val s = millis / 1000
        val words = if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
        val canvas = android.graphics.Canvas(out)
        val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { textSize = 26f; color = android.graphics.Color.WHITE }
        val w = paint.measureText(words)
        val back = android.graphics.Paint().apply { color = 0xAA000000.toInt() }
        canvas.drawRoundRect(SIDE - w - 18f, SIDE - 40f, SIDE - 4f, SIDE - 4f, 8f, 8f, back)
        canvas.drawText(words, SIDE - w - 11f, SIDE - 13f, paint)
        return out
    }

    /** A PDF's first page, as much of it as a square shows. */
    private fun page(file: java.io.File): Bitmap? = runCatching {
        android.graphics.pdf.PdfRenderer(android.os.ParcelFileDescriptor.open(file, android.os.ParcelFileDescriptor.MODE_READ_ONLY)).use { pdf ->
            pdf.openPage(0).use { p ->
                val out = Bitmap.createBitmap(SIDE, SIDE * p.height / p.width, Bitmap.Config.ARGB_8888)
                out.eraseColor(android.graphics.Color.WHITE)
                p.render(out, null, null, android.graphics.pdf.PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                out
            }
        }
    }.getOrNull()

    /** Text's first lines, small, on white. */
    private fun text(bytes: ByteArray): Bitmap {
        val out = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888)
        out.eraseColor(android.graphics.Color.WHITE)
        val canvas = android.graphics.Canvas(out)
        val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            textSize = 15f
            color = 0xFF555555.toInt()
            typeface = android.graphics.Typeface.MONOSPACE
        }
        val lines = String(bytes, Charsets.UTF_8).replace('\t', ' ').lines().take(11)
        for ((i, line) in lines.withIndex()) canvas.drawText(line.take(22), 10f, 24f + i * 16.5f, paint)
        return out
    }

    /** An app's own icon, from its .apk. */
    private fun app(context: android.content.Context, file: java.io.File): Bitmap? = runCatching {
        val pm = context.packageManager
        val info = pm.getPackageArchiveInfo(file.path, 0)?.applicationInfo ?: return null
        info.sourceDir = file.path
        info.publicSourceDir = file.path
        val drawable = info.loadIcon(pm)
        val out = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888)
        out.eraseColor(android.graphics.Color.WHITE)
        drawable.setBounds(SIDE / 5, SIDE / 5, SIDE * 4 / 5, SIDE * 4 / 5)
        drawable.draw(android.graphics.Canvas(out))
        out
    }.getOrNull()

    /** Anything else: its kind written on a white tile, ZIP. */
    private fun tile(name: String): Bitmap? {
        val ext = name.substringAfterLast('.', "").uppercase().take(4)
        if (ext.isEmpty()) return null
        val out = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888)
        out.eraseColor(android.graphics.Color.WHITE)
        val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            textSize = 40f
            color = 0xFF5F625C.toInt()
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT, android.graphics.Typeface.BOLD)
            textAlign = android.graphics.Paint.Align.CENTER
        }
        android.graphics.Canvas(out).drawText(ext, SIDE / 2f, SIDE / 2f + 14f, paint)
        return out
    }

    /** A computer's file read as a media player reads it: the parts asked for. */
    private class StreamSource(private val stream: uniffi.qurb_mobile.KeptStream) : android.media.MediaDataSource() {
        override fun readAt(position: Long, buffer: ByteArray, offset: Int, size: Int): Int {
            val total = stream.size().toLong()
            if (position >= total) return -1
            val n = minOf(size.toLong(), total - position).toInt()
            if (n <= 0) return 0
            val bytes = stream.readAt(position.toULong(), n.toUInt())
            System.arraycopy(bytes, 0, buffer, offset, bytes.size)
            return bytes.size
        }

        override fun getSize(): Long = stream.size().toLong()
        override fun close() {}
    }

    /** A bitmap made the tile's picture: filling it, its corners its own. */
    fun put(icon: ImageView, bitmap: Bitmap) {
        icon.imageTintList = null
        icon.scaleType = ImageView.ScaleType.CENTER_CROP
        icon.setImageBitmap(bitmap)
        icon.layoutParams = (icon.layoutParams as? FrameLayout.LayoutParams ?: FrameLayout.LayoutParams(0, 0)).apply {
            width = ViewGroup.LayoutParams.MATCH_PARENT
            height = ViewGroup.LayoutParams.MATCH_PARENT
        }
        (icon.parent as? ViewGroup)?.clipToOutline = true
    }

    /** A recycled row given back its icon's size before it is bound again. */
    fun reset(icon: ImageView, side: Int) {
        icon.tag = null
        icon.scaleType = ImageView.ScaleType.FIT_CENTER
        icon.layoutParams = (icon.layoutParams as FrameLayout.LayoutParams).apply { width = side; height = side }
    }

    private fun show(scope: CoroutineScope, icon: ImageView, key: String, read: suspend () -> Bitmap?) {
        icon.tag = key
        held.get(key)?.let { put(icon, it); return }
        if (key in none) return
        scope.launch {
            val got = withContext(reading) {
                // Scrolled past, or the row bound to another file: not read.
                if (icon.tag != key) return@withContext null
                runCatching { read() }.getOrNull()
            }
            if (got == null) {
                if (icon.tag == key) none += key
                return@launch
            }
            held.put(key, got)
            if (icon.tag == key) put(icon, got)
        }
    }

    private fun decode(path: String): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(path, bounds)
        if (bounds.outWidth <= 0) return null
        return BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample(bounds) })
    }

    private fun decode(bytes: ByteArray): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0) return null
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample(bounds) })
    }

    private fun sample(bounds: BitmapFactory.Options): Int {
        var n = 1
        while (bounds.outWidth / (n * 2) >= SIDE && bounds.outHeight / (n * 2) >= SIDE) n *= 2
        return n
    }

    /** The thumbnail a JPEG's EXIF block carries, from its first bytes. */
    private fun exifThumb(start: ByteArray): Bitmap? = runCatching {
        val exif = android.media.ExifInterface(java.io.ByteArrayInputStream(start))
        exif.thumbnailBitmap
    }.getOrNull()
}

/**
 * Files as a gallery, three to a row of square pictures, as the desktop
 * shows a folder that is mostly photos -- a phone's camera folder, a
 * holiday. Each square is a file's thumbnail, its kind's icon until then.
 */
object Gallery {
    /** Whether `names` are mostly pictures: six or more, two in three of them. */
    fun suits(names: List<String>) = names.size >= 6 && names.count { Thumbs.isPicture(it) } * 3 >= names.size * 2

    /** A square for each of `count`, in rows of three, added to `parent`;
     *  `bind` fills each with its picture and what a tap and a hold do. */
    fun grid(app: MainActivity, parent: ViewGroup, count: Int, bind: (Int, ImageView, View) -> Unit) {
        val gap = app.kit.dp(4)
        for (start in 0 until count step 3) {
            val row = android.widget.LinearLayout(app).apply { orientation = android.widget.LinearLayout.HORIZONTAL }
            for (i in start until start + 3) {
                val cell = SquareFrame(app).apply {
                    background = androidx.core.content.ContextCompat.getDrawable(app, R.drawable.file_tile)
                    clipToOutline = true
                }
                row.addView(cell, android.widget.LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                    if (i % 3 != 0) marginStart = gap
                })
                if (i >= count) {
                    cell.visibility = View.INVISIBLE
                    continue
                }
                val picture = ImageView(app).apply { importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO }
                cell.addView(picture, FrameLayout.LayoutParams(app.kit.dp(22), app.kit.dp(22), android.view.Gravity.CENTER))
                cell.foreground = androidx.core.content.ContextCompat.getDrawable(app, R.drawable.row_bg)
                bind(i, picture, cell)
            }
            parent.addView(row, android.widget.LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { topMargin = gap })
        }
    }

    /** As tall as it is wide. */
    private class SquareFrame(context: android.content.Context) : FrameLayout(context) {
        override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) = super.onMeasure(widthMeasureSpec, widthMeasureSpec)
    }
}
