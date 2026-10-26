package com.qurb

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import uniffi.qurb_mobile.PeerInfo
import uniffi.qurb_mobile.Qurb
import java.io.ByteArrayOutputStream
import java.util.concurrent.ConcurrentHashMap

/**
 * Devices' pictures, as their circles show them (the owner's list of
 * 2026-10-26, item 7): each device's own, as it chose it on itself and told
 * this phone at a sync; this phone's, as chosen in Settings. None yet, the
 * circle shows the first letter of the name.
 *
 * Read from the engine off the main thread by [load], and drawn from memory:
 * a picture is read and decoded again only when the device changes it, which
 * [PeerInfo.pictureAt] says without reading it.
 */
object Devices {
    /** The side of a picture this phone gives, in pixels: sharp in the
     *  largest circle (64 dp, about 190 px on the S23), and some 20 KB. */
    private const val SIDE = 256

    private class Drawn(val at: Long, val bitmap: Bitmap)

    private val drawn = ConcurrentHashMap<String, Drawn>()

    /** Read the pictures of `peers` that changed since they were drawn. Not on the main thread. */
    fun load(engine: Qurb, peers: List<PeerInfo>) {
        for (peer in peers) {
            val at = peer.pictureAt
            if (at == null) {
                drawn.remove(peer.fingerprint)
                continue
            }
            if (drawn[peer.fingerprint]?.at == at) continue
            val bytes = runCatching { engine.peerPicture(peer.fingerprint) }.getOrNull() ?: continue
            decode(bytes)?.let { drawn[peer.fingerprint] = Drawn(at, it) }
        }
    }

    /** A device's picture, as [load] last read it, or null. */
    fun picture(peer: PeerInfo?): Bitmap? = peer?.let { drawn[it.fingerprint]?.bitmap }

    fun decode(bytes: ByteArray): Bitmap? =
        runCatching { BitmapFactory.decodeByteArray(bytes, 0, bytes.size) }.getOrNull()

    /**
     * A picture from the photo picker as this phone gives it: the middle
     * square, [SIDE] pixels across, as a JPEG. Turned upright on Android 9
     * and later, whose decoder reads the camera's orientation; on 8 a photo
     * taken sideways stays sideways (a known gap -- the S23 runs 14).
     */
    fun encode(context: Context, uri: Uri): ByteArray {
        val whole = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, uri)) { decoder, info, _ ->
                decoder.setTargetSampleSize(maxOf(1, minOf(info.size.width, info.size.height) / SIDE))
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            }
        } else {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            context.contentResolver.openInputStream(uri)!!.use { BitmapFactory.decodeStream(it, null, bounds) }
            val sample = maxOf(1, minOf(bounds.outWidth, bounds.outHeight) / SIDE)
            context.contentResolver.openInputStream(uri)!!.use {
                BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample })
            } ?: error("not a picture")
        }
        val side = minOf(whole.width, whole.height)
        val square = Bitmap.createBitmap(whole, (whole.width - side) / 2, (whole.height - side) / 2, side, side)
        val small = Bitmap.createScaledBitmap(square, SIDE, SIDE, true)
        return ByteArrayOutputStream().use { out ->
            small.compress(Bitmap.CompressFormat.JPEG, 85, out)
            out.toByteArray()
        }
    }
}
