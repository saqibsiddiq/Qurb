package com.qurb

import android.content.Intent
import android.os.Bundle
import android.view.View
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.qurb.databinding.ActivitySetupBinding
import kotlinx.coroutines.launch

/**
 * First launch: make a key, or bring one over from another device.
 *
 * The whole screen exists to get one thing right — that the user writes the 24
 * words down. Nothing else here can be undone by a support ticket, because
 * there is no support ticket: the words are the only copy of the key.
 */
class SetupActivity : AppCompatActivity() {

    private lateinit var views: ActivitySetupBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        views = ActivitySetupBinding.inflate(layoutInflater)
        setContentView(views.root)

        views.create.setOnClickListener { create() }
        views.restore.setOnClickListener { showRestore() }
        views.restoreConfirm.setOnClickListener { restore() }
    }

    private fun create() {
        busy(true)
        lifecycleScope.launch {
            try {
                showPhrase(Engine.create(this@SetupActivity))
            } catch (e: Exception) {
                busy(false)
                fail("Could not set up", e)
            }
        }
    }

    /**
     * The words, and the one dialog in this app that cannot be dismissed by
     * tapping outside it.
     *
     * The user must acknowledge having written them down. That is friction on
     * purpose: every other consumer product can recover an account, and this one
     * genuinely cannot.
     */
    private fun showPhrase(phrase: String) {
        val numbered = phrase.split(" ")
            .mapIndexed { i, word -> "${(i + 1).toString().padStart(2)}. $word" }
            .chunked(12)
            .let { columns ->
                (0 until 12).joinToString("\n") { row ->
                    columns.joinToString("      ") { it.getOrElse(row) { "" } }
                }
            }

        MaterialAlertDialogBuilder(this)
            .setTitle("Write these down")
            .setMessage(
                "$numbered\n\n" +
                    "These 24 words are your key, not a backup of it. Nobody else " +
                    "has a copy — not a server, not us. Lose them and lose this " +
                    "phone, and your files cannot be recovered by anyone.\n\n" +
                    "Write them on paper, in order, now."
            )
            .setCancelable(false)
            .setPositiveButton("I have written them down") { _, _ -> done() }
            .show()
    }

    private fun showRestore() {
        views.chooser.visibility = View.GONE
        views.restorePanel.visibility = View.VISIBLE
    }

    private fun restore() {
        val phrase = views.phrase.text.toString().trim()
        if (phrase.isEmpty()) return

        busy(true)
        lifecycleScope.launch {
            try {
                Engine.restore(this@SetupActivity, phrase)
                done()
            } catch (e: Exception) {
                busy(false)
                // The commonest failure by far, and the message says which word
                // count was seen because a missing word is the usual cause.
                fail("That phrase was not accepted", e)
            }
        }
    }

    private fun done() {
        startActivity(Intent(this, MainActivity::class.java))
        finish()
    }

    private fun busy(working: Boolean) {
        views.progress.visibility = if (working) View.VISIBLE else View.GONE
        views.create.isEnabled = !working
        views.restore.isEnabled = !working
        views.restoreConfirm.isEnabled = !working
    }

    private fun fail(title: String, e: Exception) {
        MaterialAlertDialogBuilder(this)
            .setTitle(title)
            .setMessage(e.message ?: e.toString())
            .setPositiveButton("OK", null)
            .show()
    }
}
