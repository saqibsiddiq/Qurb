package com.qurb

import android.content.Context
import android.util.Log
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.WorkQuery
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.concurrent.TimeUnit

/**
 * Sync in the background, on the platform's terms.
 *
 * The half of [decision 0020] that Rust cannot do. `syncWithin(seconds)` makes
 * a sync that ends when its window does; this decides when to ask for a window
 * and what to tell the system when one runs out.
 *
 * WorkManager rather than an alarm or a foreground service. An alarm does not
 * survive Doze or a reboot; a foreground service means a permanent notification
 * and, since Android 14, a declared type that "syncing files" does not cleanly
 * fit. WorkManager is the arrangement Android actually wants, and it backs off
 * on its own when the system is busy.
 *
 * With one exception, decision 0050: a pass with a large file waiting to be
 * collected from this phone runs in the foreground, under a notification that
 * says how far it has got, for as long as a device is collecting -- because
 * Android freezes an app it cannot see, and an 800 MB video collected by the
 * laptop stopped part-way every time. Not permanent: the notification goes
 * when the pass does.
 *
 * [decision 0020]: ../../../../../docs/decisions/0020-sync-takes-a-deadline.md
 */
class SyncWorker(context: Context, params: WorkerParameters) :
    CoroutineWorker(context, params) {

    override suspend fun doWork(): Result {
        if (!Engine.isSetUp(applicationContext)) return Result.success()

        val started = System.currentTimeMillis()
        return try {
            val engine = Engine.open(applicationContext)

            // Shorter than the foreground's 25s. A periodic worker is given
            // roughly ten minutes before it is stopped, but spending them is
            // how an app's future windows get rationed, and a sync that needs
            // longer can simply have the next one.
            engine.scan()

            // A large collection waiting: run in the foreground and keep
            // answering while a device collects (decision 0050). Refused when
            // Android will not let this app start a foreground service from
            // where it is -- then the ordinary pass, and resuming carries the
            // file on next time.
            val forOthers = runCatching { engine.waitingForOthersBytes() }.getOrDefault(0uL)
            val long = forOthers >= LONG_PASS_BYTES && inForeground(forOthers)

            // Holding the multicast lock, or the phone cannot hear the devices
            // on its own Wi-Fi answering. See `Engine.hearingTheNetwork`.
            val servedBefore = engine.serving().bytes
            val outcome = coroutineScope {
                val from = servedBefore
                val showing = if (long) launch {
                    while (true) {
                        delay(2_000)
                        runCatching {
                            setForeground(Transfers.foreground(applicationContext, engine.serving().bytes - from, forOthers))
                        }
                    }
                } else null
                try {
                    withContext(Dispatchers.IO) {
                        Engine.hearingTheNetwork(applicationContext) {
                            if (long) engine.syncServing(BUDGET_SECONDS, SERVING_SECONDS)
                            else engine.syncWithin(BUDGET_SECONDS)
                        }
                    }
                } finally {
                    showing?.cancel()
                }
            }

            // Recorded because a background worker is otherwise invisible.
            // Nobody is watching when it runs, so if it does not leave a trace
            // there is no way to tell a sync that is working from one that has
            // silently stopped happening -- and "silently stopped" is the
            // failure mode a sync app actually dies of.
            record(
                applicationContext,
                when {
                    outcome.reached > 0u && outcome.adopted > 0u ->
                        "${outcome.adopted} file${if (outcome.adopted == 1u) "" else "s"} arrived"
                    outcome.reached > 0u -> "up to date"
                    outcome.unreachable > 0u -> "no device answered"
                    // Reached nobody, answered by nobody, and still out of
                    // time: the window closed before the phone could try.
                    // Once reported as "no paired devices", which it is not.
                    outcome.timedOut -> "ran out of time"
                    else -> "no paired devices"
                },
                started,
            )
            if (outcome.reached > 0u) Engine.noteSynced(applicationContext)

            // Refused the foreground with a large collection waiting, while a
            // device was there for it -- reached, or collecting on its own
            // connection: say so, with the way to carry on.
            if (long || forOthers < LONG_PASS_BYTES) {
                Transfers.clearPaused(applicationContext)
            } else {
                val left = runCatching { engine.waitingForOthersBytes() }.getOrDefault(0uL)
                val wasThere = outcome.reached > 0u || engine.serving().bytes > servedBefore
                if (left >= LONG_PASS_BYTES && wasThere) Transfers.paused(applicationContext, left)
            }
            Log.i(TAG, "sync: reached=${outcome.reached} unreachable=${outcome.unreachable} " +
                "adopted=${outcome.adopted} timedOut=${outcome.timedOut}")

            // Then free what nothing needs, as the desktop daemon does on its
            // own timer. A phone has no timer of its own but this one, and
            // until this ran here a phone kept every replaced and deleted
            // file's chunks for good. Its failure is not the sync's: the sync
            // already happened, and the next run tries again.
            runCatching { engine.housekeep() }
                .onSuccess { if (it.freed > 0uL) Log.i(TAG, "freed ${it.freed} bytes nothing needed") }
                .onFailure { Log.w(TAG, "housekeeping failed", it) }

            when {
                // Ran out of time with work outstanding. Retrying asks
                // WorkManager for another window sooner than the next period,
                // with its own backoff deciding how much sooner.
                outcome.timedOut -> Result.retry()

                // Nothing answered. This used to be a retry, on the reasoning
                // that the other device being asleep is ordinary and the
                // backoff would stop it becoming a battery drain. That was
                // exactly backwards, and measured on a real phone: every
                // unanswered attempt doubled the delay until the next sync was
                // scheduled **three hours out**, so the moment the other
                // device finally came back was the moment this one had stopped
                // looking. A file shared while a laptop was shut sat on the
                // phone for hours with the laptop running beside it.
                //
                // Backoff is for transient errors. "Nobody is awake yet" is
                // not an error, it is the steady state, and the answer to it
                // is the ordinary fifteen-minute period -- which only applies
                // if this reports success. Success also resets the attempt
                // count, so a long backoff already accumulated unwinds on the
                // first run after this.
                outcome.reached == 0u && outcome.unreachable > 0u -> Result.success()

                else -> Result.success()
            }
        } catch (e: Exception) {
            Log.w(TAG, "sync failed", e)
            record(applicationContext, "failed: ${e.message ?: "unknown"}", started)

            // Two kinds of failure, and they want opposite treatment.
            //
            // A locked keystore or a store that will not open is not going to
            // fix itself by being retried in fifteen minutes, and retrying
            // burns battery to reach the same conclusion. Anything else -- a
            // refused connection, a service that is down -- is worth another go.
            if (e is uniffi.qurb_mobile.QurbException.Locked ||
                e is uniffi.qurb_mobile.QurbException.NotSetUp
            ) {
                Result.failure()
            } else {
                Result.retry()
            }
        }
    }

    /** Required of an expedited worker on Android before 12, which runs it as
     *  a foreground service; and the notice a long pass shows. */
    override suspend fun getForegroundInfo(): ForegroundInfo =
        Transfers.foreground(applicationContext, 0uL, 0uL)

    /** Into the foreground for a long pass, if Android allows it from here. */
    private suspend fun inForeground(total: ULong): Boolean = try {
        setForeground(Transfers.foreground(applicationContext, 0uL, total))
        true
    } catch (e: Exception) {
        Log.w(TAG, "could not run in the foreground; an ordinary pass instead", e)
        false
    }

    companion object {
        private const val TAG = "qurb"

        /**
         * How much waiting to be collected makes a pass a long one, run in the
         * foreground. Below it a device collects within the ordinary window on
         * any network worth the name; above it, the 800 MB video that kept
         * stopping part-way.
         */
        val LONG_PASS_BYTES = 32uL * 1024uL * 1024uL

        /** The longest a long pass keeps answering: half an hour, well inside
         *  the six hours a day Android 15 allows a data-sync service. */
        private const val SERVING_SECONDS = 1800u
        /**
         * The schedule's name, versioned.
         *
         * A periodic schedule registered with `KEEP` survives reinstalls, and
         * so does the exponential backoff it accumulated — a phone that had
         * backed off to three hours would keep that delay across an update
         * carrying the fix for it. Changing the name retires the old schedule
         * once, for everyone, which is the only way to be sure.
         */
        private const val NAME = "qurb-sync-v2"

        /** A pass asked for now, rather than on the schedule. */
        private const val NOW = "$NAME-now"

        /** Schedules from earlier versions, cancelled on sight. */
        private val RETIRED = listOf("qurb-sync")
        private const val BUDGET_SECONDS = 20u
        private const val PREFS = "qurb"
        private const val LAST_RESULT = "last-sync-result"
        private const val LAST_AT = "last-sync-at"
        private const val LAST_WOKEN = "last-woken-at"
        private const val PERIOD = "sync-period-minutes"

        /** How recently a push must have arrived for the hourly schedule. */
        private const val PUSH_TRUSTED_FOR_MS = 7L * 24 * 3600 * 1000

        private fun record(context: Context, result: String, startedAt: Long) {
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit()
                .putString(LAST_RESULT, result)
                .putLong(LAST_AT, startedAt)
                .apply()
        }

        /** When the last background sync ran and what came of it. */
        fun lastRun(context: Context): String? {
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val result = prefs.getString(LAST_RESULT, null) ?: return null
            val at = prefs.getLong(LAST_AT, 0)

            val ago = System.currentTimeMillis() - at
            val when_ = when {
                ago < 60_000 -> "just now"
                ago < 3_600_000 -> "${ago / 60_000} minutes ago"
                ago < 86_400_000 -> "${ago / 3_600_000} hours ago"
                else -> "${ago / 86_400_000} days ago"
            }
            return "Last background sync $when_: $result."
        }

        /**
         * Another device woke this one. Remembered, because it is the proof
         * that pushes reach this phone -- which is what lets the scheduled
         * pass run less often.
         */
        fun noteWoken(context: Context) {
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                .putLong(LAST_WOKEN, System.currentTimeMillis())
                .apply()
            schedule(context)
        }

        /**
         * Minutes between scheduled passes: an hour while pushes are
         * demonstrably arriving, fifteen otherwise.
         *
         * With push, a change on another device wakes this phone within
         * seconds, and the scheduled pass is left with the rest: sending what
         * was added here some other way, and telling a rendezvous service that
         * lost its memory how to wake this phone. An hour covers those at a
         * quarter of the wake-ups. Without push the pass is the only way news
         * arrives, and fifteen -- the shortest WorkManager allows -- stays.
         *
         * "Demonstrably" is a push in the last seven days, not the app having
         * been built with Firebase: a rendezvous service with no push
         * credentials never sends one, and a phone that assumed otherwise
         * would sync hourly for nothing.
         */
        fun periodMinutes(context: Context): Long {
            val woken = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getLong(LAST_WOKEN, 0)
            val pushWorks = Push.AVAILABLE && System.currentTimeMillis() - woken < PUSH_TRUSTED_FOR_MS
            return if (pushWorks) 60 else 15
        }

        /**
         * Ask for a sync every [periodMinutes].
         *
         * Fifteen is the shortest period WorkManager accepts for periodic
         * work. In practice either is a floor rather than a promise — Doze
         * batches these, so an idle phone may go hours between runs. That is
         * the platform's decision and arguing with it loses.
         *
         * `KEEP` rather than `UPDATE` unless the period changed: re-registering
         * on every launch with `UPDATE` resets the period, so an app opened
         * often would never reach the end of one and never sync.
         */
        fun schedule(context: Context) {
            for (old in RETIRED) {
                WorkManager.getInstance(context).cancelUniqueWork(old)
            }

            val constraints = Constraints.Builder()
                // Any network, not unmetered. Two devices on the same Wi-Fi is
                // the common case and would be covered either way, but a phone
                // syncing a document over cellular is a reasonable thing to
                // want and the transfers are incremental.
                .setRequiredNetworkType(NetworkType.CONNECTED)
                // Not while the battery is critical. Sync is not urgent enough
                // to be the reason a phone dies.
                .setRequiresBatteryNotLow(true)
                .build()

            // Linear, not exponential. The only thing that retries now is a
            // sync that ran out of time with work still to do, and the right
            // answer to that is another window shortly -- not a delay that
            // doubles away into hours. Exponential backoff here is what put a
            // phone three hours out from its next attempt.
            val minutes = periodMinutes(context)
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val changed = prefs.getLong(PERIOD, 15) != minutes
            prefs.edit().putLong(PERIOD, minutes).apply()

            val request = PeriodicWorkRequestBuilder<SyncWorker>(minutes, TimeUnit.MINUTES)
                .setConstraints(constraints)
                .setBackoffCriteria(BackoffPolicy.LINEAR, 1, TimeUnit.MINUTES)
                .build()

            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                NAME,
                if (changed) ExistingPeriodicWorkPolicy.UPDATE else ExistingPeriodicWorkPolicy.KEEP,
                request,
            )
        }

        /**
         * What the scheduler is currently doing, phrased for a person.
         *
         * Blocking: `WorkManager.getWorkInfos` returns a `ListenableFuture`,
         * and the caller is already off the main thread.
         */
        fun state(context: Context): String {
            val infos = WorkManager.getInstance(context)
                .getWorkInfosForUniqueWork(NAME)
                .get()

            val info = infos.firstOrNull() ?: return "Not scheduled."
            val last = lastRun(context)?.let { "$it\n\n" } ?: ""
            return last + when (info.state) {
                androidx.work.WorkInfo.State.ENQUEUED -> when (periodMinutes(context)) {
                    60L -> "Every hour, and whenever another device has something: " +
                        "it wakes this phone."
                    else -> "About every 15 minutes, when Android allows."
                }
                androidx.work.WorkInfo.State.RUNNING -> "Running now."
                androidx.work.WorkInfo.State.BLOCKED -> "Waiting on its conditions."
                androidx.work.WorkInfo.State.CANCELLED -> "Cancelled."
                androidx.work.WorkInfo.State.FAILED ->
                    "Stopped after a failure it cannot retry past. Open the app and sync once."
                androidx.work.WorkInfo.State.SUCCEEDED -> "Last run finished cleanly."
            } + if (info.runAttemptCount > 1) {
                "\n\nRetried ${info.runAttemptCount} times — the other device may be asleep."
            } else ""
        }

        /**
         * Run one sync through the scheduler, now.
         *
         * Distinct from the Sync button, which calls the engine directly and
         * holds the screen while it works. This goes through WorkManager, so it
         * survives the app being closed mid-sync and exercises exactly the path
         * the periodic schedule uses — which is the only way to find out that
         * path works without waiting fifteen minutes for it to not.
         */
        fun runNow(context: Context) {
            val request = OneTimeWorkRequestBuilder<SyncWorker>()
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                // Expedited, because the case this exists for is somebody
                // having just shared a file and watching to see it go. Asking
                // to run soon is the whole point; without it this waits in the
                // same queue as work nobody is waiting on.
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .build()

            // Appended, not replacing. A pass already running may be a long
            // one, a device collecting a large file (decision 0050), and
            // replacing it cancelled it part-way -- which a second Sync now,
            // or the sync after any change made in the app, would do. Queued
            // behind it instead, this one runs when it ends.
            val operation = WorkManager.getInstance(context).enqueueUniqueWork(
                NOW,
                androidx.work.ExistingWorkPolicy.APPEND_OR_REPLACE,
                request,
            )

            // Waited on, off the main thread. Enqueuing is asynchronous, and
            // the caller is usually a share screen that finishes immediately
            // afterwards -- and a process with no remaining components can be
            // killed straight away, losing an enqueue that had not yet been
            // written down. A share that silently schedules nothing is the
            // failure this whole feature is supposed to not have.
            runCatching { operation.result.get(5, TimeUnit.SECONDS) }
                .onFailure { Log.w(TAG, "could not schedule a sync", it) }
        }

        /**
         * Whether a pass is running in the worker: the schedule's, or one
         * asked for now.
         *
         * The app shows it as syncing and holds its own syncs until it ends.
         * A long pass runs here for up to half an hour (decision 0050), and
         * the app, not having started it, would otherwise say "Everything is
         * synced" all the while and never notice it end.
         */
        fun running(context: Context): Flow<Boolean> =
            WorkManager.getInstance(context)
                .getWorkInfosFlow(WorkQuery.fromUniqueWorkNames(listOf(NAME, NOW)))
                .map { passes -> passes.any { it.state == WorkInfo.State.RUNNING } }
                .distinctUntilChanged()

        /** Stop asking. Used when the store is torn down. */
        fun cancel(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(NAME)
        }
    }
}
