package com.qurb

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.LayoutInflater
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.snackbar.Snackbar
import com.qurb.databinding.ActivityMainBinding
import com.qurb.databinding.RowFileBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.FileEntry
import uniffi.qurb_mobile.QurbException
import java.io.File
import java.text.DateFormat
import java.util.Date

/**
 * What is here, who it syncs with, and a button to make it happen.
 *
 * Deliberately not a file manager. Until there is a FileProvider, the synced
 * directory is private to this app and nothing else on the phone can see it, so
 * this screen is the only view onto it.
 */
class MainActivity : AppCompatActivity() {

    private lateinit var views: ActivityMainBinding
    private val files = FileAdapter()

    private val picker = registerForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri -> uri?.let { addFile(it) } }

    private val scanner = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        result.data?.getStringExtra(ScanActivity.EXTRA_CODE)?.let { joinWith(it) }
    }

    /** The file waiting for a destination, while the save dialog is open. */
    private var pendingSave: FileEntry? = null

    private val saver = registerForActivityResult(
        ActivityResultContracts.CreateDocument("*/*")
    ) { destination ->
        val entry = pendingSave
        pendingSave = null
        if (destination != null && entry != null) {
            writeCopy(entry, destination)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (!Engine.isSetUp(this)) {
            startActivity(Intent(this, SetupActivity::class.java))
            finish()
            return
        }

        views = ActivityMainBinding.inflate(layoutInflater)
        setContentView(views.root)
        setSupportActionBar(views.toolbar)
        insetContent()

        views.files.layoutManager = LinearLayoutManager(this)
        views.files.adapter = files

        // Registered here rather than in the setup screen: this runs on every
        // launch, and `KEEP` makes re-registering a no-op while still
        // re-establishing the work if the user cleared the app's data.
        SyncWorker.schedule(this)

        views.sync.setOnClickListener { sync() }
        views.add.setOnClickListener { picker.launch(arrayOf("*/*")) }
        views.refresh.setOnRefreshListener { refresh() }
    }

    /**
     * Keep the toolbar and the buttons out from under the system bars.
     *
     * Android 15 draws apps edge to edge whether they ask or not, so without
     * this the toolbar sits beneath the status bar — which looks wrong and,
     * worse, makes the overflow button half unreachable because taps in that
     * strip go to the status bar instead. The bug is invisible on a screenshot
     * until you try to press something.
     */
    private fun insetContent() {
        ViewCompat.setOnApplyWindowInsetsListener(views.root) { _, windowInsets ->
            val bars = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            // Padded on the bar rather than the toolbar. Padding the toolbar
            // pushes its contents down inside a box that does not grow, so the
            // title clips and the overflow button is squashed against the edge.
            views.appbar.updatePadding(top = bars.top)
            views.files.updatePadding(bottom = bars.bottom + FAB_CLEARANCE)

            // The floating buttons sit above the gesture bar rather than under it.
            listOf(views.add, views.sync).forEach { button ->
                (button.layoutParams as? android.view.ViewGroup.MarginLayoutParams)?.let { lp ->
                    lp.bottomMargin = bars.bottom + FAB_MARGIN
                    button.layoutParams = lp
                }
            }
            windowInsets
        }
    }

    override fun onResume() {
        super.onResume()
        if (Engine.isSetUp(this)) refresh()
    }

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.main, menu)
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean = when (item.itemId) {
        R.id.pair -> { pair(); true }
        R.id.peers -> { showPeers(); true }
        R.id.settings -> { showSettings(); true }
        R.id.background -> { showBackground(); true }
        else -> super.onOptionsItemSelected(item)
    }

    private fun refresh() {
        lifecycleScope.launch {
            try {
                val engine = Engine.open(this@MainActivity)
                val state = withContext(Dispatchers.IO) {
                    // A scan first: nothing delivers filesystem events to a
                    // process that was not running, so anything that changed
                    // while the app was closed produced no event at all.
                    engine.scan()
                    Snapshot(
                        engine.list(),
                        engine.usage(),
                        engine.peers().size,
                        engine.outstanding().files.size,
                    )
                }

                files.submit(state.listed)
                views.empty.visibility = if (state.listed.isEmpty()) View.VISIBLE else View.GONE
                views.summary.text = summary(
                    state.listed.size,
                    state.usage.logical,
                    state.usage.onDisk,
                    state.peers,
                    state.waiting,
                )
            } catch (e: Exception) {
                fail("Could not read the store", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    /** What one refresh read, so the IO block returns one thing rather than four. */
    private data class Snapshot(
        val listed: List<FileEntry>,
        val usage: uniffi.qurb_mobile.Usage,
        val peers: Int,
        val waiting: Int,
    )

    private fun summary(
        count: Int,
        logical: ULong,
        onDisk: ULong,
        peers: Int,
        waiting: Int,
    ): String {
        val saved = if (logical > 0uL) {
            " · ${size(onDisk)} on disk, from ${size(logical)}"
        } else ""
        val devices = when (peers) {
            0 -> "no paired devices"
            1 -> "1 paired device"
            else -> "$peers paired devices"
        }
        // Said last and said plainly. While this is non-zero, losing the phone
        // loses whatever is on it and nowhere else, and that is worth a line on
        // the screen rather than being left to infer from a sync that reported
        // no error.
        val outstanding = when (waiting) {
            0 -> ""
            1 -> "\n1 file is only on this phone — waiting for another device"
            else -> "\n$waiting files are only on this phone — waiting for another device"
        }
        return "$count file${if (count == 1) "" else "s"}$saved · $devices$outstanding"
    }

    private fun sync() {
        views.sync.isEnabled = false
        views.progress.visibility = View.VISIBLE

        lifecycleScope.launch {
            try {
                val engine = Engine.open(this@MainActivity)
                // 25 seconds: generous for a screen someone is watching, and
                // still inside what a background window would grant. The
                // deadline is the whole point of `syncWithin` — see
                // docs/decisions/0020-sync-takes-a-deadline.md.
                val outcome = withContext(Dispatchers.IO) {
                    engine.scan()
                    // Holding the multicast lock, or the phone cannot hear the
                    // devices on its own Wi-Fi answering.
                    Engine.hearingTheNetwork(this@MainActivity) { engine.syncWithin(25u) }
                }

                val message = when {
                    outcome.reached == 0u && outcome.unreachable == 0u ->
                        "No paired devices yet"
                    outcome.reached == 0u ->
                        "No device answered. They have to be awake and running at the same time."
                    outcome.adopted == 0u && outcome.conflicts == 0u ->
                        "Already up to date"
                    else -> buildString {
                        append("${outcome.adopted} file${if (outcome.adopted == 1u) "" else "s"}")
                        if (outcome.conflicts > 0u) append(", ${outcome.conflicts} conflict")
                        if (outcome.timedOut) append(" — ran out of time, sync again")
                    }
                }
                Snackbar.make(views.root, message, Snackbar.LENGTH_LONG).show()
                refresh()
            } catch (e: Exception) {
                fail("Sync failed", e)
            } finally {
                views.sync.isEnabled = true
                views.progress.visibility = View.GONE
            }
        }
    }

    /**
     * Pairing, by typing the code the other device shows.
     *
     * A QR scanner would be better and needs a camera dependency and a
     * permission; typing works today and the code is designed to be readable
     * aloud, which is the fallback anyway.
     */
    private fun pair() {
        MaterialAlertDialogBuilder(this)
            .setTitle("Pair with a device")
            .setMessage(
                "Run `qurb pair <dir>` on the other device. It shows a QR code — " +
                    "point the camera at it.\n\nThe code carries that device's full " +
                    "identity, which is why it travels across the room rather than " +
                    "over the network."
            )
            // Scanning first, because it is what anyone will actually do. A
            // pairing code is 107 characters; typing one is possible and
            // nobody does it twice.
            .setPositiveButton("Scan a code") { _, _ ->
                scanner.launch(Intent(this, ScanActivity::class.java))
            }
            .setNeutralButton("Type it instead") { _, _ -> typeCode() }
            .setNegativeButton("Cancel", null)
            .show()
    }

    /** The fallback, for a device with no camera or a refused permission. */
    private fun typeCode() {
        val input = EditText(this).apply {
            hint = "qurb1-..."
            setPadding(48, 32, 48, 8)
        }

        MaterialAlertDialogBuilder(this)
            .setTitle("Type the pairing code")
            .setView(input)
            .setPositiveButton("Pair") { _, _ ->
                val code = input.text.toString().trim()
                if (code.isNotEmpty()) joinWith(code)
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    /** Join, however the code arrived. */
    private fun joinWith(code: String) {
        lifecycleScope.launch {
            try {
                val peer = withContext(Dispatchers.IO) {
                    Engine.open(this@MainActivity).joinPairing(code)
                }
                Snackbar.make(views.root, "Paired with ${peer.name}", Snackbar.LENGTH_LONG).show()
                refresh()
            } catch (e: Exception) {
                fail("Pairing failed", e)
            }
        }
    }

    private fun showPeers() {
        lifecycleScope.launch {
            val peers = withContext(Dispatchers.IO) { Engine.open(this@MainActivity).peers() }
            val text = if (peers.isEmpty()) {
                "No paired devices.\n\nPair from the menu, using a code from `qurb pair`."
            } else {
                peers.joinToString("\n\n") { "${it.name}\n${it.short}" }
            }
            MaterialAlertDialogBuilder(this@MainActivity)
                .setTitle("Paired devices")
                .setMessage(text)
                .setPositiveButton("OK", null)
                .show()
        }
    }

    /**
     * Where the rendezvous service is.
     *
     * Exposed because there is no hosted one: to try this you run
     * `qurb signal` on a computer and point the phone at that machine.
     */
    private fun showSettings() {
        val input = EditText(this).apply {
            setText(Engine.signalUrl(this@MainActivity))
            setPadding(48, 32, 48, 8)
        }

        MaterialAlertDialogBuilder(this)
            .setTitle("Rendezvous service")
            .setMessage(
                "Two devices find each other through this. Run `qurb signal` on a " +
                    "computer and use ws://<that machine>:9000 — or ws://10.0.2.2:9000 " +
                    "from an emulator, which is how it reaches its host."
            )
            .setView(input)
            .setPositiveButton("Save") { _, _ ->
                Engine.setSignalUrl(this, input.text.toString().trim())
                Snackbar.make(views.root, "Saved", Snackbar.LENGTH_SHORT).show()
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    /**
     * What to do with a file the user tapped.
     *
     * Until this existed the list was inert: files synced to the phone and
     * there was no way to open one or get it anywhere else, which makes a sync
     * product that syncs into a hole. Both actions go through the app's own
     * `DocumentsProvider`, so there is one path out of the store rather than
     * two implementations of reading it.
     */
    private fun chooseAction(entry: FileEntry) {
        MaterialAlertDialogBuilder(this)
            .setTitle(entry.path.substringAfterLast('/'))
            .setItems(arrayOf("Open", "Save a copy to this phone")) { _, which ->
                when (which) {
                    0 -> openFile(entry)
                    1 -> saveCopy(entry)
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    /** Hand the file to whatever app handles its type. */
    private fun openFile(entry: FileEntry) {
        val uri = documentUri(entry.path)
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, mimeType(entry.path))
            // Without this the receiving app has no permission to read the URI
            // and fails with something that looks like a corrupt file.
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            startActivity(Intent.createChooser(intent, "Open with"))
        } catch (e: Exception) {
            fail("Nothing can open that file", e)
        }
    }

    /**
     * Copy a file out to wherever the user chooses.
     *
     * The synced directory is this app's private storage, so a file that lives
     * only there is invisible to everything else on the phone. This is how it
     * gets to Downloads, or a photo to the gallery, or anywhere the user
     * actually keeps things.
     */
    private fun saveCopy(entry: FileEntry) {
        pendingSave = entry
        try {
            saver.launch(entry.path.substringAfterLast('/'))
        } catch (e: Exception) {
            pendingSave = null
            fail("Could not open the save dialog", e)
        }
    }

    /**
     * What the background scheduler is doing, in the user's own words.
     *
     * Worth showing because the honest answer is "roughly every fifteen minutes,
     * when Android feels like it" — and an app that quietly does nothing for six
     * hours while claiming to sync is worse than one that says so.
     */
    private fun showBackground() {
        lifecycleScope.launch {
            val state = withContext(Dispatchers.IO) { SyncWorker.state(this@MainActivity) }
            MaterialAlertDialogBuilder(this@MainActivity)
                .setTitle("Background sync")
                .setMessage(
                    "$state\n\n" +
                        "Android decides when this actually runs. Fifteen minutes is the " +
                        "shortest period it accepts, and an idle phone may go much longer " +
                        "between attempts — it batches background work to save battery.\n\n" +
                        "Both devices have to be awake at the same moment for a sync to " +
                        "happen, so a computer that is switched off will be missed."
                )
                .setPositiveButton("OK", null)
                .setNeutralButton("Run one now") { _, _ ->
                    // Through the scheduler rather than directly, so this
                    // exercises the same path the periodic schedule uses.
                    SyncWorker.runNow(this@MainActivity)
                    Snackbar.make(
                        views.root,
                        "Queued. It will run when the conditions are met.",
                        Snackbar.LENGTH_LONG,
                    ).show()
                }
                .show()
        }
    }

    /**
     * Copy a file from elsewhere on the phone into the synced directory.
     *
     * Through a temporary file rather than in memory: the whole point of
     * `importFile` taking a path is that a large file never has to fit in the
     * heap. Streaming it out of the content resolver keeps that true.
     */
    private fun addFile(uri: Uri) {
        lifecycleScope.launch {
            try {
                Engine.importUri(this@MainActivity, uri)
                refresh()
            } catch (e: Exception) {
                fail("Could not add that file", e)
            }
        }
    }

    /**
     * Stream a stored file out to a location the user picked.
     *
     * Exported to a cache file first and copied from there, rather than held in
     * memory: `export` writes a chunk at a time precisely so a large file never
     * has to fit in the heap, and reading it back into a `ByteArray` here would
     * throw that away at the last step.
     */
    private fun writeCopy(entry: FileEntry, destination: Uri) {
        lifecycleScope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val staging = File(cacheDir, "save-${System.nanoTime()}")
                    try {
                        Engine.open(this@MainActivity).export(entry.path, staging.absolutePath)
                        staging.inputStream().use { input ->
                            contentResolver.openOutputStream(destination)?.use { output ->
                                input.copyTo(output)
                            } ?: error("could not open the destination")
                        }
                    } finally {
                        staging.delete()
                    }
                }
                Snackbar.make(views.root, "Saved a copy", Snackbar.LENGTH_LONG).show()
            } catch (e: Exception) {
                fail("Could not save that file", e)
            }
        }
    }

    /** This app's own document URI for a stored path. */
    private fun documentUri(path: String): Uri =
        android.provider.DocumentsContract.buildDocumentUri("com.qurb.documents", "qurb/$path")

    private fun mimeType(path: String): String {
        val extension = path.substringAfterLast('.', "").lowercase()
        return android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
            ?: "application/octet-stream"
    }

    private fun fail(title: String, e: Exception) {
        MaterialAlertDialogBuilder(this)
            .setTitle(title)
            // `readable()` rather than `e.message`: UniFFI generates
            // "detail=${detail}", which puts a struct field name in front of
            // the user, and the detail alone rarely says what to try next.
            .setMessage(if (e is QurbException) e.readable() else e.message ?: e.toString())
            .setPositiveButton("OK", null)
            .show()
    }

    private inner class FileAdapter : RecyclerView.Adapter<FileHolder>() {
        private var items: List<FileEntry> = emptyList()

        fun submit(next: List<FileEntry>) {
            items = next
            notifyDataSetChanged()
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
            FileHolder(
                RowFileBinding.inflate(LayoutInflater.from(parent.context), parent, false),
                ::chooseAction,
            )

        override fun onBindViewHolder(holder: FileHolder, position: Int) = holder.bind(items[position])
        override fun getItemCount() = items.size
    }

    private class FileHolder(
        private val views: RowFileBinding,
        private val onTap: (FileEntry) -> Unit,
    ) : RecyclerView.ViewHolder(views.root) {
        fun bind(entry: FileEntry) {
            views.root.setOnClickListener { onTap(entry) }
            views.name.text = entry.path
            views.detail.text = "${size(entry.size)} · ${when (entry.modifiedAt) {
                0L -> "unknown"
                // Nanoseconds since the epoch, which is what the index stores.
                else -> DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT)
                    .format(Date(entry.modifiedAt / 1_000_000))
            }}"
        }
    }

    private companion object {
        /** Room below the list so the last row is not hidden by the buttons. */
        const val FAB_CLEARANCE = 260
        const val FAB_MARGIN = 48

        fun size(bytes: ULong): String {
            val units = listOf("B", "KB", "MB", "GB", "TB")
            var value = bytes.toDouble()
            var unit = 0
            while (value >= 1024 && unit < units.size - 1) {
                value /= 1024
                unit++
            }
            return if (unit == 0) "${bytes} B" else String.format("%.1f %s", value, units[unit])
        }
    }
}

private fun size(bytes: ULong): String {
    val units = listOf("B", "KB", "MB", "GB", "TB")
    var value = bytes.toDouble()
    var unit = 0
    while (value >= 1024 && unit < units.size - 1) {
        value /= 1024
        unit++
    }
    return if (unit == 0) "$bytes B" else String.format("%.1f %s", value, units[unit])
}
