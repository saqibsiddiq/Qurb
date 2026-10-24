package com.qurb

import android.graphics.Bitmap
import android.graphics.Color
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
            // Six digits a device on the same Wi-Fi types instead (decision
            // 0066), large, in the number's own style (Approval).
            val digits = TextView(app).apply {
                gravity = Gravity.CENTER
                textSize = 40f
                typeface = androidx.core.content.res.ResourcesCompat.getFont(app, R.font.inter_semibold)
                fontFeatureSettings = "tnum"
                setTextColor(kit.color(R.color.text))
                setTextIsSelectable(true)
            }
            // How long it works for, by a clock: no sentence.
            val left = TextView(app).apply {
                gravity = Gravity.CENTER_VERTICAL
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.text_2))
                compoundDrawablePadding = kit.dp(6)
                setCompoundDrawablesRelativeWithIntrinsicBounds(
                    androidx.core.content.ContextCompat.getDrawable(app, R.drawable.ic_clock)?.mutate()?.apply {
                        setBounds(0, 0, kit.dp(16), kit.dp(16))
                        setTint(kit.color(R.color.text_2))
                    }, null, null, null)
            }
            val status = TextView(app).apply {
                gravity = Gravity.CENTER
                setTextAppearance(R.style.Text_Quiet)
                visibility = android.view.View.GONE
            }
            val body = LinearLayout(app).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_HORIZONTAL
                if (qr != null) {
                    // On white whatever surrounds it, in a card: a scanner
                    // finds a code by its finder patterns against a light
                    // ground.
                    addView(ImageView(app).apply {
                        setImageDrawable(BitmapDrawable(app.resources, draw(qr)).apply { isFilterBitmap = false })
                        scaleType = ImageView.ScaleType.FIT_CENTER
                        background = androidx.core.content.ContextCompat.getDrawable(app, R.drawable.qr_card)
                        setPadding(kit.dp(20), kit.dp(20), kit.dp(20), kit.dp(20))
                        contentDescription = "Pairing code"
                    }, LinearLayout.LayoutParams(kit.dp(250), kit.dp(250)))
                }
                addView(digits, LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
                ).apply { topMargin = kit.dp(24) })
                addView(left, LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT
                ).apply { topMargin = kit.dp(8) })
                addView(status, LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
                ).apply { topMargin = kit.dp(10) })
            }

            // However it closes -- Stop, back, a swipe down -- the code
            // stops working with it.
            val sheet = kit.sheet()
                .view(body, top = 6)
                .onDismiss { offer.cancel() }
            // The long code, for reading out or typing on a computer, is
            // behind *Copy code*: the redesign took it off the screen. Copying
            // leaves the sheet open, or the code copied would already be dead.
            val buttons = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
            buttons.addView(kit.button("Copy code", Kit.Style.SECONDARY) {
                val clip = app.getSystemService(android.content.ClipboardManager::class.java)
                clip?.setPrimaryClip(android.content.ClipData.newPlainText("Qurb code", offer.spoken()))
                // Android 13 and later show their own note of a copy.
                if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.TIRAMISU) app.say("Copied")
            }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
            buttons.addView(kit.button("Stop", Kit.Style.SECONDARY) { sheet.dismiss() },
                LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(12) })
            sheet.view(buttons, top = 28)
            sheet.show()
            val dialog = sheet.dialog
            // A code that goes dark while somebody fetches the other device is
            // a code they have to ask for again.
            dialog.window?.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

            val countdown = launch {
                while (isActive) {
                    val typed = offer.digits()
                    digits.text = typed ?: "——— ———"
                    digits.alpha = if (typed == null) 0.35f else 1f
                    val secs = offer.expiresAt() - System.currentTimeMillis() / 1000
                    left.text = if (secs > 0) "${secs / 60}:${"%02d".format(secs % 60)}" else "0:00"
                    // Said in words only when it stops working, and why.
                    status.text = when {
                        secs <= 0 -> "Expired. Show a new one."
                        typed == null -> "Typed wrongly too often. Scan it instead."
                        else -> ""
                    }
                    status.visibility = if (status.text.isEmpty()) android.view.View.GONE else android.view.View.VISIBLE
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
                if (dialog.isShowing) {
                    status.text = "Not connected: ${e.message ?: "it did not work"}"
                    status.visibility = android.view.View.VISIBLE
                }
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
