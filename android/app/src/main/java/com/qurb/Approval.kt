package com.qurb

import android.app.Dialog
import android.content.res.ColorStateList
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.res.ResourcesCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import com.qurb.databinding.WaitingForBinding
import uniffi.qurb_mobile.PairingApprover
import uniffi.qurb_mobile.PairingRequest
import uniffi.qurb_mobile.PairingWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Pairing, approved by the person holding both devices (decision 0053), as
 * Pairing draft 1 draws it (docs/design/redesign.md).
 *
 * A code on a screen can be seen or overheard, and whoever uses it first used
 * to get in -- since decision 0052, with the key. Now the device showing the
 * code asks first: who wants in, and the six digits it should be showing.
 * Both screens derive the number from both devices' certificates and the
 * code, so a device that took the code from somebody else's screen shows a
 * different one.
 */
object Approval {

    /**
     * Asks the person at this phone, which is showing a code, whether to let a
     * device in: *Pair with saqib?*, or *Let saqib join?* for a device that
     * takes this phone's key, which a chip says. The engine calls it on a
     * background thread and it blocks that thread until they answer -- or for
     * as long as a code lives, five minutes, after which the answer is no and
     * the sheet goes. A sheet swiped away is a no.
     */
    class Asker(private val app: AppCompatActivity) : PairingApprover {
        override fun approve(request: PairingRequest): Boolean {
            val answered = CountDownLatch(1)
            var yes = false
            var sheet: Sheet? = null
            app.runOnUiThread {
                val kit = Kit(app)
                val name = request.name
                sheet = kit.sheet()
                    .mark(States.device(name, request.kind), fill = R.color.green_soft, tint = R.color.green)
                    .title(if (request.wantsKey) "Let $name join?" else "Pair with $name?", centred = true)
                    .view(number(app, request.number), top = 16)
                    .view(sameNumber(app, kit, name), top = 10)
                    .apply { if (request.wantsKey) warning("It gets this phone's key", R.drawable.ic_key_round, compact = true) }
                    // A button dismisses the sheet before its action runs, and
                    // the dismissal is told after, so a yes is set before the
                    // latch opens; the swipe opens it with none.
                    .buttons("Approve", secondary = "Decline", onSecondary = { answered.countDown() }) {
                        yes = true
                        answered.countDown()
                    }
                    .onDismiss { answered.countDown() }
                    .show()
            }
            val inTime = answered.await(5, TimeUnit.MINUTES)
            if (!inTime) app.runOnUiThread { sheet?.dismiss() }
            return yes
        }
    }

    /** Six digits to compare, large, on the second surface. */
    private fun number(app: AppCompatActivity, number: String) = TextView(app).apply {
        text = number
        contentDescription = number.replace(" ", "")
        gravity = Gravity.CENTER
        textSize = 38f
        typeface = ResourcesCompat.getFont(app, R.font.inter_semibold)
        fontFeatureSettings = "tnum"
        letterSpacing = 0.08f
        setTextColor(app.getColor(R.color.text))
        background = app.getDrawable(R.drawable.group_quiet)
        minHeight = (82 * resources.displayMetrics.density).toInt()
    }

    /** *The same number on saqib?*, small, under it, after a tick. */
    private fun sameNumber(app: AppCompatActivity, kit: Kit, name: String): View =
        LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            addView(ImageView(app).apply {
                setImageResource(R.drawable.ic_circle_check)
                imageTintList = ColorStateList.valueOf(kit.color(R.color.text_3))
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }, LinearLayout.LayoutParams(kit.dp(16), kit.dp(16)).apply { marginEnd = kit.dp(6) })
            addView(TextView(app).apply {
                text = "The same number on $name?"
                setTextAppearance(R.style.Text_Meta)
            })
        }

    /**
     * The page while this phone joins: the number, and where to approve it --
     * *Approve on saqib*, round a laptop or a phone, by the name the device
     * showing the code gave (decision 0074); *your other device* when it gave
     * none. Setup's last step and the page over Devices.
     */
    fun fill(page: WaitingForBinding, with: PairingWith) {
        val name = with.name
        page.waitTitle.text = if (name != null) "Approve on $name" else "Approve on your other device"
        page.waitSays.text = if (name != null) "$name shows this number too" else "It shows this number too"
        page.waitNumber.text = with.number
        page.waitNumber.contentDescription = with.number.replace(" ", "")
        page.waitMark.show(StatusMark.Look.SYNCING, when (with.kind) {
            "computer", "replica" -> R.drawable.mark_laptop
            "phone" -> R.drawable.mark_smartphone
            else -> R.drawable.mark_monitor_smartphone
        })
    }

    /**
     * [fill]'s page over the whole app while this phone, already set up,
     * joins from Devices; dismissed by the caller when the join ends. Back
     * does nothing: the engine cannot call a join off midway, as in setup.
     */
    fun showWhileJoining(app: AppCompatActivity, with: PairingWith): Dialog {
        val page = WaitingForBinding.inflate(app.layoutInflater)
        fill(page, with)
        val gutter = app.resources.getDimensionPixelSize(R.dimen.gutter)
        val foot = (24 * app.resources.displayMetrics.density).toInt()
        val frame = FrameLayout(app).apply { addView(page.root) }
        // Under the bars, as every page is since Android 15 draws edge to
        // edge, and padded clear of them.
        ViewCompat.setOnApplyWindowInsetsListener(frame) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(gutter + bars.left, foot + bars.top, gutter + bars.right, foot + bars.bottom)
            insets
        }
        return Dialog(app, R.style.Theme_Qurb).apply {
            setContentView(frame)
            setCancelable(false)
            window?.let { window ->
                WindowCompat.setDecorFitsSystemWindows(window, false)
                Appearance.paintEnvironment(app, window)
            }
            show()
        }
    }
}
