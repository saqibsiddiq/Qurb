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
import uniffi.qurb_mobile.Usage

/**
 * Settings, as redesigned (docs/design/redesign.md, Settings draft 2): four
 * short groups of rows, each an icon, a name and a short value. *This phone*:
 * its name and picture, its identity, and its key (one row, opening where the
 * key is safe and the 24 words behind the screen lock). *Storage*: the space Qurb takes,
 * and Free up. *App*: notifications as one switch, the theme, motion.
 * *Connection*: background sync, and Advanced on a page of its own.
 *
 * Gone with the redesign: Backups (each device's page has them), and the
 * Devices row. Kept for now, though the design moves them: Recently deleted
 * here until Files has it, and the shared files' settings under Advanced until
 * the phone stops holding shared files (decision 0073).
 *
 * The name and picture are this phone's own, set here and nowhere else, and
 * every paired device learns them at its next sync (the owner's list of
 * 2026-10-26, item 7). Until a name is set, the phone's model.
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
        val sentCopies: ULong,
        /** What the app's cache holds, in bytes. */
        val cache: ULong,
        /** The name set here, or empty. */
        val name: String,
        val picture: android.graphics.Bitmap?,
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
                    val profile = runCatching { engine.ownProfile() }.getOrNull()
                    State(
                        engine.usage(),
                        engine.sentCopies(),
                        app.cacheDir.walkTopDown().filter { it.isFile }.sumOf { it.length() }.toULong(),
                        profile?.name.orEmpty(),
                        profile?.picture?.let { Devices.decode(it) },
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
        val name = state.name.ifEmpty { android.os.Build.MODEL ?: "Phone" }

        // This phone as one profile, as WhatsApp has it (the owner's review of
        // 2026-11-02, SE-2): its picture, its name; tapped, the profile page.
        var card = kit.card(page, top = 18)
        val profile = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            setPadding(kit.dp(16), kit.dp(16), kit.dp(16), kit.dp(16))
            contentDescription = "Profile, $name"
            setOnClickListener { app.push(ProfileScreen(app)) }
        }
        profile.addView(kit.avatar(name, null, state.picture, 64), LinearLayout.LayoutParams(kit.dp(64), kit.dp(64)))
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply { text = name; setTextAppearance(R.style.Text_Title) })
        words.addView(TextView(app).apply {
            text = "This phone"
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
        })
        profile.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(16) })
        profile.addView(android.widget.ImageView(app).apply {
            setImageResource(R.drawable.ic_chevron_right)
            imageTintList = android.content.res.ColorStateList.valueOf(kit.color(R.color.text_3))
        }, LinearLayout.LayoutParams(kit.dp(20), kit.dp(20)))
        card.addView(profile)

        // Manage storage (SE-3): the computers', each against what it gives
        // QurbSpace; and what the app keeps here that it can let go of.
        kit.label(page, "Manage storage")
        card = kit.card(page)
        kit.setting(card, R.drawable.ic_hard_drive, "Devices") { app.push(StorageScreen(app)) }
        kit.setting(card, R.drawable.ic_trash_2, "Clear cache", Words.size(state.cache)) { clearCache(state.cache) }
        // Its own row, never part of the cache: the devices sent to have
        // these, but one may have lost its copy since (decision 0030).
        if (state.sentCopies > 0uL) {
            kit.setting(card, R.drawable.ic_send, "Sent files kept here", Words.size(state.sentCopies)) {
                letGoOfSentCopies(state.sentCopies)
            }
        }

        kit.label(page, "App")
        card = kit.card(page)
        kit.setting(card, R.drawable.ic_clock, "Activity") { app.push(ActivityScreen(app)) }
        kit.setting(card, R.drawable.ic_bell, "Notifications",
            on = Notices.enabled(app) && Notices.permitted(app), onChange = { on ->
                Notices.setEnabled(app, on)
                if (on && !Notices.permitted(app)) app.askToNotify()
                refresh()
            })
        kit.setting(card, R.drawable.ic_palette, "Theme", Appearance.chosen(app).words) { chooseTheme() }
        // Update, where Version was (SE-5): up to date, or the new one, a tap
        // installing it (decision 0078). The version itself is in Advanced.
        val update = Updates.state(app)
        val row = kit.setting(card, R.drawable.ic_download, "Update", when (update) {
            is Updates.State.Ready -> update.versionName
            is Updates.State.Failed -> "Couldn't check"
            Updates.State.UpToDate -> "Up to date"
        }) {
            when {
                update is Updates.State.Ready -> askToUpdate(update.versionName)
                else -> scope.launch {
                    val now = Updates.run(app, now = true)
                    if (now is Updates.State.Ready) askToUpdate(now.versionName)
                    else app.say(if (now is Updates.State.Failed) "Couldn't check for updates" else "Up to date")
                    refresh()
                }
            }
        }
        if (update is Updates.State.Ready) row.value.setTextColor(kit.color(R.color.green))

        kit.label(page, "Connection")
        card = kit.card(page)
        kit.setting(card, R.drawable.ic_history, "Background sync",
            SyncWorker.words(SyncWorker.every(app))) { background() }
        kit.setting(card, R.drawable.ic_link, "Advanced") { app.push(AdvancedScreen(app)) }
    }

    /** The new version, and Later or Update. */
    private fun askToUpdate(version: String) {
        kit.sheet()
            .mark(R.drawable.ic_download, R.color.green_soft, R.color.green)
            .title("$version is ready", centred = true)
            .line("You have ${Updates.installedName(app)}")
            .buttons("Update", secondary = "Later") {
                if (!Updates.allowed(app)) app.startActivity(Updates.allowIntent(app)) else Updates.installReady(app)
            }
            .show()
    }

    /** The app's cache let go of, asked first: previews and files opened
     *  from the computers, none of the person's own. */
    private fun clearCache(bytes: ULong) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2, R.color.green_soft, R.color.green)
            .title("Clear ${Words.size(bytes)}?", centred = true)
            .line("Previews and files opened from your computers. Your files stay where they are.")
            .buttons("Clear") {
                scope.launch {
                    withContext(Dispatchers.IO) { app.cacheDir.listFiles()?.forEach { it.deleteRecursively() } }
                    app.say("Cleared")
                    refresh()
                }
            }
            .show()
    }

    /** Change this phone's name or picture, and show it. */
    private fun change(work: (uniffi.qurb_mobile.Qurb) -> Unit) {
        scope.launch {
            runCatching { withContext(Dispatchers.IO) { work(engine()) } }
                .onSuccess { refresh() }
                .onFailure { app.fail("Could not change it", it) }
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
     * When the background sync last ran and how often it runs, with Set (a
     * timer) and Off. Auto, until one is set, is "about every fifteen
     * minutes, when Android allows", or every hour once another device's
     * push has woken this phone lately.
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
        kit.setting(facts, R.drawable.ic_clock, "Every", SyncWorker.words(SyncWorker.every(app)))
        // Two buttons (the owner's list of 2026-10-26, item 16, as revised
        // the same day): Set, for a timer, and Off, in red. Until one is
        // set, Auto: every 15 minutes, or hourly once another device's push
        // wakes this phone. Android's own batching makes any of them a floor.
        val line = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
        line.addView(kit.button("Set") { sheet.dismiss(); timer() },
            LinearLayout.LayoutParams(0, kit.dp(48), 1f))
        line.addView(kit.button("Off", Kit.Style.DANGER) {
            sheet.dismiss()
            SyncWorker.choose(app, SyncWorker.OFF)
            app.say("Background sync off")
            refresh()
        }, LinearLayout.LayoutParams(0, kit.dp(48), 1f).apply { marginStart = kit.dp(10) })
        sheet.view(line, top = 18).show()
    }

    /**
     * How often background sync runs, as a timer: hours and minutes on two
     * wheels, from [SyncWorker.SHORTEST] to [SyncWorker.LONGEST], minutes in
     * quarters. Starts at what is set, or 15 minutes.
     */
    private fun timer() {
        val now = SyncWorker.every(app).takeIf { it > 0 } ?: SyncWorker.SHORTEST
        val quarters = arrayOf("00", "15", "30", "45")
        val hours = wheel(0, (SyncWorker.LONGEST / 60).toInt(), (now / 60).toInt())
        val minutes = wheel(0, quarters.size - 1, ((now % 60) / 15).toInt()).apply { displayedValues = quarters }
        // Never under 15 minutes, never over a day: the wheels put themselves right.
        val keep = { _: android.widget.NumberPicker, _: Int, _: Int ->
            if (hours.value == 0 && minutes.value == 0) minutes.value = 1
            if (hours.value == hours.maxValue && minutes.value != 0) minutes.value = 0
        }
        hours.setOnValueChangedListener(keep)
        minutes.setOnValueChangedListener(keep)

        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
        }
        for ((picker, unit) in listOf(hours to "h", minutes to "min")) {
            row.addView(picker, LinearLayout.LayoutParams(kit.dp(72), ViewGroup.LayoutParams.WRAP_CONTENT))
            row.addView(TextView(app).apply {
                text = unit
                setTextAppearance(R.style.Text_Title)
                setTextColor(kit.color(R.color.text_2))
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                marginStart = kit.dp(4)
                marginEnd = kit.dp(20)
            })
        }
        val sheet = kit.sheet()
            .mark(R.drawable.ic_clock, R.color.green_soft, R.color.green)
            .title("Every", centred = true)
            .view(row, top = 8)
        sheet.view(kit.button("Set") {
            sheet.dismiss()
            val every = (hours.value * 60L + minutes.value * 15L)
                .coerceIn(SyncWorker.SHORTEST, SyncWorker.LONGEST)
            SyncWorker.choose(app, every)
            app.say("Every ${SyncWorker.words(every)}")
            refresh()
        }, top = 12).show()
    }

    /** One wheel of [timer], its numbers in the page's colours. */
    private fun wheel(min: Int, max: Int, at: Int) = android.widget.NumberPicker(app).apply {
        minValue = min
        maxValue = max
        value = at.coerceIn(min, max)
        wrapSelectorWheel = false
        descendantFocusability = android.widget.NumberPicker.FOCUS_BLOCK_DESCENDANTS
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
            textColor = kit.color(R.color.text)
            selectionDividerHeight = kit.dp(1)
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
