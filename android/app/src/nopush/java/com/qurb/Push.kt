package com.qurb

import android.content.Context

/**
 * No push service configured, so this device cannot be woken.
 *
 * The build uses this source set when `google-services.json` is absent, which
 * is the case for anyone building qurb without setting up a Firebase project
 * of their own. Everything still works: the phone learns about changes at its
 * next scheduled look instead of the moment they happen, which is how it
 * behaved before push existed.
 *
 * Kept as a whole separate source set rather than a runtime check, because the
 * alternative is compiling against a Firebase SDK that is not there.
 */
object Push {
    /** Nothing to register. */
    suspend fun token(context: Context): String? = null

    const val AVAILABLE = false
}
