package com.qurb

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.util.AttributeSet
import android.view.View
import androidx.core.content.ContextCompat

/**
 * Over the camera, as the redesign's QR scanner draws it: everything dimmed
 * but a square, and four rounded corners round that square -- white while it
 * looks, green when a Qurb code is found, red for a code that is not one.
 *
 * The square is wherever [viewfinder] is laid out, so the layout places it
 * and the words under it, and this only draws.
 */
class ScanFrame @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    enum class Look { LOOKING, FOUND, NOT_OURS }

    /** The square to leave clear, laid out elsewhere in the same window. */
    var viewfinder: View? = null
        set(value) {
            field = value
            value?.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> invalidate() }
            invalidate()
        }

    var look = Look.LOOKING
        set(value) {
            field = value
            corners.color = when (value) {
                Look.LOOKING -> Color.WHITE
                Look.FOUND -> ContextCompat.getColor(context, R.color.healthy)
                Look.NOT_OURS -> ContextCompat.getColor(context, R.color.error)
            }
            invalidate()
        }

    private val density = resources.displayMetrics.density

    /** Black at 45%, the design's veil. */
    private val veil = Paint().apply { color = Color.argb(115, 0, 0, 0) }

    /** 6 dp, round-capped, 40 dp along each side from a 16 dp corner. */
    private val corners = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 6 * density
        strokeCap = Paint.Cap.ROUND
        color = Color.WHITE
    }

    private val square = RectF()
    private val path = Path()
    private val here = IntArray(2)
    private val there = IntArray(2)

    override fun onDraw(canvas: Canvas) {
        val clear = viewfinder ?: return
        getLocationInWindow(here)
        clear.getLocationInWindow(there)
        val left = (there[0] - here[0]).toFloat()
        val top = (there[1] - here[1]).toFloat()
        // The corners' stroke runs 4 dp inside the viewfinder's edge, and the
        // clear square is inside that, as the design's 248 and 240 have it.
        val inset = 4 * density
        square.set(left + inset, top + inset, left + clear.width - inset, top + clear.height - inset)

        canvas.drawRect(0f, 0f, width.toFloat(), square.top, veil)
        canvas.drawRect(0f, square.bottom, width.toFloat(), height.toFloat(), veil)
        canvas.drawRect(0f, square.top, square.left, square.bottom, veil)
        canvas.drawRect(square.right, square.top, width.toFloat(), square.bottom, veil)

        val r = 16 * density
        val arm = 40 * density
        path.reset()
        with(square) {
            path.moveTo(left, top + arm); path.lineTo(left, top + r)
            path.arcTo(left, top, left + 2 * r, top + 2 * r, 180f, 90f, false); path.lineTo(left + arm, top)
            path.moveTo(right - arm, top); path.lineTo(right - r, top)
            path.arcTo(right - 2 * r, top, right, top + 2 * r, 270f, 90f, false); path.lineTo(right, top + arm)
            path.moveTo(right, bottom - arm); path.lineTo(right, bottom - r)
            path.arcTo(right - 2 * r, bottom - 2 * r, right, bottom, 0f, 90f, false); path.lineTo(right - arm, bottom)
            path.moveTo(left + arm, bottom); path.lineTo(left + r, bottom)
            path.arcTo(left, bottom - 2 * r, left + 2 * r, bottom, 90f, 90f, false); path.lineTo(left, bottom - arm)
        }
        canvas.drawPath(path, corners)
    }
}
