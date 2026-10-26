package com.qurb

import android.content.Context
import android.view.Gravity
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Space
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.core.content.res.ResourcesCompat

/**
 * Six digits as six boxes, in two threes as the number is said (the Figma
 * redesign's *Six digits*, item 19 of the owner's list of 2026-10-26): each
 * 42 by 56 dp, 8 apart and a little more between the threes, the box the next
 * digit goes into edged in green. What is typed goes into a field nobody sees,
 * which brings up the number pad and takes a paste; the boxes show it.
 *
 * Setup's *Type 6 digits* and Home's draw the same boxes.
 */
object DigitBoxes {

    /** Fill `boxes` -- an empty horizontal row -- and keep it showing `field`. */
    fun attach(context: Context, boxes: LinearLayout, field: EditText, sixth: (String) -> Unit) {
        val density = context.resources.displayMetrics.density
        fun dp(v: Int) = (v * density).toInt()
        boxes.gravity = Gravity.CENTER
        for (i in 0 until 6) {
            if (i == 3) boxes.addView(Space(context), LinearLayout.LayoutParams(dp(10), 1).apply { marginStart = dp(8) })
            boxes.addView(TextView(context).apply {
                gravity = Gravity.CENTER
                textSize = 26f
                letterSpacing = -0.02f
                typeface = ResourcesCompat.getFont(context, R.font.inter_semibold)
                setTextColor(ContextCompat.getColor(context, R.color.text))
            }, LinearLayout.LayoutParams(dp(42), dp(56)).apply { if (i > 0) marginStart = dp(8) })
        }
        fun paint() {
            val typed = field.text?.toString().orEmpty()
            var digit = 0
            for (i in 0 until boxes.childCount) {
                val box = boxes.getChildAt(i) as? TextView ?: continue
                box.text = typed.getOrNull(digit)?.toString() ?: ""
                box.background = ContextCompat.getDrawable(context,
                    if (digit == typed.length.coerceAtMost(5)) R.drawable.digit_box_now else R.drawable.digit_box)
                digit++
            }
        }
        paint()
        field.addTextChangedListener(object : android.text.TextWatcher {
            override fun beforeTextChanged(text: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(text: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(typed: android.text.Editable?) {
                paint()
                // As WhatsApp's linking code does: the sixth digit is the go.
                if (typed?.length == 6) sixth(typed.toString())
            }
        })
        boxes.setOnClickListener { ask(context, field) }
    }

    /** The number pad up, for `field`. */
    fun ask(context: Context, field: EditText) {
        field.requestFocus()
        field.post {
            context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                ?.showSoftInput(field, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
        }
    }
}
