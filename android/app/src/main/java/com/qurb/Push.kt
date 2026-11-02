package com.qurb

import android.content.Context

/**
 * No push: this device is not woken by another (decision 0082, 2026-11-02).
 * It learns of changes at its next scheduled look, or when it is opened. Kept
 * as a stub so the engine's wake-token path stays a no-op rather than gone.
 */
object Push {
    /** Nothing to register. */
    suspend fun token(context: Context): String? = null

    const val AVAILABLE = false
}
