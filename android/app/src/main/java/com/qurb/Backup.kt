package com.qurb

import android.content.Context
import android.util.Log
import com.google.android.gms.auth.blockstore.Blockstore
import com.google.android.gms.auth.blockstore.RetrieveBytesRequest
import com.google.android.gms.auth.blockstore.StoreBytesData
import com.google.android.gms.tasks.Task
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeout
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * This phone's key, kept by Google Play services' Block Store (decision 0052).
 *
 * Kept across a reinstall, and not across *Clear data*, which empties it with
 * the rest -- both seen on the S23 on 2026-10-08. A cleared phone joins its
 * other devices with a code.
 *
 * Instead of a person writing 24 words down: Block Store keeps a few bytes for
 * an app, carries them across a reinstall, and — when the phone has a screen
 * lock — backs them up end to end encrypted with it, so a new phone restored
 * from this one's backup gets them back and Google cannot read them.
 *
 * What it brings back is the key, not the files. Files live on the person's
 * devices; a key restored here reads whatever of theirs still exists — another
 * device, or a replica — and nothing else. A phone with no screen lock keeps
 * the key on this device only, which survives a reinstall and not a lost
 * phone; that is said where the key is shown, not hidden.
 */
object Backup {
    private const val TAG = "qurb"
    private const val KEY = "com.qurb.key"
    private const val SIGNAL = "com.qurb.signal"

    /** Whether a copy would leave this phone, end to end encrypted. */
    suspend fun leavesThePhone(context: Context): Boolean =
        runCatching { Blockstore.getClient(context).isEndToEndEncryptionAvailable.await() }
            .getOrDefault(false)

    /**
     * Keep the key — as its 24 words, the form the engine gives it in. Backed
     * up to the cloud only when that is end to end encrypted; otherwise kept
     * on this phone only. A failure is logged and otherwise ignored: the
     * phone works without it, as every phone did before.
     */
    suspend fun save(context: Context, phrase: String) {
        runCatching {
            val client = Blockstore.getClient(context)
            val data = StoreBytesData.Builder()
                .setKey(KEY)
                .setBytes(phrase.toByteArray(Charsets.UTF_8))
                .setShouldBackupToCloud(leavesThePhone(context))
                .build()
            client.storeBytes(data).await()
        }.onFailure { Log.w(TAG, "could not keep the key in Block Store", it) }
    }

    /**
     * Keep the rendezvous service's address beside the key, so a reinstall or
     * a new phone restored from this one's backup has it back with the key
     * rather than none. Not across *Clear data*, which empties Block Store
     * too. Best effort, as the key's is.
     */
    suspend fun saveSignal(context: Context, url: String) {
        runCatching {
            val data = StoreBytesData.Builder()
                .setKey(SIGNAL)
                .setBytes(url.toByteArray(Charsets.UTF_8))
                .setShouldBackupToCloud(leavesThePhone(context))
                .build()
            Blockstore.getClient(context).storeBytes(data).await()
        }.onFailure { Log.w(TAG, "could not keep the rendezvous address in Block Store", it) }
    }

    /** The rendezvous address kept with the key, if there is one. */
    suspend fun findSignal(context: Context): String? = runCatching {
        val request = RetrieveBytesRequest.Builder().setKeys(listOf(SIGNAL)).build()
        val found = Blockstore.getClient(context).retrieveBytes(request).await()
        found.blockstoreDataMap[SIGNAL]?.bytes?.toString(Charsets.UTF_8)
    }.onFailure { Log.w(TAG, "could not read Block Store", it) }.getOrNull()

    /** The key kept earlier, on this phone or restored to it, if there is one. */
    suspend fun find(context: Context): String? = runCatching {
        val request = RetrieveBytesRequest.Builder().setKeys(listOf(KEY)).build()
        val found = Blockstore.getClient(context).retrieveBytes(request).await()
        found.blockstoreDataMap[KEY]?.bytes?.toString(Charsets.UTF_8)
    }.onFailure { Log.w(TAG, "could not read Block Store", it) }.getOrNull()

    /** Bounded: setup waits on Block Store, and a phone whose Play services
     *  are missing, old or busy must not hold it up for long. Every caller
     *  takes a failure as "nothing kept". */
    private suspend fun <T> Task<T>.await(): T = withTimeout(WAIT_MS) {
        suspendCancellableCoroutine { waiting ->
            addOnSuccessListener { waiting.resume(it) }
            addOnFailureListener { waiting.resumeWithException(it) }
        }
    }

    private const val WAIT_MS = 10_000L
}
