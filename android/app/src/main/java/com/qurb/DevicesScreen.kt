package com.qurb

import android.content.res.ColorStateList
import android.view.View
import android.widget.LinearLayout
import com.qurb.databinding.CardDeviceBinding
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.PeerInfo

/**
 * Devices (direction §16): my phone, my laptop -- never nodes. What each is,
 * its name, when it was last here; and, for each, whether it keeps a backup
 * of this phone's Private Vault.
 *
 * Keeping is the choice decision 0036 is about: this phone's own files go to
 * no other device unless the person picks one to keep them, and the device
 * picked keeps them where nobody using it sees them. The choice lives here
 * because it is a choice about a device.
 */
class DevicesScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_devices

    /** A device just added, to materialise as it appears (§34). */
    private var arrived: Set<String> = emptySet()
    private var known: Set<String>? = null

    init {
        views.title.text = "Devices"
        views.subtitle.text = "Your devices, and whether each is reachable."
        views.subtitle.visibility = View.VISIBLE
        views.action.text = "Add"
        views.action.setIconResource(R.drawable.ic_plus)
        views.action.visibility = View.VISIBLE
        views.action.setOnClickListener { app.pair() }
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        scope.launch {
            try {
                val (peers, holders) = withContext(Dispatchers.IO) {
                    val engine = engine()
                    engine.peers() to engine.holders().map { it.fingerprint }.toSet()
                }
                val now = peers.map { it.fingerprint }.toSet()
                arrived = known?.let { now - it } ?: emptySet()
                known = now
                show(peers, holders)
            } catch (e: Exception) {
                app.fail("Could not read your devices", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(peers: List<PeerInfo>, holders: Set<String>) {
        val page = views.sections
        page.removeAllViews()

        // Two to a row: this phone first, then the others.
        val cards = mutableListOf<View>()
        cards += card(page, R.drawable.ic_smartphone, android.os.Build.MODEL ?: "This phone",
            "This phone", on = true, extra = null, onTap = null)
        // This person's own devices, then other people's computers this phone
        // visits as a guest (decision 0060).
        val (own, visited) = peers.partition { it.relation == "own" }
        for (peer in own) cards += deviceCard(page, peer, holders)
        layOut(page, cards)
        if (visited.isNotEmpty()) {
            // Labelled as every section is, and in the desktop's words.
            kit.groupTitle(page, "Computers you visit as a guest")
            layOut(page, visited.map { deviceCard(page, it, holders) })
        }

        if (peers.isEmpty()) {
            kit.empty(page, R.drawable.ic_monitor_smartphone,
                "Add your computer or another phone, and your files move between them. On a computer, " +
                    "open Qurb, go to Devices and choose Add a device.",
                "Add a device") { app.pair() }
        }
    }

    private fun deviceCard(page: LinearLayout, peer: PeerInfo, holders: Set<String>): View {
        val seen = peer.lastSeen?.let { "Last seen ${Words.ago(it)}" } ?: "Not seen yet"
        val extra = when {
            peer.relation == "host" -> "You visit as a guest"
            peer.fingerprint in holders -> "Keeps a backup of your Private Vault"
            else -> null
        }
        val card = card(page, States.device(peer.name), peer.name, seen, on = false, extra = extra) {
            open(peer, peer.fingerprint in holders)
        }
        if (peer.fingerprint in arrived && !Kit.calm()) materialise(card)
        return card
    }

    /** Two to a row. */
    private fun layOut(page: LinearLayout, cards: List<View>) {
        for (pair in cards.chunked(2)) {
            val row = LinearLayout(app).apply {
                orientation = LinearLayout.HORIZONTAL
                setPadding(0, 0, 0, 0)
            }
            for (card in pair) row.addView(card)
            if (pair.size == 1) row.addView(View(app), LinearLayout.LayoutParams(0, 1, 1f))
            page.addView(row, LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
            ).apply { marginStart = -kit.dp(5); marginEnd = -kit.dp(5) })
        }
    }

    private fun card(
        parent: LinearLayout,
        icon: Int,
        name: String,
        presence: String,
        on: Boolean,
        extra: String?,
        onTap: (() -> Unit)?,
    ): View {
        val card = CardDeviceBinding.inflate(app.layoutInflater, parent, false)
        card.icon.setImageResource(icon)
        card.icon.imageTintList = ColorStateList.valueOf(kit.color(R.color.text_2))
        card.name.text = name
        card.presence.text = presence
        card.dot.backgroundTintList = ColorStateList.valueOf(kit.color(if (on) R.color.healthy else R.color.border_strong))
        if (on) card.presence.setTextColor(kit.color(R.color.healthy))
        card.extra.visibility = if (extra == null) View.GONE else View.VISIBLE
        card.extra.text = extra
        if (onTap != null) card.root.setOnClickListener { onTap() } else card.root.isClickable = false
        return card.root
    }

    /** §34: translucent outline, soft glow, solid -- this device has entered
     *  your Qurb space. */
    private fun materialise(card: View) {
        card.alpha = 0f
        card.scaleX = 0.94f
        card.scaleY = 0.94f
        card.animate().alpha(1f).scaleX(1f).scaleY(1f).setDuration(700)
            .setInterpolator(android.view.animation.DecelerateInterpolator(2f)).start()
    }

    /** A device's details, and what can be done about it. */
    private fun open(peer: PeerInfo, keeps: Boolean) {
        val seen = peer.lastSeen?.let { "Last seen ${Words.ago(it)}" } ?: "Not seen yet"
        val sheet = kit.sheet().header(States.device(peer.name), peer.name, seen)
        val facts = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        val group = kit.group(facts)
        if (peer.relation == "host") {
            // Another person's computer. It keeps this phone's Private Vault
            // only if chosen, and then sealed: it can open neither a name nor
            // a byte, and this phone keeps none of its own copies (decision
            // 0060).
            kit.item(group, "Who", "Someone else's computer, which this phone visits as a guest. It sees only what you send it.")
            kit.toggle(group, "Keep my files here",
                if (keeps) "Your Private Vault is kept on ${peer.name}, sealed: nobody using it can open your files. " +
                    "Each one comes back when you open it."
                else "Your Private Vault, kept on ${peer.name} and sealed so nobody there can open it. This phone then keeps none of its own copies.",
                keeps) { on ->
                sheet.dismiss()
                if (on) keepWithHost(peer) else stopKeeping(peer)
            }
            kit.item(group, "Paired", Words.ago(peer.pairedAt))
            sheet.view(facts, top = 16)
            sheet.action(R.drawable.ic_send, "Send files…") { app.pickFilesToSend(peer) }
            sheet.action(R.drawable.ic_x, "Stop visiting…", danger = true) { askToRemove(peer) }
            sheet.text("Identity ${peer.short}", R.style.Text_Meta)
            sheet.show()
            return
        }
        kit.toggle(group, "Keep a backup of my Private Vault",
            if (keeps) "${peer.name} keeps a copy of what this phone keeps private, where nobody using it sees it."
            else "Choose this, and this phone can free its own copies of private files without losing them.",
            keeps) { on ->
            sheet.dismiss()
            if (on) keep(peer) else stopKeeping(peer)
        }
        kit.item(group, "Paired", Words.ago(peer.pairedAt))
        sheet.view(facts, top = 16)
        sheet.action(R.drawable.ic_send, "Send files…") { app.pickFilesToSend(peer) }
        sheet.action(R.drawable.ic_x, "Remove this device…", danger = true) { askToRemove(peer) }
        sheet.text("Identity ${peer.short}", R.style.Text_Meta)
        sheet.show()
    }

    private fun keep(peer: PeerInfo) {
        kit.sheet()
            .header(R.drawable.ic_lock_keyhole, "Keep your Private Vault on ${peer.name}?")
            .text("${peer.name} keeps a copy of the files this phone keeps private, starting at the next " +
                "sync. Nobody using ${peer.name} sees them; they're yours, and come back to this phone " +
                "when you ask.\n\nOnce it has a file, this phone can free its own copy to save space.")
            .buttons("Keep them there", onSecondary = { refresh() }) { setKeeping(peer, true) }
            .show()
    }

    /** On a computer of another person: sealed, and nothing kept here. */
    private fun keepWithHost(peer: PeerInfo) {
        kit.sheet()
            .header(R.drawable.ic_lock_keyhole, "Keep your files on ${peer.name}?")
            .text("Your Private Vault goes to ${peer.name}, sealed on this phone first: whoever uses " +
                "${peer.name} can't open your files, or see their names. They see only how much space " +
                "they take.\n\nOnce ${peer.name} has a file, this phone lets go of its own copy, and " +
                "fetches it back when you open it — while ${peer.name} is on.\n\n" +
                "It's their computer: they can delete what it keeps, though they can't read it.")
            .buttons("Keep them there", onSecondary = { refresh() }) { setKeeping(peer, true) }
            .show()
    }

    private fun stopKeeping(peer: PeerInfo) {
        kit.sheet()
            .header(R.drawable.ic_lock_keyhole, "Stop keeping your files on ${peer.name}?")
            .text("Nothing new goes to ${peer.name}. What it already has, it keeps: this phone doesn't " +
                "reach into another device.\n\nA file this phone freed because ${peer.name} had it is still there.")
            .buttons("Stop", onSecondary = { refresh() }) { setKeeping(peer, false) }
            .show()
    }

    /**
     * Say exactly what removing a device does (brief §35) before doing it:
     * trust ends on this phone, nothing on the other device is touched, and
     * whatever this phone can no longer get back because of it is named.
     */
    private fun askToRemove(peer: PeerInfo) {
        scope.launch {
            val plan = try {
                withContext(Dispatchers.IO) { engine().removalPlan(peer.fingerprint) }
            } catch (e: Exception) {
                app.fail("Could not check what removing it would do", e)
                return@launch
            }
            val sheet = kit.sheet().header(States.device(peer.name), "Remove ${peer.name}?")
            sheet.text("This phone stops trusting ${peer.name}: it can no longer connect or sync here. " +
                "It keeps its key and everything already on it — removing it deletes nothing there.")
            if (plan.holdsOurs) {
                sheet.text("${peer.name} keeps a backup of your Private Vault. After this, nothing new goes " +
                    "there, and this phone can't get anything back from it.")
            }
            if (plan.onlyThere.isNotEmpty()) {
                sheet.text("${Words.files(plan.onlyThere.size)} freed from this phone " +
                    "${if (plan.onlyThere.size == 1) "is" else "are"} kept only on ${peer.name}. " +
                    "Once it's removed, they can't be downloaded again.", color = R.color.error)
            }
            if (plan.waiting > 0u) {
                sheet.text("${Words.files(plan.waiting.toInt())} waiting for it to collect will be cancelled.")
            }
            var deleteKept = false
            val kept = plan.kept.toInt()
            if (kept > 0) {
                // A choice rather than a side effect: these may be the only
                // copy of that device's own files anywhere.
                val group = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
                kit.toggle(kit.group(group), "Also delete what this phone keeps for it",
                    "${Words.files(kept)}, ${Words.size(plan.keptBytes)}", false) { deleteKept = it }
                sheet.view(group, top = 14)
            }
            sheet.text("Only on this phone: your other devices go on trusting it until you remove it there too.",
                R.style.Text_Meta)
            sheet.buttons("Remove device", danger = true) { remove(peer, deleteKept) }
            sheet.show()
        }
    }

    private fun remove(peer: PeerInfo, deleteKept: Boolean) {
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

    private fun setKeeping(peer: PeerInfo, keep: Boolean) {
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    if (keep) engine().addHolder(peer.fingerprint) else engine().removeHolder(peer.fingerprint)
                }
                app.say(if (keep) "${peer.name} keeps your Private Vault from the next sync" else "Stopped")
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
