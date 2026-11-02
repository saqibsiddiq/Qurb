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
    // Reached from Home's circle since the Devices tab went (the owner's
    // list of 2026-10-26, item 15).
    override val tab = R.id.tab_home

    private val backups = Backups(app) { refresh() }
    private val keeping = Keeping(app) { refresh() }

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Home"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.visibility = View.GONE
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    private class State(
        val peer: PeerInfo?,
        /** It keeps this phone's backups (a holder). */
        val keeps: Boolean,
        /** And the Private Vault's own files too (D-3). */
        val vaultFiles: Boolean,
        val whatsApp: BackupState,
        val folders: Int,
    )

    override fun refresh() {
        refreshing {
            try {
                val state = withContext(Dispatchers.IO) {
                    val engine = engine()
                    State(
                        engine.peers().firstOrNull { it.fingerprint == peer.fingerprint }
                            ?.also { Devices.load(engine, listOf(it)) },
                        engine.holders().any { it.fingerprint == peer.fingerprint },
                        runCatching { engine.keepsVaultFiles(peer.fingerprint) }.getOrDefault(false),
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
        // Its circle, as Home's: its picture or first letter, and its dot.
        head.addView(kit.avatar(peer.name, off, Devices.picture(peer), 64),
            LinearLayout.LayoutParams(kit.dp(72), kit.dp(72)).apply { marginEnd = kit.dp(14) })
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

        // No Send here: Home's is the one (the owner's list of 2026-10-26, item 5).

        // How it is reached from away (decision 0082): from anywhere, once
        // the phone has; or the plain reason it is not, and what helps.
        reach(page)

        kit.label(page, "Backs up")
        val card = kit.card(page)
        // What goes to it (the owner's review of 2026-11-02, D-3): the folders
        // and WhatsApp, always offered on one's own computer, each making it
        // keep the backups if it did not; and the Private Vault's own files,
        // a switch of their own -- off, the backups go on.
        if (!host) {
            kit.setting(card, R.drawable.ic_folder, "Folders",
                if (state.keeps && state.folders > 0) state.folders.toString() else "") {
                backUpHere { app.push(FoldersScreen(app, peer.name, R.id.tab_home)) }
            }
            val backing = state.keeps && WhatsApp.tree(app) != null
            kit.setting(card, R.drawable.ic_whatsapp, "WhatsApp",
                if (backing && state.whatsApp.bytes > 0uL) Words.size(state.whatsApp.bytes) else "",
                on = backing, onChange = { on ->
                    if (on) backUpHere { backups.whatsApp(state.whatsApp, peer.name, backing = false) }
                    else backups.stopWhatsApp(peer.name)
                    // The switch shows what is so, not what was tapped: on once
                    // WhatsApp's folder is chosen, off once stopping is confirmed.
                    refresh()
                })
            kit.setting(card, R.drawable.ic_lock_keyhole, "Private Vault", on = state.vaultFiles, onChange = { on ->
                scope.launch {
                    runCatching { withContext(Dispatchers.IO) { engine().setKeepsVaultFiles(peer.fingerprint, on) } }
                        .onFailure { app.fail("Could not change that", it) }
                    app.madeChange()
                }
            })
        } else {
            // Another person's computer: the vault sealed there, as before.
            kit.setting(card, R.drawable.ic_lock_keyhole, "Private Vault", on = state.keeps, onChange = { on ->
                if (on) keeping.keep(peer) else keeping.stop(peer)
            })
        }

        // Restore (D-2): from this computer, what it keeps of this phone put
        // back where it came from; a file already here is skipped.
        if (state.keeps) restore(page)

        // In red, and behind the phone's own lock (items 5 and 18); then
        // another device chosen, as holding Home's circle does (D-1).
        page.addView(kit.button(if (host) "Stop visiting" else "Remove", Kit.Style.DANGER) { askToRemove() },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)).apply { topMargin = kit.dp(20) })
        page.addView(kit.button("Switch device", Kit.Style.BLUE) {
            scope.launch {
                val peers = runCatching { withContext(Dispatchers.IO) { engine().peers() } }.getOrDefault(emptyList())
                kit.chooseDevice(peers, peer) { app.onBackPressedDispatcher.onBackPressed() }
            }
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(48)).apply { topMargin = kit.dp(10) })
    }

    /** Make it keep this phone's backups, without the Vault's own files if it
     *  did not keep them, then `then`. */
    private fun backUpHere(then: () -> Unit) {
        scope.launch {
            val ok = runCatching {
                withContext(Dispatchers.IO) {
                    val engine = engine()
                    if (engine.holders().none { it.fingerprint == peer.fingerprint }) engine.setKeepsVaultFiles(peer.fingerprint, false)
                }
            }.onFailure { app.fail("Could not change that", it) }.isSuccess
            if (ok) {
                app.madeChange()
                then()
            }
        }
    }

    /** Restore: three options in one card (the owner, 2026-11-02) --
     *  everything; Folders, a page of every folder backed up to pick from;
     *  and Files, the backup folder to pick files in. */
    private fun restore(page: LinearLayout) {
        kit.label(page, "Restore")
        val card = kit.card(page)
        val going = Restore.running
        if (going != null) {
            kit.setting(card, R.drawable.ic_rotate_ccw, "Restoring ${going.what}", "${going.done} of ${going.files}")
            return
        }
        kit.setting(card, R.drawable.ic_rotate_ccw, "Everything") { ask(Restore.What.Everything) }
        kit.setting(card, R.drawable.ic_folder, "Folders") { app.push(RestoreFolders(app, peer)) }
        kit.setting(card, R.drawable.ic_file, "Files") {
            scope.launch {
                val top = runCatching {
                    withContext(Dispatchers.IO) {
                        engine().openKeptFolder(peer.fingerprint, 20u).list("").folders.firstOrNull { !it.endsWith(", earlier") }
                    }
                }.getOrNull()
                if (top != null) app.push(FolderPage(app, BackupPlace(peer, top), peer.name, top))
                else app.say("${peer.name} keeps nothing for this phone yet")
            }
        }
    }

    private fun ask(what: Restore.What) = Restore.ask(app, peer, what) { refresh() }


    /** One line on how it is reached from away from home: "Reachable from
     *  anywhere" once the phone has done it; when it answered and nothing got
     *  through, why, and the one thing that usually helps. Nothing while it
     *  is not known. */
    private fun reach(page: LinearLayout) {
        scope.launch {
            val state = runCatching { withContext(Dispatchers.IO) { engine().reachState(peer.fingerprint) } }.getOrDefault("")
            val view = when (state) {
                "anywhere" -> kit.chip(R.drawable.ic_circle_check, "Reachable from anywhere")
                "blocked" -> Reach.blocked(app, peer.name)
                else -> return@launch
            }
            // Under the header, before Backs up.
            page.addView(view, 1, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                .apply { topMargin = kit.dp(14) })
        }
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
            sheet.buttons(if (host) "Stop" else "Remove", danger = true) { confirmed { remove(deleteKept) } }
            sheet.show()
        }
    }

    /**
     * The person confirming it is them before a device is removed: the
     * phone's fingerprint, face or screen lock (the owner's list of
     * 2026-10-26, item 18). A phone with no screen lock has nothing to ask
     * with, and the sheet's own *Remove* is the answer.
     */
    private fun confirmed(then: () -> Unit) {
        if (!ScreenLock.available(app)) return then()
        ScreenLock.confirm(app, if (peer.relation == "host") "Stop visiting ${peer.name}" else "Remove ${peer.name}",
            "Confirm it's you") { yes -> if (yes) then() }
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
}

/**
 * Restore › Folders: every folder this phone backed up to `computer`,
 * WhatsApp among them, each a row; one tapped is restored, asked first.
 */
class RestoreFolders(app: MainActivity, private val computer: PeerInfo) : Screen(app) {

    private val views = com.qurb.databinding.ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_home

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = computer.name
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Folders"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        val page = views.sections
        scope.launch {
            val names = runCatching {
                withContext(Dispatchers.IO) {
                    val open = engine().openKeptFolder(computer.fingerprint, 20u)
                    val top = open.list("").folders.firstOrNull { !it.endsWith(", earlier") } ?: return@withContext emptyList()
                    // The Private Vault is the Vault: put back with everything.
                    open.list(top).folders.filter { it != "Private Vault" }
                }
            }
            page.removeAllViews()
            val list = names.getOrNull()
            when {
                list == null -> kit.empty(page, R.drawable.ic_cloud_off, "${computer.name} isn't reachable", "Try again") { refresh() }
                list.isEmpty() -> kit.empty(page, R.drawable.ic_folder_open, "No folders backed up yet")
                else -> {
                    val card = kit.card(page, top = 16)
                    for (name in list.sortedBy { it.lowercase() }) {
                        val icon = if (name == "WhatsApp") R.drawable.ic_whatsapp else R.drawable.ic_folder
                        kit.setting(card, icon, name) {
                            Restore.ask(app, computer, if (name == "WhatsApp") Restore.What.WhatsApp else Restore.What.Folder(name)) {
                                app.onBackPressedDispatcher.onBackPressed()
                            }
                        }
                    }
                }
            }
            views.refresh.isRefreshing = false
        }
    }
}

