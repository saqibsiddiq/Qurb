package com.qurb

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.drawable.BitmapDrawable
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.Pairing
import uniffi.qurb_mobile.QrCode
import uniffi.qurb_mobile.qrCode

/**
 * This phone showing a pairing code, for another device to scan or type.
 *
 * What lets two phones connect with no computer in between (brief §20):
 * before this a phone could only scan a code, so the other device always had
 * to be one that could show one.
 *
 * A page of its own, as the Figma redesign draws *Show my code* (item 19 of
 * the owner's list of 2026-10-26; a sheet until then): the way back, the QR
 * code on a white card, the six digits large, a clock with the time left, and
 * *Copy code* and *Stop*. However it is left -- Stop, back, another tab --
 * the code stops working with it.
 *
 * The QR code is drawn from a matrix the engine computes, onto a canvas, as
 * dots and rounded finders (see [draw]). No image library: the app stays as
 * light as decision 0039 wants it, and the drawing is thirty lines.
 */
class ShowCode(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    private var offer: Pairing? = null
    private var work: Job? = null
    private var gone = false

    // Six digits a device on the same Wi-Fi types instead (decision 0066),
    // large, in the number's own style (Approval).
    private val digits = TextView(app).apply {
        gravity = Gravity.CENTER
        textSize = 40f
        letterSpacing = 0.06f
        typeface = androidx.core.content.res.ResourcesCompat.getFont(app, R.font.inter_semibold)
        fontFeatureSettings = "tnum"
        setTextColor(kit.color(R.color.text))
        setTextIsSelectable(true)
    }

    // How long it works for, by a clock: no sentence.
    private val left = TextView(app).apply {
        gravity = Gravity.CENTER_VERTICAL
        setTextAppearance(R.style.Text_Meta)
        setTextColor(kit.color(R.color.text_2))
        compoundDrawablePadding = kit.dp(6)
        setCompoundDrawablesRelativeWithIntrinsicBounds(
            ContextCompat.getDrawable(app, R.drawable.ic_clock)?.mutate()?.apply {
                setBounds(0, 0, kit.dp(16), kit.dp(16))
                setTint(kit.color(R.color.text_2))
            }, null, null, null)
    }

    private val status = TextView(app).apply {
        gravity = Gravity.CENTER
        setTextAppearance(R.style.Text_Quiet)
        visibility = View.GONE
    }

    private val code = ImageView(app).apply {
        scaleType = ImageView.ScaleType.FIT_CENTER
        contentDescription = "Pairing code"
    }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Home"
        views.back.setTextAppearance(R.style.Text_Name)
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.visibility = View.GONE
        views.refresh.isEnabled = false

        val centre = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
        }
        // On white whatever surrounds it, in a card: a scanner finds a code
        // by its finder patterns against a light ground.
        val card = android.widget.FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.qr_card)
            setPadding(kit.dp(24), kit.dp(24), kit.dp(24), kit.dp(24))
            addView(code, android.widget.FrameLayout.LayoutParams(kit.dp(200), kit.dp(200)))
        }
        centre.addView(card, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        centre.addView(digits, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = kit.dp(28) })
        centre.addView(left, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = kit.dp(10) })

        // The long code, for reading out or typing on a computer, is behind
        // *Copy code*: the redesign took it off the screen. Copying leaves
        // the page open, or the code copied would already be dead.
        val actions = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
        actions.addView(kit.button("Copy code", Kit.Style.SECONDARY) {
            val spoken = offer?.spoken() ?: return@button
            val clip = app.getSystemService(android.content.ClipboardManager::class.java)
            clip?.setPrimaryClip(android.content.ClipData.newPlainText("Qurb code", spoken))
            // Android 13 and later show their own note of a copy.
            if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.TIRAMISU) app.say("Copied")
        }.also(::pill), LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, kit.dp(48)))
        actions.addView(kit.button("Stop", Kit.Style.SECONDARY) { close() }.also(::pill),
            LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, kit.dp(48)).apply { marginStart = kit.dp(12) })
        centre.addView(actions, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = kit.dp(32) })
        centre.addView(status, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = kit.dp(16) })

        views.sections.addView(centre, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = kit.dp(28) })
        start()
    }

    /** Nothing changes under a code but its clock, which [start] keeps. */
    override fun refresh() {}

    private fun start() {
        work = scope.launch {
            val offer = try {
                withContext(Dispatchers.IO) { engine().offerPairing() }
            } catch (e: Exception) {
                app.fail("Could not show a code", e)
                close()
                return@launch
            }
            if (gone) {
                offer.cancel()
                return@launch
            }
            this@ShowCode.offer = offer
            val qr = withContext(Dispatchers.IO) { runCatching { qrCode(offer.code()) }.getOrNull() }
            if (qr != null) {
                code.setImageDrawable(BitmapDrawable(app.resources, draw(qr)))
            } else {
                (code.parent as View).visibility = View.GONE
            }
            // A code that goes dark while somebody fetches the other device
            // is a code they have to ask for again.
            app.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

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
                    status.visibility = if (status.text.isEmpty()) View.GONE else View.VISIBLE
                    delay(1000)
                }
            }

            // A device that uses the code is let in only when the person here
            // approves it, comparing the number it shows (decision 0053).
            val joined = withContext(Dispatchers.IO) { runCatching { offer.wait(Approval.Asker(app)) } }
            countdown.cancel()
            joined.onSuccess { peer ->
                app.say("Connected to ${peer.name}")
                SyncWorker.runNow(app)
                close()
                app.changed()
            }.onFailure { e ->
                // Left by the person: nothing to say. Otherwise, say why.
                if (!gone) {
                    status.text = "Not connected: ${e.message ?: "it did not work"}"
                    status.visibility = View.VISIBLE
                }
            }
        }
    }

    /** Back to where the page was opened from, if it is still showing. */
    private fun close() {
        if (!gone && app.showing(this)) app.onBackPressedDispatcher.onBackPressed()
    }

    override fun left() {
        gone = true
        app.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        offer?.cancel()
        work?.cancel()
    }

    /** One of the two buttons: 120 dp across as Figma draws them, wider
     *  rather than on two lines where the phone's text is larger. */
    private fun pill(button: com.google.android.material.button.MaterialButton) {
        button.minWidth = kit.dp(120)
        button.minimumWidth = kit.dp(120)
        button.maxLines = 1
        button.setPadding(kit.dp(22), 0, kit.dp(22), 0)
    }

    /**
     * The matrix as Figma draws it: each module a dot, and the three finder
     * patterns as rounded squares. Still a code any scanner reads -- the
     * finders keep their 1:1:3:1:1 proportions, and a dot of 86% covers a
     * module's centre, where a scanner samples it. No quiet zone in the
     * picture: the card's 24 dp of white is more than its four modules.
     */
    private fun draw(qr: QrCode): Bitmap {
        val width = qr.width.toInt()
        val m = 16f
        val side = (width * m).toInt()
        val bitmap = Bitmap.createBitmap(side, side, Bitmap.Config.ARGB_8888)
        val canvas = android.graphics.Canvas(bitmap)
        canvas.drawColor(Color.WHITE)
        val ink = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { color = Color.BLACK }
        val paper = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE }
        val finders = listOf(0 to 0, width - 7 to 0, 0 to width - 7)
        fun inFinder(x: Int, y: Int) = finders.any { (fx, fy) -> x in fx until fx + 7 && y in fy until fy + 7 }
        for (y in 0 until width) {
            for (x in 0 until width) {
                if (!qr.dark[y * width + x] || inFinder(x, y)) continue
                canvas.drawCircle((x + 0.5f) * m, (y + 0.5f) * m, m * 0.43f, ink)
            }
        }
        for ((fx, fy) in finders) {
            fun square(inset: Float, size: Float, radius: Float, paint: android.graphics.Paint) =
                canvas.drawRoundRect(
                    (fx + inset) * m, (fy + inset) * m, (fx + inset + size) * m, (fy + inset + size) * m,
                    radius * m, radius * m, paint)
            square(0f, 7f, 2.2f, ink)
            square(1f, 5f, 1.5f, paper)
            square(2f, 3f, 0.9f, ink)
        }
        return bitmap
    }
}
