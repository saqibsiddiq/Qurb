package com.qurb

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.lifecycleScope
import com.qurb.databinding.ActivityShareBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

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
                "qurb is not set up on this phone",
                "Open qurb first and either create a key or enter your 24 words.",
            )
            incoming.isEmpty() -> refuse(
                "Nothing to save",
                "That share did not contain a file qurb can read.",
            )
            else -> save(incoming)
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
    private fun save(uris: List<Uri>) {
        views.headline.text = if (uris.size == 1) "Saving to qurb" else "Saving ${uris.size} files to qurb"
        views.detail.text = ""

        lifecycleScope.launch {
            var saved = 0
            var failed = 0
            for (uri in uris) {
                try {
                    Engine.importUri(this@ShareActivity, uri)
                    saved++
                } catch (e: Exception) {
                    android.util.Log.w("qurb", "could not save a shared file", e)
                    failed++
                }
            }

            if (saved == 0) {
                refuse("Could not save that", "qurb could not read the file it was handed.")
                return@launch
            }

            // Ask for a sync through the scheduler rather than syncing here.
            // A share should not hold the screen open while a phone looks for a
            // computer that may be switched off, and going through WorkManager
            // means the attempt survives this screen closing and is retried on
            // its own if nothing answers.
            SyncWorker.runNow(this@ShareActivity)

            val waiting = withContext(Dispatchers.IO) {
                runCatching { Engine.open(this@ShareActivity).outstanding().files.size }
                    .getOrDefault(0)
            }

            views.progress.visibility = android.view.View.GONE
            views.done.visibility = android.view.View.VISIBLE
            views.headline.text = when {
                failed > 0 -> "Saved $saved of ${saved + failed}"
                saved == 1 -> "Saved to qurb"
                else -> "Saved $saved files to qurb"
            }
            views.detail.text = buildString {
                append(
                    if (waiting > 0) {
                        "Your other devices will get " +
                            (if (waiting == 1) "it" else "them") +
                            " the next time one is switched on and reachable. " +
                            "You do not need to do anything."
                    } else {
                        "Already copied to your other device."
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
