package com.qurb

import android.animation.ValueAnimator
import android.content.res.ColorStateList
import android.os.Build
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.annotation.DrawableRes
import androidx.annotation.StyleRes
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.setPadding
import androidx.core.widget.NestedScrollView
import com.google.android.material.bottomsheet.BottomSheetBehavior
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.button.MaterialButton
import com.qurb.databinding.RowItemBinding
import com.qurb.databinding.RowSettingBinding
import uniffi.qurb_mobile.Available
import uniffi.qurb_mobile.FileEntry
import uniffi.qurb_mobile.PeerInfo

/**
 * The components every place is built from (docs/design/direction.md §48):
 * rows, states, groups, attention, empty states, sheets. One definition each,
 * so a pattern looks and behaves the same wherever it appears -- the Android
 * half of what `crates/desktop/ui/core.js` is to the window.
 */
class Kit(private val app: MainActivity) {

    fun dp(value: Int): Int = (value * app.resources.displayMetrics.density).toInt()

    fun color(id: Int): Int = ContextCompat.getColor(app, id)

    // ------------------------------------------------------------------ rows

    /**
     * A row: an icon in its tile, a name, one line under it -- a state as an
     * icon and words where there is one -- and something at the end.
     */
    fun row(
        parent: ViewGroup,
        @DrawableRes icon: Int,
        name: CharSequence,
        meta: CharSequence = "",
        state: State? = null,
        trail: View? = null,
        iconTint: Int = R.color.text_2,
        onTap: (() -> Unit)? = null,
    ): RowItemBinding {
        val row = RowItemBinding.inflate(app.layoutInflater, parent, false)
        bindRow(row, icon, name, meta, state, trail, iconTint, onTap)
        parent.addView(row.root)
        return row
    }

    fun bindRow(
        row: RowItemBinding,
        @DrawableRes icon: Int,
        name: CharSequence,
        meta: CharSequence = "",
        state: State? = null,
        trail: View? = null,
        iconTint: Int = R.color.text_2,
        onTap: (() -> Unit)? = null,
    ) {
        row.icon.setImageResource(icon)
        row.icon.imageTintList = ColorStateList.valueOf(color(iconTint))
        row.name.text = name
        val line = listOfNotNull(state?.words, meta.toString().ifEmpty { null }).joinToString("  ·  ")
        row.meta.text = line
        row.sub.visibility = if (line.isEmpty()) View.GONE else View.VISIBLE
        if (state != null) {
            row.stateIcon.visibility = View.VISIBLE
            row.stateIcon.setImageResource(state.icon)
            row.stateIcon.imageTintList = ColorStateList.valueOf(color(state.color))
        } else {
            row.stateIcon.visibility = View.GONE
        }
        row.trail.removeAllViews()
        if (trail != null) {
            (trail.parent as? ViewGroup)?.removeView(trail)
            row.trail.addView(trail)
        }
        row.root.isClickable = onTap != null
        if (onTap != null) row.root.setOnClickListener { onTap() } else row.root.setOnClickListener(null)
    }

    /** A chevron at the end of a row that leads somewhere. */
    fun chevron(): View = ImageView(app).apply {
        setImageResource(R.drawable.ic_chevron_right)
        imageTintList = ColorStateList.valueOf(color(R.color.text_3))
        layoutParams = FrameLayout.LayoutParams(dp(18), dp(18))
    }

