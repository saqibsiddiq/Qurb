package com.qurb

import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.DeletedFile
import uniffi.qurb_mobile.Usage

/**
 * Settings, as redesigned (docs/design/redesign.md, Settings draft 2): four
 * short groups of rows, each an icon, a name and a short value. *This phone*:
 * its name, its identity, and its key (one row, opening where the key is safe
 * and the 24 words behind the screen lock). *Storage*: the space Qurb takes,
 * and Free up. *App*: notifications as one switch, the theme, motion.
 * *Connection*: background sync, and Advanced on a page of its own.
 *
 * Gone with the redesign: Backups (each device's page has them), and the
 * Devices row. Kept for now, though the design moves them: Recently deleted
 * here until Files has it, and the shared files' settings under Advanced until
 * the phone stops holding shared files (decision 0073).
 *
 * The name is shown and not changed here: a device learns this phone's name
 * when they pair and keeps it, so a new name would reach no device already
 * paired. Renaming waits on the engine telling devices a new name.
 */
class SettingsScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.title.text = "Settings"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(
        val usage: Usage,
        val identity: String,
        val deleted: List<DeletedFile>,
        val sentCopies: ULong,
        /** Whether the key this phone holds is the one its backup gives back. */
        val keyKept: Boolean,
        /** Whether that backup leaves the phone: Google's, with a screen lock. */
        val keyInCloud: Boolean,
    )

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    // Access just granted, in the system's page the folders
                    // opened: the folders listed at once, for the computer to
                    // show, and the nightly run scheduled (decision 0063).
                    if (Folders.allowed(app) && !Folders.noticed(app)) {
                        runCatching { Folders.run(app, engine, 0u, listNow = true) }
                        WhatsApp.schedule(app)
                        SyncWorker.runNow(app)
                    }
                    // Where the key is safe, checked rather than assumed
                    // (decision 0053): the key the backup gives back is
                    // compared with this phone's own.
                    val found = Backup.find(app)
                    val mine = runCatching { engine.recoveryPhrase() }.getOrNull()
                    State(
                        engine.usage(),
                        runCatching { engine.identityShort() }.getOrDefault(""),
                        engine.recentlyDeleted(),
                        engine.sentCopies(),
                        found != null && found == mine,
                        Backup.leavesThePhone(app),
                    )
                }
                show(state)
            } catch (e: Exception) {
                app.fail("Could not read the settings", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(state: State) {
        val page = views.sections
        page.removeAllViews()

        kit.label(page, "This phone", top = 12)
        var card = kit.card(page)
        kit.setting(card, R.drawable.ic_smartphone, "Name", android.os.Build.MODEL ?: "Phone")
        if (state.identity.isNotEmpty()) kit.setting(card, R.drawable.ic_info, "Identity", state.identity)
        kit.setting(card, R.drawable.ic_shield_check, "Key", when {
            !state.keyKept -> "Not kept yet"
            state.keyInCloud -> "Safe"
            else -> "This phone only"
        }) { key(state) }

        kit.label(page, "Storage")
        card = kit.card(page)
        // Free up beside the space it frees, as words: never the destructive
        // one, so never red.
        val used = kit.setting(card, R.drawable.ic_hard_drive, "Space used", Words.size(state.usage.onDisk))
        used.root.addView(kit.button("Free up", Kit.Style.TEXT) { tidy() }.apply {
            minWidth = 0
            minimumWidth = 0
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginEnd = -kit.dp(8) })
        // Its own row, never part of the one above: the devices sent to have
        // these, but one may have lost its copy since (decision 0030).
        if (state.sentCopies > 0uL) {
            kit.setting(card, R.drawable.ic_send, "Copies of files you sent", Words.size(state.sentCopies)) {
                letGoOfSentCopies(state.sentCopies)
            }
        }
        // Until Files has it (the redesign puts Deleted there).
        kit.setting(card, R.drawable.ic_trash_2, "Recently deleted",
            if (state.deleted.isEmpty()) "" else Words.files(state.deleted.size)) {
            app.push(DeletedScreen(app, fromSettings = true))
        }

        kit.label(page, "App")
        card = kit.card(page)
        // One switch, and no words under it (the owner, 2026-10-24: "lets
        // keep notification one switch only"). On means asked of Android too.
        // What happened, moved here from Home's top bar (the owner's list of
        // 2026-10-26, item 8): 30 days of it, each file told once.
        kit.setting(card, R.drawable.ic_clock, "Activity") { app.push(ActivityScreen(app)) }
        kit.setting(card, R.drawable.ic_bell, "Notifications",
            on = Notices.enabled(app) && Notices.permitted(app), onChange = { on ->
                Notices.setEnabled(app, on)
                if (on && !Notices.permitted(app)) app.askToNotify()
                refresh()
            })
        kit.setting(card, R.drawable.ic_palette, "Theme", Appearance.chosen(app).words) { chooseTheme() }
        // Follows the phone's own animation setting; nothing to choose here.
        kit.setting(card, R.drawable.ic_sliders_horizontal, "Motion", if (Kit.calm()) "Reduced" else "Full")

        kit.label(page, "Connection")
        card = kit.card(page)
        kit.setting(card, R.drawable.ic_history, "Background sync",
            SyncWorker.CHOICES.firstOrNull { it.first == SyncWorker.every(app) }?.second ?: "") { background() }
        kit.setting(card, R.drawable.ic_link, "Advanced") { app.push(AdvancedScreen(app)) }
    }

    /**
     * Where the key is safe (decision 0053), checked: on this phone, in the
     * Android Keystore; and in Google's backup, end-to-end encrypted with the
     * screen lock, when there is one. The 24 words behind the screen lock.
     */
    private fun key(state: State) {
        val title = when {
            !state.keyKept -> "Your key isn't kept yet"
            state.keyInCloud -> "Your key is safe"
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
            !state.keyKept -> "Not kept"
            state.keyInCloud -> "Encrypted with your screen lock"
            else -> "Off: no screen lock"
        })
        val sheet = kit.sheet()
            .mark(R.drawable.ic_shield_check,
                if (state.keyKept && state.keyInCloud) R.color.green_soft else R.color.attention_bg,
                if (state.keyKept && state.keyInCloud) R.color.green else R.color.attention)
            .title(title, centred = true)
            .view(facts, top = 16)
        if (!state.keyKept) {
            sheet.view(kit.button("Keep it now") { sheet.dismiss(); keepKey() },
                top = 18)
        }
        sheet.view(kit.button("Show the 24 words", Kit.Style.SECONDARY) {
            sheet.dismiss()
            showPhrase()
        }, top = if (state.keyKept) 18 else 10)
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
    private fun showPhrase() {
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

    /** Light, dark, or as the phone is set (decision 0056), applied at once: three of one width. */
    private fun chooseTheme() {
        val chosen = Appearance.chosen(app)
        val sheet = kit.sheet().title("Theme")
        val row = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
        for (theme in Appearance.Theme.entries) {
            row.addView(kit.button(theme.words, if (theme == chosen) Kit.Style.PRIMARY else Kit.Style.SECONDARY) {
                sheet.dismiss()
                Appearance.choose(app, theme)
                refresh()
            }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                if (row.childCount > 0) marginStart = kit.dp(8)
            })
        }
        sheet.view(row, top = 16).show()
    }

    /**
     * When the background sync last ran, and how often it does. The honest
     * answer is "about every fifteen minutes, when Android allows", or every
     * hour once another device's push has woken this phone lately.
     */
    private fun background() {
        val facts = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.group_quiet)
            setPadding(0, kit.dp(4), 0, kit.dp(4))
        }
        val at = SyncWorker.lastAt(app)
        val result = SyncWorker.lastResult(app)
        kit.setting(facts, R.drawable.ic_circle_check, "Last",
            if (at == null) "Not yet" else listOfNotNull(Words.ago(at / 1000), result).joinToString(", "))
        val sheet = kit.sheet()
            .mark(R.drawable.ic_history, R.color.green_soft, R.color.green)
            .title("Background sync", centred = true)
            .view(facts, top = 16)
        // How often, or not at all (the owner's list of 2026-10-26, item
        // 16): three of one width a row, the chosen one filled. Auto is
        // every 15 minutes, or hourly once another device's push wakes this
        // phone; Android's own batching makes any of them a floor.
        val chosen = SyncWorker.every(app)
        for (row in SyncWorker.CHOICES.chunked(3)) {
            val line = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
            for ((minutes, words) in row) {
                line.addView(kit.button(words, if (minutes == chosen) Kit.Style.PRIMARY else Kit.Style.SECONDARY) {
                    sheet.dismiss()
                    SyncWorker.choose(app, minutes)
                    app.say(if (minutes == SyncWorker.OFF) "Background sync off" else "Background sync: $words")
                    refresh()
                }, LinearLayout.LayoutParams(0, kit.dp(44), 1f).apply {
                    if (line.childCount > 0) marginStart = kit.dp(8)
                })
            }
            sheet.view(line, top = 10)
        }
        sheet.view(kit.button("Run now") {
            sheet.dismiss()
            // Through the scheduler rather than directly, so this exercises
            // the same path the periodic schedule uses.
            SyncWorker.runNow(app)
            app.say("Queued. It runs when Android allows.")
        }, top = 18).show()
    }

    /** Keep the key in Block Store now, for a phone that does not have it. */
    private fun keepKey() {
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

    private fun tidy() {
        scope.launch {
            try {
                val tidied = withContext(Dispatchers.IO) { engine().housekeep() }
                app.say(if (tidied.freed > 0uL) "Freed ${Words.size(tidied.freed)}" else "Nothing to free")
            } catch (e: Exception) {
                app.fail("Could not free space", e)
            } finally {
                refresh()
            }
        }
    }

    /**
     * This phone's copies of files it sent that have arrived (decision 0030).
     * Kept until asked for by name, and the asking says what it can cost: on
     * 2026-10-07 the S23 held the last copy of a video the laptop had taken
     * and since lost.
     */
    private fun letGoOfSentCopies(bytes: ULong) {
        kit.sheet()
            .mark(R.drawable.ic_send)
            .title("Let go of ${Words.size(bytes)}?", centred = true)
            .line(SentCopies.COST)
            .buttons("Let go", danger = true, secondary = "Keep") {
                scope.launch {
                    try {
                        val freed = withContext(Dispatchers.IO) { engine().releaseSentCopies() }
                        app.say("Freed ${Words.size(freed)}")
                    } catch (e: Exception) {
                        app.fail("Could not free that", e)
                    } finally {
                        refresh()
                    }
                }
            }
            .show()
    }
}
