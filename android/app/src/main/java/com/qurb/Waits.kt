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
 * Waiting: one row for each device files are going to or coming from, never
 * one row per file (the owner's review of 2026-11-03, H-1), drawn as option
 * A (docs/design/system/waiting.html, chosen that evening): one picture with
 * the other device's face on its corner, a title whose verb says the way,
 * one quiet line -- amber only when it waits on you -- and how far it has
 * got. Backups and the files only this phone has go to the same computer and
 * are one row: "only here" is not a category of its own.
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
        /** A backup, said "Backing up" rather than "Sending". */
        val backup: Boolean = false,
    ) {
        /** The title: the way, then how many. */
        fun title(): String {
            val verb = when { coming -> "Receiving"; backup -> "Backing up"; else -> "Sending" }
            return "$verb ${Words.files(count.toInt())}"
        }

        /** Whether it waits on a phone, which takes what it was sent only
         *  while QurbSpace is open on it (decision 0082). */
        fun waitsOnPhone() = waitingOn != null && peer?.kind == "phone"

        /** The one quiet line: to or from whom, and how much; or what it
         *  waits on. */
        fun line(): String = when {
            waitingOn != null && waitsOnPhone() -> "Open QurbSpace on $name"
            waitingOn != null -> "To $name, when it's on"
            progress != null -> "From $name, $progress"
            else -> "${if (coming) "From" else "To"} $name" + (if (bytes > 0uL) ", ${Words.size(bytes)}" else "")
        }
    }

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
                backup = backupFiles > 0,
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

    /** A group's row, as option A: its picture with the device's face,
     *  the title, the quiet line and its bar, and how far, or what it waits
     *  on, at the end. */
    fun row(app: MainActivity, scope: CoroutineScope, parent: ViewGroup, g: Group, onTap: () -> Unit): View {
        val kit = app.kit
        val row = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = kit.dp(72)
            setPadding(kit.dp(16), kit.dp(12), kit.dp(16), kit.dp(12))
            background = ContextCompat.getDrawable(app, R.drawable.row_bg)
            setOnClickListener { onTap() }
        }
        val moving = g.fraction?.takeIf { it < 1f }
        row.addView(picture(app, scope, g, 48, moving),
            LinearLayout.LayoutParams(kit.dp(53), kit.dp(53)).apply { marginEnd = kit.dp(9) })
        row.addView(words(app, g, bar = moving), LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        end(app, g, moving)?.let { row.addView(it) }
        row.contentDescription = listOfNotNull(g.title(), g.line(), g.fraction?.let { "${(it * 100).toInt()}% there" })
            .joinToString(", ")
        parent.addView(row)
        return row
    }

    /** The title and the quiet line, amber when it waits on the person, and
     *  the bar under them while it moves. */
    fun words(app: MainActivity, g: Group, bar: Float? = null, big: Boolean = false): LinearLayout {
        val kit = app.kit
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = g.title()
            setTextAppearance(R.style.Text_Name)
            textSize = if (big) 16f else 15f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT, 600, false)
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        })
        words.addView(TextView(app).apply {
            text = g.line()
            setTextAppearance(R.style.Text_Meta)
            textSize = 13f
            setTextColor(kit.color(if (g.waitsOnPhone()) R.color.attention else R.color.text_3))
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(2) })
        if (bar != null) words.addView(android.widget.ProgressBar(app, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 1000
            progress = (bar * 1000).toInt()
            progressTintList = ColorStateList.valueOf(kit.color(R.color.green))
            progressBackgroundTintList = ColorStateList.valueOf(kit.color(R.color.surface_2))
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(3)).apply { topMargin = kit.dp(8) })
        return words
    }

    /** How far, as a number; or the phone, amber, that has to be opened; or
     *  the clock, for a computer that is off. */
    private fun end(app: MainActivity, g: Group, moving: Float?): View? {
        val kit = app.kit
        if (moving != null) return TextView(app).apply {
            text = "${(moving * 100).toInt()}%"
            setTextAppearance(R.style.Text_Meta)
            textSize = 13f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT, 600, false)
            setTextColor(kit.color(R.color.text_2))
            fontFeatureSettings = "tnum"
            gravity = Gravity.END
            minWidth = kit.dp(36)
        }
        if (g.waitingOn == null) return null
        return ImageView(app).apply {
            setImageResource(if (g.waitsOnPhone()) R.drawable.ic_smartphone else R.drawable.ic_clock)
            imageTintList = ColorStateList.valueOf(kit.color(if (g.waitsOnPhone()) R.color.attention else R.color.text_3))
            layoutParams = LinearLayout.LayoutParams(kit.dp(18), kit.dp(18)).apply { marginStart = kit.dp(10) }
        }
    }

    /** One picture, the first file's, with the device's face on its corner,
     *  and a ring over it while it moves. */
    fun picture(app: MainActivity, scope: CoroutineScope, g: Group, side: Int, done: Float?): View {
        val kit = app.kit
        val box = FrameLayout(app).apply { clipChildren = false; clipToPadding = false }
        val cell = FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.file_tile)
            clipToOutline = true
        }
        val picture = ImageView(app)
        cell.addView(picture, FrameLayout.LayoutParams(kit.dp(22), kit.dp(22), Gravity.CENTER))
        bind(scope, picture, g.previews.firstOrNull() ?: Preview.Kind(""))
        if (done != null) cell.addView(Ring(app, done), FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        box.addView(cell, FrameLayout.LayoutParams(kit.dp(side), kit.dp(side)))
        // The face: 22 across, ringed in the card's colour.
        val face = FrameLayout(app).apply {
            background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                setColor(kit.color(R.color.surface))
            }
            setPadding(kit.dp(2), kit.dp(2), kit.dp(2), kit.dp(2))
            addView(kit.avatar(g.name, null, g.peer?.let { Devices.picture(it) }, 18))
        }
        box.addView(face, FrameLayout.LayoutParams(kit.dp(22), kit.dp(22), Gravity.END or Gravity.BOTTOM))
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
            val r = minOf(13f * dp, minOf(width, height) * 0.28f)
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
        // As its row on Home: the title, the quiet line (option A).
        page.addView(Waits.words(app, g, big = true), LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(26); bottomMargin = kit.dp(10); marginStart = kit.dp(4) })
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
