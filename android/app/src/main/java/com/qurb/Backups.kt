package com.qurb

import android.widget.LinearLayout
import uniffi.qurb_mobile.BackupState

/**
 * The phone's backups to the computer that keeps its Private Vault, said the
 * same way wherever they are reached: WhatsApp's (decision 0062) and the
 * phone's folders (0063), from Home and from Settings (the owner's items 8 and
 * 10, 2026-10-14). The folders have a place of their own, [FoldersScreen].
 */
class Backups(private val app: MainActivity, private val refresh: () -> Unit) {

    private val kit get() = app.kit

    /** The folders' line: how many, and how far the computer has them. */
    fun foldersLine(chosen: Int, backup: BackupState, keeper: String?): String = when {
        !Folders.allowed(app) -> "Off. Photos, documents, any folder, kept on ${keeper ?: "your computer"}"
        chosen == 0 -> "Choose which, here or on ${keeper ?: "your computer"}"
        backup.files == 0uL -> "${Words.folders(chosen)}: starting, on Wi-Fi"
        backup.heldFiles == backup.files ->
            "${Words.folders(chosen)}, all ${Words.files(backup.files.toInt())}, on $keeper"
        else -> "${Words.folders(chosen)}: ${Words.count(backup.heldFiles)} of ${Words.files(backup.files.toInt())} " +
            "on ${keeper ?: "your computer"} so far"
    }

    fun whatsAppLine(backup: BackupState, keeper: String?, backing: Boolean): String = when {
        !backing -> "Off. Keeps WhatsApp's chats and media on ${keeper ?: "your computer"} " +
            "instead of in your Google account"
        backup.files == 0uL -> "Starting: it runs on Wi-Fi"
        backup.heldFiles == backup.files ->
            "All ${Words.files(backup.files.toInt())}, ${Words.size(backup.bytes)}, on $keeper"
        else -> "${Words.count(backup.heldFiles)} of ${Words.files(backup.files.toInt())} on ${keeper ?: "your computer"} so far"
    }

    /** Setting the backup up, or what it is doing (decision 0062). */
    fun whatsApp(backup: BackupState, keeper: String?, backing: Boolean) {
        if (!backing) {
            val sheet = kit.sheet().header(R.drawable.ic_hard_drive, "Back up WhatsApp",
                keeper?.let { "To $it" } ?: "")
            if (keeper == null) {
                sheet.text("No computer keeps this phone's Private Vault yet. Choose one in Devices: " +
                    "the backup goes to it.")
                    .buttons("Devices") { app.go(R.id.tab_devices) }
                    .show()
                return
            }
            sheet.text("WhatsApp's chats, photos, videos and documents, kept on $keeper in this phone's " +
                "Private Vault: out of sight there, and nothing copied on this phone. Photos and videos " +
                "you delete in WhatsApp stay on $keeper.", R.style.Text_Body)
                .text("Over Wi-Fi only. The first time is everything WhatsApp keeps; after that, " +
                    "what changed each night.")
                // Decision 0062, answer 4: said where it is set up, and again
                // below, until a restore has been watched working.
                .text("Restoring WhatsApp from it hasn't been tried yet, so keep WhatsApp's own Google " +
                    "backup on for now. Turning off Include videos there saves the most space.")
                .text("Next, Android opens WhatsApp's folder: choose Use this folder, then Allow.")
                // Short: on the S23's 360 dp, "Choose WhatsApp's folder" broke
                // over two lines inside the button.
                .buttons("Choose folder", secondary = "Not now") { app.pickWhatsApp() }
                .show()
            return
        }

        val sheet = kit.sheet().header(R.drawable.ic_hard_drive, "WhatsApp backup",
            keeper?.let { "On $it" } ?: "No computer keeps it yet")
        val facts = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        val group = kit.group(facts)
        kit.item(group, "On ${keeper ?: "a computer"}",
            if (backup.files == 0uL) "Nothing yet"
            else "${Words.count(backup.heldFiles)} of ${Words.files(backup.files.toInt())}, " +
                "${Words.size(backup.heldBytes)} of ${Words.size(backup.bytes)}")
        if (backup.onlyKept > 0uL) {
            kit.item(group, "Deleted in WhatsApp", "${Words.files(backup.onlyKept.toInt())}, kept on $keeper")
        }
        kit.item(group, "Last run", WhatsApp.lastRun(app)?.let { (at, said) -> "${Words.ago(at)}: $said" }
            ?: "Not yet. It runs on Wi-Fi.")
        sheet.view(facts, top = 16)
            .text("Restoring WhatsApp from it hasn't been tried yet: keep WhatsApp's own Google backup on " +
                "for now.")
            .action(R.drawable.ic_refresh_cw, "Back up now", meta = "On Wi-Fi") {
                WhatsApp.runNow(app)
                app.say("Backing up WhatsApp on Wi-Fi")
            }
            .action(R.drawable.ic_x, "Stop backing up…", danger = true) { stopWhatsApp(keeper) }
            .show()
    }

    fun stopWhatsApp(keeper: String?) {
        kit.sheet().header(R.drawable.ic_hard_drive, "Stop backing up WhatsApp?")
            .text("Nothing new goes to ${keeper ?: "the computer"}. What it keeps already stays there.")
            .buttons("Stop", danger = true) {
                WhatsApp.stop(app)
                refresh()
            }
            .show()
    }
}
