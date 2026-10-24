package com.qurb

import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.PeerInfo

/**
 * Keeping this phone's Private Vault on a device, and stopping (decisions
 * 0036 and 0060), asked the same way from a device's page and from Backups:
 * a centred mark, a title, one line, two buttons of one width. However the
 * sheet closes, [refresh] runs, so a switch that opened it shows what is so.
 */
class Keeping(private val app: MainActivity, private val refresh: () -> Unit) {

    private val kit get() = app.kit

    /** Keep it there: on a device of this person's, or sealed on another person's computer. */
    fun keep(peer: PeerInfo) {
        val sheet = kit.sheet().mark(R.drawable.ic_lock_keyhole, R.color.green_soft, R.color.green)
        if (peer.relation == "host") {
            sheet.title("Keep your files on ${peer.name}?", centred = true)
                .line("Sealed on this phone first: whoever uses ${peer.name} can't open them or see their names.")
                .warning("It's their computer: they can delete what it keeps")
        } else {
            sheet.title("Keep your Private Vault on ${peer.name}?", centred = true)
                .line("Nobody using ${peer.name} sees it. This phone can then free its own copies.")
        }
        sheet.buttons(if (peer.relation == "host") "Keep them there" else "Keep it there") { set(peer, true) }
            .onDismiss { refresh() }
            .show()
    }

    fun stop(peer: PeerInfo) {
        kit.sheet()
            .mark(R.drawable.ic_lock_keyhole, R.color.neutral_bg, R.color.neutral)
            .title("Stop keeping it on ${peer.name}?", centred = true)
            .line("Nothing new goes there. What it has, it keeps.")
            .buttons("Stop") { set(peer, false) }
            .onDismiss { refresh() }
            .show()
    }

    private fun set(peer: PeerInfo, keep: Boolean) {
        app.lifecycleScope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val engine = Engine.open(app)
                    if (keep) engine.addHolder(peer.fingerprint) else engine.removeHolder(peer.fingerprint)
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