    fun iconButton(@DrawableRes icon: Int, description: String, onTap: () -> Unit): View =
        ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(R.color.text_2))
            contentDescription = description
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            setPadding(dp(10))
            layoutParams = FrameLayout.LayoutParams(dp(44), dp(44))
            setOnClickListener { onTap() }
        }

    /** A button, sized for a row or a sheet. */
    fun button(
        label: String,
        style: Style = Style.PRIMARY,
        @DrawableRes icon: Int? = null,
        small: Boolean = false,
        onTap: () -> Unit,
    ): MaterialButton {
        val button = MaterialButton(app, null, style.attr)
        button.text = label
        if (icon != null) {
            button.setIconResource(icon)
            button.iconSize = dp(18)
        }
        style.apply(this, button)
        if (small) {
            button.minHeight = dp(36)
            button.minimumHeight = dp(36)
            button.setPadding(dp(14), 0, dp(14), 0)
            button.textSize = 13.5f
        }
        button.setOnClickListener { onTap() }
        return button
    }

    enum class Style(val attr: Int) {
        PRIMARY(com.google.android.material.R.attr.materialButtonStyle),
        SECONDARY(com.google.android.material.R.attr.materialButtonStyle),
        TEXT(com.google.android.material.R.attr.borderlessButtonStyle),
        DANGER(com.google.android.material.R.attr.materialButtonStyle);

        fun apply(kit: Kit, b: MaterialButton) {
            when (this) {
                PRIMARY -> Unit
                SECONDARY -> {
                    b.backgroundTintList = ColorStateList.valueOf(kit.color(R.color.glass_frosted))
                    b.setTextColor(kit.color(R.color.text))
                    b.iconTint = ColorStateList.valueOf(kit.color(R.color.text_2))
                    b.strokeColor = ColorStateList.valueOf(kit.color(R.color.hairline))
                    b.strokeWidth = kit.dp(1)
                }
                TEXT -> {
                    b.backgroundTintList = ColorStateList.valueOf(android.graphics.Color.TRANSPARENT)
                    b.setTextColor(kit.color(R.color.green))
                    b.iconTint = ColorStateList.valueOf(kit.color(R.color.green))
                    b.elevation = 0f
                    b.stateListAnimator = null
                }
                DANGER -> b.backgroundTintList = ColorStateList.valueOf(kit.color(R.color.error))
            }
        }
    }

    // ---------------------------------------------------------------- groups

    /** A title over a group, quieter than a page title (§23). */
    fun groupTitle(parent: ViewGroup, text: String) {
        parent.addView(TextView(app).apply {
            this.text = text
            setTextAppearance(R.style.Text_Label)
            setPadding(0, dp(24), 0, dp(8))
        })
    }

    /** A section heading with an optional action at its end. */
    fun section(parent: ViewGroup, text: String, action: String? = null, onAction: (() -> Unit)? = null) {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dp(24), 0, dp(4))
        }
        row.addView(TextView(app).apply {
            this.text = text
            setTextAppearance(R.style.Text_Section)
            // Through the compat call: Android 9 added the attribute, and
            // setting it on 8 stopped the app until 2026-10-18.
            ViewCompat.setAccessibilityHeading(this, true)
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        // A text button's words end at the gutter, as Home's *See all* does.
        if (action != null && onAction != null) row.addView(
            button(action, Style.TEXT, onTap = onAction).apply { minWidth = 0; minimumWidth = 0 },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { marginEnd = -dp(12) },
        )
        parent.addView(row)
    }

    /** A frosted group that items go into, divided by hairlines. */
    fun group(parent: ViewGroup): LinearLayout {
        val group = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.glass_group)
            clipToOutline = true
            dividerDrawable = ContextCompat.getDrawable(app, R.drawable.divider)
            showDividers = LinearLayout.SHOW_DIVIDER_MIDDLE
        }
        parent.addView(group)
        return group
    }

    /** One item in a group: a label, a line about it, and a way to change it. */
    fun item(
        group: ViewGroup,
        label: String,
        value: String = "",
        chevron: Boolean? = null,
        onTap: (() -> Unit)? = null,
    ): RowSettingBinding {
        val row = RowSettingBinding.inflate(app.layoutInflater, group, false)
        row.label.text = label
        row.value.text = value
        row.value.visibility = if (value.isEmpty()) View.GONE else View.VISIBLE
        row.chevron.visibility = if (chevron ?: (onTap != null)) View.VISIBLE else View.GONE
        if (onTap != null) row.root.setOnClickListener { onTap() } else row.root.isClickable = false
        group.addView(row.root)
        return row
    }

    /** An item with a switch, applied as soon as it is flipped. */
    fun toggle(
        group: ViewGroup,
        label: String,
        value: String,
        on: Boolean,
        onChange: (Boolean) -> Unit,
    ): RowSettingBinding {
        val row = item(group, label, value, chevron = false)
        row.toggle.visibility = View.VISIBLE
        row.toggle.isChecked = on
        row.root.setOnClickListener { row.toggle.toggle() }
        row.toggle.setOnCheckedChangeListener { _, checked -> onChange(checked) }
        return row
    }

    // --------------------------------------------------- the redesign's parts
    // docs/design/redesign.md. Information as icons, dots, badges and bars
    // rather than words (the owner, 2026-10-23: "i want to convey the
    // information while keeping minimum texts").

    /**
     * A device as a chip: its icon, its name, and a dot, green when it
     * answered the last pass and grey when it did not. With [onTap], a small
     * arrow too, for choosing another (the redesign's Device selector).
     */
    fun deviceChip(peer: PeerInfo, off: Boolean, onTap: (() -> Unit)? = null): LinearLayout {
        val chip = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.chip)
            minimumHeight = dp(38)
            setPadding(dp(14), 0, dp(if (onTap != null) 12 else 14), 0)
            contentDescription = "${peer.name}, ${if (off) "off" else "on"}" +
                if (onTap != null) ". Choose another device" else ""
        }
        chip.addView(ImageView(app).apply {
            setImageResource(States.device(peer.name, peer.kind))
            imageTintList = ColorStateList.valueOf(color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(18), dp(18)))
        chip.addView(TextView(app).apply {
            text = peer.name
            setTextAppearance(R.style.Text_Name)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = dp(8) })
        chip.addView(dot(off), LinearLayout.LayoutParams(dp(8), dp(8)).apply { marginStart = dp(8) })
        if (onTap != null) {
            chip.addView(ImageView(app).apply {
                setImageResource(R.drawable.ic_chevron_down)
                imageTintList = ColorStateList.valueOf(color(R.color.text_2))
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }, LinearLayout.LayoutParams(dp(18), dp(18)).apply { marginStart = dp(8) })
            chip.foreground = ContextCompat.getDrawable(app, R.drawable.row_bg)
            chip.setOnClickListener { onTap() }
        }
        return chip
    }

    /** Whether a device answered the last pass: green if it did, grey if not. */
    fun dot(off: Boolean): View = View(app).apply {
        background = ContextCompat.getDrawable(app, R.drawable.dot)
        backgroundTintList = ColorStateList.valueOf(color(if (off) R.color.text_3 else R.color.healthy))
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    }

    /** A count as a small pill (the redesign's Count badge). */
    fun badge(count: Int, tone: Tone? = null): TextView = TextView(app).apply {
        text = if (count > 99) "99+" else count.toString()
        setTextAppearance(R.style.Text_Label)
        isAllCaps = false
        letterSpacing = 0f
        gravity = Gravity.CENTER
        minWidth = dp(24)
        minHeight = dp(22)
        setPadding(dp(7), 0, dp(7), 0)
        background = ContextCompat.getDrawable(app, R.drawable.badge)
        val (fill, words) = when (tone) {
            Tone.ATTENTION -> R.color.attention to R.color.surface
            Tone.ERROR -> R.color.error to R.color.surface
            else -> R.color.green_soft to R.color.green
        }
        backgroundTintList = ColorStateList.valueOf(color(fill))
        setTextColor(color(words))
    }

    /**
     * A device to choose, as a card (the redesign's Device card): its icon
     * in a green tile, its name, its dot and how long since it answered, and
     * a tick on the one chosen.
     */
    fun deviceCard(parent: ViewGroup, peer: PeerInfo, off: Boolean, chosen: Boolean, onTap: () -> Unit): View {
        val card = cardRow(if (chosen) R.drawable.card_chosen else R.drawable.card, States.device(peer.name, peer.kind))
        val words = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        words.addView(TextView(app).apply {
            text = peer.name
            setTextAppearance(R.style.Text_Section)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 0f))
        words.addView(dot(off), LinearLayout.LayoutParams(dp(8), dp(8)).apply { marginStart = dp(8) })
        peer.lastSeen?.let { seen ->
            words.addView(TextView(app).apply {
                text = Words.since(seen)
                setTextAppearance(R.style.Text_Meta)
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { marginStart = dp(8) })
        }
        card.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        if (chosen) card.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_circle_check)
            imageTintList = ColorStateList.valueOf(color(R.color.green))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(20), dp(20)))
        card.contentDescription = "${peer.name}, ${if (off) "off" else "on"}${if (chosen) ", chosen" else ""}"
        card.setOnClickListener { onTap() }
        parent.addView(card, cardParams(parent))
        return card
    }

    /** The card that adds one more, the same size as the cards above it, its
     *  edge dashed (the owner, 2026-10-24: the old one was "asymmetric"). */
    fun addCard(parent: ViewGroup, label: String, onTap: () -> Unit): View {
        val card = cardRow(R.drawable.card_dashed, R.drawable.ic_plus)
        card.addView(TextView(app).apply {
            text = label
            setTextAppearance(R.style.Text_Section)
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        card.setOnClickListener { onTap() }
        parent.addView(card, cardParams(parent))
        return card
    }

    private fun cardRow(@DrawableRes background: Int, @DrawableRes icon: Int): LinearLayout {
        val card = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            this.background = ContextCompat.getDrawable(app, background)
            foreground = ContextCompat.getDrawable(app, R.drawable.row_bg)
            minimumHeight = dp(76)
            setPadding(dp(16), dp(12), dp(16), dp(12))
        }
        val tile = FrameLayout(app).apply { this.background = ContextCompat.getDrawable(app, R.drawable.tile_green) }
        tile.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(R.color.green))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(dp(28), dp(28), Gravity.CENTER))
        card.addView(tile, LinearLayout.LayoutParams(dp(52), dp(52)).apply { marginEnd = dp(14) })
        return card
    }

    private fun cardParams(parent: ViewGroup) = LinearLayout.LayoutParams(
        ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
    ).apply { if (parent.childCount > 0) topMargin = dp(8) }

    /**
     * Something on its way, in a queue (the redesign's Queue row): its icon,
     * its name, how much of it, and, where how far it has got is known, a
     * bar under the name.
     */
    fun queueRow(parent: ViewGroup, @DrawableRes icon: Int, name: String, size: String, done: Float? = null): View {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(64)
            setPadding(dp(12), 0, dp(14), 0)
        }
        val tile = FrameLayout(app).apply { background = ContextCompat.getDrawable(app, R.drawable.tile_quiet) }
        tile.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(dp(20), dp(20), Gravity.CENTER))
        row.addView(tile, LinearLayout.LayoutParams(dp(40), dp(40)).apply { marginEnd = dp(12) })
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = name
            setTextAppearance(R.style.Text_Name)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.MIDDLE
        })
        if (done != null) words.addView(
            android.widget.ProgressBar(app, null, android.R.attr.progressBarStyleHorizontal).apply {
                progressDrawable = ContextCompat.getDrawable(app, R.drawable.progress_bar)
                max = 1000
                progress = (done.coerceIn(0f, 1f) * 1000).toInt()
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(4)).apply { topMargin = dp(6) },
        )
        row.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(TextView(app).apply {
            text = size
            setTextAppearance(R.style.Text_Meta)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = dp(12) })
        row.contentDescription = "$name, $size" + (done?.let { ", ${(it * 100).toInt()}% there" } ?: "")
        parent.addView(row)
        return row
    }

    // ------------------------------------------------------ attention, empty

    enum class Tone { ATTENTION, ERROR, HEALTHY }

    /** Something that needs a decision, and its one action (§14). */
    fun attention(
        parent: ViewGroup,
        @DrawableRes icon: Int,
        title: String,
        says: String,
        action: String? = null,
        tone: Tone = Tone.ATTENTION,
        onAction: (() -> Unit)? = null,
    ): View {
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.attention)
            if (tone != Tone.ATTENTION) {
                backgroundTintList = ColorStateList.valueOf(
                    color(if (tone == Tone.ERROR) R.color.error_bg else R.color.healthy_bg)
                )
            }
            setPadding(dp(14), dp(12), dp(10), dp(12))
        }
        val tile = FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.tile)
        }
        tile.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(
                when (tone) {
                    Tone.ATTENTION -> R.color.attention
                    Tone.ERROR -> R.color.error
                    Tone.HEALTHY -> R.color.healthy
                }
            ))
        }, FrameLayout.LayoutParams(dp(20), dp(20), Gravity.CENTER))
        box.addView(tile, LinearLayout.LayoutParams(dp(38), dp(38)))
        val text = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(12), 0, dp(8), 0)
        }
        text.addView(TextView(app).apply {
            this.text = title
            setTextAppearance(R.style.Text_Name)
            setTypeface(typeface, android.graphics.Typeface.BOLD)
        })
        if (says.isNotEmpty()) text.addView(TextView(app).apply {
            this.text = says
            setTextAppearance(R.style.Text_Quiet)
            textSize = 13.5f
        })
        box.addView(text, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        if (action != null && onAction != null) {
            box.addView(button(action, Style.SECONDARY, small = true, onTap = onAction))
        }
        parent.addView(box, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = dp(12) })
        return box
    }

    /** An outline icon and one line, with the action to take (brief §1). */
    fun empty(
        parent: ViewGroup,
        @DrawableRes icon: Int,
        words: String,
        action: String? = null,
        onAction: (() -> Unit)? = null,
    ) {
        val box = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(dp(24), dp(32), dp(24), dp(24))
        }
        val orb = FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.mark)
        }
        orb.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(R.color.text_3))
        }, FrameLayout.LayoutParams(dp(26), dp(26), Gravity.CENTER))
        box.addView(orb, LinearLayout.LayoutParams(dp(64), dp(64)))
        box.addView(TextView(app).apply {
            text = words
            gravity = Gravity.CENTER
            setTextAppearance(R.style.Text_Quiet)
            setPadding(0, dp(14), 0, 0)
        })
        if (action != null && onAction != null) {
            box.addView(button(action, Style.PRIMARY, onTap = onAction), LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT
            ).apply { topMargin = dp(18) })
        }
        parent.addView(box, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT
        ))
    }

    fun text(parent: ViewGroup, words: String, @StyleRes style: Int = R.style.Text_Quiet, top: Int = 8): TextView {
        val view = TextView(app).apply {
            text = words
            setTextAppearance(style)
        }
        parent.addView(view, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = dp(top) })
        return view
    }

    // ---------------------------------------------------------------- sheets

    /**
     * A sheet (§36): elevated glass rising from the bottom, the place behind
     * it dimmed -- and blurred, where the phone blurs behind windows.
     */
    fun sheet(): Sheet = Sheet(app, this)

    companion object {
        /** Whether the person asked for less motion (§42): animations off. */
        fun calm(): Boolean = !ValueAnimator.areAnimatorsEnabled()
    }
}

