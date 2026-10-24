package com.qurb

import android.Manifest
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.os.Bundle
import android.os.SystemClock
import android.view.View
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ExperimentalGetImage
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding
import androidx.lifecycle.lifecycleScope
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import com.qurb.databinding.ActivityScanBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.qurb_mobile.pairingWith
import java.util.concurrent.Executors

/**
 * Point the camera at a pairing code, as the redesign's QR scanner draws it:
 * the camera filling the screen, a frame in the middle and the rest dimmed,
 * Close and the torch at the top, *Type 6 digits* at hand. A Qurb code turns
 * the frame green and names the device showing it (decision 0074); any other
 * code turns it red, *Not a Qurb code*, and the camera goes on looking.
 *
 * The code carries the inviter's full fingerprint and has to travel *outside*
 * the network being paired — someone who can change what you see has already
 * won, so sending it over the link it authorises would defeat the point. A
 * camera reading a screen is exactly that: the bytes cross the room as light.
 *
 * Nothing is weakened by scanning rather than typing. It is the same code, the
 * same one-time token, the same five-minute expiry; only the transcription is
 * automated, and transcription is the part people get wrong.
 *
 * Gives back [EXTRA_CODE], or [EXTRA_TYPE] for the six digits instead.
 */
class ScanActivity : AppCompatActivity() {

    private lateinit var views: ActivityScanBinding
    private val analysis = Executors.newSingleThreadExecutor()
    private var camera: Camera? = null

    /** Set once a Qurb code is found, so a second frame does not finish twice. */
    @Volatile
    private var done = false

    /** When a code that is not Qurb's was last in sight: red until a moment after. */
    @Volatile
    private var otherSeenAt = 0L

