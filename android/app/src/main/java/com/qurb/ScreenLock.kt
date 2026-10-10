package com.qurb

import android.app.Activity
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
 * (decision 0039), and Android 9 and later have it built in. A phone with no
 * screen lock cannot confirm anything, and says so.
 */
object ScreenLock {
    /** Whether the phone has a screen lock to confirm with at all. */
    fun available(context: Context): Boolean =
        (context.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager).isDeviceSecure

    /** Ask, and call `then` with whether the person confirmed. */
    fun confirm(activity: Activity, title: String, subtitle: String, then: (Boolean) -> Unit) {
        if (!available(activity) || Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
            then(false)
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
}