/** Where a file's bytes are, in the direction's words (§11): icon and words. */
data class State(@DrawableRes val icon: Int, val words: String, val color: Int)

object States {
    /** `keptOn` names a computer of another person keeping this phone's
     *  vault, for a vault file that is there and not here (decision 0060). */
    fun of(entry: FileEntry, downloading: Boolean = false, keptOn: String? = null): State = when {
        downloading && entry.available == Available.ELSEWHERE ->
            State(R.drawable.ic_download, "Downloading", R.color.green)
        keptOn != null && entry.private && entry.available == Available.ELSEWHERE ->
            State(R.drawable.ic_cloud, "On $keptOn", R.color.neutral)
        entry.available == Available.HERE -> State(R.drawable.ic_hard_drive, "On this phone", R.color.text_3)
        entry.available == Available.ONLY_HERE -> State(R.drawable.ic_triangle_alert, "Only copy here", R.color.attention)
        // Decision 0055: listed, and no device this phone syncs with has it.
        entry.available == Available.NOWHERE -> State(R.drawable.ic_circle_alert, "On no device", R.color.attention)
        else -> State(R.drawable.ic_cloud, "Available elsewhere", R.color.neutral)
    }

    /** Whether a file's bytes are on this phone. */
    fun here(entry: FileEntry): Boolean = entry.available == Available.HERE || entry.available == Available.ONLY_HERE

