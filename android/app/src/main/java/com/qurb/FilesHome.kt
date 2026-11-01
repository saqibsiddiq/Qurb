package com.qurb

import android.view.View
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.ComputerPlace
import uniffi.qurb_mobile.PeerInfo

/**
 * Files (the owner's list of 2026-11-01, items 1 to 4 and 10): every device,
 * as a card -- this phone first, with its picture, then each computer with
 * its own picture and its dot, blue on and red off. A tap opens the device:
 * this phone's two folders, *Vault* and *Received*; a computer's folder as
 * its `~/qurb` has it -- *Shared*, this phone's own folder under the phone's
 * name, and other people's, locked. Every folder the same card, with no
 * words under it.
 */
class FilesHome(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    init {
        views.title.text = "Files"
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        scope.launch {
            val (me, peers) = runCatching {
                withContext(Dispatchers.IO) {
                    val engine = engine()
                    val peers = engine.peers()
                    Devices.load(engine, peers)
                    engine.ownProfile() to peers
                }
            }.getOrNull() ?: return@launch
            show(me.name.ifEmpty { android.os.Build.MODEL ?: "This phone" }, me.picture?.let { Devices.decode(it) }, peers)
            views.refresh.isRefreshing = false
        }
        // The computers reached now, in the background, so that opening one
        // finds it connected (item 7).
        Engine.later.launch { runCatching { engine().prepareComputers(8u) } }
    }

    private fun show(name: String, picture: android.graphics.Bitmap?, peers: List<PeerInfo>) {
        val page = views.sections
        page.removeAllViews()
        // This phone, which is always on: no dot.
        kit.deviceCard(page, R.drawable.ic_smartphone, name, null, null,
            end = R.drawable.ic_chevron_right, lead = kit.avatar(name, null, picture, 48)) {
            app.push(PhoneFolders(app, name))
        }
        // Each computer: the person's own, and any this phone visits.
        val computers = peers.filter { it.relation == "host" || (it.relation == "own" && it.kind != "phone") }
        for (computer in computers) {
            val off = Engine.isOff(app, computer)
            // Its dot on its picture, as Home draws it: once, not again by the name.
            kit.deviceCard(page, States.device(computer.name, computer.kind), computer.name, null, null,
                end = R.drawable.ic_chevron_right, lead = kit.avatar(computer.name, off, Devices.picture(computer), 48)) {
                app.push(ComputerFolders(app, computer))
            }.contentDescription = "${computer.name}, ${if (off) "off" else "on"}"
        }
    }
}

/** This phone's two folders, as cards: its Vault, and what was sent to it. */
class PhoneFolders(app: MainActivity, private val name: String) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Files"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = name
        views.refresh.isEnabled = false
    }

    override fun refresh() {
        val page = views.sections
        page.removeAllViews()
        kit.folderGrid(page, listOf(
            kit.folderCard("Vault", R.drawable.ic_lock_keyhole) { app.push(FilesScreen(app, private = true)) },
            kit.folderCard("Received", R.drawable.ic_download) { app.push(ReceivedScreen(app)) },
        ))
    }
}

/**
 * A computer's folder as its `~/qurb` has it (items 2 and 3): *Shared*; this
 * phone's own folder, named as the phone is; and other people's, locked to
 * this phone. Asked of the computer over the connection kept for it, so
 * opening it is quick once Files has reached it.
 */
class ComputerFolders(app: MainActivity, private val computer: PeerInfo) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_files

    /** What it held last time, shown at once while it is asked again. */
    private var places: List<ComputerPlace>? = Shown.places[computer.fingerprint]

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Files"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = computer.name
        views.refresh.setColorSchemeResources(R.color.green)
        views.refresh.setOnRefreshListener { refresh() }
    }

    override fun refresh() {
        places?.let { show(it) } ?: run {
            views.sections.removeAllViews()
            kit.empty(views.sections, R.drawable.ic_monitor_smartphone, "Reaching ${computer.name}…")
        }
        scope.launch {
            try {
                val got = withContext(Dispatchers.IO) {
                    Engine.hearingTheNetwork(app) { engine().openKeptFolder(computer.fingerprint, 20u) }.places()
                }
                places = got
                Shown.places[computer.fingerprint] = got
                show(got)
            } catch (e: Exception) {
                if (places == null) {
                    views.sections.removeAllViews()
                    kit.empty(views.sections, R.drawable.ic_cloud_off,
                        "${computer.name} isn't reachable. It has to be on with Qurb running, and this phone online.",
                        "Try again") { refresh() }
                }
                android.util.Log.w("qurb", "could not reach ${computer.name}", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun show(places: List<ComputerPlace>) {
        val page = views.sections
        page.removeAllViews()
        kit.folderGrid(page, places.map { place ->
            when (place.kind) {
                "shared" -> kit.folderCard(place.name, R.drawable.ic_folder) { app.push(SharedScreen(app, computer)) }
                "mine" -> kit.folderCard(place.name, R.drawable.ic_smartphone) {
                    // From a computer too old to name it: the folder's top.
                    app.push(KeptScreen(app, computer, place.name.takeIf { it != "Private" }))
                }
                else -> kit.folderCard(place.name, R.drawable.ic_lock_keyhole) {
                    app.say("${place.name}'s folder. Only they can open it.")
                }.apply { alpha = 0.55f }
            }
        })
    }

    /** What each computer held at its top, as last seen, for this run of the app. */
    private object Shown {
        val places = java.util.concurrent.ConcurrentHashMap<String, List<ComputerPlace>>()
    }
}
