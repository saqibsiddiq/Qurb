package com.qurb

import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder
import uniffi.qurb_mobile.PeerInfo

/**
 * Putting backups back on this phone (the owner's review of 2026-11-02,
 * D-2): everything, WhatsApp, one folder, or files chosen -- from the
 * computer whose page it is. Each file goes back where it came from: into
 * WhatsApp's folder, the phone folder it was backed up from, or the Private
 * Vault. One already there is skipped (his answer to (a)).
 *
 * Runs in the app's own scope, so leaving the page does not stop it; its
 * progress shows in Home's *Waiting*. **Not yet a foreground job:** Android
 * may stop it if the app is left in the background for long, and it then has
 * to be started again, which skips what already came back.
 */
object Restore {

    /** How far a restore has got, for Home to show. */
    class Progress(val from: String, val what: String) {
        @Volatile var files = 0
        @Volatile var done = 0
        @Volatile var skipped = 0
        @Volatile var bytes = 0uL
        @Volatile var bytesDone = 0uL
        @Volatile var current = ""
    }

    @Volatile var running: Progress? = null
        private set
    private var job: Job? = null

    /** What to bring back: a folder of the phone's backup, under its top. */
    sealed class What(val words: String) {
        object Everything : What("Everything")
        object WhatsApp : What("WhatsApp")
        class Folder(val name: String) : What(name)
        class Files(val files: List<KeptFile>) : What("${files.size} files")
    }

    fun start(app: MainActivity, computer: PeerInfo, what: What) {
        if (running != null) {
            app.say("Already restoring")
            return
        }
        val progress = Progress(computer.name, what.words)
        running = progress
        app.changed()
        job = Engine.later.launch {
            var failed: Exception? = null
            try {
                val engine = Engine.open(app)
                val folder = engine.openKeptFolder(computer.fingerprint, 20u)
                val files = when (what) {
                    is What.Files -> what.files
                    else -> {
                        val top = folder.list("").folders.firstOrNull { !it.endsWith(", earlier") } ?: error("nothing kept for this phone")
                        when (what) {
                            What.Everything -> walk(folder, top)
                            What.WhatsApp -> walk(folder, "$top/WhatsApp")
                            is What.Folder -> walk(folder, "$top/${what.name}")
                            else -> emptyList()
                        }
                    }
                }
                progress.files = files.size
                progress.bytes = files.sumOf { it.size }
                val inVault = engine.list().mapTo(HashSet()) { it.path }
                for (file in files) {
                    progress.current = file.name
                    if (!one(app, engine, folder, file, inVault)) progress.skipped++
                    progress.done++
                    progress.bytesDone += file.size
                    if (progress.done % 5 == 0) app.runOnUiThread { app.changed() }
                }
            } catch (e: Exception) {
                failed = e
            } finally {
                running = null
                app.runOnUiThread {
                    val f = failed
                    if (f != null) app.fail("Restore stopped", f)
                    else app.say(when {
                        progress.done == 0 -> "Nothing to restore"
                        progress.skipped == progress.done -> "Everything was already on this phone"
                        progress.skipped > 0 -> "Restored ${Words.files(progress.done - progress.skipped)}, ${progress.skipped} already here"
                        else -> "Restored ${Words.files(progress.done)}"
                    })
                    app.madeChange()
                }
            }
        }
    }

    /** Asked first: from where, back where each file was, what is skipped. */
    fun ask(app: MainActivity, computer: PeerInfo, what: What, then: () -> Unit) {
        app.kit.sheet()
            .mark(R.drawable.ic_rotate_ccw, R.color.green_soft, R.color.green)
            .title("Restore ${if (what is What.Folder || what is What.WhatsApp) what.words else what.words.lowercase()}?", centred = true)
            .line("From ${computer.name}, back where each file was. Files already on this phone are skipped.")
            .buttons("Restore") {
                start(app, computer, what)
                then()
            }
            .show()
    }

    fun cancel() {
        job?.cancel()
    }

    /** Every file under `dir` of the computer's listing of this phone. */
    private fun walk(folder: KeptFolder, dir: String): List<KeptFile> {
        val listing = folder.list(dir)
        return listing.files + listing.folders.flatMap { walk(folder, "$dir/$it") }
    }

    /** One file put back; false when it was already there. */
    private fun one(app: MainActivity, engine: uniffi.qurb_mobile.Qurb, folder: KeptFolder, file: KeptFile, inVault: Set<String>): Boolean {
        val outside = file.path.startsWith(WHATSAPP) || file.path.startsWith(STORAGE)
        if (outside) {
            if (!Folders.allowed(app)) error("Allow QurbSpace to manage this phone's files first")
            val rest = if (file.path.startsWith(WHATSAPP)) "${WhatsApp.relative()}/${file.path.removePrefix(WHATSAPP)}"
                else file.path.removePrefix(STORAGE)
            val to = java.io.File("${Folders.root()}/$rest")
            if (to.exists()) return false
            to.parentFile?.mkdirs()
            folder.download(file, to.path)
            return true
        }
        if (file.path in inVault) return false
        val staging = java.io.File(app.cacheDir, "restore-${System.nanoTime()}")
        try {
            folder.download(file, staging.path)
            engine.importInto(staging.path, file.path, true)
        } finally {
            staging.delete()
        }
        return true
    }

    private const val WHATSAPP = ".qurb-outside/whatsapp/"
    private const val STORAGE = ".qurb-outside/storage/"
}
