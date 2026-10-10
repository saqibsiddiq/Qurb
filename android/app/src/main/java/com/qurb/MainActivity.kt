package com.qurb

import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.animation.LinearInterpolator
import android.widget.EditText
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.qurb.databinding.ActivityMainBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.FileEntry
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.SyncOutcome
import uniffi.qurb_mobile.Waiting
import uniffi.qurb_mobile.pairingNumber
import java.io.File

/**
 * The app: four places under a tab bar (direction §25), the few places
 * reached from them, and the actions more than one of them offers.
 *
 * Home says whether this phone's Qurb space is okay; Files is what is in it,
 * with Private Vault a step inside; Devices is who it knows; Settings is the
 * rest. The screens are in their own files. What lives here is what they
 * share -- pairing, syncing, adding, opening, saving, sending, transfers --
 * because each needs an activity to launch a picker or a camera from, and
 * there is one.
 */
class MainActivity : AppCompatActivity() {

    private lateinit var views: ActivityMainBinding
    lateinit var kit: Kit
        private set

    private val screens = mutableMapOf<Int, Screen>()

    /** Places reached from a tab's place, newest last: Back leaves them. */
    private val stack = ArrayDeque<Screen>()
    private var current: Screen? = null

    /** Set while the tab bar is moved to match a pushed place, so moving it
     *  does not also go there. */
    private var quietly = false

    /** Whether a sync is running: one this screen started, or a pass in the
     *  background worker. Home shows it. */
    val syncing get() = syncingHere || syncingInWorker

    /** A sync this screen started. */
    private var syncingHere = false

    /**
     * A pass in the background worker: the schedule's, or a long one handed
     * to it (decision 0050). Watched while the app is on screen, so it shows
     * here as syncing and the screen redraws when it ends. Until 2026-10-05
     * it was not, and Home said "Everything is synced" for the three minutes
     * a 765 MB file was leaving.
     */
    private var syncingInWorker = false

    /** Asked for while a sync was running: another runs when it ends, so a
     *  change made part-way through is not left for the hour after. Quiet
     *  unless any of the asks was somebody pressing Sync now. */
    private var again = false
    private var againQuiet = true

    /** What to do when the running sync ends, and when the one after it does. */
    private val afterThisSync = mutableListOf<suspend (Synced) -> Unit>()
    private val afterNextSync = mutableListOf<suspend (Synced) -> Unit>()

    /** How a sync went: its outcome, or null if it failed -- or that it was
     *  handed to the worker to run in the foreground (decision 0050). */
    class Synced(val outcome: SyncOutcome?, val inBackground: Boolean)

    private val notifyAsk = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { }

    /** Whether this launch has freed what nothing needs yet. Once is enough. */
    private var housekept = false

    /** Where files being picked go, while the picker is open. */
    private var addingInto = ""
    private var addingPrivate = false

