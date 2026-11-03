package com.qurb

import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.BackupWaiting
import uniffi.qurb_mobile.Outstanding
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.Waiting

/**
 * Waiting, as the owner's review of 2026-11-03 has it (H-1, the mockup
 * docs/design/mockups/devices-waiting-2026-11-03.html): one row for each
 * device files are going to or coming from, an arrow for which way, never
 * one row per file. The files themselves show, a stack of their previews,
 * where the rows had icons; a number, a size, a bar while it moves, and a
 * chip only when it waits on something. Backups and the files only this
 * phone has go to the same computer and are one row: "only here" is not a
 * category of its own.
 */
object Waits {

    /** What a preview is read from. */
    sealed class Preview {
        /** A file in this phone's folder. */
        class Here(val path: String, val modifiedAt: Long) : Preview()
        /** A file of the phone's own folders, which a backup is taking. */
        class OnPhone(val file: java.io.File) : Preview()
        /** One whose bytes are not here: its kind. */
        class Kind(val name: String) : Preview()
    }

    /** One device's row. */
    class Group(
        /** Coming here, rather than going. */
        val coming: Boolean,
        val peer: PeerInfo?,
        val name: String,
        val previews: List<Preview>,
        val count: Long,
        val bytes: ULong,
        /** How far it has got, while that is known. */
        val fraction: Float?,
        /** What it waits on, when it waits: the device off. */
        val waitingOn: String?,
        /** "41 of 90", where a count says more than a size. */
        val progress: String? = null,
        /** The sends in it, each of which may be stopped. */
        val sends: List<Waiting> = emptyList(),
    )

    /** The rows, from what Home reads. */
    fun groups(
        app: MainActivity,
        peers: List<PeerInfo>,
        holders: List<PeerInfo>,
        outstanding: Outstanding,
        sends: List<Waiting>,
        backup: BackupWaiting,
        round: BackingUp.Round?,
        /** Previews read for each: a row's stack, or the page's grid. */
        shown: Int = PREVIEWS,
    ): List<Group> {
        val out = mutableListOf<Group>()
        // To the computer keeping this phone's files: the ones only here,
        // and the backups.
        val keeper = holders.firstOrNull()
        val backupFiles = if (keeper == null) 0L else backup.filesLeft.toLong()
        val toKeeper = outstanding.files.size + backupFiles
        if (toKeeper > 0) {
            val previews = outstanding.files.take(shown).map { Preview.Here(it.path, it.modifiedAt) } +
                (if (keeper == null) emptyList() else backup.files.take(shown).map { Preview.OnPhone(onPhone(it.folder, it.name)) })
            out += Group(
                coming = false,
                peer = keeper,
                name = keeper?.name ?: "Your devices",
                previews = previews.take(shown),
                count = toKeeper,
                bytes = outstanding.bytes + (if (keeper == null) 0uL else backup.bytesLeft),
                fraction = round?.takeIf { backupFiles > 0 }?.fraction,
                waitingOn = keeper?.takeIf { Engine.isOff(app, it) }?.let { "Off" },
            )
        }
        // To each device something was sent to, not collected yet.
        for ((fingerprint, its) in sends.groupBy { it.toFingerprint }) {
            val peer = peers.firstOrNull { it.fingerprint == fingerprint }
            out += Group(
                coming = false,
                peer = peer,
                name = its.first().to,
                previews = its.take(shown).map { Preview.Here(it.path, 0) },
                count = its.size.toLong(),
                bytes = its.fold(0uL) { sum, w -> sum + w.size },
                fraction = null,
                waitingOn = peer?.takeIf { Engine.isOff(app, it) }?.let { "Off" },
                sends = its,
            )
        }
        // From the computer a restore is putting back.
        Restore.running?.let { r ->
            out += Group(
                coming = true,
                peer = peers.firstOrNull { it.name == r.from },
                name = r.from,
                previews = listOfNotNull(r.current.ifEmpty { null }?.let { Preview.Kind(it) }),
                count = r.files.toLong(),
                bytes = r.bytes,
                fraction = if (r.bytes > 0uL) (r.bytesDone.toDouble() / r.bytes.toDouble()).toFloat() else 0f,
                waitingOn = null,
                progress = "${r.done} of ${r.files}",
            )
        }
        return out
    }

