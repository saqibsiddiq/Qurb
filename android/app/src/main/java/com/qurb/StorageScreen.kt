package com.qurb

import android.graphics.drawable.GradientDrawable
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.ComputerStorage
import uniffi.qurb_mobile.PeerInfo

/**
 * Settings → Storage (the owner's list of 2026-11-01, item 8): the storage of
 * each of this person's computers, managed from here as One UI's is -- the
 * disk as one bar, coloured by what fills it, a row for each kind with its
 * size, and Recently deleted emptied with one button. Then this phone, which
 * holds little: what the app takes, and its cache let go of.
 *
 * Asked of each computer over the connection kept for it
 * (`Request::Storage`); a computer off or too old to say is shown so.
 */
class StorageScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Settings"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Storage"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        scope.launch {
            val computers = runCatching {
                withContext(Dispatchers.IO) {
                    engine().peers().filter { it.relation == "own" && it.kind != "phone" }
                }
            }.getOrDefault(emptyList())
            val page = views.sections
            page.removeAllViews()
            for (computer in computers) {
                val box = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
                page.addView(box)
                kit.label(box, computer.name)
                kit.empty(box, R.drawable.ic_hard_drive, "Reaching ${computer.name}…")
                scope.launch {
                    val told = runCatching {
                        withContext(Dispatchers.IO) {
                            Engine.hearingTheNetwork(app) { engine().openKeptFolder(computer.fingerprint, 20u) }.storage()
                        }
                    }
                    box.removeViews(1, box.childCount - 1)
                    val storage = told.getOrNull()
                    when {
                        storage != null -> show(box, computer, storage)
                        told.isSuccess -> kit.empty(box, R.drawable.ic_hard_drive, "Update QurbSpace on ${computer.name} to see its storage.")
                        else -> kit.empty(box, R.drawable.ic_cloud_off, "${computer.name} isn't reachable.", "Try again") { refresh() }
                    }
                }
            }
            thisPhone(page)
            views.refresh.isRefreshing = false
        }
    }

    /** One computer: free of its disk, the bar, a row for each kind. */
    private fun show(box: LinearLayout, computer: PeerInfo, storage: ComputerStorage) {
        val card = kit.card(box, top = 8)
        card.setPadding(kit.dp(16), kit.dp(16), kit.dp(16), kit.dp(16))
        if (storage.disk > 0uL) {
            card.addView(TextView(app).apply {
                text = "${Words.size(storage.free)} free"
                setTextAppearance(R.style.Text_Hero)
            })
            card.addView(TextView(app).apply {
                text = "of ${Words.size(storage.disk)}"
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.text_2))
            })
        }
        val parts = storage.parts.filter { it.bytes > 0uL }
        val qurb = parts.sumOf { it.bytes }
        // The bar: each kind by its share of the disk, the rest of what is
        // used grey, and what is free left empty.
        val bar = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            background = GradientDrawable().apply { cornerRadius = kit.dp(6).toFloat(); setColor(kit.color(R.color.neutral_bg)) }
            clipToOutline = true
        }
        val whole = if (storage.disk > 0uL) storage.disk else qurb.coerceAtLeast(1uL)
        fun segment(bytes: ULong, color: Int) {
            if (bytes == 0uL) return
            bar.addView(View(app).apply { setBackgroundColor(color) },
                LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, bytes.toFloat() / whole.toFloat()))
        }
        for (part in parts) segment(part.bytes, colour(part.kind))
        if (storage.disk > 0uL) {
            val others = (storage.disk - storage.free).let { if (it > qurb) it - qurb else 0uL }
            segment(others, kit.color(R.color.text_3))
            segment(storage.free, kit.color(R.color.neutral_bg))
        }
        card.addView(bar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(12)).apply { topMargin = kit.dp(12) })

        val rows = kit.card(box, top = 8)
        for (part in parts) {
            val row = kit.setting(rows, icon(part.kind), words(part.kind, part.name), Words.size(part.bytes))
            row.icon.imageTintList = android.content.res.ColorStateList.valueOf(colour(part.kind))
            if (part.kind == "deleted") {
                row.root.setOnClickListener { empty(computer, part.bytes) }
            }
        }
    }

    /** Recently deleted on `computer` emptied for good, asked first. */
    private fun empty(computer: PeerInfo, bytes: ULong) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2)
            .title("Empty Recently deleted on ${computer.name}?", centred = true)
            .line("${Words.size(bytes)}, gone for good")
            .buttons("Empty", danger = true) {
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) {
                            engine().openKeptFolder(computer.fingerprint, 20u).emptyDeleted()
                        }
                        app.say("Emptied")
                    } catch (e: Exception) {
                        app.fail("Not emptied", e)
                    }
                    refresh()
                }
            }
            .show()
    }

    /** This phone: what the app holds, and its cache let go of. */
    private fun thisPhone(page: LinearLayout) {
        kit.label(page, "This phone")
        val card = kit.card(page, top = 8)
        scope.launch {
            val (used, cache) = withContext(Dispatchers.IO) {
                val used = runCatching { engine().usage().onDisk }.getOrDefault(0uL)
                used to app.cacheDir.walkTopDown().filter { it.isFile }.sumOf { it.length() }.toULong()
            }
            kit.setting(card, R.drawable.ic_smartphone, "QurbSpace", Words.size(used))
            kit.setting(card, R.drawable.ic_trash_2, "Cache", Words.size(cache)) {
                scope.launch {
                    withContext(Dispatchers.IO) { app.cacheDir.listFiles()?.forEach { it.deleteRecursively() } }
                    app.say("Cleared")
                    refresh()
                }
            }
        }
    }

    private fun colour(kind: String) = when (kind) {
        "shared" -> kit.color(R.color.green)
        "backups" -> 0xFF3B82F6.toInt()
        "vault" -> 0xFF8B5CF6.toInt()
        "guest" -> kit.color(R.color.attention)
        "deleted" -> kit.color(R.color.error)
        else -> kit.color(R.color.text_2)
    }

    private fun icon(kind: String) = when (kind) {
        "shared" -> R.drawable.ic_users
        "backups" -> R.drawable.ic_smartphone
        "vault" -> R.drawable.ic_lock_keyhole
        "guest" -> R.drawable.ic_users
        "deleted" -> R.drawable.ic_trash_2
        else -> R.drawable.ic_hard_drive
    }

    private fun words(kind: String, name: String) = when (kind) {
        "shared" -> "Shared"
        "backups" -> "$name backups"
        "vault" -> "$name Vault"
        "guest" -> name
        "deleted" -> "Recently deleted"
        else -> "Other"
    }
}