    private val adder = registerForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments()
    ) { uris -> if (uris.isNotEmpty()) addFiles(uris) }

    /** The device files are being picked for, while the picker is open. */
    private var sendingTo: PeerInfo? = null

    private val sender = registerForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments()
    ) { uris ->
        val to = sendingTo
        sendingTo = null
        if (to != null && uris.isNotEmpty()) sendPicked(uris, to)
    }

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
        if (destination != null && entry != null) writeCopy(entry, destination)
    }

    /** Files waiting for a folder to be saved into, while the picker is open. */
    private var pendingSaveAll: List<FileEntry> = emptyList()

    private val folderSaver = registerForActivityResult(
        ActivityResultContracts.OpenDocumentTree()
    ) { tree ->
        val entries = pendingSaveAll
        pendingSaveAll = emptyList()
        if (tree != null && entries.isNotEmpty()) writeAll(entries, tree)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Not set up, or set up and closed before the 24 words were typed
        // back: the setup screen knows which, and picks up from there.
        if (!Engine.isSetUp(this) || !Engine.phraseConfirmed(this)) {
            startActivity(Intent(this, SetupActivity::class.java))
            finish()
            return
        }

        views = ActivityMainBinding.inflate(layoutInflater)
        kit = Kit(this)
        setContentView(views.root)
        insetContent()

        // Registered on every launch: `KEEP` makes it a no-op when the work is
        // already scheduled, and re-establishes it if the app's data was
        // cleared.
        SyncWorker.schedule(this)

        val tab = savedInstanceState?.getInt(TAB)?.takeIf { it in TABS } ?: R.id.tab_home
        views.tabs.selectedItemId = tab
        views.tabs.setOnItemSelectedListener { item ->
            if (!quietly) {
                stack.clear()
                show(screenFor(item.itemId))
                current?.refresh()
            }
            true
        }
        views.tabs.setOnItemReselectedListener {
            if (stack.isNotEmpty()) {
                stack.clear()
                show(screenFor(it.itemId))
            }
            current?.refresh()
        }
        views.transfersBar.setOnClickListener { openTransfers() }
        // The screen first -- a folder goes up -- then a place reached from
        // another goes back to it, then the usual.
        onBackPressedDispatcher.addCallback(this, object : androidx.activity.OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                if (current?.back() == true) return
                if (stack.isNotEmpty()) {
                    pop()
                    return
                }
                isEnabled = false
                onBackPressedDispatcher.onBackPressed()
                isEnabled = true
            }
        })
        // Not refreshed here: onResume follows, and refreshes whatever is showing.
        show(screenFor(tab))

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                SyncWorker.running(this@MainActivity).collect { workerRunning(it) }
            }
        }
        continueSending(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        continueSending(intent)
    }

    /**
     * Opened from the notification a refused large transfer leaves
     * ([Transfers.paused]): sync now, while the app is on screen and Android
     * lets the transfer carry on in the foreground.
     */
    private fun continueSending(intent: Intent?) {
        if (intent?.getBooleanExtra(Transfers.CONTINUE, false) != true) return
        intent.removeExtra(Transfers.CONTINUE)
        Transfers.clearPaused(this)
        sync()
    }

    private fun workerRunning(running: Boolean) {
        if (running == syncingInWorker) return
        syncingInWorker = running
        changed()
        if (!running) askedMeanwhile()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        if (::views.isInitialized) outState.putInt(TAB, views.tabs.selectedItemId)
    }

    override fun onResume() {
        super.onResume()
        Notices.onScreen = true
        if (!::views.isInitialized) return
        current?.refresh()
        updateDock()
        catchUp()
    }

    override fun onPause() {
        super.onPause()
        Notices.onScreen = false
    }

    private fun screenFor(tab: Int): Screen = screens.getOrPut(tab) {
        when (tab) {
            R.id.tab_files -> FilesScreen(this, private = false)
            R.id.tab_devices -> DevicesScreen(this)
            R.id.tab_settings -> SettingsScreen(this)
            else -> HomeScreen(this)
        }
    }

    private fun show(screen: Screen) {
        if (screen === current) return
        views.screen.removeAllViews()
        (screen.view.parent as? android.view.ViewGroup)?.removeView(screen.view)
        views.screen.addView(screen.view)
        current = screen
        // §35: the next place comes forward a little; nothing flies.
        if (!Kit.calm()) {
            screen.view.alpha = 0f
            screen.view.translationY = kit.dp(8).toFloat()
            screen.view.animate().alpha(1f).translationY(0f).setDuration(280)
                .setInterpolator(android.view.animation.DecelerateInterpolator(2f)).start()
        }
        if (views.tabs.selectedItemId != screen.tab) {
            quietly = true
            views.tabs.selectedItemId = screen.tab
            quietly = false
        }
    }

    /** Go to a place reached from another: Private Vault, Activity, Recently
     *  deleted. Back returns. */
    fun push(screen: Screen) {
        stack.addLast(screen)
        show(screen)
        screen.refresh()
    }

    private fun pop() {
        stack.removeLast()
        show(stack.lastOrNull() ?: screenFor(views.tabs.selectedItemId))
        current?.refresh()
    }

    /** Move to another tab, as a screen's action does ("Choose a device"). */
    fun go(tab: Int) {
        stack.clear()
        if (views.tabs.selectedItemId == tab) show(screenFor(tab)) else views.tabs.selectedItemId = tab
    }

    /**
     * Keep the screens out from under the status bar, and the floating tab
     * bar above the gesture bar.
     *
     * Android 15 draws every app edge to edge, so without this a heading sits
     * beneath the status bar and taps near the top go to the system instead.
     */
    private fun insetContent() {
        ViewCompat.setOnApplyWindowInsetsListener(views.root) { _, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            views.screen.updatePadding(top = bars.top, left = bars.left, right = bars.right)
            views.dock.updatePadding(bottom = bars.bottom + kit.dp(10))
            insets
        }
        // The tab bar pads itself for the gesture bar when it is at the edge;
        // floating, it is already above it.
        ViewCompat.setOnApplyWindowInsetsListener(views.tabs) { _, insets -> insets }
    }

    /**
     * Whatever changed while the app was away.
     *
     * The screen has already drawn what the index knows. Then the scan, because
     * nothing delivers filesystem events to a process that was not running;
     * redrawn only if it found something. Then, once per launch, freeing what
     * nothing needs -- the routine the background worker runs after each sync,
     * here too so a phone the worker has not reached lately does not wait.
     */
    private fun catchUp() {
        lifecycleScope.launch {
            try {
                val engine = Engine.open(this@MainActivity)
                val found = withContext(Dispatchers.IO) { engine.scan() }
                if (found.stored > 0u || found.deleted > 0u) current?.refresh()
                Engine.keepKeyOnce(this@MainActivity)

                if (!housekept) {
                    housekept = true
                    val tidied = withContext(Dispatchers.IO) {
                        runCatching { engine.housekeep() }.getOrNull()
                    }
                    if (tidied != null && tidied.freed > 0uL) current?.refresh()
                }
            } catch (e: Exception) {
                Words.fail(this@MainActivity, "Could not read this phone's files", e)
            }
        }
    }

    /** Something every screen might need to say. */
    fun say(message: String) = Words.say(views.root, message, above = views.dock)

    fun fail(title: String, e: Throwable) = Words.fail(this, title, e)

    /** After anything that changes what the engine knows. */
    fun changed() {
        current?.refresh()
        updateDock()
    }

    /**
     * After a change made here that the other devices should have: files
     * added, renamed, moved or deleted, a device chosen to keep a backup, a
     * device added. Redraws, and starts a sync now.
     *
     * A phone has no daemon to notice a change and tell the others, the way
     * the desktop does. Without this, a change made in the app waited for the
     * next background pass -- an hour away once push is working -- or for
     * somebody to press Sync now, which is what people found themselves doing
     * after every send.
     */
    fun madeChange() {
        changed()
        sync(quiet = true)
    }

    // ----------------------------------------------------------- transfers

    private var turning: ObjectAnimator? = null

    /**
     * The Transfers bar (§21, §25): there while this phone is syncing or has
     * sent something not yet collected, and gone otherwise. A phone sends in
     * short background windows, so it says what is waiting rather than a
     * percentage it cannot know (docs/design/brief.md §2).
     */
    fun updateDock() {
        lifecycleScope.launch {
            val waiting = withContext(Dispatchers.IO) {
                runCatching { Engine.open(this@MainActivity).waiting() }.getOrDefault(emptyList())
            }
            val bar = views.transfersBar
            when {
                syncing -> {
                    views.transfersText.text = "Syncing with your devices…"
                    views.transfersIcon.setImageResource(R.drawable.ic_refresh_cw)
                    if (turning == null && !Kit.calm()) {
                        turning = ObjectAnimator.ofFloat(views.transfersIcon, View.ROTATION, 0f, 360f).apply {
                            duration = 1400
                            repeatCount = ValueAnimator.INFINITE
                            interpolator = LinearInterpolator()
                            start()
                        }
                    }
                }
                waiting.isNotEmpty() -> {
                    views.transfersText.text = if (waiting.size == 1) {
                        "${waiting[0].path.substringAfterLast('/')} is waiting for ${waiting[0].to}"
                    } else {
                        "${waiting.size} files waiting to be collected"
                    }
                    views.transfersIcon.setImageResource(R.drawable.ic_arrow_up_down)
                }
            }
            if (!syncing) {
                turning?.cancel()
                turning = null
                views.transfersIcon.rotation = 0f
            }
            val show = syncing || waiting.isNotEmpty()
            if (show && bar.visibility != View.VISIBLE) {
                bar.visibility = View.VISIBLE
                if (!Kit.calm()) {
                    bar.alpha = 0f
                    bar.translationY = kit.dp(12).toFloat()
                    bar.animate().alpha(1f).translationY(0f).setDuration(320).start()
                }
            } else if (!show) {
                bar.visibility = View.GONE
            }
        }
    }

    /** What is moving and what finished, in a sheet from the Transfers bar. */
    fun openTransfers() {
        lifecycleScope.launch {
            val (waiting, history) = try {
                withContext(Dispatchers.IO) {
                    val engine = Engine.open(this@MainActivity)
                    engine.waiting() to engine.history(100u, null)
                }
            } catch (e: Exception) {
                fail("Could not read what is moving", e)
                return@launch
            }
            val sheet = kit.sheet().header(R.drawable.ic_arrow_up_down, "Transfers",
                "What you sent on purpose, and what was sent to you")
            val list = android.widget.LinearLayout(this@MainActivity).apply {
                orientation = android.widget.LinearLayout.VERTICAL
            }
            if (syncing) {
                kit.groupTitle(list, "Active")
                kit.row(list, R.drawable.ic_refresh_cw, "Syncing with your devices", iconTint = R.color.green)
            }
            kit.groupTitle(list, "Waiting to be collected")
            if (waiting.isEmpty()) kit.text(list, "Nothing waiting. Files you send wait here until the device collects them.")
            for (w in waiting) {
                kit.row(
                    list, States.icon(w.path), w.path.substringAfterLast('/'),
                    "Waiting for ${w.to}  ·  ${Words.size(w.size)}",
                    trail = kit.button("Stop", Kit.Style.SECONDARY, small = true) {
                        sheet.dismiss()
                        cancelSend(w)
                    },
                )
            }
            val done = history.filter { Words.finished(it) != null }.take(8)
            if (done.isNotEmpty()) {
                kit.groupTitle(list, "Done")
                for (h in done) {
                    val failed = h.kind == "failed" || h.kind == "cancelled"
                    kit.row(
                        list,
                        if (failed) R.drawable.ic_circle_alert else R.drawable.ic_circle_check,
                        (h.path ?: "").substringAfterLast('/'),
                        "${Words.finished(h)}  ·  ${Words.ago(h.at)}",
                        iconTint = if (failed) R.color.error else R.color.healthy,
                    )
                }
            }
            sheet.view(list, top = 4).show()
        }
    }

    private fun cancelSend(w: Waiting) {
        kit.sheet()
            .header(States.icon(w.path), "Stop sending ${w.path.substringAfterLast('/')}?")
            .text("${w.to} hasn't collected it yet, so it never arrives there.")
            .buttons("Stop sending", danger = true, secondary = "Keep sending") {
                lifecycleScope.launch {
                    try {
                        withContext(Dispatchers.IO) {
                            Engine.open(this@MainActivity).cancelSend(w.path, w.toFingerprint)
                        }
                        say("Stopped")
                    } catch (e: Exception) {
                        // Most likely collected between the list and the tap:
                        // the engine refuses then, and says why.
                        fail("Could not stop that", e)
                    } finally {
                        changed()
                    }
                }
            }
            .show()
    }

    // ---------------------------------------------------------------- sync

    /**
     * Ask, once, to show notifications: the first time a long transfer is
     * handed to the worker, whose notification says how far it has got. Not
     * at install -- a permission asked for before it means anything is a
     * permission refused. The transfer runs either way.
     */
    fun askToNotify() {
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) !=
            android.content.pm.PackageManager.PERMISSION_GRANTED
        ) {
            notifyAsk.launch(android.Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    /**
     * Sync with every paired device that answers. `quiet` when it was started
     * by a change rather than by somebody asking: the Transfers bar shows it
     * moving, and its outcome is not announced. `then` runs when the pass that
     * includes this request has ended -- with its outcome, or null if it
     * failed.
     */
    fun sync(quiet: Boolean = false, then: (suspend (Synced) -> Unit)? = null) {
        if (syncing) {
            // The running pass may have started before this change existed.
            again = true
            againQuiet = againQuiet && quiet
            then?.let { afterNextSync += it }
            return
        }
        then?.let { afterThisSync += it }
        syncingHere = true
        changed()

        lifecycleScope.launch {
            var result: SyncOutcome? = null
            var inBackground = false
            try {
                val engine = Engine.open(this@MainActivity)
                val forOthers = withContext(Dispatchers.IO) {
                    engine.scan()
                    runCatching { engine.waitingForOthersBytes() }.getOrDefault(0uL)
                }
                // A large file for a device to collect, and Android freezes an
                // app nobody is looking at: handed to the worker, which runs it
                // in the foreground under a notification (decision 0050).
                if (forOthers >= SyncWorker.LONG_PASS_BYTES) {
                    inBackground = true
                    askToNotify()
                    withContext(Dispatchers.IO) { SyncWorker.runNow(this@MainActivity) }
                    if (!quiet) say(
                        "Sending ${Words.size(forOthers)} to your devices. It carries on if you " +
                            "leave Qurb; a notification shows how far it has got."
                    )
                    return@launch
                }
                // 25 seconds: generous for someone watching, and still inside
                // what a background window would grant. The deadline is the
                // point of `syncWithin` -- see decision 0020.
                val outcome = withContext(Dispatchers.IO) {
                    // Holding the multicast lock, or the phone cannot hear the
                    // devices on its own Wi-Fi answering.
                    Engine.hearingTheNetwork(this@MainActivity) { engine.syncWithin(25u) }
                }
                result = outcome
                if (outcome.reached > 0u) Engine.noteSynced(this@MainActivity)
                // On screen, so nothing is raised; what was shown here is
                // marked as seen, and not announced later.
                withContext(Dispatchers.IO) { Notices.tell(this@MainActivity, engine) }
                // A computer this phone visits, asking to open its folder
                // there (decision 0060).
                engine.openAsks().firstOrNull()?.let { askToOpen(engine, it) }
                if (!quiet) say(
                    when {
                        outcome.reached == 0u && outcome.unreachable == 0u && outcome.timedOut ->
                            "Ran out of time before reaching a device. Try again."
                        outcome.reached == 0u && outcome.unreachable == 0u ->
                            "No devices connected yet"
                        outcome.reached == 0u ->
                            "No device answered. It has to be switched on and running Qurb."
                        outcome.adopted == 0u && outcome.conflicts == 0u ->
                            "Everything is synced"
                        else -> buildString {
                            append("${Words.files(outcome.adopted.toInt())} updated")
                            if (outcome.conflicts > 0u) {
                                append(", ${outcome.conflicts} with two versions")
                            }
                            if (outcome.timedOut) append(" — ran out of time, sync again")
                        }
                    }
                )
            } catch (e: Exception) {
                if (quiet) android.util.Log.w("qurb", "a sync after a change failed", e)
                else fail("Sync failed", e)
            } finally {
                syncingHere = false
                changed()
                val done = afterThisSync.toList()
                afterThisSync.clear()
                for (callback in done) callback(Synced(result, inBackground))
                askedMeanwhile()
            }
        }
    }

    /**
     * A pass has ended, here or in the worker: run the one asked for while it
     * ran. Not while the other is still running -- its end calls this again.
     */
    private fun askedMeanwhile() {
        if (!again || syncing) return
        val quietly = againQuiet
        again = false
        againQuiet = true
        afterThisSync += afterNextSync
        afterNextSync.clear()
        sync(quiet = quietly)
    }

    /**
     * Files just sent to one device: start a sync, and say how it went (brief
     * §2) -- *Sent to Laptop* if the device collected them during the pass,
     * which waits up to ten seconds for that, or *Waiting for Laptop* if it is
     * switched off or out of reach.
     */
    private fun sendGoes(to: PeerInfo, names: List<String>) {
        say(if (names.size == 1) "Sending ${names[0]} to ${to.name}…" else "Sending ${names.size} files to ${to.name}…")
        sync(quiet = true) { synced ->
            if (synced.inBackground) {
                say("Sending to ${to.name}. It carries on if you leave Qurb; a notification shows how far it has got.")
                return@sync
            }
            val left = runCatching {
                withContext(Dispatchers.IO) { Engine.open(this@MainActivity).waiting() }
            }.getOrDefault(emptyList()).count { it.toFingerprint == to.fingerprint && it.path in names }
            say(
                when {
                    left == 0 -> "Sent to ${to.name}"
                    // Read from where it is when collected: no copy is kept
                    // (decision 0060).
                    names.size == 1 -> "Waiting for ${to.name}. It collects it the next time it’s online — keep the file as it is until then."
                    else -> "Waiting for ${to.name}: $left of ${names.size} not collected yet. It gets them the next time it’s online — keep the files as they are until then."
                }
            )
        }
    }

    /** The ask on screen, so a second sync does not stack another. */
    private var askShown: String? = null

    /**
     * A computer this phone visits asks to open its folder there (decision
     * 0060, step 5): said plainly, and answered only behind the phone's
     * fingerprint, face or screen lock. Approved, the folder's key goes to
     * that computer at the next sync, which follows at once.
     */
    private fun askToOpen(engine: uniffi.qurb_mobile.Qurb, ask: uniffi.qurb_mobile.OpenAsk) {
        if (askShown == ask.fingerprint) return
        askShown = ask.fingerprint
        kit.sheet()
            .header(R.drawable.ic_lock_keyhole, "Open your folder on ${ask.name}?",
                "${ask.name} is asking")
            .text("Your folder is kept on ${ask.name} sealed: it can't open it. Approve, and it can — " +
                "for whoever is at that computer, until it's locked there, or ten minutes unused.\n\n" +
                "Only approve if you're there and asked for it.")
            .buttons("Open it there", secondary = "Not now", onSecondary = {
                askShown = null
                engine.declineOpen(ask.fingerprint)
            }) {
                if (!ScreenLock.available(this)) {
                    askShown = null
                    say("Set a screen lock on this phone first, so it can confirm it's you.")
                    return@buttons
                }
                ScreenLock.confirm(this, "Open your folder on ${ask.name}", "Confirm it's you") { yes ->
                    askShown = null
                    if (!yes) {
                        engine.declineOpen(ask.fingerprint)
                        return@confirm
                    }
                    runCatching { engine.approveOpen(ask.fingerprint) }
                        .onSuccess {
                            say("Opening your folder on ${ask.name}…")
                            sync(quiet = true)
                        }
                        .onFailure { fail("Could not answer ${ask.name}", it) }
                }
            }
            .show()
    }

    // ------------------------------------------------------------- pairing

    /**
     * Add a device: scan the code it shows, show one here, or type one.
     *
     * The code carries the other device's whole identity, which is why it
     * travels across the room by camera rather than over the network.
     */
    fun pair() {
        // Scanning first, because it is what anyone will actually do; showing
        // a code is how two phones connect with no computer; typing is the
        // fallback nobody does twice.
        kit.sheet()
            .header(R.drawable.ic_monitor_smartphone, "Add a device",
                "Your computer or another phone. It joins with a code, once.")
            .action(R.drawable.ic_qr_code, "Scan the other device's code") {
                scanner.launch(Intent(this, ScanActivity::class.java))
            }
            .action(R.drawable.ic_smartphone, "Show a code on this phone") { ShowCode.show(this) }
            .action(R.drawable.ic_keyboard, "Type a code") { typeCode() }
            // Another person's computer, as a guest (decision 0060). Its code
            // says so, so scanning it from here or above does the same.
            .action(R.drawable.ic_laptop, "Visit someone's computer") {
                scanner.launch(Intent(this, ScanActivity::class.java))
            }
            .show()
    }

    /** The fallback, for a phone with no camera or a refused permission. */
    private fun typeCode() {
        val input = EditText(this).apply {
            hint = "qurb1-…"
            setPadding(kit.dp(18), kit.dp(14), kit.dp(18), kit.dp(14))
            background = androidx.core.content.ContextCompat.getDrawable(this@MainActivity, R.drawable.glass_group)
        }
        kit.sheet()
            .header(R.drawable.ic_keyboard, "Type the code", "The code the other device is showing")
            .view(input, top = 16)
            .buttons("Connect") {
                val code = input.text.toString().trim()
                if (code.isNotEmpty()) joinWith(code)
            }
            .show()
    }

    private fun joinWith(code: String) {
        lifecycleScope.launch {
            try {
                // The other device asks its person to approve this phone: the
                // number to compare is on this screen meanwhile (decision 0053).
                val number = withContext(Dispatchers.IO) {
                    pairingNumber(Engine.root(this@MainActivity).absolutePath, code)
                }
                val showing = Approval.showWhileJoining(this@MainActivity, number)
                // A guest code: another person's computer, which this phone
                // visits keeping its own key (decision 0060).
                val guest = code.trim().lowercase().startsWith("qurbg1-")
                val peer = try {
                    withContext(Dispatchers.IO) {
                        val engine = Engine.open(this@MainActivity)
                        if (guest) engine.visitComputer(code) else engine.joinPairing(code)
                    }
                } finally {
                    showing.dismiss()
                }
                say(if (guest) "You visit ${peer.name} as a guest now" else "Connected to ${peer.name}")
                madeChange()
            } catch (e: Exception) {
                fail("Could not connect", e)
            }
        }
    }

    // --------------------------------------------------------------- files

    /** Add files, into `into` in the area being looked at: Files, or Private
     *  Vault -- whatever Keep new files private says. */
    fun pickFilesToAdd(into: String, private: Boolean) {
        addingInto = into
        addingPrivate = private
        adder.launch(arrayOf("*/*"))
    }

    /**
     * Copy files from elsewhere on the phone into it. Through the cache rather
     * than memory -- see [Engine.importUri] -- so a long video never has to fit
     * in the heap.
     */
    private fun addFiles(uris: List<Uri>) {
        lifecycleScope.launch {
            var added = 0
            try {
                for (uri in uris) {
                    Engine.importUri(this@MainActivity, uri, addingInto, addingPrivate)
                    added++
                }
                say("Added ${Words.files(added)}${if (addingPrivate) " to Private Vault" else ""}")
            } catch (e: Exception) {
                fail(if (added == 0) "Could not add that file" else "Added $added, then stopped", e)
            } finally {
                if (added > 0) madeChange() else changed()
            }
        }
    }

    /** Ask which device, then do something with it. */
    fun chooseDevice(title: String, then: (PeerInfo) -> Unit) {
        lifecycleScope.launch {
            val peers = try {
                withContext(Dispatchers.IO) { Engine.open(this@MainActivity).peers() }
            } catch (e: Exception) {
                fail("Could not read your devices", e)
                return@launch
            }
            val sheet = kit.sheet().header(R.drawable.ic_send, title)
            if (peers.isEmpty()) {
                sheet.text("No devices yet. Add one first.")
                    .buttons("Add a device") { pair() }
            }
            for (peer in peers) {
                sheet.action(States.device(peer.name), peer.name) { then(peer) }
            }
            sheet.show()
        }
    }

    fun pickFilesToSend(to: PeerInfo) {
        sendingTo = to
        sender.launch(arrayOf("*/*"))
    }

    /**
     * Send files picked with the system's picker. Each is read from where it
     * is when the other device collects it: nothing is copied (decision
     * 0060).
     */
    private fun sendPicked(uris: List<Uri>, to: PeerInfo) {
        lifecycleScope.launch {
            val sent = mutableListOf<String>()
            try {
                val sources = uris.map { it.toString() }
                val earlier = Engine.sentBefore(this@MainActivity, sources, to.fingerprint)
                val names = uris.associate { it.toString() to Engine.displayName(this@MainActivity, it) }
                val leaveOut = askAgain(to, earlier, everything = earlier.size == uris.size) { names[it] ?: it }
                    ?: return@launch
                for (uri in uris.filter { it.toString() !in leaveOut }) {
                    sent += Engine.sendDocument(this@MainActivity, uri, to.fingerprint)
                }
            } catch (e: Exception) {
                fail(if (sent.isEmpty()) "Could not send that" else "Sent ${sent.size}, then stopped", e)
            } finally {
                changed()
                if (sent.isNotEmpty()) sendGoes(to, sent)
            }
        }
    }

    /**
     * Send a file already on this phone (§17). Read from the folder when the
     * other device collects it, so this works while that device is off; no
     * copy is kept, and changing or deleting the file first calls the send
     * off (decision 0060).
     */
    fun send(entry: FileEntry, to: PeerInfo) {
        lifecycleScope.launch {
            val name = entry.path.substringAfterLast('/')
            val source = File(Engine.root(this@MainActivity), entry.path)
            try {
                val earlier = Engine.sentBefore(this@MainActivity, listOf(source.absolutePath), to.fingerprint)
                askAgain(to, earlier, everything = true) { name } ?: return@launch
                withContext(Dispatchers.IO) {
                    Engine.open(this@MainActivity).sendFile(source.absolutePath, name, to.fingerprint)
                }
                changed()
                sendGoes(to, listOf(name))
            } catch (e: Exception) {
                fail("Could not send that", e)
                changed()
            }
        }
    }

    /**
     * Files that went to `to` before, and whether to send them again
     * (decision 0059): rather than send a second copy unasked, or drop it
     * without a word, which is what used to happen.
     *
     * Returns the files to leave out -- none, to send them all again -- or
     * null to send nothing. `everything` is whether every file being sent is
     * one of them, when leaving them out leaves nothing to send.
     */
    private suspend fun askAgain(
        to: PeerInfo,
        earlier: List<uniffi.qurb_mobile.EarlierSend>,
        everything: Boolean,
        nameOf: (String) -> String,
    ): Set<String>? {
        if (earlier.isEmpty()) return emptySet()
        return kotlinx.coroutines.suspendCancellableCoroutine { answer ->
            var answered = false
            fun say(leaveOut: Set<String>?) {
                if (!answered) {
                    answered = true
                    answer.resumeWith(Result.success(leaveOut))
                }
            }
            val one = earlier.size == 1
            val sheet = kit.sheet().header(R.drawable.ic_send, "Sent to ${to.name} before")
            for (e in earlier) sheet.text("${nameOf(e.source)} — as ${e.sentAs}, ${Words.ago(e.at)}", R.style.Text_Body)
            sheet.text(
                if (one) "${to.name} may still have it. Sending it again puts another copy there."
                else "${to.name} may still have them. Sending them again puts another copy there."
            )
            sheet.buttons(
                primary = if (one) "Send it again" else "Send them again",
                secondary = if (everything) "Don't send" else if (one) "Leave it out" else "Leave them out",
                onSecondary = { say(if (everything) null else earlier.map { it.source }.toSet()) },
            ) { say(emptySet()) }
            sheet.onDismiss { say(null) }
            sheet.show()
        }
    }

    /**
     * Hand the file to whatever app handles its type, through the app's own
     * DocumentsProvider, so there is one way out of the store rather than two.
     */
    fun open(entry: FileEntry) {
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(documentUri(entry.path), mimeType(entry.path))
            // Without this the receiving app cannot read the URI, and fails
            // with something that looks like a corrupt file.
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            startActivity(Intent.createChooser(intent, "Open with"))
        } catch (e: Exception) {
            fail("Nothing on this phone can open that", e)
        }
    }

    /**
     * Copy a file out to wherever the person chooses: Downloads, the gallery,
     * anywhere they keep things. The folder is this app's private storage, so
     * a file that lives only there is invisible to everything else.
     */
    fun saveCopy(entry: FileEntry) {
        pendingSave = entry
        try {
            saver.launch(entry.path.substringAfterLast('/'))
        } catch (e: Exception) {
            pendingSave = null
            fail("Could not open the save dialog", e)
        }
    }

    /** Save several files at once, into a folder the person picks. */
    fun saveAll(entries: List<FileEntry>) {
        pendingSaveAll = entries
        try {
            folderSaver.launch(null)
        } catch (e: Exception) {
            pendingSaveAll = emptyList()
            fail("Could not open the folder picker", e)
        }
    }

    /** Each file exported and copied in turn, as [writeCopy] does one. */
    private fun writeAll(entries: List<FileEntry>, tree: Uri) {
        lifecycleScope.launch {
            var saved = 0
            try {
                withContext(Dispatchers.IO) {
                    val parent = android.provider.DocumentsContract.buildDocumentUriUsingTree(
                        tree,
                        android.provider.DocumentsContract.getTreeDocumentId(tree),
                    )
                    for (entry in entries) {
                        val name = entry.path.substringAfterLast('/')
                        val destination = android.provider.DocumentsContract.createDocument(
                            contentResolver, parent, mimeType(entry.path), name,
                        ) ?: error("could not create $name")
                        val staging = File(cacheDir, "save-${System.nanoTime()}")
                        try {
                            Engine.open(this@MainActivity).export(entry.path, staging.absolutePath)
                            staging.inputStream().use { input ->
                                contentResolver.openOutputStream(destination)?.use { output ->
                                    input.copyTo(output)
                                } ?: error("could not open $name")
                            }
                        } finally {
                            staging.delete()
                        }
                        saved++
                    }
                }
                say("Saved ${Words.files(saved)}")
            } catch (e: Exception) {
                fail(if (saved == 0) "Could not save them" else "Saved $saved, then stopped", e)
            }
        }
    }

    /**
     * Stream a stored file out to where the person picked. Exported to a cache
     * file and copied from there rather than held in memory: `export` writes a
     * chunk at a time so a large file never has to fit in the heap.
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
                say("Saved a copy")
            } catch (e: Exception) {
                fail("Could not save that file", e)
            }
        }
    }

    private fun documentUri(path: String): Uri =
        android.provider.DocumentsContract.buildDocumentUri("$packageName.documents", "qurb/$path")

    private fun mimeType(path: String): String {
        val extension = path.substringAfterLast('.', "").lowercase()
        return android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
            ?: "application/octet-stream"
    }

    private companion object {
        const val TAB = "tab"
        val TABS = setOf(R.id.tab_home, R.id.tab_files, R.id.tab_devices, R.id.tab_settings)
    }
}
