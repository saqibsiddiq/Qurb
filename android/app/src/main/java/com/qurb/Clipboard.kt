package com.qurb

import android.content.Context
import uniffi.qurb_mobile.KeptFile
import uniffi.qurb_mobile.KeptFolder
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.Progress
import uniffi.qurb_mobile.Qurb

/**
 * What was copied, from any of the phone's folders, to paste in another (the
 * owner's list of 2026-11-01, item 5): a file or folder of this phone's Vault
 * -- Received's files are among them -- or of a computer, its *Shared* or
 * this phone's backup folder there. One at a time, for this run of the app.
 */
sealed class Clip(val name: String) {
    /** This phone's own Vault, by its path there. */
    class Vault(val path: String) : Clip(path.trimEnd('/').substringAfterLast('/'))

    /**
     * A computer's: in its *Shared* (`shared`), or this phone's backup folder
     * there. `dir` is the place as the folder lists it; `file` is null for a
     * folder.
     */
    class Remote(val computer: PeerInfo, val shared: Boolean, val dir: String, val file: KeptFile?) :
        Clip(file?.name ?: dir.trimEnd('/').substringAfterLast('/'))
}

/**
 * Pasting the clip. Where both ends are the same computer's *Shared*, the
 * computer copies it there and nothing crosses the connection; otherwise each
 * file comes to the phone, through its cache, and goes on from there.
 * Called off the main thread.
 */
object Clipboard {
    @Volatile
    var clip: Clip? = null

    fun copy(app: MainActivity, clip: Clip) {
        this.clip = clip
        app.say("Copied ${clip.name}")
    }

    /** Into this phone's Vault, in `dir` there. */
    fun intoVault(context: Context, engine: Qurb, dir: String) {
        when (val clip = clip ?: return) {
            is Clip.Vault -> engine.copyKept(clip.path, under(dir, clip.name))
            is Clip.Remote -> {
                val taken = engine.list().mapTo(HashSet()) { it.path }
                fromComputer(context, engine, clip) { local, rel ->
                    val to = free(taken, under(dir, rel))
                    engine.importInto(local.path, to, true)
                    taken += to
                }
            }
        }
    }

    /** Into `computer`'s *Shared*, in `dir` there, over `folder`. */
    fun intoShared(context: Context, engine: Qurb, folder: KeptFolder, computer: PeerInfo, dir: String) {
        val clip = clip ?: return
        if (clip is Clip.Remote && clip.shared && clip.computer.fingerprint == computer.fingerprint) {
            folder.copy(clip.dir, under(dir, clip.name))
            return
        }
        val send = { local: java.io.File, rel: String ->
            folder.addFile(local.path, under(dir, rel), object : Progress {
                override fun sent(bytes: ULong) {}
            })
            Unit
        }
        when (clip) {
            is Clip.Vault -> {
                val top = clip.path.trimEnd('/')
                val files = engine.list().filter { it.path == top || it.path.startsWith("$top/") }
                if (files.isEmpty()) throw IllegalStateException("${clip.name} is no longer in the Vault")
                for (entry in files) {
                    val rel = if (entry.path == top) clip.name else "${clip.name}/${entry.path.removePrefix("$top/")}"
                    val local = staged(context, rel)
                    try {
                        engine.export(entry.path, local.path)
                        send(local, rel)
                    } finally {
                        local.delete()
                    }
                }
            }
            is Clip.Remote -> fromComputer(context, engine, clip, send)
        }
    }

    /** Each file of a computer's clip, fetched to the cache and handed on
     *  with its place under the clip's name, then let go. */
    private fun fromComputer(context: Context, engine: Qurb, clip: Clip.Remote, then: (java.io.File, String) -> Unit) {
        val from = engine.openKeptFolder(clip.computer.fingerprint, 20u)
        fun one(file: KeptFile, rel: String) {
            val local = staged(context, rel)
            try {
                val went = java.io.File(from.download(file, local.path))
                try {
                    then(went, rel)
                } finally {
                    went.delete()
                }
            } finally {
                local.delete()
            }
        }
        fun walk(dir: String, rel: String) {
            val listing = if (clip.shared) from.shared(dir) else from.list(dir)
            for (file in listing.files) one(file, "$rel/${file.name}")
            for (sub in listing.folders) walk("$dir/$sub", "$rel/$sub")
        }
        val file = clip.file
        if (file != null) one(file, clip.name) else walk(clip.dir, clip.name)
    }

    private fun staged(context: Context, rel: String): java.io.File {
        val dir = java.io.File(context.cacheDir, "paste").apply { mkdirs() }
        return java.io.File(dir, "${System.nanoTime()}-${rel.substringAfterLast('/')}")
    }

    private fun under(dir: String, name: String) = if (dir.trim('/').isEmpty()) name else "${dir.trim('/')}/$name"

    /** `path`, or numbered as the computer numbers one: `beach (2).jpg`. */
    private fun free(taken: Set<String>, path: String): String {
        if (path !in taken) return path
        val slash = path.lastIndexOf('/')
        val (head, name) = path.substring(0, slash + 1) to path.substring(slash + 1)
        val dot = name.lastIndexOf('.')
        for (n in 2..1000) {
            val tried = if (dot > 0) "$head${name.substring(0, dot)} ($n)${name.substring(dot)}" else "$head$name ($n)"
            if (tried !in taken) return tried
        }
        throw IllegalStateException("too many files are called $name")
    }
}
