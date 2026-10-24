package com.qurb

import android.view.Gravity
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import uniffi.qurb_mobile.BackupState

/**
 * The phone's backups to the computer that keeps its Private Vault, said the
 * same way wherever they are reached: WhatsApp's (decision 0062) and the
 * phone's folders (0063). As redesigned (docs/design/redesign.md, Backups
 * and Activity, draft 1): a card's state in an icon and a few words, a bar
 * while one runs; WhatsApp's details and its setting up as sheets of facts.
 *
 * One sentence is kept wherever WhatsApp's backup is shown or set up: keep
 * WhatsApp's own Google backup on too. Restoring from the computer has not
 * been tried (decision 0062), and until it has, nothing here suggests
 * turning Google's off.
 */
class Backups(private val app: MainActivity, private val refresh: () -> Unit) {

    private val kit get() = app.kit

    /** A backup's state for its card: off, starting, how many to go, or up to date. */
    fun progress(backup: BackupState, on: Boolean, none: String = "Off"): Kit.Progress = when {
        !on -> Kit.Progress(R.drawable.ic_info, none)
        backup.files == 0uL -> Kit.Progress(R.drawable.ic_history, "Starting, on Wi-Fi")
        backup.heldFiles < backup.files -> Kit.Progress(R.drawable.ic_history,
            "${Words.count(backup.files - backup.heldFiles)} files to go",
            backup.heldFiles.toFloat() / backup.files.toFloat())
        else -> Kit.Progress(R.drawable.ic_circle_check, "${Words.size(backup.bytes)}, up to date")
    }

    /** Setting the backup up, or what it is doing (decision 0062). */
    fun whatsApp(backup: BackupState, keeper: String?, backing: Boolean) {
        if (!backing) {
            val sheet = kit.sheet()
                .mark(R.drawable.ic_whatsapp, R.color.green_soft, R.color.green, round = false)
                .title("Back up WhatsApp", centred = true)
            if (keeper == null) {
                sheet.line("No device keeps this phone's Private Vault yet: the backup goes to it")
                    .buttons("Devices", secondary = "Not now") { app.go(R.id.tab_devices) }
                    .show()
                return
            }
            sheet.view(where(R.drawable.ic_laptop, "To $keeper, on Wi-Fi"), top = 4)
            // The two steps Android asks for, named before it asks.
            val steps = facts()
            kit.setting(steps, R.drawable.ic_folder, "Next, choose WhatsApp's folder")
            kit.setting(steps, R.drawable.ic_circle_check, "Then tap Allow")
            sheet.view(steps, top = 16)
            // Decision 0062, answer 4: said where it is set up, and on the
            // page, until a restore has been watched working.
            kit.note(sheet.body, "Keep WhatsApp's own Google backup on too", top = 12)
            sheet.buttons("Choose folder", secondary = "Not now") { app.pickWhatsApp() }
                .show()
            return
        }

        val facts = facts()
        kit.setting(facts, R.drawable.ic_circle_check, "Files",
            if (backup.heldFiles < backup.files) "${Words.count(backup.heldFiles)} of ${Words.count(backup.files)}"
            else Words.count(backup.files))
        kit.setting(facts, R.drawable.ic_download, "Size", Words.size(backup.bytes))
        if (backup.onlyKept > 0uL) {
            kit.setting(facts, R.drawable.ic_trash_2, "Deleted in WhatsApp", "${Words.count(backup.onlyKept)} kept")
        }
        kit.setting(facts, R.drawable.ic_history, "Last",
            WhatsApp.lastRun(app)?.let { (at, _) -> Words.ago(at) } ?: "Not yet")
        kit.setting(facts, R.drawable.ic_wifi, "Runs on", "Wi-Fi")
        kit.sheet()
            .mark(R.drawable.ic_whatsapp, R.color.green_soft, R.color.green, round = false)
            .title("WhatsApp", centred = true)
            .apply { if (keeper != null) view(where(R.drawable.ic_laptop, "On $keeper"), top = 4) }
            .view(facts, top = 16)
            .buttons("Back up now", secondary = "Stop", onSecondary = { stopWhatsApp(keeper) }) {
                WhatsApp.runNow(app)
                app.say("Backing up WhatsApp on Wi-Fi")
            }
            .show()
    }

    fun stopWhatsApp(keeper: String?) {
        kit.sheet()
            .mark(R.drawable.ic_whatsapp)
            .title("Stop backing up WhatsApp?", centred = true)
            .line("What ${keeper ?: "the computer"} keeps stays there")
            .buttons("Stop", danger = true) {
                WhatsApp.stop(app)
                refresh()
            }
            .onDismiss { refresh() }
            .show()
    }

    /** Facts in a sheet, on the second surface. */
    private fun facts(): LinearLayout = LinearLayout(app).apply {
        orientation = LinearLayout.VERTICAL
        background = ContextCompat.getDrawable(app, R.drawable.group_quiet)
        setPadding(0, kit.dp(4), 0, kit.dp(4))
    }

    /** Where it goes, under a centred title: a small icon and a few words. */
    private fun where(icon: Int, words: String): ViewGroup {
        val line = TextView(app).apply {
            text = words
            setTextAppearance(R.style.Text_Quiet)
            compoundDrawablePadding = kit.dp(6)
            gravity = Gravity.CENTER_VERTICAL
            setCompoundDrawablesRelativeWithIntrinsicBounds(
                ContextCompat.getDrawable(app, icon)?.mutate()?.apply {
                    setBounds(0, 0, kit.dp(16), kit.dp(16))
                    setTint(kit.color(R.color.text_2))
                }, null, null, null)
        }
        val centre = android.widget.FrameLayout(app)
        centre.addView(line, android.widget.FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
        return centre
    }
}