    private val permission = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        // Not an error to dwell on: the digits can always be typed, so a
        // refusal costs convenience rather than the feature.
        if (granted) start() else views.hint.text = "Camera not allowed"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        views = ActivityScanBinding.inflate(layoutInflater)
        setContentView(views.root)
        // Under the bars, which are light on the camera's dark.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowCompat.getInsetsController(window, window.decorView).apply {
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = false
        }
        ViewCompat.setOnApplyWindowInsetsListener(views.overlay) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.updatePadding(top = bars.top, bottom = bars.bottom)
            insets
        }
        views.frame.viewfinder = views.viewfinder

        views.close.setOnClickListener { finish() }
        views.typeDigits.setOnClickListener {
            setResult(RESULT_OK, intent.putExtra(EXTRA_TYPE, true))
            finish()
        }
        views.torch.setOnClickListener {
            val on = camera?.cameraInfo?.torchState?.value != androidx.camera.core.TorchState.ON
            camera?.cameraControl?.enableTorch(on)
            views.torch.isActivated = on
            views.torch.backgroundTintList =
                if (on) ColorStateList.valueOf(0x66FFFFFF) else null
        }

        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA)
            == PackageManager.PERMISSION_GRANTED
        ) {
            start()
        } else {
            // Asked here rather than at launch: a sync app that demands the
            // camera on first run looks like it wants something else.
            permission.launch(Manifest.permission.CAMERA)
        }
    }

    // The camera frame's image is read for the barcode reader, which CameraX
    // marks as an API that may change.
    @androidx.annotation.OptIn(markerClass = [ExperimentalGetImage::class])
    private fun start() {
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            val provider = try {
                future.get()
            } catch (e: Exception) {
                android.util.Log.w("qurb", "could not open the camera", e)
                views.hint.text = "No camera"
                return@addListener
            }

            val preview = Preview.Builder().build()
                .also { it.setSurfaceProvider(views.preview.surfaceProvider) }

            // QR only. Restricting the formats is a real speed-up — the
            // scanner stops looking for a dozen barcode symbologies that a
            // pairing code will never be.
            val scanner = BarcodeScanning.getClient(
                com.google.mlkit.vision.barcode.BarcodeScannerOptions.Builder()
                    .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
                    .build()
            )

            val reader = ImageAnalysis.Builder()
                // Dropping frames is right here: the code is not going
                // anywhere, and a backlog would make the preview lag behind
                // what the camera is actually pointed at.
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()

            reader.setAnalyzer(analysis) { proxy ->
                val image = proxy.image
                if (image == null || done) {
                    proxy.close()
                    return@setAnalyzer
                }

                val input = InputImage.fromMediaImage(image, proxy.imageInfo.rotationDegrees)
                scanner.process(input)
                    .addOnSuccessListener { codes ->
                        val read = codes.mapNotNull { it.rawValue }
                        // Ours, a device's code or a guest's (decision 0060).
                        // A camera pointed at a room finds URLs, Wi-Fi codes
                        // and product labels: those say so, and it goes on.
                        val qurb = read.firstOrNull { ours(it) }
                        if (qurb != null && !done) {
                            done = true
                            found(qurb)
                        } else if (read.isNotEmpty() && !done) {
                            otherSeenAt = SystemClock.elapsedRealtime()
                            notOurs()
                        }
                    }
                    .addOnCompleteListener { proxy.close() }
            }

            try {
                provider.unbindAll()
                val bound = provider.bindToLifecycle(this, CameraSelector.DEFAULT_BACK_CAMERA, preview, reader)
                camera = bound
                // The torch only on a camera with a light.
                views.torch.visibility = if (bound.cameraInfo.hasFlashUnit()) View.VISIBLE else View.INVISIBLE
            } catch (e: Exception) {
                android.util.Log.w("qurb", "could not start the camera", e)
                views.hint.text = "No camera"
            }
        }, ContextCompat.getMainExecutor(this))
    }

    private fun ours(text: String): Boolean {
        val lower = text.trim().lowercase()
        return lower.startsWith("qurb1-") || lower.startsWith("qurbg1-")
    }

    /**
     * A Qurb code: the frame green, and the device showing it named when it
     * says who it is (decision 0074) -- asked here, a few seconds at most --
     * then back to whoever asked, which joins.
     */
    private fun found(code: String) {
        runOnUiThread {
            views.frame.look = ScanFrame.Look.FOUND
            views.hint.visibility = View.GONE
            views.typeDigits.visibility = View.GONE
            showResult(R.drawable.ic_circle_check, R.color.healthy, R.color.surface, R.drawable.ic_monitor_smartphone, "Found")
            lifecycleScope.launch {
                val with = withContext(Dispatchers.IO) {
                    runCatching {
                        Engine.hearingTheNetwork(this@ScanActivity) {
                            pairingWith(Engine.root(this@ScanActivity).absolutePath, code)
                        }
                    }.getOrNull()
                }
                val name = with?.name
                if (name != null) {
                    views.resultDevice.setImageResource(States.device(name, with.kind))
                    views.resultWords.text = name
                    delay(700)
                }
                setResult(RESULT_OK, intent.putExtra(EXTRA_CODE, code))
                finish()
            }
        }
    }

    /** A code, not Qurb's: red, said in four words, until it is out of sight. */
    private fun notOurs() {
        runOnUiThread {
            if (views.frame.look == ScanFrame.Look.NOT_OURS) return@runOnUiThread
            views.frame.look = ScanFrame.Look.NOT_OURS
            views.hint.visibility = View.GONE
            showResult(R.drawable.ic_triangle_alert, R.color.error, R.color.error_bg, null, "Not a Qurb code")
            lifecycleScope.launch {
                while (SystemClock.elapsedRealtime() - otherSeenAt < OTHER_FOR_MS) delay(250)
                if (done) return@launch
                views.frame.look = ScanFrame.Look.LOOKING
                views.result.visibility = View.GONE
                views.hint.visibility = View.VISIBLE
            }
        }
    }

    private fun showResult(mark: Int, markTint: Int, fill: Int, device: Int?, words: String) {
        views.resultMark.setImageResource(mark)
        views.resultMark.imageTintList = ColorStateList.valueOf(ContextCompat.getColor(this, markTint))
        views.resultDevice.visibility = if (device != null) View.VISIBLE else View.GONE
        device?.let { views.resultDevice.setImageResource(it) }
        views.resultWords.text = words
        views.result.backgroundTintList = ColorStateList.valueOf(ContextCompat.getColor(this, fill))
        views.result.visibility = View.VISIBLE
    }

    override fun onDestroy() {
        super.onDestroy()
        analysis.shutdown()
    }

    companion object {
        const val EXTRA_CODE = "code"
        /** Set when the person chose to type the six digits instead. */
        const val EXTRA_TYPE = "type"

        /** How long a code that is not Qurb's keeps the frame red after it was last seen. */
        private const val OTHER_FOR_MS = 1_500L
    }
}
