package com.qurb

import android.content.Intent
import android.os.Bundle
import android.view.View
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.lifecycleScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.qurb.databinding.ActivitySetupBinding
import kotlinx.coroutines.launch
import uniffi.qurb_mobile.QurbException

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

    private val scanner = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result -> result.data?.getStringExtra(ScanActivity.EXTRA_CODE)?.let { join(it) } }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        views = ActivitySetupBinding.inflate(layoutInflater)
        setContentView(views.root)

        // Android 15 draws edge to edge whether an app asks or not; without
        // this the first button sits under the status bar.
        ViewCompat.setOnApplyWindowInsetsListener(views.root) { view, windowInsets ->
            val bars = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            view.updatePadding(top = bars.top, bottom = bars.bottom)
            windowInsets
        }

        views.create.setOnClickListener { create() }
        views.restore.setOnClickListener { show(views.joinPanel) }
        views.scan.setOnClickListener { scanner.launch(Intent(this, ScanActivity::class.java)) }
        views.joinConfirm.setOnClickListener {
            views.code.text?.toString()?.trim()?.takeIf { it.isNotEmpty() }?.let { join(it) }
        }
        views.useWords.setOnClickListener { show(views.restorePanel) }
        views.restoreConfirm.setOnClickListener { restore() }
        views.fromBackup.setOnClickListener { fromBackup() }

        // Begun under the old rules and never finished: a key was made and its
        // words never typed back. Either keep that key, or put it aside and
        // join the person's other devices -- which a phone set up as new by
        // mistake needs to do.
        if (Engine.unfinished(this)) {
            views.createSays.text = "Keep the key this phone made when you started, and use it as your first device."
        }

        lifecycleScope.launch {
            if (Backup.find(this@SetupActivity) != null) views.fromBackup.visibility = View.VISIBLE
        }
    }

    private fun show(panel: View) {
        views.chooser.visibility = View.GONE
        views.joinPanel.visibility = View.GONE
        views.restorePanel.visibility = View.GONE
        panel.visibility = View.VISIBLE
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

    /** Join with a code another device shows; its key comes with it. */
    private fun join(code: String) {
        settle("Could not join") {
            if (Engine.unfinished(this)) Engine.discardUnfinished(this)
            Engine.join(this, code)
        }
    }

    /** The fallback: the 24 words, for somebody who has them. */
    private fun restore() {
        val phrase = views.phrase.text.toString().trim()
        if (phrase.isEmpty()) return
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

    /** Run one way of setting up, and go on to the app when it has worked. */
    private fun settle(failure: String, work: suspend () -> Unit) {
        busy(true)
        lifecycleScope.launch {
            try {
                work()
                done()
            } catch (e: Exception) {
                busy(false)
                fail(failure, e)
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
            views.create, views.restore, views.scan, views.joinConfirm,
            views.useWords, views.restoreConfirm, views.fromBackup,
        ).forEach { it.isEnabled = !working }
    }

    private fun fail(title: String, e: Exception) {
        MaterialAlertDialogBuilder(this)
            .setTitle(title)
            .setMessage(if (e is QurbException) e.readable() else e.message ?: e.toString())
            .setPositiveButton("OK", null)
            .show()
    }
}
