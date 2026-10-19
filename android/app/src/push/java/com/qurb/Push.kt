package com.qurb

import android.content.Context
import android.util.Log
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

/**
 * Being woken when another device has something.
 *
 * A phone cannot hold a socket open in the background, so the device most in
 * need of being told something is the one that cannot be told. This is the way
 * through, and on both mobile platforms it is the only way through.
 *
 * The message carries nothing — no filenames, no sizes, not even which peer.
 * A woken device syncs with the peers it already knows, so there is nothing
 * useful to put in it and every reason not to: the push service sees the
 * message, and qurb's promise is that nobody outside the devices learns what
 * is being synced.
 */
object Push {
    private const val TAG = "qurb"

    /**
     * This device's push token, or null if one cannot be had.
     *
     * Failure is ordinary rather than exceptional — a device without Play
     * Services, a network that is down — and the answer to it is the same as
     * never having had push: sync on the usual schedule.
     */
    suspend fun token(context: Context): String? = suspendCancellableCoroutine { waiting ->
        FirebaseMessaging.getInstance().token
            .addOnSuccessListener { token -> waiting.resume(token) }
            .addOnFailureListener { e ->
                Log.w(TAG, "no push token available", e)
                waiting.resume(null)
            }
    }

    const val AVAILABLE = true
}

/**
 * What happens when the poke arrives.
 *
 * Asks for a sync through the scheduler rather than syncing here. This runs in
 * a short window the system grants for a high-priority message, and a sync can
 * take longer than that; WorkManager is the thing allowed to finish.
 */
class Wakeup : FirebaseMessagingService() {
    override fun onMessageReceived(message: RemoteMessage) {
        Log.i("qurb", "woken by another device")
        SyncWorker.noteWoken(applicationContext)
        SyncWorker.runNow(applicationContext)
    }

    /**
     * The token changed, so the rendezvous service is holding a stale one.
     * Handed to the engine, which gives it on every pass, and a pass now to
     * give it. Until 2026-10-18 only the pass: the engine kept the token it
     * opened with, so the stale one was given again until the app restarted.
     */
    override fun onNewToken(token: String) {
        Log.i("qurb", "push token changed")
        Engine.wakeBy(token)
        SyncWorker.runNow(applicationContext)
    }
}
