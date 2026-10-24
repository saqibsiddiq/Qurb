package com.qurb

import android.content.res.ColorStateList
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.BackupState
import uniffi.qurb_mobile.PeerInfo

/**
 * One device's own page (docs/design/redesign.md, Devices draft 2 onwards):
 * its icon and name, whether it is on, Send, what it backs up of this phone,
 * and Remove. Who it is -- its identity and when it was paired -- is behind
 * the info button, and in Settings.
 *
 * What it backs up is what the engine does today: keeping this phone's
 * Private Vault is the one choice made for a device, and WhatsApp and the
 * folders travel inside the vault, to every device keeping it. So the
 * WhatsApp and Folders rows show only on a device that keeps the vault, and
 * switching WhatsApp here switches it for every such device. A different set
 * for each device (the owner's item 4 of 2026-10-23) is engine work not done
 * yet (decision 0073).
 */
class DeviceScreen(app: MainActivity, private var peer: PeerInfo) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_devices

    private val backups = Backups(app) { refresh() }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Devices"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.visibility = View.GONE
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(
        val peer: PeerInfo?,
        val keeps: Boolean,
        val whatsApp: BackupState,
        val folders: Int,
    )

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    State(
                        engine.peers().firstOrNull { it.fingerprint == peer.fingerprint },
                        engine.holders().any { it.fingerprint == peer.fingerprint },
                        engine.whatsappBackup(),
                        if (Folders.allowed(app)) engine.chosenFolders().size else 0,
                    )
                }
                // Removed meanwhile, here or by the sheet below: nothing left
                // to show.
                val now = state.peer
                if (now == null) {
                    app.onBackPressedDispatcher.onBackPressed()
                    return@refreshing
                }
                peer = now
                show(state)
            } catch (e: Exception) {
                app.fail("Could not read this device", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(state: State) {
        val page = views.sections
        page.removeAllViews()
        val off = Engine.isOff(app, peer)
        val host = peer.relation == "host"

        // Its icon large in a green tile, its name, on or off, and the info.
        val head = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        val tile = FrameLayout(app).apply {
            background = ContextCompat.getDrawable(app, R.drawable.tile_green)
        }
        tile.addView(ImageView(app).apply {
            setImageResource(States.device(peer.name, peer.kind))
            imageTintList = ColorStateList.valueOf(kit.color(R.color.green))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(kit.dp(34), kit.dp(34), Gravity.CENTER))
        head.addView(tile, LinearLayout.LayoutParams(kit.dp(64), kit.dp(64)).apply { marginEnd = kit.dp(16) })
        val words = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        words.addView(TextView(app).apply {
            text = peer.name
            setTextAppearance(R.style.Text_Hero)
            maxLines = 2
            ellipsize = android.text.TextUtils.TruncateAt.END
            androidx.core.view.ViewCompat.setAccessibilityHeading(this, true)
        })
        val state0 = LinearLayout(app).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        state0.addView(kit.dot(off), LinearLayout.LayoutParams(kit.dp(8), kit.dp(8)).apply { marginEnd = kit.dp(6) })
        state0.addView(TextView(app).apply {
            text = if (off) "Off" else "On"
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_2))
        })
        // How long since it answered, when it is off: what to look at first
        // when something has not arrived.
        val seen = peer.lastSeen
        if (off && seen != null) state0.addView(TextView(app).apply {
            text = Words.since(seen)
            setTextAppearance(R.style.Text_Meta)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { marginStart = kit.dp(8) })
        words.addView(state0, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = kit.dp(4) })
        head.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        head.addView(kit.iconButton(R.drawable.ic_info, "About ${peer.name}") { details() }.apply {
            (this as ImageView).imageTintList = ColorStateList.valueOf(kit.color(R.color.text))
        }, LinearLayout.LayoutParams(kit.dp(44), kit.dp(44)).apply { marginEnd = -kit.dp(10) })
        page.addView(head, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = kit.dp(12) })

        page.addView(kit.button("Send", icon = R.drawable.ic_send) { app.pickFilesToSend(peer) },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)).apply { topMargin = kit.dp(20) })

        kit.label(page, "Backs up")
        val card = kit.card(page)
        if (state.keeps && !host) {
            val backing = WhatsApp.tree(app) != null
            kit.setting(card, R.drawable.ic_whatsapp, "WhatsApp",
                if (backing && state.whatsApp.bytes > 0uL) Words.size(state.whatsApp.bytes) else "",
                on = backing, onChange = { on ->
                if (on) backups.whatsApp(state.whatsApp, peer.name, backing = false)
                else backups.stopWhatsApp(peer.name)
                // The switch shows what is so, not what was tapped: on once
                // WhatsApp's folder is chosen, off once stopping is confirmed.
                refresh()
            })
            kit.setting(card, R.drawable.ic_folder, "Folders",
                if (state.folders > 0) state.folders.toString() else "") {
                app.push(FoldersScreen(app, peer.name, R.id.tab_devices))
            }
        }
        kit.setting(card, R.drawable.ic_lock_keyhole, "Private Vault", on = state.keeps, onChange = { on ->
            when {
                on && host -> keepWithHost()
                on -> keep()
                else -> stopKeeping()
            }
        })

        page.addView(kit.button(if (host) "Stop visiting" else "Remove", Kit.Style.SECONDARY) { askToRemove() },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)).apply { topMargin = kit.dp(20) })
    }

    /** Who it is, behind the info button: its identity and when it was paired. */
    private fun details() {
        val facts = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            background = ContextCompat.getDrawable(app, R.drawable.group_quiet)
            setPadding(0, kit.dp(4), 0, kit.dp(4))
        }
        kit.setting(facts, R.drawable.ic_key_round, "Identity", peer.short)
        kit.setting(facts, R.drawable.ic_clock, "Paired", Words.ago(peer.pairedAt))
        if (peer.relation == "host") kit.setting(facts, R.drawable.ic_laptop, "Someone else's", "You visit as a guest")
        kit.sheet().title(peer.name).view(facts, top = 14).show()
    }

    private fun keep() {
        kit.sheet()
            .mark(R.drawable.ic_lock_keyhole, R.color.green_soft, R.color.green)
            .title("Keep your Private Vault on ${peer.name}?", centred = true)
            .line("Nobody using ${peer.name} sees it. This phone can then free its own copies.")
            .buttons("Keep it there") { setKeeping(true) }
            // However it closes, the switch goes back to what is so.
            .onDismiss { refresh() }
            .show()
    }

    /** On a computer of another person: sealed, and nothing kept here (decision 0060). */
    private fun keepWithHost() {
        kit.sheet()
            .mark(R.drawable.ic_lock_keyhole, R.color.green_soft, R.color.green)
            .title("Keep your files on ${peer.name}?", centred = true)
            .line("Sealed on this phone first: whoever uses ${peer.name} can't open them or see their names.")
            .warning("It's their computer: they can delete what it keeps")
            .buttons("Keep them there") { setKeeping(true) }
            // However it closes, the switch goes back to what is so.
            .onDismiss { refresh() }
            .show()
    }

    private fun stopKeeping() {
        kit.sheet()
            .mark(R.drawable.ic_lock_keyhole, R.color.neutral_bg, R.color.neutral)
            .title("Stop keeping it on ${peer.name}?", centred = true)
            .line("Nothing new goes there. What it has, it keeps.")
            .buttons("Stop") { setKeeping(false) }
            // However it closes, the switch goes back to what is so.
            .onDismiss { refresh() }
            .show()
    }

    /**
     * Say what removing it does (brief §35) before doing it, as the
     * redesign's Remove: centred, the one warning that matters, the one
     * choice, two equal buttons.
     */
    private fun askToRemove() {
        scope.launch {
            val plan = try {
                withContext(Dispatchers.IO) { engine().removalPlan(peer.fingerprint) }
            } catch (e: Exception) {
                app.fail("Could not check what removing it would do", e)
                return@launch
            }
            val host = peer.relation == "host"
            val sheet = kit.sheet()
                .mark(States.device(peer.name, peer.kind))
                .title(if (host) "Stop visiting ${peer.name}?" else "Remove ${peer.name}?", centred = true)
                .line("It stops syncing with this phone")
            // What can't be had back afterwards, first; then what stops.
            when {
                plan.onlyThere.isNotEmpty() ->
                    sheet.warning("${Words.files(plan.onlyThere.size)} ${if (plan.onlyThere.size == 1) "is" else "are"} only on ${peer.name}")
                plan.holdsOurs -> sheet.warning("Your Private Vault there can't be fetched back")
            }
            if (plan.waiting > 0u) {
                sheet.warning("${Words.files(plan.waiting.toInt())} sent to it, cancelled", R.drawable.ic_send)
            }
            var deleteKept = false
            val kept = plan.kept.toInt()
            if (kept > 0) {
                // A choice rather than a side effect: these may be the only
                // copy of that device's own files anywhere.
                val card = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
                kit.setting(kit.card(card), R.drawable.ic_trash_2, "Delete its copies here",
                    Words.size(plan.keptBytes), on = false, onChange = { deleteKept = it })
                sheet.view(card, top = 10)
            }
            sheet.buttons(if (host) "Stop" else "Remove", danger = true) { remove(deleteKept) }
            sheet.show()
        }
    }

    private fun remove(deleteKept: Boolean) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) { engine().removeDevice(peer.fingerprint, deleteKept) }
                app.say("${peer.name} removed")
            } catch (e: Exception) {
                app.fail("Could not remove it", e)
            } finally {
                app.changed()
            }
        }
    }

    private fun setKeeping(keep: Boolean) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    if (keep) engine().addHolder(peer.fingerprint) else engine().removeHolder(peer.fingerprint)
                }
                // The next sync is now: choosing a device to keep the vault is
                // a change it should act on while the person is looking.
                if (keep) app.madeChange() else app.changed()
            } catch (e: Exception) {
                app.fail("Could not change that", e)
                app.changed()
            }
        }
    }
}
