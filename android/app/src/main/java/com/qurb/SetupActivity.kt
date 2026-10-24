package com.qurb

import android.content.Intent
import android.os.Bundle
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Space
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.lifecycleScope
import com.qurb.databinding.ActivitySetupBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.pairingWith

/**
 * First launch: make a key, or join the devices the person already has.
 *
 * Nothing to write down (decision 0052). A new key is kept in Block Store; a
 * second device gets the key from the first through the code it shows, which
 * this phone scans. The 24 words remain a way in for somebody who has them,
 * and are no longer asked of anybody.
 */
class SetupActivity : AppCompatActivity() {

    private lateinit var views: ActivitySetupBinding
    private val kit by lazy { Kit(this) }

    private val scanner = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result -> result.data?.getStringExtra(ScanActivity.EXTRA_CODE)?.let { join(it) } }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        views = ActivitySetupBinding.inflate(layoutInflater)
        setContentView(views.root)
        Appearance.paintEnvironment(this)

        // Android 15 draws edge to edge whether an app asks or not; without
        // this the first button sits under the status bar.
        ViewCompat.setOnApplyWindowInsetsListener(views.root) { view, windowInsets ->
            val bars = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            view.updatePadding(top = bars.top, bottom = bars.bottom)
            windowInsets
        }

        // Begun under the old rules and never finished: a key was made and its
        // words never typed back. Start here keeps that key; joining puts it
        // aside -- which a phone set up as new by mistake needs to do.
        views.start.setOnClickListener { if (keyFound) fromBackup() else create() }
        views.other.setOnClickListener { if (keyFound) askToStartAgain() else show(views.joinPanel) }
        // With a key found, joining is still a way in: for a key left from a
        // setup nobody uses any more, which joining replaces.
        views.joinInstead.setOnClickListener { show(views.joinPanel) }
        // A step goes back a step, by its link or the phone's back.
        views.joinBack.setOnClickListener { show(null) }
        views.digitsBack.setOnClickListener { show(views.joinPanel) }
        views.restoreBack.setOnClickListener { show(views.joinPanel) }
        onBackPressedDispatcher.addCallback(this, stepBack)
        views.scan.setOnClickListener { scanner.launch(Intent(this, ScanActivity::class.java)) }
        views.typeDigits.setOnClickListener { show(views.digitsPanel); askForDigits() }
        views.joinConfirm.setOnClickListener { join(views.code.text.toString()) }
        views.useWords.setOnClickListener { show(views.restorePanel) }
        views.restoreConfirm.setOnClickListener { restore() }
        whereTheCodeIs()
        digitBoxes()
        wordBoxes()
        ready()

        lifecycleScope.launch {
            // The key Block Store kept, on this phone or restored to it with
            // the rest of a backup: Use my key first, and Start again here.
            if (Backup.find(this@SetupActivity) != null) {
                keyFound = true
                views.found.visibility = View.VISIBLE
                views.start.text = "Use my key"
                views.other.text = "Start again here"
                views.joinInstead.visibility = View.VISIBLE
            }
            // Shown only once Block Store has answered (ten seconds at most):
            // shown before, Start here could become Use my key under a finger,
            // and a tap meant for the kept key made a new one in its place --
            // which it did on the emulator, 2026-10-24.
            views.start.visibility = View.VISIBLE
            views.other.visibility = View.VISIBLE
            // The rendezvous address kept with the key, for a phone set up
            // again after a reinstall or restored from a backup: back before
            // the first sync, however the phone then joins.
            val kept = Backup.findSignal(this@SetupActivity)
            if (!kept.isNullOrEmpty() && Engine.signalUrl(this@SetupActivity).isEmpty()) {
                Engine.setSignalUrl(this@SetupActivity, kept)
            }
        }
    }

    /** Whether the Google backup holds a key, and Welcome offers it first. */
    private var keyFound = false

    /**
     * Back from a step: to Join from the digits or the words, else to Welcome.
     * Not while joining: the engine cannot call a join off, and going back
     * would leave it running behind a page that says otherwise.
     */
    private val stepBack = object : androidx.activity.OnBackPressedCallback(false) {
        override fun handleOnBackPressed() {
            if (views.waitPanel.visibility == View.VISIBLE) return
            val inStep = views.digitsPanel.visibility == View.VISIBLE || views.restorePanel.visibility == View.VISIBLE
            show(if (inStep) views.joinPanel else null)
        }
    }

    /** One step, or Welcome for none. */
    private fun show(panel: View?) {
        views.welcome.visibility = if (panel == null) View.VISIBLE else View.GONE
        for (p in listOf(views.joinPanel, views.digitsPanel, views.restorePanel, views.waitPanel)) p.visibility = View.GONE
        panel?.visibility = View.VISIBLE
        stepBack.isEnabled = panel != null
        if (panel != views.digitsPanel) hideKeyboard()
    }

    /**
     * Where the other device shows its code, as the path to it: Qurb, then
     * Devices, then the plus -- three icons and two words.
     */
    private fun whereTheCodeIs() {
        val path = views.where
        fun step(icon: Int, words: String?) {
            path.addView(android.widget.ImageView(this).apply {
                setImageResource(icon)
                imageTintList = android.content.res.ColorStateList.valueOf(getColor(R.color.text_2))
            }, LinearLayout.LayoutParams(dp(18), dp(18)))
            if (words != null) path.addView(TextView(this).apply {
                text = words
                setTextAppearance(R.style.Text_Name)
            }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT)
                .apply { marginStart = dp(6) })
        }
        fun arrow() = path.addView(android.widget.ImageView(this).apply {
            setImageResource(R.drawable.ic_chevron_right)
            imageTintList = android.content.res.ColorStateList.valueOf(getColor(R.color.text_3))
        }, LinearLayout.LayoutParams(dp(14), dp(14)).apply { marginStart = dp(8); marginEnd = dp(8) })
        step(R.drawable.ic_laptop, "Qurb")
        arrow()
        step(R.drawable.ic_monitor_smartphone, "Devices")
        arrow()
        step(R.drawable.ic_plus, null)
    }

    /**
     * Six boxes showing what is typed into the field under them, in two threes
     * as the number is said; joining starts at the sixth.
     */
    private fun digitBoxes() {
        val boxes = views.boxes
        for (i in 0 until 6) {
            if (i == 3) boxes.addView(Space(this), LinearLayout.LayoutParams(dp(24), 1))
            boxes.addView(TextView(this).apply {
                gravity = android.view.Gravity.CENTER
                textSize = 26f
                typeface = androidx.core.content.res.ResourcesCompat.getFont(this@SetupActivity, R.font.inter_semibold)
                setTextColor(getColor(R.color.text))
            }, LinearLayout.LayoutParams(0, dp(56), 1f).apply { if (i % 3 != 0) marginStart = dp(8) })
        }
        fun paint() {
            val typed = views.code.text?.toString().orEmpty()
            var digit = 0
            for (i in 0 until boxes.childCount) {
                val box = boxes.getChildAt(i) as? TextView ?: continue
                box.text = typed.getOrNull(digit)?.toString() ?: ""
                box.background = androidx.core.content.ContextCompat.getDrawable(this,
                    if (digit == typed.length.coerceAtMost(5)) R.drawable.digit_box_now else R.drawable.digit_box)
                digit++
            }
        }
        paint()
        views.code.addTextChangedListener(object : android.text.TextWatcher {
            override fun beforeTextChanged(text: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(text: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(typed: android.text.Editable?) {
                paint()
                ready()
                // As WhatsApp's linking code does: the sixth digit is the go.
                if (typed?.length == 6) join(typed.toString())
            }
        })
        boxes.setOnClickListener { askForDigits() }
    }

    private fun askForDigits() {
        views.code.requestFocus()
        views.code.post {
            getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                ?.showSoftInput(views.code, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
        }
    }

    private fun hideKeyboard() {
        getSystemService(android.view.inputmethod.InputMethodManager::class.java)
            ?.hideSoftInputFromWindow(views.root.windowToken, 0)
    }

    /** The 24 boxes, three to a row, each numbered. A phrase pasted into any of them fills them all. */
    private val words = mutableListOf<EditText>()

    private fun wordBoxes() {
        var row: LinearLayout? = null
        for (i in 0 until 24) {
            if (i % 3 == 0) {
                row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
                views.words.addView(row, LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT,
                ).apply { if (i > 0) topMargin = dp(8) })
            }
            val box = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = android.view.Gravity.CENTER_VERTICAL
                background = androidx.core.content.ContextCompat.getDrawable(this@SetupActivity, R.drawable.digit_box)
                setPadding(dp(10), 0, dp(6), 0)
            }
            box.addView(TextView(this).apply {
                text = "${i + 1}"
                setTextAppearance(R.style.Text_Meta)
            }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT)
                .apply { marginEnd = dp(6) })
            val word = EditText(this).apply {
                background = null
                setPadding(0, 0, 0, 0)
                isSingleLine = true
                inputType = android.text.InputType.TYPE_CLASS_TEXT or
                    android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS or
                    android.text.InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
                // After the input type, which as a password's sets a monospace face.
                setTextAppearance(R.style.Text_Quiet)
                setTextColor(getColor(R.color.text))
                imeOptions = if (i == 23) android.view.inputmethod.EditorInfo.IME_ACTION_DONE
                    else android.view.inputmethod.EditorInfo.IME_ACTION_NEXT
                importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO
                contentDescription = "Word ${i + 1}"
            }
            val at = i
            word.addTextChangedListener(object : android.text.TextWatcher {
                override fun beforeTextChanged(text: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun onTextChanged(text: CharSequence?, start: Int, before: Int, count: Int) {}
                override fun afterTextChanged(typed: android.text.Editable?) {
                    ready()
                    val parts = typed?.toString()?.trim()?.split(Regex("\\s+"))?.filter { it.isNotEmpty() } ?: return
                    if (parts.size < 2) return
                    // Pasted: spread across this box and those after it.
                    parts.forEachIndexed { k, w -> words.getOrNull(at + k)?.setText(w) }
                }
            })
            // The box being typed in has the green edge, as the next digit's does.
            word.setOnFocusChangeListener { _, focused ->
                box.background = androidx.core.content.ContextCompat.getDrawable(this,
                    if (focused) R.drawable.digit_box_now else R.drawable.digit_box)
            }
            // A tap on its number too; the box itself never takes the focus,
            // which Next would otherwise stop at, between one word and the next.
            box.setOnClickListener {
                word.requestFocus()
                getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                    ?.showSoftInput(word, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
            }
            box.isFocusable = false
            words += word
            box.addView(word, LinearLayout.LayoutParams(0, dp(38), 1f))
            row!!.addView(box, LinearLayout.LayoutParams(0, dp(38), 1f).apply { if (i % 3 != 0) marginStart = dp(8) })
        }
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    /**
     * A step's action only once its boxes are full: six digits, 24 words.
     * Before, Join and Restore did nothing when tapped short, and said nothing.
     */
    private fun ready() {
        if (views.progress.visibility == View.VISIBLE) return
        views.joinConfirm.isEnabled = views.code.text?.length == 6
        views.restoreConfirm.isEnabled = words.size == 24 && words.all { it.text.isNotBlank() }
    }

    /**
     * Starting again with a key found makes a new key and keeps it in Block
     * Store in place of the one there, which this phone then no longer has
     * any way back to: asked first, as removing a device is.
     */
    private fun askToStartAgain() {
        kit.sheet()
            .mark(R.drawable.ic_key_round, R.color.attention_bg, R.color.attention)
            .title("Start again with a new key?", centred = true)
            .warning("The key in your Google backup is replaced")
            .buttons("Start again", danger = true) { create() }
            .show()
    }

    private fun create() {
        settle("Could not set up") {
            if (Engine.unfinished(this)) {
                Engine.setPhraseConfirmed(this, true)
                Backup.save(this, Engine.open(this).recoveryPhrase())
            } else {
                Engine.create(this)
            }
        }
    }

    /**
     * Join with a code another device shows; its key comes with it once the
     * person there approves this phone, comparing the number shown here
     * meanwhile (decision 0053). A join that fails goes back to the step it
     * came from, its digits still in their boxes.
     */
    private fun join(code: String) {
        if (joining) return
        joining = true
        // Nothing more to type; left up, the keyboard turned to letters behind
        // a failure's dialog.
        hideKeyboard()
        val from = if (views.digitsPanel.visibility == View.VISIBLE) views.digitsPanel else views.joinPanel
        settle("Could not join", undo = { joining = false; show(from) }) {
            if (Engine.unfinished(this)) Engine.discardUnfinished(this)
            // After putting an unfinished key aside: the number comes from the
            // certificate that then joins.
            // Six digits are looked for on this Wi-Fi (decision 0066), which
            // the phone hears only while it holds the multicast lock.
            val with = withContext(Dispatchers.IO) {
                Engine.root(this@SetupActivity).mkdirs()
                Engine.hearingTheNetwork(this@SetupActivity) {
                    pairingWith(Engine.root(this@SetupActivity).absolutePath, code)
                }
            }
            // Where to approve, by name when the other device says it
            // (decision 0074); one from before then is "your other device".
            val name = with.name
            views.waitTitle.text = if (name != null) "Approve on $name" else "Approve on your other device"
            views.waitSays.text = if (name != null) "$name shows this number too" else "It shows this number too"
            views.waitNumber.text = with.number
            views.waitNumber.contentDescription = with.number.replace(" ", "")
            views.waitMark.show(StatusMark.Look.SYNCING, when (with.kind) {
                "computer", "replica" -> R.drawable.mark_laptop
                "phone" -> R.drawable.mark_smartphone
                else -> R.drawable.mark_monitor_smartphone
            })
            // The travelling ring is the waiting; the bar would say it twice.
            views.progress.visibility = View.GONE
            show(views.waitPanel)
            Engine.join(this, code)
        }
    }

    /** A join under way, which the sixth digit and Join must not start twice. */
    private var joining = false

    /** The fallback: the 24 words, for somebody who has them. */
    private fun restore() {
        val phrase = words.joinToString(" ") { it.text.toString().trim().lowercase() }
        settle("That phrase was not accepted") {
            if (Engine.unfinished(this)) Engine.discardUnfinished(this)
            Engine.restore(this, phrase)
            // All 24 typed just now: nothing further to check.
            Engine.setPhraseConfirmed(this, true)
        }
    }

    /**
     * The key Block Store kept, on this phone or restored to it with the rest
     * of a backup. It brings back the key, not the files: those come from the
     * person's devices once this one is paired with them.
     */
    private fun fromBackup() {
        settle("Could not use the backed-up key") {
            val phrase = Backup.find(this) ?: error("There is no key in your Google backup any more.")
            if (Engine.unfinished(this)) Engine.discardUnfinished(this)
            Engine.restore(this, phrase)
            Engine.setPhraseConfirmed(this, true)
        }
    }

    /**
     * Run one way of setting up, and go on to the app when it has worked;
     * [undo] puts back what the way changed on screen, when it has not.
     */
    private fun settle(failure: String, undo: () -> Unit = {}, work: suspend () -> Unit) {
        busy(true)
        lifecycleScope.launch {
            try {
                work()
                done()
            } catch (e: Exception) {
                busy(false)
                undo()
                Words.fail(this@SetupActivity, failure, e)
            }
        }
    }

    private fun done() {
        startActivity(Intent(this, MainActivity::class.java))
        finish()
    }

    private fun busy(working: Boolean) {
        views.progress.visibility = if (working) View.VISIBLE else View.GONE
        listOf(
            views.start, views.other, views.joinInstead, views.scan, views.typeDigits, views.joinConfirm, views.joinBack,
            views.digitsBack, views.useWords, views.restoreConfirm, views.restoreBack,
        ).forEach { it.isEnabled = !working }
        if (!working) ready()
    }
}
