package com.qurb

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.BitmapDrawable
import android.view.Gravity
import android.view.WindowManager
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.QrCode
import uniffi.qurb_mobile.qrCode

/**
 * This phone showing a pairing code, for another device to scan or type.
 *
 * What lets two phones connect with no computer in between (brief §20):
 * before this a phone could only scan a code, so the other device always had
 * to be one that could show one.
 *
 * The QR code is drawn from a matrix the engine computes, into a bitmap, one
 * pixel per module and scaled without smoothing. No image library: the app
 * stays as light as decision 0039 wants it, and the drawing is twenty lines.
 */
object ShowCode {

    fun show(app: MainActivity) {
        app.lifecycleScope.launch {
            val offer = try {
                withContext(Dispatchers.IO) { Engine.open(app).offerPairing() }
            } catch (e: Exception) {
                app.fail("Could not show a code", e)
                return@launch
            }
            val qr = withContext(Dispatchers.IO) { runCatching { qrCode(offer.code()) }.getOrNull() }

            val kit = app.kit
            val status = TextView(app).apply {
                gravity = Gravity.CENTER
                setTextAppearance(R.style.Text_Quiet)
            }
            val body = LinearLayout(app).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_HORIZONTAL
                if (qr != null) {
                    // On a white tile whatever surrounds it: a scanner finds a
                    // code by its finder patterns against a light ground.
                    val size = kit.dp(232)
                    addView(ImageView(app).apply {
                        setImageDrawable(BitmapDrawable(app.resources, draw(qr)).apply { isFilterBitmap = false })
                        scaleType = ImageView.ScaleType.FIT_CENTER
                        setBackgroundColor(Color.WHITE)
                        setPadding(kit.dp(12), kit.dp(12), kit.dp(12), kit.dp(12))
                        clipToOutline = true
                        outlineProvider = object : android.view.ViewOutlineProvider() {
                            override fun getOutline(view: android.view.View, outline: android.graphics.Outline) {
                                outline.setRoundRect(0, 0, view.width, view.height, kit.dp(18).toFloat())
                            }
                        }
                    }, LinearLayout.LayoutParams(size, size))
                }
                addView(status, LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
                ).apply { topMargin = kit.dp(14) })
                addView(TextView(app).apply {
                    text = "Or read this out, or type it:"
                    setTextAppearance(R.style.Text_Meta)
                    setPadding(0, kit.dp(18), 0, kit.dp(4))
                })
                addView(TextView(app).apply {
                    text = offer.spoken()
                    typeface = Typeface.MONOSPACE
                    textSize = 13f
                    setTextIsSelectable(true)
                    setTextColor(kit.color(R.color.text))
                })
            }

            val sheet = kit.sheet()
                .header(R.drawable.ic_qr_code, "Scan this on the other device", "It works once, for five minutes")
                .view(body, top = 18)
                // However it closes -- the button, back, a swipe down -- the
                // code stops working with it.
                .onDismiss { offer.cancel() }
            sheet.buttons("Done", secondary = "Stop showing it") {}
            sheet.show()
            val dialog = sheet.dialog
            // A code that goes dark while somebody fetches the other device is
            // a code they have to ask for again.
            dialog.window?.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

            val countdown = launch {
                while (isActive) {
                    val left = offer.expiresAt() - System.currentTimeMillis() / 1000
                    status.text = if (left > 0) {
                        "Waiting — this code works for ${left / 60}:${"%02d".format(left % 60)}"
                    } else {
                        "That code has expired. Close this and show a new one."
                    }
                    delay(1000)
                }
            }

            // A device that uses the code is let in only when the person here
            // approves it, comparing the number it shows (decision 0053).
            val joined = withContext(Dispatchers.IO) { runCatching { offer.wait(Approval.Asker(app)) } }
            countdown.cancel()
            joined.onSuccess { peer ->
                dialog.dismiss()
                app.say("Connected to ${peer.name}")
                SyncWorker.runNow(app)
                app.changed()
            }.onFailure { e ->
                // Closed by the person: nothing to say. Otherwise, say why.
                if (dialog.isShowing) status.text = "Not connected: ${e.message ?: "it did not work"}"
            }
        }
    }

    /** The matrix as a bitmap, with the four-module quiet zone scanners need. */
    private fun draw(qr: QrCode): Bitmap {
        val quiet = 4
        val width = qr.width.toInt()
        val side = width + quiet * 2
        val pixels = IntArray(side * side) { Color.WHITE }
        for (y in 0 until width) {
            for (x in 0 until width) {
                if (qr.dark[y * width + x]) pixels[(y + quiet) * side + x + quiet] = Color.BLACK
            }
        }
        return Bitmap.createBitmap(pixels, side, side, Bitmap.Config.ARGB_8888)
    }
}