    private val kinds = mapOf(
        R.drawable.ic_file_image to setOf("jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "avif", "bmp", "svg", "dng"),
        R.drawable.ic_file_video to setOf("mp4", "mov", "mkv", "webm", "avi", "m4v", "3gp"),
        R.drawable.ic_file_audio to setOf("mp3", "m4a", "flac", "wav", "ogg", "opus", "aac"),
        R.drawable.ic_file_archive to setOf("zip", "tar", "gz", "tgz", "xz", "7z", "rar", "zst"),
        R.drawable.ic_file_code to setOf("js", "ts", "rs", "py", "kt", "java", "json", "toml", "yaml", "yml", "html", "css", "sh"),
        R.drawable.ic_file_text to setOf("txt", "md", "pdf", "doc", "docx", "odt", "rtf", "csv", "xls", "xlsx", "ppt", "pptx", "epub"),
    )

    @DrawableRes
    fun icon(path: String): Int {
        val ext = path.substringAfterLast('/').substringAfterLast('.', "").lowercase()
        return kinds.entries.firstOrNull { ext in it.value }?.key ?: R.drawable.ic_file
    }

    fun kind(path: String): String = when (icon(path)) {
        R.drawable.ic_file_image -> "Image"
        R.drawable.ic_file_video -> "Video"
        R.drawable.ic_file_audio -> "Audio"
        R.drawable.ic_file_archive -> "Archive"
        R.drawable.ic_file_code -> "Code"
        R.drawable.ic_file_text -> "Document"
        else -> "File"
    }

    /** A device's icon, from the kind it said it is (decision 0053): when
     *  pairing, or for a device paired before that, at its first sync since.
     *  Until then it is guessed from the name: a guess that costs an icon
     *  when wrong, never a decision. */
    @DrawableRes
    fun device(name: String, kind: String? = null): Int = when (kind) {
        "phone" -> R.drawable.ic_smartphone
        "computer" -> R.drawable.ic_laptop
        "replica" -> R.drawable.ic_hard_drive
        else -> if (Regex("phone|galaxy|pixel|android|iphone|sm-[a-z]\\d|oneplus|xiaomi|redmi", RegexOption.IGNORE_CASE)
                .containsMatchIn(name)) R.drawable.ic_smartphone else R.drawable.ic_laptop
    }
}

class Sheet(private val app: MainActivity, private val kit: Kit) {

