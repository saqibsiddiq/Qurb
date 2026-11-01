package com.qurb

import android.view.Gravity
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The key: where it is safe, checked, and its 24 words. In Advanced since
 * the owner's review of 2026-11-02 (SE-1); it was a row of Settings.
 */
class KeySheet(private val app: MainActivity, private val scope: CoroutineScope, private val refresh: () -> Unit) {

    private val kit get() = app.kit
    private suspend fun engine() = Engine.open(app)

    /** Whether the key this phone holds is the one its backup gives back, and
     *  whether that backup leaves the phone (Google's, with a screen lock). */
    class Kept(val kept: Boolean, val inCloud: Boolean)

    /** Read off the main thread. */
    suspend fun kept(): Kept = withContext(Dispatchers.IO) {
        val found = Backup.find(app)
        val mine = runCatching { engine().recoveryPhrase() }.getOrNull()
        Kept(found != null && found == mine, Backup.leavesThePhone(app))
    }

    /** What the row says. */
    fun words(state: Kept) = when {
        !state.kept -> "Not kept yet"
        state.inCloud -> "Backed up"
        else -> "This phone only"
    }

    /**
     * Where the key is safe (decision 0053), checked: on this phone, in the
     * Android Keystore; and in Google's backup, end-to-end encrypted with the
     * screen lock, when there is one. The 24 words behind the screen lock.
     */
    fun show(state: Kept) {
        val title = when {
            !state.kept -> "Your key isn't kept yet"
            state.inCloud -> "Your key is safe"
            else -> "Your key is on this phone only"
        }
        val facts = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.group_quiet)
            setPadding(0, kit.dp(4), 0, kit.dp(4))
        }
        // Said as it is: the Keystore key is not tied to unlocking the phone
        // (AndroidKeyStore.kt says why), so background syncing works locked.
        kit.setting(facts, R.drawable.ic_smartphone, "This phone", "Android Keystore")
        kit.setting(facts, R.drawable.ic_cloud, "Google backup", when {
            !state.kept -> "Not kept"
            state.inCloud -> "Encrypted with your screen lock"
            else -> "Off: no screen lock"
        })
        val sheet = kit.sheet()
            .mark(R.drawable.ic_shield_check,
                if (state.kept && state.inCloud) R.color.green_soft else R.color.attention_bg,
                if (state.kept && state.inCloud) R.color.green else R.color.attention)
            .title(title, centred = true)
            .view(facts, top = 16)
        if (!state.kept) {
            sheet.view(kit.button("Keep it now") { sheet.dismiss(); keepKey() },
                top = 18)
        }
        sheet.view(kit.button("Show the 24 words", Kit.Style.SECONDARY) {
            sheet.dismiss()
            showPhrase()
        }, top = if (state.kept) 18 else 10)
        if (ScreenLock.available(app)) {
            val note = TextView(app).apply {
                text = "Asks for your screen lock"
                setTextAppearance(R.style.Text_Meta)
                gravity = Gravity.CENTER
                compoundDrawablePadding = kit.dp(6)
                setCompoundDrawablesRelativeWithIntrinsicBounds(
                    ContextCompat.getDrawable(app, R.drawable.ic_lock_keyhole)?.mutate()?.apply {
                        setBounds(0, 0, kit.dp(14), kit.dp(14))
                        setTint(kit.color(R.color.text_3))
                    }, null, null, null)
            }
            val centre = FrameLayout(app)
            centre.addView(note, FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
            sheet.view(centre, top = 10)
        }
        sheet.show()
    }

    /**
     * The words again, for a new paper copy before the old one is lost.
     *
     * Not a secret kept from the person holding the phone: anyone who can
     * open this app can read every file already, so the words give away
     * nothing new (decision 0033). But they are the key, so the screen lock is
     * asked first where there is one (the redesign), and the sheet is kept
     * out of screenshots while they are on it.
     */
    fun showPhrase() {
        if (ScreenLock.available(app)) {
            ScreenLock.confirm(app, "Show your 24 words", "Anyone who sees them can read your files") { yes ->
                if (yes) app.runOnUiThread { phrase() }
            }
        } else {
            kit.sheet()
                .mark(R.drawable.ic_key_round, R.color.attention_bg, R.color.attention)
                .title("Show your 24 words?", centred = true)
                .line("Anyone who sees them can read every file you keep in Qurb")
                .buttons("Show") { phrase() }
                .show()
        }
    }

    private fun phrase() {
        scope.launch {
            try {
                val words = withContext(Dispatchers.IO) { engine().recoveryPhrase() }
                val sheet = kit.sheet()
                    .title("Your 24 words")
                    .view(Words.phraseView(app, words), top = 16)
                // As a text file, wherever the person picks (the owner's list
                // of 2026-10-26, item 17): numbered, one a line.
                sheet.view(kit.button("Save as text file", Kit.Style.SECONDARY, R.drawable.ic_download) {
                    app.saveText("QurbSpace 24 words.txt",
                        words.trim().split(Regex("\\s+")).mapIndexed { i, w -> "${i + 1}. $w" }.joinToString("\n", postfix = "\n"))
                }, top = 16)
                    .closing("Hide")
                    .secure()
                    .show()
            } catch (e: Exception) {
                app.fail("Could not show the words", e)
            }
        }
    }

    /** Keep the key in Block Store now, for a phone that does not have it. */
    fun keepKey() {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { Backup.save(app, engine().recoveryPhrase()) }
                app.say("Your key is kept")
            } catch (e: Exception) {
                app.fail("Could not keep the key", e)
            } finally {
                refresh()
            }
        }
    }

}
