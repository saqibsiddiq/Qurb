package com.qurb

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Typeface
import android.view.View
import android.widget.ImageView
import android.widget.TextView
import java.io.File

/**
 * What a version of a file looks like, beside the other one when two devices
 * changed it at once (brief §2): an image, or the start of a text, read from
 * this phone's folder. Nothing for anything else, or for a version not here.
 *
 * Read off the main thread by the caller; an image decoded at a fraction of
 * its size, since a preview is a few hundred pixels wide.
 */
object Previews {
    sealed interface Look {
        class Image(val bitmap: Bitmap) : Look
        class Text(val text: String) : Look
    }

    private val images = setOf("jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif")
    private const val TEXT_BYTES = 1024
    private const val WIDE = 480

    fun of(context: Context, path: String): Look? {
        val file = File(Engine.root(context), path)
        if (!file.isFile) return null
        val ext = path.substringAfterLast('.', "").lowercase()
        return if (ext in images) image(file) else text(file)
    }

    private fun image(file: File): Look? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        if (bounds.outWidth <= 0) return null
        var sample = 1
        while (bounds.outWidth / (sample * 2) >= WIDE) sample *= 2
        val bitmap = BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample })
        return bitmap?.let { Look.Image(it) }
    }

    private fun text(file: File): Look? {
        val start = file.inputStream().use { input ->
            val buffer = ByteArray(TEXT_BYTES)
            val n = input.read(buffer)
            if (n <= 0) return null
            buffer.copyOf(n)
        }
        if (start.any { it == 0.toByte() }) return null
        // Text if it decodes as UTF-8 strictly, allowing only a character cut
        // off by the end of what was read.
        for (cut in 0..3) {
            if (cut >= start.size) break
            val text = runCatching {
                Charsets.UTF_8.newDecoder().decode(java.nio.ByteBuffer.wrap(start, 0, start.size - cut)).toString()
            }.getOrNull()
            if (text != null) return Look.Text(text)
        }
        return null
    }

    fun view(context: Context, look: Look): View = when (look) {
        is Look.Image -> ImageView(context).apply {
            setImageBitmap(look.bitmap)
            scaleType = ImageView.ScaleType.FIT_CENTER
            contentDescription = "This version"
        }
        is Look.Text -> TextView(context).apply {
            text = look.text
            typeface = Typeface.MONOSPACE
            textSize = 11f
            maxLines = 9
            setTextColor(context.getColor(R.color.text_2))
            val pad = (8 * resources.displayMetrics.density).toInt()
            setPadding(pad, pad, pad, pad)
        }
    }
}
