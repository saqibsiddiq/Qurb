package com.qurb

import android.content.Context
import android.view.View
import androidx.core.content.ContextCompat
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.snackbar.Snackbar
import uniffi.qurb_mobile.Happening
import uniffi.qurb_mobile.QurbException

/**
 * How the app says things, in one place.
 *
 * The vocabulary is the direction's (docs/design/direction.md §11, §52): *on
 * this phone*, *available elsewhere*, *only copy here*, *free local space*,
 * *send to device* -- never replica, chunk, peer or evict. A screen that
 * improvised its own wording would drift from it, so every screen asks here.
 */
object Words {

    /** Names as a sentence lists them: "A", "A and B", "A, B and C". */
    fun list(names: List<String>): String = when (names.size) {
        0 -> ""
        1 -> names[0]
        else -> names.dropLast(1).joinToString(", ") + " and " + names.last()
    }

    fun size(bytes: ULong): String {
        val units = listOf("B", "KB", "MB", "GB", "TB")
        var value = bytes.toDouble()
        var unit = 0
        while (value >= 1024 && unit < units.size - 1) {
            value /= 1024
            unit++
        }
        return if (unit == 0) "${bytes} B" else "%.1f %s".format(value, units[unit])
    }

    /** Unix seconds as a relative time, the way the desktop says it. */
    fun ago(seconds: Long): String {
        val now = System.currentTimeMillis() / 1000
        val gone = (now - seconds).coerceAtLeast(0)
        return when {
            gone < 90 -> "just now"
            gone < 5400 -> plural(gone / 60, "minute")
            gone < 86400 -> plural(gone / 3600, "hour")
            // Rounded rather than truncated past a day: 46 hours is "2 days".
            else -> plural((gone + 43200) / 86400, "day")
        }
    }

    private fun plural(n: Long, unit: String) = "$n $unit${if (n == 1L) "" else "s"} ago"

    fun files(n: Int) = if (n == 1) "1 file" else "$n files"

    fun devices(n: Int) = if (n == 1) "1 device" else "$n devices"

    /**
     * One entry of history: its icon, its subject -- the file, or the device
     * for a pairing -- and what happened, in words rather than the engine's
     * event names.
     */
    fun happened(h: Happening): Triple<Int, String, String> {
        val subject = h.path?.substringAfterLast('/') ?: h.device ?: ""
        val who = h.device ?: "another device"
        val (icon, what) = when (h.kind) {
            "stored" -> R.drawable.ic_hard_drive to "Saved on this phone"
            "deleted" -> R.drawable.ic_trash_2 to "Deleted"
            "received" -> R.drawable.ic_download to
                if (h.detail?.startsWith("sent to this device") == true) "Sent to you by $who" else "Arrived from $who"
            "sent" -> R.drawable.ic_send to "Ready for $who"
            "collected" -> R.drawable.ic_circle_check to "Collected by $who"
            "evicted" -> R.drawable.ic_cloud_off to "Local space freed"
            "restored" -> R.drawable.ic_rotate_ccw to "Restored"
            "conflicted" -> R.drawable.ic_git_compare to "Changed here and on $who"
            "paired" -> R.drawable.ic_monitor_smartphone to "Added to your devices"
            "removed" -> R.drawable.ic_x to "Removed from this phone"
            "cancelled" -> R.drawable.ic_x to "Send cancelled"
            "failed" -> R.drawable.ic_circle_alert to "Didn't finish"
            else -> R.drawable.ic_info to h.kind
        }
        // The detail only where it adds something: why it failed. A
        // connection's detail is the device's name, which the subject says.
        val extra = h.detail?.takeIf { h.kind == "failed" && it.isNotBlank() }
        return Triple(icon, subject, listOfNotNull(what, ago(h.at), extra).joinToString("  ·  "))
    }

    /** What a finished transfer somebody meant came to, or null for sync. */
    fun finished(h: Happening): String? {
        val who = h.device ?: "another device"
        return when (h.kind) {
            "received" -> if (h.detail?.startsWith("sent to this device") == true) "Received from $who" else null
            "collected" -> "Sent to $who"
            "cancelled" -> "Taken back before $who collected it"
            "failed" -> "Didn't finish"
            else -> null
        }
    }

    /**
     * The 24 words laid out to copy onto paper (§47): numbered, in order,
     * three to a row, each on a tile -- the same as the desktop draws them.
     * The realistic failure is losing one's place halfway down the list.
     */
    fun phraseView(context: Context, phrase: String): View {
        val words = phrase.trim().split(Regex("\\s+"))
        val scale = context.resources.displayMetrics.density
        val grid = android.widget.GridLayout(context).apply {
            columnCount = 3
            useDefaultMargins = false
        }
        words.forEachIndexed { i, word ->
            grid.addView(android.widget.TextView(context).apply {
                text = android.text.SpannableStringBuilder().apply {
                    append("${i + 1}  ", android.text.style.ForegroundColorSpan(
                        ContextCompat.getColor(context, R.color.text_3)
                    ), 0)
                    append(word)
                }
                setTextAppearance(R.style.Text_Name)
                background = ContextCompat.getDrawable(context, R.drawable.tile)
                setPadding((10 * scale).toInt(), (9 * scale).toInt(), (6 * scale).toInt(), (9 * scale).toInt())
                maxLines = 1
            }, android.widget.GridLayout.LayoutParams().apply {
                width = 0
                columnSpec = android.widget.GridLayout.spec(android.widget.GridLayout.UNDEFINED, 1f)
                setMargins((3 * scale).toInt(), (3 * scale).toInt(), (3 * scale).toInt(), (3 * scale).toInt())
            })
        }
        return grid
    }

    /** Say a failure, in words a person can act on. */
    fun fail(context: Context, title: String, e: Throwable) {
        // A coroutine cancelled because its screen went away -- recreated for
        // a theme change, or left -- is not a failure to tell anybody about.
        // It was: "Could not read the settings: Job was cancelled", on choosing
        // a theme (2026-10-08).
        if (e is kotlinx.coroutines.CancellationException) return
        MaterialAlertDialogBuilder(context)
            .setTitle(title)
            // `readable()` rather than `e.message`: UniFFI generates
            // "detail=${detail}", which puts a struct field name in front of
            // the person, and the detail alone rarely says what to try next.
            .setMessage(if (e is QurbException) e.readable() else e.message ?: e.toString())
            .setPositiveButton("OK", null)
            .show()
    }

    /**
     * A passing message. `above` is the view it must not cover -- the tab bar,
     * which a snackbar otherwise sits on top of for its whole three seconds.
     */
    fun say(anchor: View, message: String, above: View? = null) {
        Snackbar.make(anchor, message, Snackbar.LENGTH_LONG).setAnchorView(above).show()
    }
}
