package com.qurb

import android.app.Dialog
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.TextView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder

/**
 * Quick preview, as the desktop's (the owner's list of 2026-11-01, item 6):
 * a picture or the start of a text, large, over the folder; a swipe to the
 * next and the one before; *Open* hands it to an app. What it cannot show
 * -- a video, a PDF, anything else -- is opened in an app at once instead,
 * as a tap did before.
 */
object Peek {

    /** One file, as the preview sees it. */
    class Item(
        val name: String,
        /** Its bytes, here: read from this phone, or fetched from a computer. */
        val local: suspend () -> java.io.File?,
        /** Handed to an app. */
        val open: () -> Unit,
    )

    fun shows(name: String) = Thumbs.isPicture(name) || text(name)

    private val texts = setOf("txt", "md", "csv", "log", "json", "xml", "yml", "yaml", "toml", "ini", "conf", "html", "css", "js", "kt", "rs", "py", "sh")
    private fun text(name: String) = name.substringAfterLast('.', "").lowercase() in texts

    /** `items[at]` opened: in the preview, with the others to swipe to, when it
     *  can be shown; else in an app. */
    fun show(app: MainActivity, scope: CoroutineScope, items: List<Item>, at: Int) {
        val shown = items.filter { shows(it.name) }
        val item = items.getOrNull(at) ?: return
        if (!shows(item.name)) {
            item.open()
            return
        }
        Viewer(app, scope, shown, shown.indexOf(item).coerceAtLeast(0)).dialog.show()
    }

    /** A Vault or Received file, here or fetched for the preview. */
    fun local(app: MainActivity, entry: uniffi.qurb_mobile.FileEntry) = Item(
        entry.path.substringAfterLast('/'),
        local = { java.io.File(Engine.root(app), entry.path).takeIf { it.isFile } },
        open = { app.open(entry) },
    )

    /** A file of a computer's folder, fetched into the cache of opened files. */
    fun remote(app: MainActivity, scope: CoroutineScope, folder: () -> KeptFolder?, file: KeptFile) = Item(
        file.name.substringAfterLast('/'),
        local = { folder()?.let { Opened.fetched(app, it, file) } },
        open = { Opened.open(app, scope, folder(), file) },
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
        private var loading: Job? = null

        init {
            val root = FrameLayout(app).apply { setBackgroundColor(Color.BLACK) }
            root.addView(stage, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            val bar = LinearLayout(app).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setPadding(app.kit.dp(8), app.kit.dp(36), app.kit.dp(8), app.kit.dp(8))
                setBackgroundColor(0x99000000.toInt())
            }
            bar.addView(button(R.drawable.ic_x, "Close") { dialog.dismiss() })
            bar.addView(name, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = app.kit.dp(8) })
            bar.addView(button(R.drawable.ic_external_link, "Open") { dialog.dismiss(); items[at].open() })
            root.addView(bar, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.TOP))
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
            dialog.setOnDismissListener { loading?.cancel() }
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

        private fun step(by: Int) {
            at = (at + by + items.size) % items.size
            show()
        }

        private fun show() {
            val item = items[at]
            name.text = if (items.size > 1) "${item.name}  (${at + 1}/${items.size})" else item.name
            stage.removeAllViews()
            stage.addView(ProgressBar(app), FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
            loading?.cancel()
            val wanted = at
            loading = scope.launch {
                val look = try {
                    withContext(Dispatchers.IO) {
                        val file = item.local() ?: return@withContext null
                        if (Thumbs.isPicture(item.name)) picture(file) else java.io.FileInputStream(file).use { input ->
                            val bytes = ByteArray(TEXT_BYTES)
                            val n = input.read(bytes).coerceAtLeast(0)
                            String(bytes, 0, n, Charsets.UTF_8)
                        }
                    }
                } catch (e: Exception) {
                    android.util.Log.w("qurb", "could not preview ${item.name}", e)
                    null
                }
                if (wanted != at) return@launch
                stage.removeAllViews()
                when (look) {
                    is Bitmap -> stage.addView(ImageView(app).apply {
                        setImageBitmap(look)
                        scaleType = ImageView.ScaleType.FIT_CENTER
                    }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                    is String -> stage.addView(ScrollView(app).apply {
                        addView(TextView(app).apply {
                            text = look
                            setTextColor(Color.WHITE)
                            typeface = android.graphics.Typeface.MONOSPACE
                            textSize = 13f
                            setPadding(app.kit.dp(16), app.kit.dp(100), app.kit.dp(16), app.kit.dp(32))
                        })
                    }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                    else -> stage.addView(TextView(app).apply {
                        text = "Can't show this here"
                        setTextColor(Color.LTGRAY)
                        gravity = Gravity.CENTER
                    }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                }
            }
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
    }

    /** What of a text is shown: its start, as the desktop's preview. */
    private const val TEXT_BYTES = 4096
}
