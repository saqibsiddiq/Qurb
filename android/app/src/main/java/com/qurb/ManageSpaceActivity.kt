package com.qurb

import android.app.ActivityManager
import android.content.Intent
import android.os.Bundle
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.lifecycleScope
import com.google.android.material.button.MaterialButton
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.OnlyHere

/**
 * What Android's Settings opens in place of *Clear data* (decision 0053).
 *
 * On 2026-10-05 the S23's data was cleared from Settings, and with it the only
 * copies of 18 files and a Private Vault nobody had kept: one tap, under a
 * button that says nothing about what it costs. Declared as the app's
 * `manageSpaceActivity`, this screen takes that button's place. It says what
 * exists only on this phone before anything goes, offers to free space that
 * costs nothing, and still lets the person delete everything -- that is their
 * right, and they should do it knowing.
 */
class ManageSpaceActivity : AppCompatActivity() {

    private lateinit var body: LinearLayout

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val side = dp(24)
        body = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(side, side, side, side)
        }
        val scroll = ScrollView(this).apply {
            addView(body)
            setBackgroundColor(ContextCompat.getColor(this@ManageSpaceActivity, R.color.bg))
        }
        setContentView(scroll)
        ViewCompat.setOnApplyWindowInsetsListener(scroll) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            view.updatePadding(top = bars.top, bottom = bars.bottom)
            insets
        }
        load()
    }

    private fun load() {
        body.removeAllViews()
        text("Qurb's storage on this phone", R.style.Text_Title)
        lifecycleScope.launch {
            if (!Engine.isSetUp(this@ManageSpaceActivity)) {
                text("Qurb is not set up on this phone, so it keeps nothing here worth keeping.", R.style.Text_Body)
                deleteButton(emptyList())
                return@launch
            }
            val (usage, only) = try {
                withContext(Dispatchers.IO) {
                    val engine = Engine.open(this@ManageSpaceActivity)
                    engine.usage() to engine.onlyHere()
                }
            } catch (e: Exception) {
                text("Could not read Qurb's files: ${e.message}", R.style.Text_Body)
                deleteButton(emptyList())
                return@launch
            }

            text("Qurb uses ${Words.size(usage.onDisk)} here.", R.style.Text_Body)
            if (only.isEmpty()) {
                text("Every file here is also on another of your devices. Deleting Qurb's data loses " +
                    "none of them, though this phone would have to be set up again and joined to " +
                    "your other devices with a code.", R.style.Text_Body)
            } else {
                val total = only.sumOf { it.size.toLong() }.toULong()
                text("${Words.files(only.size)} (${Words.size(total)}) exist only on this phone. " +
                    "Deleting Qurb's data deletes them for good:", R.style.Text_Body)
                only.take(20).forEach { text("· ${it.path}${if (it.private) "  (private)" else ""}", R.style.Text_Quiet) }
                if (only.size > 20) text("and ${only.size - 20} more", R.style.Text_Quiet)
                text("Open Qurb and sync with your computer first, and they will be safe there.", R.style.Text_Meta)
            }

            button("Free space safely", primary = true) {
                lifecycleScope.launch {
                    val freed = withContext(Dispatchers.IO) {
                        runCatching { Engine.open(this@ManageSpaceActivity).housekeep().freed }.getOrDefault(0uL)
                    }
                    MaterialAlertDialogBuilder(this@ManageSpaceActivity)
                        .setTitle(if (freed > 0uL) "Freed ${Words.size(freed)}" else "Nothing to free")
                        .setMessage("Only space nothing needs was freed: no file was removed.")
                        .setPositiveButton("OK") { _, _ -> load() }
                        .show()
                }
            }
            button("Open Qurb") { startActivity(Intent(this@ManageSpaceActivity, MainActivity::class.java)) }
            deleteButton(only)
        }
    }

    /** Still possible, as it should be -- after saying what it costs. */
    private fun deleteButton(only: List<OnlyHere>) {
        button("Delete everything Qurb keeps here…", danger = true) {
            val lost = if (only.isEmpty()) "Nothing exists only on this phone."
            else "${Words.files(only.size)} that exist only on this phone will be gone for good."
            MaterialAlertDialogBuilder(this)
                .setTitle("Delete everything Qurb keeps here?")
                .setMessage("$lost This phone's key and its pairings go too. Your other devices keep what they have.")
                .setNegativeButton("Keep", null)
                .setPositiveButton("Delete everything") { _, _ ->
                    getSystemService(ActivityManager::class.java)?.clearApplicationUserData()
                }
                .show()
        }
    }

    private fun text(words: String, style: Int) = body.addView(TextView(this).apply {
        text = words
        setTextAppearance(style)
        setPadding(0, dp(6), 0, dp(6))
    })

    private fun button(label: String, primary: Boolean = false, danger: Boolean = false, act: () -> Unit) {
        val button = if (primary) MaterialButton(this)
        else MaterialButton(this, null, com.google.android.material.R.attr.materialButtonOutlinedStyle)
        button.text = label
        if (danger) button.setTextColor(ContextCompat.getColor(this, R.color.error))
        button.setOnClickListener { act() }
        body.addView(button, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = dp(12) })
    }

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
}
