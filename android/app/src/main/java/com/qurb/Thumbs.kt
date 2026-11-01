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

    /** Into `icon`, once it is read: a file of this phone, at `path` in its folder. */
    fun local(scope: CoroutineScope, icon: ImageView, path: String, modifiedAt: Long) {
        if (!isPicture(path)) return
        val key = "local:$path:$modifiedAt"
        show(scope, icon, key) {
            val file = java.io.File(Engine.root(icon.context), path)
            if (file.isFile) decode(file.path) else null
        }
    }

    /** Into `icon`, once it is read: a file of a computer's folder. */
    fun remote(scope: CoroutineScope, icon: ImageView, folder: () -> KeptFolder?, file: KeptFile) {
        if (!isPicture(file.name)) return
        show(scope, icon, "kept:${file.content}") {
            val open = folder() ?: return@show null
            val stream = open.stream(file)
            val jpeg = file.name.substringAfterLast('.').lowercase().let { it == "jpg" || it == "jpeg" }
            if (jpeg) {
                val start = stream.readAt(0uL, minOf(file.size, EXIF_BYTES.toULong()).toUInt())
                exifThumb(start)?.let { return@show it }
            }
            if (file.size.toLong() > WHOLE_MOST) return@show null
            val bytes = stream.readAt(0uL, file.size.toUInt())
            decode(bytes)
        }
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
