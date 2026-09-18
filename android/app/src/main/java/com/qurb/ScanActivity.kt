package com.qurb

import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.core.content.ContextCompat
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import com.qurb.databinding.ActivityScanBinding
import java.util.concurrent.Executors

/**
 * Point the camera at a pairing code.
 *
 * The code carries the inviter's full fingerprint and has to travel *outside*
 * the network being paired — someone who can change what you see has already
 * won, so sending it over the link it authorises would defeat the point. A
 * camera reading a screen is exactly that: the bytes cross the room as light.
 *
 * Nothing is weakened by scanning rather than typing. It is the same code, the
 * same one-time token, the same five-minute expiry; only the transcription is
 * automated, and transcription is the part people get wrong.
 */
class ScanActivity : AppCompatActivity() {

    private lateinit var views: ActivityScanBinding
    private val analysis = Executors.newSingleThreadExecutor()

    /** Set once a code is found, so a second frame does not finish twice. */
    @Volatile
    private var done = false

    private val permission = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (granted) {
            start()
        } else {
            // Not an error to dwell on: the code can always be typed, so a
            // refusal costs convenience rather than the feature.
            views.hint.text = "Camera permission refused. Go back and type the code instead."
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        views = ActivityScanBinding.inflate(layoutInflater)
        setContentView(views.root)

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

    private fun start() {
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            val provider = try {
                future.get()
            } catch (e: Exception) {
                views.hint.text = "Could not open the camera: ${e.message}"
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
                        val found = codes.firstNotNullOfOrNull { it.rawValue }
                        // Only ours. A camera pointed at a room finds URLs,
                        // wifi codes and product labels, and handing any of
                        // them to the pairing routine would produce a baffling
                        // error instead of simply continuing to look.
                        if (found != null && found.startsWith("qurb1-") && !done) {
                            done = true
                            finishWith(found)
                        }
                    }
                    .addOnCompleteListener { proxy.close() }
            }

            try {
                provider.unbindAll()
                provider.bindToLifecycle(this, CameraSelector.DEFAULT_BACK_CAMERA, preview, reader)
            } catch (e: Exception) {
                views.hint.text = "Could not start the camera: ${e.message}"
            }
        }, ContextCompat.getMainExecutor(this))
    }

    private fun finishWith(code: String) {
        runOnUiThread {
            setResult(RESULT_OK, intent.putExtra(EXTRA_CODE, code))
            finish()
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        analysis.shutdown()
    }

    companion object {
        const val EXTRA_CODE = "code"
    }
}