    val dialog = BottomSheetDialog(app)
    val body = LinearLayout(app).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(kit.dp(20), kit.dp(10), kit.dp(20), kit.dp(28))
    }

    init {
        body.addView(View(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.handle)
        }, LinearLayout.LayoutParams(kit.dp(36), kit.dp(4)).apply {
            gravity = Gravity.CENTER_HORIZONTAL
            bottomMargin = kit.dp(14)
        })
        dialog.setContentView(NestedScrollView(app).apply { addView(body) })
        dialog.behavior.state = BottomSheetBehavior.STATE_EXPANDED
        dialog.behavior.skipCollapsed = true
    }

    /** What the sheet is about: an icon in its tile, a title, a line. */
    fun header(@DrawableRes icon: Int?, title: String, sub: String = "", iconTint: Int = R.color.text_2): Sheet {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        if (icon != null) {
            val tile = FrameLayout(app).apply { background = ContextCompat.getDrawable(app, R.drawable.tile) }
            tile.addView(ImageView(app).apply {
                setImageResource(icon)
                imageTintList = ColorStateList.valueOf(kit.color(iconTint))
            }, FrameLayout.LayoutParams(kit.dp(24), kit.dp(24), Gravity.CENTER))
            row.addView(tile, LinearLayout.LayoutParams(kit.dp(48), kit.dp(48)).apply { marginEnd = kit.dp(14) })
        }
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = title
            setTextAppearance(R.style.Text_Title)
            ViewCompat.setAccessibilityHeading(this, true)
        })
        if (sub.isNotEmpty()) words.addView(TextView(app).apply {
            text = sub
            setTextAppearance(R.style.Text_Meta)
            setPadding(0, kit.dp(2), 0, 0)
        })
        row.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        body.addView(row)
        return this
    }

    fun text(words: String, @StyleRes style: Int = R.style.Text_Quiet, color: Int? = null): Sheet {
        val view = kit.text(body, words, style, top = 12)
        if (color != null) view.setTextColor(kit.color(color))
        return this
    }

    fun view(view: View, top: Int = 12): Sheet {
        body.addView(view, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = kit.dp(top) })
        return this
    }

    /** One of a list of things to do: closes the sheet, then does it. */
    fun action(
        @DrawableRes icon: Int,
        label: String,
        danger: Boolean = false,
        meta: String = "",
        onTap: () -> Unit,
    ): Sheet {
        if (body.getTag(R.id.sheet_actions) == null) {
            body.setTag(R.id.sheet_actions, true)
            body.addView(View(app), LinearLayout.LayoutParams(1, kit.dp(10)))
        }
        val row = kit.row(body, icon, label, meta, iconTint = if (danger) R.color.error else R.color.text_2) {
            dialog.dismiss()
            onTap()
        }
        if (danger) row.name.setTextColor(kit.color(R.color.error))
        row.tile.background = null
        row.root.minimumHeight = kit.dp(52)
        // One left edge with the header (consistency.md, rule 1): the row's
        // padding reaches into the gutter, and its bare tile is the header's
        // size, so each icon sits under the header's and each label starts
        // where the title does.
        (row.root.layoutParams as LinearLayout.LayoutParams).apply {
            marginStart = -kit.dp(8)
            marginEnd = -kit.dp(8)
        }
        row.tile.layoutParams = row.tile.layoutParams.apply { width = kit.dp(48); height = kit.dp(48) }
        row.icon.layoutParams = row.icon.layoutParams.apply { width = kit.dp(24); height = kit.dp(24) }
        return this
    }

    /** Nothing to decide, only a way out: one quiet button at the foot. */
    fun closing(label: String): Sheet {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END
        }
        row.addView(kit.button(label, Kit.Style.SECONDARY) { dialog.dismiss() })
        return view(row, top = 22)
    }

    /** The decision at the foot: a primary action, and a way out. */
    fun buttons(
        primary: String,
        danger: Boolean = false,
        secondary: String = "Cancel",
        onSecondary: (() -> Unit)? = null,
        onPrimary: () -> Unit,
    ): Sheet {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END
        }
        row.addView(kit.button(secondary, Kit.Style.SECONDARY) {
            dialog.dismiss()
            onSecondary?.invoke()
        })
        row.addView(kit.button(primary, if (danger) Kit.Style.DANGER else Kit.Style.PRIMARY) {
            dialog.dismiss()
            onPrimary()
        }, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT
        ).apply { marginStart = kit.dp(10) })
        return view(row, top = 22)
    }

    fun onDismiss(then: () -> Unit): Sheet {
        dialog.setOnDismissListener { then() }
        return this
    }

    /** Keep the sheet out of screenshots and the recent-apps preview. */
    fun secure(): Sheet {
        dialog.window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        return this
    }

    fun show(): Sheet {
        // Behind a window, the phone can blur (Android 12 and later, where
        // the device allows it); elsewhere the dim alone separates the sheet.
        val window = dialog.window
        if (window != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
            app.windowManager.isCrossWindowBlurEnabled && !Kit.calm()
        ) {
            window.addFlags(WindowManager.LayoutParams.FLAG_BLUR_BEHIND)
            window.attributes = window.attributes.apply { blurBehindRadius = kit.dp(12) }
        }
        dialog.show()
        return this
    }

    fun dismiss() = dialog.dismiss()
}
