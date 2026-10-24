package com.qurb

import android.content.Intent
import android.content.res.ColorStateList
import android.net.Uri
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.annotation.DrawableRes
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.google.android.material.button.MaterialButton
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.progressindicator.LinearProgressIndicator
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.PeerInfo

/**
 * The share sheet's way in, as Sharing into Qurb draft 1 draws it: sheets over
 * the app the person was sharing from -- *Save to*, then the file going, then
 * what happened -- and nothing of this activity's own behind them.
 *
 * Saving needs no network. A file saved writes into this phone's folder and
 * is indexed here and now, whether or not any other device is on; reaching
 * them is a separate question, answered whenever one is next awake. So there
 * is no outbox and no retry queue: the file is in the folder like any other,
 * and the index knows which devices hold it.
 *
 * A send is the engine's once handed to it, kept until that device collects
 * it (decision 0060). The sheet watches for that, and says *Sent* only when it
 * has happened, as Home does; Hide leaves it to the sync worker.
 */
class ShareActivity : AppCompatActivity() {

    private val kit by lazy { Kit(this) }

    /** The sheet up now. Each step puts its own in its place; the last one
     *  closed, by Done, a swipe or back, closes this. */
    private var shown: Sheet? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val incoming = incomingUris()
        when {
            !Engine.isSetUp(this) -> outcome(Outcome.ATTENTION, "Set up Qurb first")
            incoming.isEmpty() -> outcome(Outcome.ATTENTION, "Nothing to save")
            else -> chooseWhere(incoming)
        }
    }

    /** Puts [sheet] up in place of the one before. */
    private fun present(sheet: Sheet) {
        val before = shown
        shown = sheet
        sheet.onDismiss { if (shown === sheet) finish() }
        sheet.show()
        before?.dismiss()
    }

    /**
     * Save to this phone's Vault, or send to one of the devices (brief §39):
     * the choice the share sheet exists to make fast. No Files: a phone keeps
     * no shared files (decision 0073). With no device to send to there is
     * nothing to choose, and the file goes into the Vault.
     */
    private fun chooseWhere(uris: List<Uri>) {
        lifecycleScope.launch {
            val peers = runCatching {
                withContext(Dispatchers.IO) { Engine.open(this@ShareActivity).peers() }
            }.getOrDefault(emptyList())
            if (peers.isEmpty()) {
                save(uris)
                return@launch
            }
            val list = LinearLayout(this@ShareActivity).apply { orientation = LinearLayout.VERTICAL }
            kit.label(list, "This phone", top = 0)
            kit.deviceCard(list, R.drawable.ic_lock_keyhole, "Vault", off = null, since = null) {
                save(uris)
            }
            kit.label(list, "Send to", top = 14)
            for (peer in peers) {
                kit.deviceCard(
                    list, States.device(peer.name, peer.kind), peer.name, Engine.isOff(this@ShareActivity, peer),
                    peer.lastSeen?.let { Words.since(it) }, end = R.drawable.ic_chevron_right,
                ) { send(uris, peer) }
            }
            present(kit.sheet().title("Save to").view(list, top = 10))
        }
    }

    /**
     * Straight to one device, privately, and nowhere else (decision 0030): the
     * file, a bar, where it goes, and Hide. Each file is copied in first, as
     * the share sheet's permission to read it ends with this screen; Hide
     * waits for that, and for the question about files sent before, then
     * leaves the rest to the sync worker.
     */
    private fun send(uris: List<Uri>, to: PeerInfo) {
        val going = going(uris, listOf(R.drawable.ic_send, States.device(to.name, to.kind)), to.name, hide = true)
        lifecycleScope.launch {
            var failed = 0
            val staged = mutableListOf<Engine.Staged>()
            val sending = mutableSetOf<Engine.Staged>()
            val names = mutableListOf<String>()
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
                        names += Engine.send(this@ShareActivity, file, to.fingerprint)
                        sending += file
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
            if (names.isEmpty()) {
                outcome(Outcome.ATTENTION, "Couldn't read that")
                return@launch
            }
            val engine = Engine.open(this@ShareActivity)
            val from = withContext(Dispatchers.IO) { engine.serving().bytes }
            SyncWorker.runNow(this@ShareActivity)
            going.hide?.isEnabled = true
            going.sheet.dialog.setCancelable(true)
            val total = sending.sumOf { it.file.length() }.coerceAtLeast(1L)
            val of = if (failed > 0) " ${names.size} of ${names.size + failed}" else ""

            // Collected when the engine no longer has them waiting for that
            // device; going while bytes go out. Nothing moving for a while is
            // a device off or out of reach: it collects them when it is next
            // on, which the sheet says rather than wait with a still bar.
            var last = from
            var moved = System.currentTimeMillis()
            while (true) {
                delay(500)
                val (served, left) = withContext(Dispatchers.IO) {
                    engine.serving().bytes to runCatching { engine.waiting() }.getOrDefault(emptyList())
                        .count { it.toFingerprint == to.fingerprint && it.path in names }
                }
                if (left == 0) {
                    outcome(Outcome.DONE, if (of.isEmpty()) "Sent to ${to.name}" else "Sent$of to ${to.name}")
                    return@launch
                }
                val done = (served - from).toDouble() / total.toDouble()
                going.bar.setProgressCompat((done.coerceIn(0.0, 1.0) * 1000).toInt(), true)
                if (served != last) {
                    last = served
                    moved = System.currentTimeMillis()
                } else if (System.currentTimeMillis() - moved > STILL_FOR_MS) {
                    outcome(Outcome.WAITING, "Waiting for ${to.name}")
                    return@launch
                }
            }
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
     * Save every shared file into this phone's folder, then say so.
     *
     * One failure does not abandon the rest: someone sharing eleven photos
     * should get ten of them and be told about the one that did not work,
     * rather than losing all eleven to it.
     */
    private fun save(uris: List<Uri>) {
        going(uris, listOf(R.drawable.ic_lock_keyhole), "Vault", hide = false)
        lifecycleScope.launch {
            var saved = 0
            var failed = 0
            for (uri in uris) {
                try {
                    Engine.importUri(this@ShareActivity, uri, private = true)
                    saved++
                } catch (e: Exception) {
                    android.util.Log.w("qurb", "could not save a shared file", e)
                    failed++
                }
            }
            if (saved == 0) {
                outcome(Outcome.ATTENTION, "Couldn't read that")
                return@launch
            }
            // Through the scheduler rather than syncing here: a share should
            // not hold the screen open while the phone looks for a computer
            // that may be off, and the worker outlives this screen and tries
            // again on its own.
            SyncWorker.runNow(this@ShareActivity)
            outcome(
                if (failed > 0) Outcome.ATTENTION else Outcome.DONE,
                if (failed > 0) "Saved $saved of ${saved + failed}" else "Saved to Vault",
            )
        }
    }

    /** The sheet while files go, and the parts of it that change. */
    private class Going(val sheet: Sheet, val bar: LinearProgressIndicator, val hide: MaterialButton?)

    /**
     * The file going (Sharing into Qurb, *Sending*): its kind in a quiet tile,
     * its name over a bar, and where it goes as icons and a name. The bar
     * turns while files are copied in, then fills as the other device
     * collects. Not closed by a swipe or back until it can carry on without
     * this screen; [hide] is the way out from then.
     */
    private fun going(uris: List<Uri>, to: List<Int>, where: String, hide: Boolean): Going {
        val first = runCatching { Engine.displayName(this, uris[0]) }.getOrDefault("")
        val name = if (uris.size == 1) first else "${uris.size} files"

        val file = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        val tile = FrameLayout(this).apply {
            background = ContextCompat.getDrawable(this@ShareActivity, R.drawable.tile_mark)
            backgroundTintList = ColorStateList.valueOf(kit.color(R.color.surface_2))
        }
        tile.addView(ImageView(this).apply {
            setImageResource(if (uris.size == 1) States.icon(first) else R.drawable.ic_file)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(kit.dp(26), kit.dp(26), Gravity.CENTER))
        file.addView(tile, LinearLayout.LayoutParams(kit.dp(52), kit.dp(52)).apply { marginEnd = kit.dp(14) })
        val words = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(this).apply {
            text = name
            setTextAppearance(R.style.Text_Section)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.MIDDLE
        })
        val bar = LinearProgressIndicator(this).apply {
            isIndeterminate = true
            max = 1000
            trackThickness = kit.dp(6)
            trackCornerRadius = kit.dp(3)
            setIndicatorColor(kit.color(R.color.green))
            trackColor = kit.color(R.color.neutral_bg)
            // One bar, as the design draws it: Material 3 cuts a gap between
            // what is done and what is not, and puts a dot at the end.
            indicatorTrackGapSize = 0
            trackStopIndicatorSize = 0
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }
        words.addView(bar, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(8) })
        file.addView(words, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))

        val towards = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            contentDescription = if (hide) "To $where" else "Into $where"
        }
        for (icon in to) towards.addView(ImageView(this).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(16), kit.dp(16)).apply { marginEnd = kit.dp(6) })
        towards.addView(TextView(this).apply {
            text = where
            setTextAppearance(R.style.Text_Name)
            setTextColor(kit.color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        })

        val sheet = kit.sheet().view(file, top = 6).view(towards, top = 14)
        val button = if (hide) sheet.wide("Hide", Kit.Style.SECONDARY) { finish() }.apply { isEnabled = false } else null
        sheet.dialog.setCancelable(false)
        present(sheet)
        return Going(sheet, bar, button)
    }

    /** How it ended: a tick, something to look at, or a device not there yet. */
    private enum class Outcome(@DrawableRes val icon: Int, val fill: Int, val tint: Int) {
        DONE(R.drawable.ic_circle_check, R.color.healthy_bg, R.color.healthy),
        WAITING(R.drawable.ic_clock, R.color.surface_2, R.color.text_2),
        ATTENTION(R.drawable.ic_triangle_alert, R.color.attention_bg, R.color.attention),
    }

    /** What happened, in its mark and a few words, and Done. */
    private fun outcome(kind: Outcome, title: String) {
        val sheet = kit.sheet().mark(kind.icon, fill = kind.fill, tint = kind.tint).title(title, centred = true)
        sheet.wide("Done") { sheet.dismiss() }
        present(sheet)
    }

    private companion object {
        /** How long nothing goes out before the sheet says it is waiting. */
        const val STILL_FOR_MS = 30_000L
    }
}
