package com.qurb

import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.DeletedFile

/**
 * Recently deleted (decision 0042, direction §22): files deleted on this
 * phone or on another device, kept here for thirty days. Restoring one puts
 * it back where it was: a shared file on every device, the way any change
 * travels, and a Private Vault file in the vault. Reached from Files and from
 * Settings.
 */
class DeletedScreen(app: MainActivity, fromSettings: Boolean = false) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = if (fromSettings) R.id.tab_settings else R.id.tab_files

    init {
        views.back.visibility = View.VISIBLE
        // Named for where it goes back to, as every other page's is.
        views.back.text = if (fromSettings) "Settings" else "Files"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Recently deleted"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        refreshing {
            try {
                val deleted = withContext(Dispatchers.IO) { engine().recentlyDeleted() }
                val page = views.sections
                page.removeAllViews()
                if (deleted.isEmpty()) kit.empty(page, R.drawable.ic_trash_2, "Nothing deleted recently.")
                for (d in deleted) {
                    // The last day counts as one: a 0 would read as gone.
                    val days = (RETENTION_DAYS - (System.currentTimeMillis() / 1000 - d.deletedAt) / 86400).coerceAtLeast(1)
                    val by = d.deletedBy?.let { " on $it" } ?: ""
                    // When and where it was deleted; the days it has left as
                    // a clock and a number, and Restore as its mark (Files
                    // draft 1). Every one is a Private Vault file on a phone
                    // out of the shared area (decision 0073), so that goes
                    // unsaid here and is said when one is opened.
                    val trail = LinearLayout(app).apply {
                        orientation = LinearLayout.HORIZONTAL
                        gravity = Gravity.CENTER_VERTICAL
                        addView(kit.pill(R.drawable.ic_clock, "$days").apply {
                            contentDescription = "$days day${if (days == 1L) "" else "s"} left"
                        })
                        addView(kit.iconButton(R.drawable.ic_rotate_ccw, "Restore") { restore(d) })
                    }
                    kit.row(
                        page, States.icon(d.path), d.path.substringAfterLast('/'),
                        "${Words.ago(d.deletedAt)}$by".replaceFirstChar(Char::uppercase),
                        trail = trail,
                    ) { choose(d) }
                }
            } catch (e: Exception) {
                app.fail("Could not read Recently deleted", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun choose(d: DeletedFile) {
        kit.sheet()
            // No size for one whose bytes are kept on a computer, not here.
            .header(States.icon(d.path), d.path.substringAfterLast('/'), if (d.size > 0uL) Words.size(d.size) else "")
            .text((if (d.private) "Restoring puts it back in your Private Vault."
                else "Restoring puts it back in Qurb, and it returns on your other devices at their next sync.") +
                (d.why?.let { "\n\n${it.replaceFirstChar(Char::uppercase)}." } ?: ""))
            .action(R.drawable.ic_rotate_ccw, "Restore") { restore(d) }
            .action(R.drawable.ic_trash_2, "Delete for good", danger = true) { forget(d) }
            .show()
    }

    private fun restore(d: DeletedFile) {
        scope.launch {
            try {
                val at = withContext(Dispatchers.IO) { engine().restoreDeleted(d.id) }
                app.say(when {
                    at != d.path -> "Restored as ${at.substringAfterLast('/')}"
                    d.private -> "Restored to your Private Vault"
                    else -> "Restored, on all your devices"
                })
                SyncWorker.runNow(app)
            } catch (e: Exception) {
                app.fail("Could not restore it", e)
            } finally {
                app.changed()
            }
        }
    }

    private fun forget(d: DeletedFile) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { engine().forgetDeleted(d.id) }
                app.say("Deleted for good")
            } catch (e: Exception) {
                app.fail("Could not delete it", e)
            } finally {
                app.changed()
            }
        }
    }

    private companion object {
        const val RETENTION_DAYS = 30L
    }
}