    /** Where a backed-up file is on this phone: the computer's folder for it
     *  read back as Restore does (`WhatsApp/…` is WhatsApp's own folder). */
    private fun onPhone(folder: String, name: String): java.io.File {
        val rest = if (folder == "WhatsApp" || folder.startsWith("WhatsApp/"))
            listOf(WhatsApp.relative(), folder.removePrefix("WhatsApp").trimStart('/')).filter { it.isNotEmpty() }.joinToString("/")
        else folder
        return java.io.File(Folders.root(), if (rest.isEmpty()) name else "$rest/$name")
    }

    /** A group's row: its stack, which way and to whom, how much, its bar. */
    fun row(app: MainActivity, scope: CoroutineScope, parent: ViewGroup, g: Group, onTap: () -> Unit): View {
        val kit = app.kit
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = kit.dp(68)
            setPadding(kit.dp(12), kit.dp(10), kit.dp(14), kit.dp(10))
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            setOnClickListener { onTap() }
        }
        row.addView(stack(app, scope, g.previews, g.fraction?.takeIf { g.fraction < 1f }),
            LinearLayout.LayoutParams(kit.dp(62), kit.dp(48)).apply { marginEnd = kit.dp(10) })
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(who(app, g))
        val chips = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        chips.addView(chip(app, g.progress ?: Words.count(g.count)))
        if (g.progress == null && g.bytes > 0uL) chips.addView(chip(app, Words.size(g.bytes)))
        g.waitingOn?.let { chips.addView(chip(app, it, waiting = true)) }
        words.addView(chips, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(5) })
        row.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_chevron_right)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.text_3))
        }, LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)))
        row.contentDescription = listOfNotNull(
            "${if (g.coming) "From" else "To"} ${g.name}", Words.files(g.count.toInt()),
            Words.size(g.bytes), g.waitingOn, g.fraction?.let { "${(it * 100).toInt()}% there" },
        ).joinToString(", ")
        parent.addView(row)
        return row
    }

    /** ↑ or ↓, the device's circle, its name. */
    fun who(app: MainActivity, g: Group): View {
        val kit = app.kit
        return LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(ImageView(app).apply {
                setImageResource(if (g.coming) R.drawable.ic_arrow_down else R.drawable.ic_arrow_up)
                imageTintList = ColorStateList.valueOf(kit.color(R.color.text_3))
            }, LinearLayout.LayoutParams(kit.dp(14), kit.dp(14)).apply { marginEnd = kit.dp(5) })
            addView(kit.avatar(g.name, null, g.peer?.let { Devices.picture(it) }, 18),
                LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)).apply { marginEnd = kit.dp(7) })
            addView(TextView(app).apply {
                text = g.name
                setTextAppearance(R.style.Text_Name)
                maxLines = 1
                ellipsize = android.text.TextUtils.TruncateAt.END
            })
        }
    }

    /** A few words in a small rounded box: a count, a size, what it waits on. */
    fun chip(app: MainActivity, words: String, waiting: Boolean = false): View = TextView(app).apply {
        text = words
        setTextAppearance(R.style.Text_Meta)
        if (waiting) setTextColor(app.kit.color(R.color.attention))
        background = GradientDrawable().apply {
            cornerRadius = app.kit.dp(11).toFloat()
            setColor(app.kit.color(if (waiting) R.color.attention_bg else R.color.neutral_bg))
        }
        setPadding(app.kit.dp(8), app.kit.dp(2), app.kit.dp(8), app.kit.dp(2))
        layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginEnd = app.kit.dp(6) }
    }

    /** Up to three previews, fanned, the first on top with a ring over it
     *  while it moves (`done`), as WhatsApp shows a file on its way. */
    fun stack(app: MainActivity, scope: CoroutineScope, previews: List<Preview>, done: Float?): View {
        val kit = app.kit
        val box = FrameLayout(app).apply { clipChildren = false; clipToPadding = false }
        val shown = previews.take(PREVIEWS).ifEmpty { listOf(Preview.Kind("")) }
        for ((i, p) in shown.withIndex().reversed()) {
            val cell = FrameLayout(app).apply {
                background = ContextCompat.getDrawable(app, R.drawable.file_tile)
                clipToOutline = true
                rotation = i * 6f
                elevation = kit.dp(3 - i).toFloat()
            }
            val picture = ImageView(app)
            cell.addView(picture, FrameLayout.LayoutParams(kit.dp(22), kit.dp(22), Gravity.CENTER))
            bind(scope, picture, p)
            if (i == 0 && done != null) cell.addView(Ring(app, done), FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            box.addView(cell, FrameLayout.LayoutParams(kit.dp(44), kit.dp(44), Gravity.CENTER_VERTICAL).apply {
                marginStart = kit.dp(i * 8)
            })
        }
        return box
    }

    fun bind(scope: CoroutineScope, picture: ImageView, p: Preview) {
        when (p) {
            is Preview.Here -> Thumbs.local(scope, picture, p.path, p.modifiedAt)
            is Preview.OnPhone -> Thumbs.file(scope, picture, p.file)
            is Preview.Kind -> if (p.name.isNotEmpty()) Thumbs.kind(scope, picture, p.name)
        }
    }

    /** How far, as a ring over a preview, its picture dimmed under it. */
    class Ring(context: android.content.Context, private val done: Float) : View(context) {
        private val dp = context.resources.displayMetrics.density
        private val track = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            style = android.graphics.Paint.Style.STROKE
            strokeWidth = 2.5f * dp
            color = 0x66FFFFFF
        }
        private val arc = android.graphics.Paint(track).apply { color = 0xFFFFFFFF.toInt(); strokeCap = android.graphics.Paint.Cap.ROUND }

        override fun onDraw(canvas: android.graphics.Canvas) {
            canvas.drawColor(0x55000000)
            val r = minOf(width, height) * 0.28f
            val cx = width / 2f
            val cy = height / 2f
            canvas.drawCircle(cx, cy, r, track)
            canvas.drawArc(cx - r, cy - r, cx + r, cy + r, -90f, 360f * done.coerceIn(0.03f, 1f), false, arc)
        }
    }

    /** Files shown in a row's stack, and in the page's first look. */
    const val PREVIEWS = 3
}

