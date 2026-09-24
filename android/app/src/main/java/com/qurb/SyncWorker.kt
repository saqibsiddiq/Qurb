package com.qurb

import android.content.Context
import android.util.Log
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
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
            // Holding the multicast lock, or the phone cannot hear the devices
            // on its own Wi-Fi answering. See `Engine.hearingTheNetwork`.
            val outcome = Engine.hearingTheNetwork(applicationContext) {
                engine.syncWithin(BUDGET_SECONDS)
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
                    else -> "no paired devices"
                },
                started,
            )
            Log.i(TAG, "sync: reached=${outcome.reached} unreachable=${outcome.unreachable} " +
                "adopted=${outcome.adopted} timedOut=${outcome.timedOut}")

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

    companion object {
        private const val TAG = "qurb"
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

        /** Schedules from earlier versions, cancelled on sight. */
        private val RETIRED = listOf("qurb-sync")
        private const val BUDGET_SECONDS = 20u
        private const val PREFS = "qurb"
        private const val LAST_RESULT = "last-sync-result"
        private const val LAST_AT = "last-sync-at"

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
         * Ask for a sync roughly every fifteen minutes.
         *
         * Fifteen is not a choice: it is the shortest period WorkManager
         * accepts for periodic work, and asking for less silently becomes
         * fifteen anyway. In practice it is a floor rather than a promise —
         * Doze batches these, so an idle phone may go hours between runs. That
         * is the platform's decision and arguing with it loses.
         *
         * `KEEP` rather than `UPDATE`: re-registering on every launch with
         * `UPDATE` resets the period, so an app opened often would never reach
         * the end of one and never sync.
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
            val request = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(constraints)
                .setBackoffCriteria(BackoffPolicy.LINEAR, 1, TimeUnit.MINUTES)
                .build()

            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
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
                androidx.work.WorkInfo.State.ENQUEUED ->
                    "Scheduled. Waiting for a network connection and Android's permission to run."
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

            val operation = WorkManager.getInstance(context).enqueueUniqueWork(
                "$NAME-now",
                androidx.work.ExistingWorkPolicy.REPLACE,
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

        /** Stop asking. Used when the store is torn down. */
        fun cancel(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(NAME)
        }
    }
}
