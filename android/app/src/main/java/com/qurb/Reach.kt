package com.qurb

import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

/**
 * The IPv6 nudge (decision 0082): a computer that answered this phone's call
 * from away, and that no direct path reached. Its internet provider holds it
 * behind a shared address that lets nothing in; turning on IPv6, which most
 * providers in India offer and many leave off, usually fixes it. Said plainly,
 * with the one thing to do, and that home Wi-Fi still works.
 */
object Reach {
    fun blocked(app: MainActivity, computer: String): View {
        val kit = app.kit
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            background = GradientDrawable().apply {
                cornerRadius = kit.dp(16).toFloat()
                setColor(kit.color(R.color.attention_bg))
            }
            setPadding(kit.dp(14), kit.dp(12), kit.dp(14), kit.dp(12))
        }
        box.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_cloud_off)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.attention))
        }, LinearLayout.LayoutParams(kit.dp(20), kit.dp(20)).apply { topMargin = kit.dp(2) })
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = "$computer only on your home Wi-Fi"
            setTextAppearance(R.style.Text_Name)
        })
        words.addView(TextView(app).apply {
            text = "Your internet provider blocks connections to it from outside. Ask them to turn on IPv6, " +
                "and $computer will work from anywhere."
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_2))
            setPadding(0, kit.dp(4), 0, 0)
        })
        box.addView(words, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(12) })
        return box
    }
}
