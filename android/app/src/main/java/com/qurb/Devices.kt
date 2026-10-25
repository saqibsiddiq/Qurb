package com.qurb

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import uniffi.qurb_mobile.PeerInfo
import java.io.File

/**
 * Devices' pictures, as the circle on Home shows them (the owner's list of
 * 2026-10-26, item 7): each device's own, as it chose it, kept here by its
 * fingerprint once it has said it; this phone's, as chosen in Settings. None
 * yet, the circle shows the first letter of the name.
 */
object Devices {
    private const val DIR = "pictures"

    /** Where a device's picture is kept here. */
    fun file(context: Context, fingerprint: String): File = File(File(context.filesDir, DIR), "$fingerprint.jpg")

    /** A device's picture, small enough for its circle, or null. */
    fun picture(context: Context, peer: PeerInfo?): Bitmap? {
        val file = file(context, peer?.fingerprint ?: return null)
        if (!file.isFile) return null
        return runCatching {
            BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = 2 })
        }.getOrNull()
    }
}