/**
 * Everything waiting, one section for each of Home's rows, in the same
 * order: its files as previews in a grid, the moving one ringed, a send's
 * with a tap to stop it.
 */
class WaitingScreen(app: MainActivity) : Screen(app) {

    private val views = com.qurb.databinding.ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Home"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Waiting"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        refreshing {
            try {
                val groups = withContext(Dispatchers.IO) {
                    val engine = engine()
                    val peers = engine.peers().also { Devices.load(engine, it) }
                    val backup = engine.backupWaiting(SHOWN.toUInt())
                    val served = runCatching { engine.serving().bytes }.getOrDefault(0uL)
                    Waits.groups(app, peers, engine.holders(), engine.outstanding(),
                        runCatching { engine.waiting() }.getOrDefault(emptyList()), backup,
                        BackingUp.round(app, backup, served), SHOWN)
                }
                val page = views.sections
                page.removeAllViews()
                if (groups.isEmpty()) {
                    kit.empty(page, R.drawable.ic_circle_check, "Nothing waiting")
                    return@refreshing
                }
                for (g in groups) section(page, g)
            } catch (e: Exception) {
                app.fail("Could not read what is waiting", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun section(page: LinearLayout, g: Waits.Group) {
        val head = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(Waits.who(app, g), LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            addView(Waits.chip(app, g.progress ?: Words.size(g.bytes)))
            g.waitingOn?.let { addView(Waits.chip(app, it, waiting = true)) }
        }
        page.addView(head, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(22); bottomMargin = kit.dp(4) })
        val previews = g.previews
        val more = g.count - previews.size
        val cells = previews.size + if (more > 0) 1 else 0
        Gallery.grid(app, page, cells) { i, picture, cell ->
            if (i >= previews.size) {
                picture.visibility = View.GONE
                (cell as ViewGroup).addView(TextView(app).apply {
                    text = "+${Words.count(more)}"
                    setTextAppearance(R.style.Text_Name)
                    gravity = Gravity.CENTER
                }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                return@grid
            }
            Waits.bind(scope, picture, previews[i])
            if (i == 0 && g.fraction != null && g.fraction < 1f) (cell as ViewGroup).addView(Waits.Ring(app, g.fraction),
                FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            // A send not collected yet: a tap asks whether to stop it.
            g.sends.getOrNull(i)?.let { w -> cell.setOnClickListener { app.cancelSend(w) } }
        }
    }

    private companion object {
        /** Files shown, at most, before the rest are one tile. */
        const val SHOWN = 8
    }
}
