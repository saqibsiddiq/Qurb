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
 * travels, and a Private Vault file in the vault. Reached from Settings →
 * Manage storage, since the owner's review of 2026-11-03.
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
                // Empty, beside the title, in red, as on the desktop (the
                // owner's review of 2026-11-03, K-2); asked first.
                views.action.apply {
                    visibility = if (deleted.isEmpty()) View.GONE else View.VISIBLE
                    text = "Empty"
                    icon = null
                    setTextColor(kit.color(R.color.error))
                    setOnClickListener { empty(deleted.size) }
                }
                if (deleted.isEmpty()) {
                    kit.empty(page, R.drawable.ic_trash_2, "Nothing deleted recently")
                    return@refreshing
                }
                // One card, as every list in Settings is: the file, who
                // deleted it, and Restore. The days it has left only in its
                // last week, when they matter.
                val card = kit.card(page, top = 8)
                for (d in deleted) {
                    // The last day counts as one: a 0 would read as gone.
                    val days = (RETENTION_DAYS - (System.currentTimeMillis() / 1000 - d.deletedAt) / 86400).coerceAtLeast(1)
                    val trail = LinearLayout(app).apply {
                        orientation = LinearLayout.HORIZONTAL
                        gravity = Gravity.CENTER_VERTICAL
                        if (days <= LAST_WEEK) addView(kit.pill(R.drawable.ic_clock, "$days").apply {
                            contentDescription = "$days day${if (days == 1L) "" else "s"} left"
                        })
                        addView(kit.iconButton(R.drawable.ic_rotate_ccw, "Restore") { restore(d) })
                    }
                    val by = d.deletedBy?.replaceFirstChar(Char::uppercase) ?: Words.ago(d.deletedAt).replaceFirstChar(Char::uppercase)
                    kit.row(card, States.icon(d.path), d.path.substringAfterLast('/'), by, trail = trail) { choose(d) }
                }
            } catch (e: Exception) {
                app.fail("Could not read Recently deleted", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun empty(count: Int) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2)
            .title("Empty Recently deleted?", centred = true)
            .line("${Words.files(count)}, gone for good")
            .buttons("Empty", danger = true) {
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) { engine().emptyRecentlyDeleted() }
                        app.say("Emptied")
                    } catch (e: Exception) {
                        app.fail("Not emptied", e)
                    } finally {
                        app.changed()
                    }
                }
            }
            .show()
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
        /** Days left are shown from here down. */
        const val LAST_WEEK = 7L
    }
}
