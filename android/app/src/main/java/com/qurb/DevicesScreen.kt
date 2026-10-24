package com.qurb

import android.view.View
import android.widget.LinearLayout
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.PeerInfo

/**
 * Devices, as redesigned (docs/design/redesign.md, Devices draft 4): each
 * device a card with its icon, its name, a dot for whether it is on, how long
 * since it answered, and small pills for what it keeps of this phone -- WhatsApp,
 * the folders with how many, a lock for the Private Vault. This phone is
 * listed apart, under them. A card opens the device's own page
 * ([DeviceScreen]); the round button at the top adds one.
 *
 * Nothing is selected here: the device Home shows is chosen on Home (decision
 * 0073, item 5).
 *
 * What a device keeps is what the engine does today: every device keeping
 * this phone's Private Vault keeps all of it, WhatsApp and the folders
 * included. A different set for each device (the owner's item 4 of
 * 2026-10-23) is engine work not done yet.
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
        // The round Add of the redesign: an icon on the soft green, no word.
        views.action.apply {
            text = ""
            contentDescription = "Add a device"
            setIconResource(R.drawable.ic_plus)
            iconSize = kit.dp(22)
            iconPadding = 0
            iconGravity = com.google.android.material.button.MaterialButton.ICON_GRAVITY_TEXT_START
            iconTint = android.content.res.ColorStateList.valueOf(kit.color(R.color.green))
            backgroundTintList = android.content.res.ColorStateList.valueOf(kit.color(R.color.green_soft))
            minWidth = kit.dp(44)
            minimumWidth = kit.dp(44)
            setPadding(0, 0, 0, 0)
            layoutParams = layoutParams.apply { width = kit.dp(44); height = kit.dp(44) }
            visibility = View.VISIBLE
            setOnClickListener { app.pair() }
        }
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(
        val peers: List<PeerInfo>,
        val holders: Set<String>,
        /** Folders of this phone chosen to back up. */
        val folders: Int,
    )

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    State(
                        engine.peers(),
                        engine.holders().map { it.fingerprint }.toSet(),
                        if (Folders.allowed(app)) engine.chosenFolders().size else 0,
                    )
                }
                val now = state.peers.map { it.fingerprint }.toSet()
                arrived = known?.let { now - it } ?: emptySet()
                known = now
                show(state)
            } catch (e: Exception) {
                app.fail("Could not read your devices", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(state: State) {
        val page = views.sections
        page.removeAllViews()
        val list = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        page.addView(list, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = kit.dp(12) })

        // This person's own devices, then other people's computers this phone
        // visits as a guest (decision 0060).
        val (own, visited) = state.peers.partition { it.relation == "own" }
        for (peer in own) card(list, peer, state)
        if (state.peers.isEmpty()) kit.addCard(list, "Add a device") { app.pair() }
        if (visited.isNotEmpty()) {
            kit.label(page, "Visiting", top = 28)
            val guests = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
            page.addView(guests)
            for (peer in visited) card(guests, peer, state)
        }

        kit.label(page, "This phone", top = 28)
        val here = LinearLayout(app).apply { orientation = LinearLayout.VERTICAL }
        page.addView(here)
        kit.deviceCard(here, R.drawable.ic_smartphone, android.os.Build.MODEL ?: "This phone",
            off = null, since = "This phone", onTap = null)
    }

    private fun card(parent: LinearLayout, peer: PeerInfo, state: State) {
        val keeps = if (peer.fingerprint in state.holders) {
            listOfNotNull(
                if (WhatsApp.tree(app) != null) Kit.Keeps(R.drawable.ic_whatsapp) else null,
                if (state.folders > 0) Kit.Keeps(R.drawable.ic_folder, state.folders.toString()) else null,
                Kit.Keeps(R.drawable.ic_lock_keyhole),
            )
        } else emptyList()
        val card = kit.deviceCard(parent, States.device(peer.name, peer.kind), peer.name,
            off = Engine.isOff(app, peer), since = peer.lastSeen?.let { Words.since(it) },
            keeps = keeps, end = R.drawable.ic_chevron_right) {
            app.push(DeviceScreen(app, peer))
        }
        if (peer.fingerprint in arrived && !Kit.calm()) materialise(card)
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
}
