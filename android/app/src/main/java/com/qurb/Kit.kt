package com.qurb

import android.animation.ValueAnimator
import android.content.res.ColorStateList
import android.os.Build
import android.text.Editable
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.annotation.DrawableRes
import androidx.annotation.StyleRes
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.setPadding
import androidx.core.widget.NestedScrollView
import com.google.android.material.bottomsheet.BottomSheetBehavior
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.button.MaterialButton
import com.qurb.databinding.RowCardBinding
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
class Kit(private val app: AppCompatActivity) {

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
        // A state is its icon, its words read out rather than shown beside
        // the meta (the owner, 2026-10-23: no "·" joins); shown only when
        // there is nothing else to say.
        val line = meta.toString().ifEmpty { state?.words ?: "" }
        row.meta.text = line
        row.meta.contentDescription = if (state != null && meta.isNotEmpty()) "${state.words}, $meta" else null
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
        keepWeight(button, null, style.attr)
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
                    b.backgroundTintList = ColorStateList.valueOf(kit.color(R.color.surface))
                    b.setTextColor(kit.color(R.color.text))
                    b.iconTint = ColorStateList.valueOf(kit.color(R.color.text))
                    b.strokeColor = ColorStateList.valueOf(kit.color(R.color.border))
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
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
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

    /** A search field (the redesign's Search): its icon, then what is
     *  typed, handed to `typed` trimmed as it changes. Returns the field,
     *  and the box to clear. */
    fun searchField(hint: String, typed: (String) -> Unit): Pair<View, EditText> {
        val field = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.search_field)
            setPadding(dp(16), 0, dp(16), 0)
        }
        field.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_search)
            imageTintList = ColorStateList.valueOf(color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(20), dp(20)).apply { marginEnd = dp(10) })
        val box = EditText(app).apply {
            this.hint = hint
            background = null
            setTextAppearance(R.style.Text_Body)
            setHintTextColor(color(R.color.text_3))
            setSingleLine()
            setPadding(0, 0, 0, 0)
            imeOptions = android.view.inputmethod.EditorInfo.IME_ACTION_SEARCH
        }
        box.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(text: Editable?) = typed(text?.toString()?.trim().orEmpty())
        })
        field.addView(box, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        return field to box
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
        val card = deviceCard(parent, States.device(peer.name, peer.kind), peer.name, off,
            peer.lastSeen?.let { Words.since(it) }, end = if (chosen) R.drawable.ic_circle_check else null, onTap = onTap)
        card.contentDescription = "${peer.name}, ${if (off) "off" else "on"}${if (chosen) ", chosen" else ""}"
        if (chosen) card.background = ContextCompat.getDrawable(app, R.drawable.card_chosen)
        return card
    }

    /** Something a device keeps, shown on its card: an icon, and a count beside it if there is one. */
    class Keeps(@DrawableRes val icon: Int, val count: String? = null)

    /**
     * A device as a card, wherever devices are listed: its icon in a green
     * tile, its name, a dot for whether it is on ([off] null for this phone,
     * which always is), a few characters beside it ([since]), what it keeps
     * as small pills under the name, and at the [end] a tick or an arrow.
     */
    fun deviceCard(
        parent: ViewGroup,
        @DrawableRes icon: Int,
        name: String,
        off: Boolean?,
        since: String?,
        keeps: List<Keeps> = emptyList(),
        @DrawableRes end: Int? = null,
        onTap: (() -> Unit)?,
    ): View {
        val card = cardShell(R.drawable.card, icon)
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        val line = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        // The name gives way when the line is too long, and the dot and the
        // time stay beside it: weighted, in a line only as wide as it needs.
        line.addView(TextView(app).apply {
            text = name
            setTextAppearance(R.style.Text_Section)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        if (off != null) line.addView(dot(off), LinearLayout.LayoutParams(dp(8), dp(8)).apply { marginStart = dp(8) })
        if (since != null) line.addView(TextView(app).apply {
            text = since
            setTextAppearance(R.style.Text_Meta)
            setTextColor(color(R.color.text_2))
            maxLines = 1
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = dp(8) })
        words.addView(line, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        if (keeps.isNotEmpty()) {
            val pills = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
            for (k in keeps) {
                pills.addView(pill(k.icon, k.count), LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
                ).apply { if (pills.childCount > 0) marginStart = dp(6) })
            }
            words.addView(pills, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
            ).apply { topMargin = dp(6) })
        }
        card.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        if (end != null) {
            val tick = end == R.drawable.ic_circle_check
            card.addView(ImageView(app).apply {
                setImageResource(end)
                imageTintList = ColorStateList.valueOf(color(if (tick) R.color.green else R.color.text_3))
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }, LinearLayout.LayoutParams(dp(if (tick) 20 else 18), dp(if (tick) 20 else 18)))
        }
        card.minimumHeight = dp(if (keeps.isEmpty()) 76 else 92)
        card.contentDescription = listOfNotNull(name, off?.let { if (it) "off" else "on" }, since).joinToString(", ")
        if (onTap != null) {
            card.setOnClickListener { onTap() }
        } else {
            card.isClickable = false
            card.foreground = null
        }
        parent.addView(card, cardParams(parent))
        return card
    }

    /** A small quiet pill: an icon, and a count or a word beside it. */
    fun pill(@DrawableRes icon: Int, words: String? = null): View {
        val pill = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.tile_small)
            setPadding(dp(7), dp(3), dp(8), dp(3))
            minimumHeight = dp(22)
        }
        pill.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(14), dp(14)))
        if (words != null) pill.addView(TextView(app).apply {
            text = words
            setTextAppearance(R.style.Text_Meta)
            setTextColor(color(R.color.text_2))
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = dp(4) })
        return pill
    }

    /** The card that adds one more, the same size as the cards above it, its
     *  edge dashed (the owner, 2026-10-24: the old one was "asymmetric"). */
    fun addCard(parent: ViewGroup, label: String, onTap: () -> Unit): View {
        val card = cardShell(R.drawable.card_dashed, R.drawable.ic_plus)
        card.addView(TextView(app).apply {
            text = label
            setTextAppearance(R.style.Text_Section)
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        card.setOnClickListener { onTap() }
        parent.addView(card, cardParams(parent))
        return card
    }

    private fun cardShell(@DrawableRes background: Int, @DrawableRes icon: Int): LinearLayout {
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

    /** Rows on the surface, a hairline edge, rounded wide (the redesign's groups). */
    fun card(parent: ViewGroup, top: Int = 0): LinearLayout {
        val card = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.card)
            clipToOutline = true
            setPadding(0, dp(4), 0, dp(4))
        }
        parent.addView(card, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = dp(top) })
        return card
    }

    /** A label over a card, quiet and in capitals (the redesign's section labels). */
    fun label(parent: ViewGroup, text: String, top: Int = 24) {
        parent.addView(TextView(app).apply {
            this.text = text
            setTextAppearance(R.style.Text_Label)
            ViewCompat.setAccessibilityHeading(this, true)
            setPadding(0, dp(top), 0, dp(8))
        })
    }

    /**
     * A row in a card (the redesign's Setting row): an icon in a small tile,
     * a name, a short value under it, and at the end a switch ([on] not
     * null), an arrow ([onTap] and no switch), or nothing.
     */
    fun setting(
        card: ViewGroup,
        @DrawableRes icon: Int,
        name: String,
        value: String = "",
        on: Boolean? = null,
        onChange: ((Boolean) -> Unit)? = null,
        onTap: (() -> Unit)? = null,
    ): RowCardBinding {
        val row = RowCardBinding.inflate(app.layoutInflater, card, false)
        row.icon.setImageResource(icon)
        row.name.text = name
        row.value.text = value
        row.value.visibility = if (value.isEmpty()) View.GONE else View.VISIBLE
        if (on != null) {
            row.toggle.visibility = View.VISIBLE
            row.toggle.isChecked = on
            row.toggle.contentDescription = name
            row.toggle.setOnCheckedChangeListener { _, checked -> onChange?.invoke(checked) }
            row.root.setOnClickListener { row.toggle.toggle() }
        } else if (onTap != null) {
            row.chevron.visibility = View.VISIBLE
            row.root.setOnClickListener { onTap() }
        } else {
            row.root.isClickable = false
            row.root.background = null
        }
        card.addView(row.root)
        return row
    }

    /** What happened to a file, as the small badge on its tile (the redesign's What). */
    enum class What(@DrawableRes val icon: Int, val fill: Int, val tint: Int, val words: String) {
        ARRIVED(R.drawable.ic_arrow_down, R.color.healthy, R.color.surface, "arrived"),
        SENT(R.drawable.ic_arrow_up, R.color.green, R.color.surface, "sent"),
        DELETED(R.drawable.ic_trash_2, R.color.error_bg, R.color.error, "deleted"),
        CHANGED(R.drawable.ic_pencil, R.color.neutral_bg, R.color.neutral, "changed"),
        TWO_VERSIONS(R.drawable.ic_git_compare, R.color.attention, R.color.surface, "two versions"),
        FREED(R.drawable.ic_cloud_off, R.color.neutral_bg, R.color.neutral, "space freed"),
        RESTORED(R.drawable.ic_rotate_ccw, R.color.neutral_bg, R.color.neutral, "restored"),
        PAIRED(R.drawable.ic_plus, R.color.green, R.color.surface, "paired"),
        STOPPED(R.drawable.ic_x, R.color.neutral_bg, R.color.neutral, "stopped"),
        FAILED(R.drawable.ic_circle_alert, R.color.error, R.color.surface, "failed"),
    }

    /**
     * One thing that happened (the redesign's Activity row): the file's icon
     * in a tile with a badge for what happened, its name, the device involved
     * by its icon and name, and the time at the end.
     */
    fun activityRow(
        parent: ViewGroup,
        @DrawableRes icon: Int,
        what: What,
        name: String,
        @DrawableRes deviceIcon: Int?,
        device: String,
        time: String,
    ): View {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(60)
            setPadding(dp(4), 0, dp(4), 0)
            contentDescription = "$name, ${what.words}, $device, $time"
        }
        val corner = FrameLayout(app).apply { clipChildren = false }
        val tile = FrameLayout(app).apply { background = ContextCompat.getDrawable(app, R.drawable.card_small) }
        tile.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(dp(20), dp(20), Gravity.CENTER))
        corner.addView(tile, FrameLayout.LayoutParams(dp(40), dp(40), Gravity.TOP or Gravity.START))
        val badge = FrameLayout(app).apply {
            background = android.graphics.drawable.GradientDrawable().apply {
                shape = android.graphics.drawable.GradientDrawable.OVAL
                setColor(color(what.fill))
                setStroke(dp(2), color(R.color.bg))
            }
        }
        badge.addView(ImageView(app).apply {
            setImageResource(what.icon)
            imageTintList = ColorStateList.valueOf(color(what.tint))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(dp(11), dp(11), Gravity.CENTER))
        corner.addView(badge, FrameLayout.LayoutParams(dp(20), dp(20), Gravity.BOTTOM or Gravity.END))
        row.addView(corner, LinearLayout.LayoutParams(dp(46), dp(46)).apply { marginEnd = dp(10) })

        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = name
            setTextAppearance(R.style.Text_Name)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.MIDDLE
        })
        val who = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        if (deviceIcon != null) who.addView(ImageView(app).apply {
            setImageResource(deviceIcon)
            imageTintList = ColorStateList.valueOf(color(R.color.text_3))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(13), dp(13)).apply { marginEnd = dp(4) })
        who.addView(TextView(app).apply {
            text = device
            setTextAppearance(R.style.Text_Meta)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        })
        words.addView(who, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = dp(2) })
        row.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(TextView(app).apply {
            text = time
            setTextAppearance(R.style.Text_Meta)
            setTextColor(color(R.color.text_2))
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = dp(10) })
        parent.addView(row)
        return row
    }

    /** A backup's state, said with an icon and a few words, and a bar while it runs. */
    class Progress(@DrawableRes val icon: Int, val words: String, val done: Float? = null)

    /**
     * One backup as a card (the redesign's Backups): its icon in a green
     * tile, its name, its state under it with a bar while it runs, and at the
     * end a switch ([on] not null) or an arrow ([onTap]).
     */
    fun backupCard(
        parent: ViewGroup,
        @DrawableRes icon: Int,
        name: String,
        state: Progress,
        on: Boolean? = null,
        onChange: ((Boolean) -> Unit)? = null,
        onTap: (() -> Unit)? = null,
    ): View {
        val card = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.card)
            foreground = ContextCompat.getDrawable(app, R.drawable.row_bg)
            minimumHeight = dp(78)
            setPadding(dp(16), dp(14), dp(14), dp(14))
        }
        val tile = FrameLayout(app).apply { background = ContextCompat.getDrawable(app, R.drawable.tile_soft) }
        tile.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(color(R.color.green))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(dp(22), dp(22), Gravity.CENTER))
        card.addView(tile, LinearLayout.LayoutParams(dp(44), dp(44)).apply { marginEnd = dp(14) })
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = name
            setTextAppearance(R.style.Text_Name)
        })
        val status = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        status.addView(ImageView(app).apply {
            setImageResource(state.icon)
            imageTintList = ColorStateList.valueOf(color(
                if (state.icon == R.drawable.ic_circle_check) R.color.healthy else R.color.text_2))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(14), dp(14)).apply { marginEnd = dp(6) })
        status.addView(TextView(app).apply {
            text = state.words
            setTextAppearance(R.style.Text_Meta)
            setTextColor(color(R.color.text_2))
        })
        words.addView(status, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = dp(6) })
        if (state.done != null) words.addView(
            android.widget.ProgressBar(app, null, android.R.attr.progressBarStyleHorizontal).apply {
                progressDrawable = ContextCompat.getDrawable(app, R.drawable.progress_bar)
                max = 1000
                progress = (state.done.coerceIn(0f, 1f) * 1000).toInt()
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(6)).apply { topMargin = dp(6) },
        )
        card.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        card.contentDescription = "$name, ${state.words}"
        if (on != null) {
            val toggle = com.google.android.material.materialswitch.MaterialSwitch(app).apply {
                isChecked = on
                thumbTintList = ContextCompat.getColorStateList(app, R.color.switch_thumb)
                trackTintList = ContextCompat.getColorStateList(app, R.color.switch_track)
                contentDescription = name
                setOnCheckedChangeListener { _, checked -> onChange?.invoke(checked) }
            }
            card.addView(toggle, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
            ).apply { marginStart = dp(8) })
            card.setOnClickListener { toggle.toggle() }
        } else if (onTap != null) {
            card.addView(ImageView(app).apply {
                setImageResource(R.drawable.ic_chevron_right)
                imageTintList = ColorStateList.valueOf(color(R.color.text_3))
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }, LinearLayout.LayoutParams(dp(18), dp(18)).apply { marginStart = dp(8) })
            card.setOnClickListener { onTap() }
        } else {
            card.foreground = null
        }
        parent.addView(card, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { if (parent.childCount > 0) topMargin = dp(12) })
        return card
    }

    /** A note to keep in mind, as a chip the width of the page: the one sentence a page keeps. */
    fun note(parent: ViewGroup, words: String, top: Int = 14): View {
        val chip = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.warning)
            setPadding(dp(14), dp(12), dp(14), dp(12))
        }
        chip.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_info)
            imageTintList = ColorStateList.valueOf(color(R.color.attention))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(18), dp(18)).apply { marginEnd = dp(10) })
        chip.addView(TextView(app).apply {
            text = words
            setTextAppearance(R.style.Text_Quiet)
            setTextColor(color(R.color.text))
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        parent.addView(chip, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = dp(top) })
        return chip
    }

    /**
     * The devices to choose from, as cards, the chosen one ticked, and one
     * more that adds a device (Home draft 3; the owner, 2026-10-24: "choose a
     * device in home page"). From Home, and from Backups, which shows the same
     * device. Every device goes on syncing whichever is chosen.
     */
    fun chooseDevice(peers: List<PeerInfo>, chosen: PeerInfo, onChosen: () -> Unit) {
        val sheet = sheet()
        val cards = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        for (peer in peers) {
            deviceCard(cards, peer, Engine.isOff(app, peer), peer.fingerprint == chosen.fingerprint) {
                sheet.dismiss()
                Engine.choose(app, peer.fingerprint)
                onChosen()
            }
        }
        addCard(cards, "Add a device") {
            sheet.dismiss()
            (app as? MainActivity)?.pair()
        }
        sheet.view(cards, top = 0).show()
    }

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

    /**
     * A name asked for in a sheet: the field, and Cancel and `verb`. The
     * name without its extension is chosen once the field has focus -- set
     * before, the focus put the cursor at the end, and what was typed went
     * after `.png` (on the emulator) -- and the keyboard is up for it.
     * `then` hears the name, trimmed, when it is something new.
     */
    fun askName(title: String, start: String, verb: String, then: (String) -> Unit) {
        val input = android.widget.EditText(app).apply {
            setText(start)
            setSingleLine()
            setTextAppearance(R.style.Text_Body)
            background = ContextCompat.getDrawable(app, R.drawable.search_field)
            setPadding(dp(16), 0, dp(16), 0)
            minHeight = dp(48)
        }
        sheet().title(title).view(input, top = 14).buttons(verb) {
            val name = input.text.toString().trim()
            if (name.isNotEmpty() && name != start) then(name)
        }.show()
        input.requestFocus()
        input.post {
            input.setSelection(0, start.substringBeforeLast('.', start).length)
            app.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                ?.showSoftInput(input, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
        }
    }

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

class Sheet(private val app: AppCompatActivity, private val kit: Kit) {

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

    /**
     * The decision at the foot: a way out and a primary action, side by side
     * and of one width (the redesign: "two equal buttons").
     */
    fun buttons(
        primary: String,
        danger: Boolean = false,
        secondary: String = "Cancel",
        onSecondary: (() -> Unit)? = null,
        onPrimary: () -> Unit,
    ): Sheet {
        val row = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
        row.addView(kit.button(secondary, Kit.Style.SECONDARY) {
            dialog.dismiss()
            onSecondary?.invoke()
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(kit.button(primary, if (danger) Kit.Style.DANGER else Kit.Style.PRIMARY) {
            dialog.dismiss()
            onPrimary()
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(12) })
        return view(row, top = 22)
    }

    /**
     * One way on, the width of the sheet, at its foot: Done after a share, or
     * Hide while one goes (Sharing into Qurb draft 1). Returned, for a caller
     * that enables it later; it does not close the sheet by itself.
     */
    fun wide(label: String, style: Kit.Style = Kit.Style.PRIMARY, onTap: () -> Unit): MaterialButton {
        val button = kit.button(label, style) { onTap() }
        view(button, top = 24)
        return button
    }

    /** A title with nothing beside it, at the start or in the middle. */
    fun title(words: String, centred: Boolean = false): Sheet {
        body.addView(TextView(app).apply {
            text = words
            setTextAppearance(R.style.Text_Title)
            gravity = if (centred) Gravity.CENTER else Gravity.START
            ViewCompat.setAccessibilityHeading(this, true)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        return this
    }

    /** One line under a centred title. */
    fun line(words: String): Sheet {
        body.addView(TextView(app).apply {
            text = words
            setTextAppearance(R.style.Text_Quiet)
            gravity = Gravity.CENTER
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(4) })
        return this
    }

    /**
     * A mark in the middle, over a centred title: what a grave choice is
     * about, round; or what a sheet is about, in a rounded tile ([round] false).
     */
    fun mark(@DrawableRes icon: Int, fill: Int = R.color.error_bg, tint: Int = R.color.error, round: Boolean = true): Sheet {
        val disc = FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, if (round) R.drawable.round else R.drawable.tile_mark)
            backgroundTintList = ColorStateList.valueOf(kit.color(fill))
        }
        disc.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(kit.color(tint))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(kit.dp(30), kit.dp(30), Gravity.CENTER))
        body.addView(disc, LinearLayout.LayoutParams(kit.dp(64), kit.dp(64)).apply {
            gravity = Gravity.CENTER_HORIZONTAL
            topMargin = kit.dp(10)
            bottomMargin = kit.dp(14)
        })
        return this
    }

    /**
     * The one warning a choice carries, as a chip the width of the sheet.
     * [compact]: a size smaller and its words in the middle, under a number
     * to compare (Pairing draft 1).
     */
    fun warning(words: String, @DrawableRes icon: Int = R.drawable.ic_triangle_alert, compact: Boolean = false): Sheet {
        val chip = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = if (compact) Gravity.CENTER else Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.warning)
            if (compact) setPadding(kit.dp(14), kit.dp(12), kit.dp(14), kit.dp(12))
            else setPadding(kit.dp(16), kit.dp(14), kit.dp(16), kit.dp(14))
        }
        val side = kit.dp(if (compact) 18 else 20)
        chip.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.attention))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(side, side).apply { marginEnd = kit.dp(if (compact) 8 else 10) })
        chip.addView(TextView(app).apply {
            text = words
            setTextAppearance(R.style.Text_Name)
            // A long name wraps it: its lines stay in the middle too.
            if (compact) gravity = Gravity.CENTER_HORIZONTAL
        }, if (compact) LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        else LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        return view(chip, top = if (compact) 12 else 18)
    }

    /**
     * One of a few ways to go on, as a row (the redesign's Option row): its
     * icon in a soft green tile, its name, an arrow. Closes the sheet, then
     * does it.
     */
    fun option(@DrawableRes icon: Int, label: String, onTap: () -> Unit): Sheet {
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            minimumHeight = kit.dp(60)
            setPadding(kit.dp(8), 0, kit.dp(8), 0)
            setOnClickListener {
                dialog.dismiss()
                onTap()
            }
        }
        val tile = FrameLayout(app).apply { background = ContextCompat.getDrawable(app, R.drawable.tile_soft) }
        tile.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.green))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(kit.dp(24), kit.dp(24), Gravity.CENTER))
        row.addView(tile, LinearLayout.LayoutParams(kit.dp(40), kit.dp(40)).apply { marginEnd = kit.dp(14) })
        row.addView(TextView(app).apply {
            text = label
            setTextAppearance(R.style.Text_Name)
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_chevron_right)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.text_3))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)))
        val first = body.getTag(R.id.sheet_actions) == null
        body.setTag(R.id.sheet_actions, true)
        body.addView(row, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply {
                topMargin = kit.dp(if (first) 12 else 4)
                marginStart = -kit.dp(8)
                marginEnd = -kit.dp(8)
            })
        return this
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
