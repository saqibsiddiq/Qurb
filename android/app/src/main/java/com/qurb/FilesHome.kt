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
import uniffi.qurb_mobile.PeerInfo

/**
 * Files (the owner's list of 2026-10-26, items 9 to 12): *This phone* -- its
 * Vault and what was sent to it; Recently deleted is in Settings -- then
 * *Remote access*: each computer, its circle's dot blue when it is on and red
 * when it is off, which opens to its *Shared* folder, reached live, or to
 * *Private*, this phone's own folder there, which only this phone opens. A
 * phone keeps no shared files since decision 0073, so nothing here is a
 * folder of the phone's own but its Vault.
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
            val peers = runCatching { withContext(Dispatchers.IO) { engine().peers() } }.getOrDefault(emptyList())
            show(peers)
            views.refresh.isRefreshing = false
        }
    }

    private fun show(peers: List<PeerInfo>) {
        val page = views.sections
        page.removeAllViews()

        kit.label(page, "This phone", top = 0)
        val places = LinearLayout(app).apply { orientation = LinearLayout.HORIZONTAL }
        places.addView(place(R.drawable.ic_lock_keyhole, "Vault") { app.push(FilesScreen(app, private = true)) },
            LinearLayout.LayoutParams(0, kit.dp(92), 1f))
        places.addView(place(R.drawable.ic_download, "Received") { app.push(ReceivedScreen(app)) },
            LinearLayout.LayoutParams(0, kit.dp(92), 1f).apply { marginStart = kit.dp(10) })
        page.addView(places)

        // Each computer: the owner's, and any this phone visits as a guest.
        val computers = peers.filter { it.relation == "host" || (it.relation == "own" && it.kind == "computer") }
        if (computers.isNotEmpty()) {
            kit.label(page, "Remote access", top = 20)
            // Rectangular cards, the device's icon in its tile and the dot by
            // its name, blue on and red off: not circles here (the owner,
            // 2026-10-26: "list the devices as it is listing right").
            for (computer in computers) {
                val off = Engine.isOff(app, computer)
                kit.deviceCard(page, States.device(computer.name, computer.kind), computer.name, off,
                    computer.lastSeen?.let { Words.since(it) }, end = R.drawable.ic_chevron_right) {
                    open(computer)
                }.contentDescription = "${computer.name}, ${if (off) "off" else "on"}"
            }
        }
    }

    /**
     * A computer's two folders, to choose between (item 11): its Shared
     * folder, and this phone's own private one there -- which a computer this
     * phone only visits as a guest keeps none of, so it opens Shared alone.
     */
    private fun open(computer: PeerInfo) {
        if (computer.relation == "host") return app.push(SharedScreen(app, computer))
        kit.sheet()
            .header(States.device(computer.name, computer.kind), computer.name, "")
            .action(R.drawable.ic_users, "Shared") {
                app.push(SharedScreen(app, computer))
            }
            .action(R.drawable.ic_lock_keyhole, "Private") {
                app.push(KeptScreen(app, computer))
            }
            .show()
    }

    /** One of this phone's places, as a tile (the redesign's Place tile):
     *  its icon on the soft green, its name under it. */
    private fun place(icon: Int, name: String, onTap: () -> Unit): View {
        val tile = LinearLayout(app).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            background = ContextCompat.getDrawable(app, R.drawable.card)
            foreground = ContextCompat.getDrawable(app, R.drawable.row_bg)
            contentDescription = name
            setOnClickListener { onTap() }
        }
        val mark = FrameLayout(app).apply { background = ContextCompat.getDrawable(app, R.drawable.tile_soft) }
        mark.addView(ImageView(app).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.green))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(kit.dp(22), kit.dp(22), Gravity.CENTER))
        tile.addView(mark, LinearLayout.LayoutParams(kit.dp(40), kit.dp(40)))
        tile.addView(TextView(app).apply {
            text = name
            setTextAppearance(R.style.Text_Label)
            isAllCaps = false
            letterSpacing = 0f
            textSize = 12f
            setTextColor(kit.color(R.color.text))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(8) })
        return tile
    }
}
