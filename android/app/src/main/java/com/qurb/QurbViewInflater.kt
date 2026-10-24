package com.qurb

import android.content.Context
import android.graphics.Typeface
import android.os.Build
import android.util.AttributeSet
import android.view.View
import android.widget.TextView
import androidx.appcompat.widget.AppCompatButton
import androidx.appcompat.widget.AppCompatTextView
import androidx.appcompat.widget.DialogTitle
import com.google.android.material.button.MaterialButton
import com.google.android.material.theme.MaterialComponentsViewInflater

/**
 * The views a layout names, as Material makes them, with the weight of their
 * text appearance kept. Named in the theme as `viewInflaterClass`.
 *
 * AppCompat (1.7.0, `AppCompatTextHelper.updateTypefaceAndStyle`) reads a
 * text view's appearance, then its own attributes, and the second reading
 * sets the weight back to unspecified -- having already cleared bold for it --
 * and finds Inter again in the theme's `android:fontFamily`, which then
 * resolves regular. So from Android 9 every semibold or medium appearance set
 * in a layout came out regular: Home's state, setup's titles. Set from code
 * with `setTextAppearance`, as Kit's text is, it was right, which is how the
 * two were told apart (2026-10-24). A button takes its appearance from its
 * style, never by `setTextAppearance`, so every button was regular, Kit's
 * too. Android 8 is untouched: there bold, kept through both readings, finds
 * Inter's semibold (themes.xml, "Weights").
 *
 * A layout names a MaterialButton in full, and an alert dialog's title is
 * AppCompat's DialogTitle: views named so are made by reflection rather than
 * through [createButton] or [createTextView], and [createView] is where they
 * come. The dialog's title, every failure's, was regular too.
 */
class QurbViewInflater : MaterialComponentsViewInflater() {

    override fun createTextView(context: Context, attrs: AttributeSet?): AppCompatTextView =
        super.createTextView(context, attrs).also { keepWeight(it, attrs, android.R.attr.textViewStyle) }

    override fun createButton(context: Context, attrs: AttributeSet): AppCompatButton =
        super.createButton(context, attrs).also { keepWeight(it, attrs, BUTTON) }

    override fun createView(context: Context, name: String, attrs: AttributeSet): View? = when (name) {
        MaterialButton::class.java.name -> MaterialButton(context, attrs).also { keepWeight(it, attrs, BUTTON) }
        DialogTitle::class.java.name -> DialogTitle(context, attrs).also { keepWeight(it, attrs, 0) }
        else -> null
    }

    private companion object {
        val BUTTON = com.google.android.material.R.attr.materialButtonStyle
    }
}

/**
 * The weight of [view]'s text appearance, put back after AppCompat lost it;
 * [defaultStyle] is the theme attribute naming the view's style, where a
 * button's appearance is.
 */
fun keepWeight(view: TextView, attrs: AttributeSet?, defaultStyle: Int) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return
    val context = view.context
    // A weight the view names itself is one AppCompat already keeps.
    val own = context.obtainStyledAttributes(attrs,
        intArrayOf(android.R.attr.textAppearance, android.R.attr.textFontWeight), defaultStyle, 0)
    val appearance = own.getResourceId(0, 0)
    val named = own.hasValue(1)
    own.recycle()
    if (appearance == 0 || named) return
    val read = context.obtainStyledAttributes(appearance, intArrayOf(android.R.attr.textFontWeight))
    val weight = read.getInt(0, -1)
    read.recycle()
    if (weight > 0) view.typeface = Typeface.create(view.typeface, weight, false)
}
