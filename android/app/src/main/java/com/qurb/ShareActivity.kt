package com.qurb

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.lifecycleScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.qurb.databinding.ActivityShareBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.PeerInfo

/**
 * The share sheet's way in.
 *
 * The point of this screen is what it does *not* need: a network. Sharing a
 * photo writes it into the synced folder and indexes it, here and now, whether
 * or not any other device is switched on. Reaching them is a separate question,
 * answered whenever one is next awake — by the periodic worker if nothing else
 * happens first.
 *
 * So there is no outbox and no retry queue, because there is nothing queued: the
 * file is simply in the folder like any other, and the index already knows that
 * no other device holds it. "Waiting to be delivered" is a question the store
 * can answer at any time rather than a list this screen has to keep.
 *
 * It does ask for a sync immediately, which usually succeeds and makes the
 * whole thing feel instant. When it does not, nothing is lost and nothing needs
 * saying beyond when to expect it.
 */
class ShareActivity : AppCompatActivity() {

    private lateinit var views: ActivityShareBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        views = ActivityShareBinding.inflate(layoutInflater)
        setContentView(views.root)
        insetFor(views.root)

        views.done.setOnClickListener { finish() }

        val incoming = incomingUris()
        when {
            !Engine.isSetUp(this) -> refuse(
                "Qurb isn't set up on this phone",
                "Open Qurb first and either create a key or enter your 24 words.",
            )
            incoming.isEmpty() -> refuse(
                "Nothing to save",
                "That share did not contain a file Qurb can read.",
            )
            else -> chooseWhere(incoming)
        }
    }

    /**
     * Save to Private Vault, save to Files, or send straight to a device
     * (brief §39): the choice the share sheet exists to make fast. Asked only
     * when there is a device to send to; with none, the file is saved where
     * *Keep new files private* says, and nothing is asked.
     */
    private fun chooseWhere(uris: List<Uri>) {
        views.headline.text = "Qurb"
        lifecycleScope.launch {
            val peers = runCatching {
                withContext(Dispatchers.IO) { Engine.open(this@ShareActivity).peers() }
            }.getOrDefault(emptyList())
            if (peers.isEmpty()) {
                save(uris)
                return@launch
            }
            val what = if (uris.size == 1) "it" else "${uris.size} files"
            // Item 4 of the owner's list (decision 0060): into the folder a
            // computer this phone visits keeps for it, or into that
            // computer's Downloads, named as such.
            val keepers = runCatching {
                withContext(Dispatchers.IO) { Engine.open(this@ShareActivity).holders().map { it.fingerprint }.toSet() }
            }.getOrDefault(emptySet())
            val host = peers.firstOrNull { it.relation == "host" && it.fingerprint in keepers }
            val choices = listOf(
                host?.let { "Save to my folder on ${it.name}" } ?: "Save to Private Vault",
                "Save to Files, on all your devices",
            ) + peers.map { if (it.relation == "host") "Send to ${it.name}'s Downloads" else "Send to ${it.name}" }
            MaterialAlertDialogBuilder(this@ShareActivity)
                .setTitle("Where should $what go?")
                .setItems(choices.toTypedArray()) { _, which ->
                    when (which) {
                        0 -> save(uris, private = true)
                        1 -> save(uris, private = false)
                        else -> send(uris, peers[which - 2])
                    }
                }
                .setNegativeButton("Cancel") { _, _ -> finish() }
                .setOnCancelListener { finish() }
                .show()
        }
    }

    /**
     * Straight to one device, privately, and nowhere else (decision 0030). The
     * engine keeps its copy until that device collects it, so this works while
     * the device is off; it is not added to this phone's Vault.
     */
    private fun send(uris: List<Uri>, to: PeerInfo) {
        views.headline.text = "Sending to ${to.name}"
        views.detail.text = ""
        lifecycleScope.launch {
            var sent = 0
            var failed = 0
            val staged = mutableListOf<Engine.Staged>()
            val sending = mutableSetOf<Engine.Staged>()
            try {
                for (uri in uris) {
                    try {
                        staged += Engine.stage(this@ShareActivity, uri)
                    } catch (e: Exception) {
                        android.util.Log.w("qurb", "could not read a shared file", e)
                        failed++
                    }
                }
                val earlier = runCatching {
                    Engine.sentBefore(this@ShareActivity, staged.map { it.file.absolutePath }, to.fingerprint)
                }.getOrDefault(emptyList())
                val leaveOut = askAgain(to, earlier, staged)
                if (leaveOut == null) {
                    finish()
                    return@launch
                }
                for (file in staged.filter { it.file.absolutePath !in leaveOut }) {
                    try {
                        Engine.send(this@ShareActivity, file, to.fingerprint)
                        sending += file
                        sent++
                    } catch (e: Exception) {
                        android.util.Log.w("qurb", "could not send a shared file", e)
                        failed++
                    }
                }
            } finally {
                // Each copy sent is the engine's now, deleted once collected
                // (decision 0060). The rest were never sent.
                staged.filter { it !in sending }.forEach { it.file.delete() }
            }
            if (sent == 0) {
                refuse("Could not send that", "Qurb could not read the file it was handed.")
                return@launch
            }
            SyncWorker.runNow(this@ShareActivity)
            views.progress.visibility = android.view.View.GONE
            views.done.visibility = android.view.View.VISIBLE
            views.headline.text = if (failed > 0) "Sent $sent of ${sent + failed}" else "Sent to ${to.name}"
            views.detail.text = "${to.name} collects " + (if (sent == 1) "it" else "them") +
                " the next time it is switched on and reachable. Your other devices never see " +
                (if (sent == 1) "it." else "them.") +
                if (failed > 0) "\n\n$failed could not be read and were not sent." else ""
        }
    }

    /**
     * Files that went to `to` before, and whether to send them again
     * (decision 0059). The files to leave out -- none, to send them all again
     * -- or null to send nothing.
     */
    private suspend fun askAgain(
        to: PeerInfo,
        earlier: List<uniffi.qurb_mobile.EarlierSend>,
        staged: List<Engine.Staged>,
    ): Set<String>? {
        if (earlier.isEmpty()) return emptySet()
        val names = staged.associate { it.file.absolutePath to it.name }
        val one = earlier.size == 1
        val everything = earlier.size == staged.size
        val lines = earlier.joinToString("\n") { "${names[it.source] ?: it.source} — as ${it.sentAs}, ${Words.ago(it.at)}" }
        return kotlinx.coroutines.suspendCancellableCoroutine { answer ->
            var answered = false
            fun say(leaveOut: Set<String>?) {
                if (!answered) {
                    answered = true
                    answer.resumeWith(Result.success(leaveOut))
                }
            }
            MaterialAlertDialogBuilder(this@ShareActivity)
                .setTitle("Sent to ${to.name} before")
                .setMessage(
                    "$lines\n\n" + if (one) "${to.name} may still have it. Sending it again puts another copy there."
                    else "${to.name} may still have them. Sending them again puts another copy there."
                )
                .setPositiveButton(if (one) "Send it again" else "Send them again") { _, _ -> say(emptySet()) }
                .setNegativeButton(if (everything) "Don't send" else if (one) "Leave it out" else "Leave them out") { _, _ ->
                    say(if (everything) null else earlier.map { it.source }.toSet())
                }
                .setOnCancelListener { say(null) }
                .show()
        }
    }

    /** Everything the share sheet handed over, whichever form it came in. */
    private fun incomingUris(): List<Uri> = when (intent?.action) {
        Intent.ACTION_SEND -> listOfNotNull(uriExtra(Intent.EXTRA_STREAM))
        Intent.ACTION_SEND_MULTIPLE -> uriListExtra(Intent.EXTRA_STREAM)
        else -> emptyList()
    }

    @Suppress("DEPRECATION")
    private fun uriExtra(name: String): Uri? =
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(name, Uri::class.java)
        } else {
            intent.getParcelableExtra(name)
        }

    @Suppress("DEPRECATION")
    private fun uriListExtra(name: String): List<Uri> =
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableArrayListExtra(name, Uri::class.java).orEmpty()
        } else {
            intent.getParcelableArrayListExtra<Uri>(name).orEmpty()
        }

    /**
     * Save every shared file, then say what will happen to it.
     *
     * One failure does not abandon the rest: someone sharing eleven photos
     * should get ten of them and be told about the one that did not work,
     * rather than losing all eleven to it.
     */
    private fun save(uris: List<Uri>, private: Boolean? = null) {
        val area = private ?: Engine.ownFilesPrivate(this)
        val place = if (area) "Private Vault" else "Qurb"
        views.headline.text = if (uris.size == 1) "Saving to $place" else "Saving ${uris.size} files to $place"
        views.detail.text = ""

        lifecycleScope.launch {
            var saved = 0
            var failed = 0
            for (uri in uris) {
                try {
                    Engine.importUri(this@ShareActivity, uri, private = area)
                    saved++
                } catch (e: Exception) {
                    android.util.Log.w("qurb", "could not save a shared file", e)
                    failed++
                }
            }

            if (saved == 0) {
                refuse("Could not save that", "Qurb could not read the file it was handed.")
                return@launch
            }

            // Ask for a sync through the scheduler rather than syncing here.
            // A share should not hold the screen open while a phone looks for a
            // computer that may be switched off, and going through WorkManager
            // means the attempt survives this screen closing and is retried on
            // its own if nothing answers.
            SyncWorker.runNow(this@ShareActivity)

            views.progress.visibility = android.view.View.GONE
            views.done.visibility = android.view.View.VISIBLE
            views.headline.text = when {
                failed > 0 -> "Saved $saved of ${saved + failed}"
                saved == 1 -> "Saved to $place"
                else -> "Saved $saved files to $place"
            }

            // About what was just shared, not about the backlog. Someone who
            // shared one photo and is told their devices will get "them" is
            // being answered about something they did not ask.
            //
            // And about where it will actually go. Files added on the phone
            // are private to it unless the person has said otherwise (decision
            // 0036), so "your other devices will get it" is true only of the
            // device chosen to keep them, if there is one.
            val it = if (saved == 1) "it" else "them"
            val keepers = runCatching {
                withContext(Dispatchers.IO) { Engine.open(this@ShareActivity).holders() }
            }.getOrDefault(emptyList())
            views.detail.text = buildString {
                append(
                    when {
                        !area ->
                            "Your other devices will get $it the next time one is " +
                                "online. You don't need to do anything."
                        keepers.isNotEmpty() ->
                            "${keepers.joinToString(", ") { k -> k.name }} keeps a backup " +
                                "the next time it's online. Nobody else sees $it."
                        else ->
                            (if (saved == 1) "It stays" else "They stay") +
                                " on this phone and nowhere else. To keep a backup, choose a " +
                                "device in Qurb's Devices."
                    }
                )
                if (failed > 0) {
                    append("\n\n$failed could not be read and was not saved.")
                }
            }
        }
    }

    private fun refuse(headline: String, detail: String) {
        views.progress.visibility = android.view.View.GONE
        views.done.visibility = android.view.View.VISIBLE
        views.headline.text = headline
        views.detail.text = detail
    }

    private fun insetFor(view: android.view.View) {
        ViewCompat.setOnApplyWindowInsetsListener(view) { target, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            target.updatePadding(left = bars.left, top = bars.top, right = bars.right, bottom = bars.bottom)
            insets
        }
    }
}
