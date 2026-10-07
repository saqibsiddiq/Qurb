package com.qurb

import android.app.Activity
import android.graphics.Typeface
import android.view.Gravity
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import uniffi.qurb_mobile.PairingApprover
import uniffi.qurb_mobile.PairingRequest
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Pairing, approved by the person holding both devices (decision 0053).
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
     * device in. The engine calls it on a background thread and it blocks that
     * thread until they answer -- or for as long as a code lives, five minutes,
     * after which the answer is no.
     */
    class Asker(private val activity: Activity) : PairingApprover {
        override fun approve(request: PairingRequest): Boolean {
            val answered = CountDownLatch(1)
            var yes = false
            activity.runOnUiThread {
                val what = if (request.wantsKey) "wants to join, and take this phone's key"
                else "wants to pair with this phone"
                MaterialAlertDialogBuilder(activity)
                    .setTitle("${request.name} $what")
                    .setView(numberView(activity, request.number,
                        "Approve only if it shows this number.",
                        "A different number means someone else has this code: decline."))
                    .setCancelable(false)
                    .setNegativeButton("Decline") { _, _ -> answered.countDown() }
                    .setPositiveButton("Approve") { _, _ -> yes = true; answered.countDown() }
                    .show()
            }
            answered.await(5, TimeUnit.MINUTES)
            return yes
        }
    }

    /**
     * The number this phone shows while the other device approves it. Shown
     * for the length of the join and dismissed by the caller.
     */
    fun showWhileJoining(activity: Activity, number: String): AlertDialog =
        MaterialAlertDialogBuilder(activity)
            .setTitle("Approve this phone on the other device")
            .setView(numberView(activity, number,
                "It asks whether to let this phone in. Check it shows this number.", null))
            .setCancelable(false)
            .show()

    private fun numberView(activity: Activity, number: String, above: String, below: String?) =
        LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            val side = (24 * resources.displayMetrics.density).toInt()
            setPadding(side, side / 2, side, 0)
            addView(TextView(activity).apply { text = above; setTextAppearance(R.style.Text_Body) })
            addView(TextView(activity).apply {
                text = number
                gravity = Gravity.CENTER
                textSize = 36f
                typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                letterSpacing = 0.08f
                setPadding(0, side / 2, 0, side / 2)
            })
            below?.let { addView(TextView(activity).apply { text = it; setTextAppearance(R.style.Text_Quiet) }) }
        }
}
