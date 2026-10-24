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
 * Files, as Files draft 4 has it (decision 0073): *This phone* -- its Vault,
 * what was sent to it, what was deleted -- then each computer's *Shared*
 * folder, reached live, then *Devices*, this phone's folder on each of its
 * computers. A phone keeps no shared files since 0073, so nothing here is a
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
        places.addView(place(R.drawable.ic_trash_2, "Deleted") { app.push(DeletedScreen(app)) },
            LinearLayout.LayoutParams(0, kit.dp(92), 1f).apply { marginStart = kit.dp(10) })
        page.addView(places)

        // Each computer's shared folder: the owner's computers, and any this
        // phone visits.
        val computers = peers.filter { it.relation == "host" || (it.relation == "own" && it.kind == "computer") }
        if (computers.isNotEmpty()) {
            kit.label(page, "Shared", top = 20)
            for (computer in computers) {
                kit.deviceCard(page, R.drawable.ic_users, "Shared", Engine.isOff(app, computer), null,
                    keeps = listOf(Kit.Keeps(States.device(computer.name, computer.kind), computer.name)),
                    end = R.drawable.ic_chevron_right) { app.push(SharedScreen(app, computer)) }
                    .contentDescription = "Shared, on ${computer.name}"
            }
        }

        // This phone's folder on each of its own computers.
        val own = computers.filter { it.relation == "own" }
        if (own.isNotEmpty()) {
            kit.label(page, "Devices", top = 20)
            for (computer in own) {
                val off = Engine.isOff(app, computer)
                kit.deviceCard(page, States.device(computer.name, computer.kind), computer.name, off,
                    computer.lastSeen?.let { Words.since(it) }, end = R.drawable.ic_chevron_right) {
                    app.push(KeptScreen(app, computer))
                }.contentDescription = "${computer.name}, ${if (off) "off" else "on"}"
            }
        }
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
