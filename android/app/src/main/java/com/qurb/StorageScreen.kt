package com.qurb

import android.graphics.drawable.GradientDrawable
import android.view.Gravity
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
import uniffi.qurb_mobile.StoragePart

/**
 * Settings → Manage storage → Devices (the owner's list of 2026-11-01, item
 * 8, and his review of 2026-11-02, SE-3 and SE-6): each of this person's
 * computers as a card -- what QurbSpace uses on it, as a bar of the space it
 * is given there, not the whole disk. Tapped, what fills it
 * ([DeviceStorageScreen]).
 *
 * Asked of each computer over the connection kept for it (`Request::Storage`);
 * a computer off shows what it said last, this run, dimmed.
 */
class StorageScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Settings"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Devices"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        scope.launch {
            val computers = runCatching {
                withContext(Dispatchers.IO) { engine().peers().filter { it.relation == "own" && it.kind != "phone" } }
            }.getOrDefault(emptyList())
            val page = views.sections
            page.removeAllViews()
            if (computers.isEmpty()) {
                kit.empty(page, R.drawable.ic_hard_drive, "No computers yet")
            }
            for (computer in computers) {
                val slot = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
                page.addView(slot, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                    .apply { topMargin = kit.dp(12) })
                Told.of[computer.fingerprint]?.let { card(slot, computer, it, stale = true) } ?: card(slot, computer, null, stale = true)
                scope.launch {
                    val told = runCatching { Storage.ask(app, computer) }.getOrNull()
                    if (told != null) Told.of[computer.fingerprint] = told
                    slot.removeAllViews()
                    card(slot, computer, told ?: Told.of[computer.fingerprint], stale = told == null)
                }
            }
            views.refresh.isRefreshing = false
        }
    }

    private fun card(slot: LinearLayout, computer: PeerInfo, storage: ComputerStorage?, stale: Boolean) {
        val card = kit.card(slot)
        card.setPadding(kit.dp(16), kit.dp(16), kit.dp(16), kit.dp(16))
        card.alpha = if (stale && storage != null) 0.6f else 1f
        val head = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        head.addView(kit.avatar(computer.name, Engine.isOff(app, computer), Devices.picture(computer), 40),
            LinearLayout.LayoutParams(kit.dp(40), kit.dp(40)))
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply { text = computer.name; setTextAppearance(R.style.Text_Name) })
        words.addView(TextView(app).apply {
            text = when {
                storage != null -> "${Words.size(storage.used)} used"
                stale -> "Reaching ${computer.name}…"
                else -> ""
            }
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
        })
        head.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = kit.dp(12) })
        head.addView(android.widget.ImageView(app).apply {
            setImageResource(R.drawable.ic_chevron_right)
            imageTintList = android.content.res.ColorStateList.valueOf(kit.color(R.color.text_3))
        }, LinearLayout.LayoutParams(kit.dp(20), kit.dp(20)))
        card.addView(head)
        if (storage != null) {
            card.addView(Storage.bar(app, storage, 10), LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(10))
                .apply { topMargin = kit.dp(12) })
            card.addView(TextView(app).apply {
                text = Storage.free(storage)
                setTextAppearance(R.style.Text_Meta)
                setTextColor(kit.color(R.color.text_3))
                setPadding(0, kit.dp(6), 0, 0)
            })
            card.setOnClickListener { app.push(DeviceStorageScreen(app, computer, storage)) }
        }
        card.background = androidx.core.content.ContextCompat.getDrawable(app, R.drawable.card)
        card.foreground = androidx.core.content.ContextCompat.getDrawable(app, R.drawable.row_bg)
    }

    /** What each computer said last, for this run of the app. */
    private object Told {
        val of = java.util.concurrent.ConcurrentHashMap<String, ComputerStorage>()
    }
}

/**
 * One computer's storage (SE-3, SE-6): free of what it gives QurbSpace, a bar
 * of what fills it, a row for each kind, and Recently deleted emptied after
 * asking. Nothing of the rest of its disk.
 */
