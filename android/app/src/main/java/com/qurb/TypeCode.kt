package com.qurb

import android.view.View
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.qurb.databinding.ScreenPageBinding

/**
 * The six digits a device on the same Wi-Fi shows beside its QR code
 * (decision 0066), typed: a page of its own, as the Figma redesign draws
 * *Six digits* and Setup has it (item 19 of the owner's list of 2026-10-26; a
 * sheet with one field until then). Six boxes, *Same Wi-Fi*, and Join at the
 * foot; joining starts as the sixth is typed, as WhatsApp's linking code
 * does. The digits fetch the long code, which the engine still takes typed
 * in full.
 */
class TypeCode(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    private val field = EditText(app).apply {
        background = null
        isCursorVisible = false
        importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO
        inputType = android.text.InputType.TYPE_CLASS_NUMBER
        filters = arrayOf(android.text.InputFilter.LengthFilter(6))
        setTextColor(android.graphics.Color.TRANSPARENT)
        contentDescription = "Six digits"
    }

    private val join = kit.button("Join") { go() }

    // Join at the foot of the page, above the tabs, as Setup's.
    override val dockBar: View = FrameLayout(app).apply {
        setPadding(0, kit.dp(8), 0, kit.dp(8))
        addView(join, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, kit.dp(48)))
    }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Home"
        views.back.setTextAppearance(R.style.Text_Name)
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Type 6 digits"
        views.title.setTextAppearance(R.style.Text_Hero)
        views.refresh.isEnabled = false

        val digits = FrameLayout(app)
        digits.addView(field, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, kit.dp(56)))
        val boxes = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        }
        digits.addView(boxes, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, kit.dp(56)))
        DigitBoxes.attach(app, boxes, field) { go() }
        views.sections.addView(digits, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = kit.dp(20) })

        views.sections.addView(TextView(app).apply {
            text = "Same Wi-Fi"
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
            compoundDrawablePadding = kit.dp(6)
            gravity = android.view.Gravity.CENTER_VERTICAL
            setCompoundDrawablesRelativeWithIntrinsicBounds(
                ContextCompat.getDrawable(app, R.drawable.ic_wifi)?.mutate()?.apply {
                    setBounds(0, 0, kit.dp(16), kit.dp(16))
                    setTint(kit.color(R.color.text_3))
                }, null, null, null)
        }, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply {
            topMargin = kit.dp(16)
            gravity = android.view.Gravity.CENTER_HORIZONTAL
        })

        field.addTextChangedListener(object : android.text.TextWatcher {
            override fun beforeTextChanged(text: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(text: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(typed: android.text.Editable?) { ready() }
        })
        ready()
    }

    /** The number pad up at once: the digits are all this page is for. */
    override fun refresh() {
        // Once the page is on screen: asked before, the pad does not come.
        field.postDelayed({ if (app.showing(this)) DigitBoxes.ask(app, field) }, 250)
    }

    /** Join only once the six are in. */
    private fun ready() {
        val full = field.text?.length == 6
        join.isEnabled = full
        join.alpha = if (full) 1f else 0.5f
    }

    private var going = false

    private fun go() {
        val code = field.text?.toString().orEmpty()
        if (code.length != 6 || going) return
        going = true
        app.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
            ?.hideSoftInputFromWindow(field.windowToken, 0)
        if (app.showing(this)) app.onBackPressedDispatcher.onBackPressed()
        app.joinWith(code)
    }
}
