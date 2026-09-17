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
                val (listed, usage, peers) = withContext(Dispatchers.IO) {
                    // A scan first: nothing delivers filesystem events to a
                    // process that was not running, so anything that changed
                    // while the app was closed produced no event at all.
                    engine.scan()
                    Triple(engine.list(), engine.usage(), engine.peers().size)
                }

                files.submit(listed)
                views.empty.visibility = if (listed.isEmpty()) View.VISIBLE else View.GONE
                views.summary.text = summary(listed.size, usage.logical, usage.onDisk, peers)
            } catch (e: Exception) {
                fail("Could not read the store", e)
            } finally {
                views.refresh.isRefreshing = false
            }
        }
    }

    private fun summary(count: Int, logical: ULong, onDisk: ULong, peers: Int): String {
        val saved = if (logical > 0uL) {
            " · ${size(onDisk)} on disk, from ${size(logical)}"
        } else ""
        val devices = when (peers) {
            0 -> "no paired devices"
            1 -> "1 paired device"
            else -> "$peers paired devices"
        }
        return "$count file${if (count == 1) "" else "s"}$saved · $devices"
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
                    engine.syncWithin(25u)
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
        val input = EditText(this).apply {
            hint = "qurb1-..."
            setPadding(48, 32, 48, 8)
        }

        MaterialAlertDialogBuilder(this)
            .setTitle("Pair with a device")
            .setMessage("Run `qurb pair <dir>` on the other device and type the code it shows.")
            .setView(input)
            .setPositiveButton("Pair") { _, _ ->
                val code = input.text.toString().trim()
                if (code.isEmpty()) return@setPositiveButton

                lifecycleScope.launch {
                    try {
                        val peer = withContext(Dispatchers.IO) {
                            Engine.open(this@MainActivity).joinPairing(code)
                        }
                        Snackbar.make(views.root, "Paired with ${peer.name}", Snackbar.LENGTH_LONG)
                            .show()
                        refresh()
                    } catch (e: Exception) {
                        fail("Pairing failed", e)
                    }
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
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
                val name = displayName(uri)
                withContext(Dispatchers.IO) {
                    val staging = File(cacheDir, name)
                    contentResolver.openInputStream(uri).use { input ->
                        staging.outputStream().use { output ->
                            requireNotNull(input) { "could not read that file" }.copyTo(output)
                        }
                    }
                    Engine.open(this@MainActivity).importFile(staging.absolutePath, name)
                    staging.delete()
                }
                refresh()
            } catch (e: Exception) {
                fail("Could not add that file", e)
            }
        }
    }

    private fun displayName(uri: Uri): String {
        contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val column = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (column >= 0 && cursor.moveToFirst()) {
                cursor.getString(column)?.let { return it }
            }
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "file"
    }

    private fun fail(title: String, e: Exception) {
        MaterialAlertDialogBuilder(this)
            .setTitle(title)
            .setMessage(e.message ?: e.toString())
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
            FileHolder(RowFileBinding.inflate(LayoutInflater.from(parent.context), parent, false))

        override fun onBindViewHolder(holder: FileHolder, position: Int) = holder.bind(items[position])
        override fun getItemCount() = items.size
    }

    private class FileHolder(private val views: RowFileBinding) :
        RecyclerView.ViewHolder(views.root) {
        fun bind(entry: FileEntry) {
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