class DeviceStorageScreen(app: MainActivity, private val computer: PeerInfo, private var storage: ComputerStorage) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Devices"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = computer.name
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener {
            scope.launch {
                runCatching { Storage.ask(app, computer) }.getOrNull()?.let { storage = it }
                refresh()
            }
        }
    }

    override fun refresh() {
        val page = views.sections
        page.removeAllViews()
        page.addView(TextView(app).apply {
            text = if (storage.given > 0uL) "${Words.size(Storage.left(storage))} free" else "${Words.size(storage.used)} used"
            setTextAppearance(R.style.Text_Hero)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = kit.dp(4) })
        page.addView(TextView(app).apply {
            text = if (storage.given > 0uL) "of ${Words.size(storage.given)} given to QurbSpace" else "No limit set on ${computer.name}"
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
        })
        page.addView(Storage.bar(app, storage, 14), LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(14))
            .apply { topMargin = kit.dp(12) })
        val card = kit.card(page, top = 16)
        for (part in storage.parts.filter { it.bytes > 0uL || it.kind == "shared" }) {
            val row = kit.setting(card, R.drawable.ic_circle, Storage.words(part), Words.size(part.bytes))
            row.icon.imageTintList = android.content.res.ColorStateList.valueOf(Storage.colour(app, part.kind))
            row.icon.layoutParams = row.icon.layoutParams.apply { width = kit.dp(12); height = kit.dp(12) }
            if (part.kind == "deleted" && part.bytes > 0uL) {
                row.value.text = "Empty"
                row.value.setTextColor(kit.color(R.color.error))
                row.root.setOnClickListener { empty(part.bytes) }
            }
        }
        page.addView(TextView(app).apply {
            text = "How much QurbSpace may use is set on ${computer.name}."
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
            setPadding(kit.dp(6), kit.dp(10), kit.dp(6), 0)
        })
        views.refresh.isRefreshing = false
    }

    private fun empty(bytes: ULong) {
        kit.sheet()
            .mark(R.drawable.ic_trash_2)
            .title("Empty Recently deleted on ${computer.name}?", centred = true)
            .line("${Words.size(bytes)}, gone for good")
            .buttons("Empty", danger = true) {
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) { engine().openKeptFolder(computer.fingerprint, 20u).emptyDeleted() }
                        app.say("Emptied")
                        runCatching { Storage.ask(app, computer) }.getOrNull()?.let { storage = it }
                    } catch (e: Exception) {
                        app.fail("Not emptied", e)
                    }
                    refresh()
                }
            }
            .show()
    }
}

/** What both storage pages share: asking, the bar, the words and colours. */
object Storage {
    /** Asked of `computer` over its kept connection; null from one too old to say. */
    suspend fun ask(app: MainActivity, computer: PeerInfo): ComputerStorage? = withContext(Dispatchers.IO) {
        Engine.hearingTheNetwork(app) { Engine.open(app).openKeptFolder(computer.fingerprint, 20u) }.storage()
    }

    fun left(s: ComputerStorage): ULong = if (s.given > s.used) s.given - s.used else 0uL

    fun free(s: ComputerStorage) =
        if (s.given > 0uL) "${Words.size(left(s))} free of ${Words.size(s.given)}" else "No limit set"

    /** Each kind by its share of what QurbSpace is given (or uses, with no limit). */
    fun bar(app: MainActivity, s: ComputerStorage, height: Int): View {
        val bar = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            background = GradientDrawable().apply { cornerRadius = app.kit.dp(height / 2).toFloat(); setColor(app.kit.color(R.color.neutral_bg)) }
            clipToOutline = true
        }
        val whole = maxOf(if (s.given > 0uL) s.given else s.used, 1uL).toFloat()
        var shown = 0f
        for (part in s.parts) {
            if (part.bytes == 0uL) continue
            val share = part.bytes.toFloat() / whole
            shown += share
            bar.addView(View(app).apply { setBackgroundColor(colour(app, part.kind)) },
                LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, share))
        }
        if (shown < 1f) bar.addView(View(app), LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f - shown))
        return bar
    }

    fun colour(app: MainActivity, kind: String) = when (kind) {
        "shared" -> app.kit.color(R.color.green)
        "backups" -> 0xFF3B82F6.toInt()
        "vault" -> 0xFF8B5CF6.toInt()
        "guest" -> app.kit.color(R.color.attention)
        "deleted" -> app.kit.color(R.color.error)
        else -> app.kit.color(R.color.text_3)
    }

    fun words(part: StoragePart) = when (part.kind) {
        "shared" -> "Shared"
        "backups" -> "${part.name} backups"
        "vault" -> "${part.name} Vault"
        "guest" -> part.name
        "deleted" -> "Recently deleted"
        else -> "Other"
    }
}
