package com.qurb

import android.app.KeyguardManager
import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal

/**
 * The person confirming it is them, behind the phone's own fingerprint, face
 * or screen lock: before this phone sends a computer the key to open its
 * folder there (decision 0060, step 5).
 *
 * The framework's prompt rather than a library's: the app stays light
 * (decision 0039). From Android 10 it offers the fingerprint, the face and
 * the PIN or pattern alike. Before, Android 8 and 9, the screen lock's own
 * page asks instead: 9's prompt offers no PIN or pattern, and 8 has none. A
 * phone with no screen lock cannot confirm anything, and says so.
 *
 * Until 2026-10-18 a phone on Android 9 stopped here, at a call Android 10
 * added, and one on 8 could not confirm at all; lint found it.
 */
object ScreenLock {
    /** Whether the phone has a screen lock to confirm with at all. */
    fun available(context: Context): Boolean =
        (context.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager).isDeviceSecure

    /** Waiting for the screen lock's page, before Android 10. */
    private var waiting: ((Boolean) -> Unit)? = null

    /** Ask, and call `then` with whether the person confirmed. */
    fun confirm(activity: MainActivity, title: String, subtitle: String, then: (Boolean) -> Unit) {
        if (!available(activity)) {
            then(false)
            return
        }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            val keyguard = activity.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
            @Suppress("DEPRECATION")
            val asking = keyguard.createConfirmDeviceCredentialIntent(title, subtitle)
            if (asking == null) {
                then(false)
                return
            }
            waiting = then
            activity.confirmWithLock(asking)
            return
        }
        val builder = BiometricPrompt.Builder(activity).setTitle(title).setSubtitle(subtitle)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            builder.setAllowedAuthenticators(
                BiometricManager.Authenticators.BIOMETRIC_STRONG or
                    BiometricManager.Authenticators.BIOMETRIC_WEAK or
                    BiometricManager.Authenticators.DEVICE_CREDENTIAL
            )
        } else {
            @Suppress("DEPRECATION")
            builder.setDeviceCredentialAllowed(true)
        }
        builder.build().authenticate(
            CancellationSignal(),
            activity.mainExecutor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult?) = then(true)
                override fun onAuthenticationError(errorCode: Int, errString: CharSequence?) = then(false)
            },
        )
    }

    /** The screen lock's page answered. */
    fun answered(yes: Boolean) {
        val then = waiting ?: return
        waiting = null
        then(yes)
    }
}
