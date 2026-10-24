package com.qurb

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.AttributeSet
import android.view.View
import android.view.animation.LinearInterpolator
import androidx.annotation.ColorRes
import androidx.annotation.DrawableRes
import androidx.core.content.ContextCompat

/**
 * Home's status mark (the redesign's *Status mark*, docs/design/redesign.md):
 * a disc in the middle of the screen with a wide ring in the state's colour,
 * faint, and the state's icon drawn large and thin inside it.
 *
 * While syncing, part of the ring is lit in green and travels round it (§32):
 * a light moving, not a spinner, and still when the system asks for no
 * motion. The pulse when everything becomes synced (§33) is the screen's.
 *
 * Setup's *Approve on* page draws it smaller, travelling the same way while
 * the other device decides; the ring keeps Home's proportion, 8 dp at 168.
 */
class StatusMark @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    /** What the mark can show: its disc, its ring, and its icon's colour. */
    enum class Look(@ColorRes val fill: Int, @ColorRes val ring: Int, @ColorRes val tint: Int) {
        SYNCED(R.color.surface, R.color.healthy_ring, R.color.healthy),
        SYNCING(R.color.surface, R.color.neutral_bg, R.color.green),
        NEEDS_YOU(R.color.attention_bg, R.color.attention_ring, R.color.attention),
        OFF(R.color.neutral_bg, R.color.neutral_ring, R.color.neutral),
        WAITING(R.color.surface, R.color.neutral_ring, R.color.neutral),
        ERROR(R.color.error_bg, R.color.error_ring, R.color.error),
        /** No device yet: the first Home, whose one action is to add one. */
        FIRST(R.color.surface, R.color.green_soft, R.color.green),
    }

    private var look = Look.SYNCED
    private var icon = ContextCompat.getDrawable(context, R.drawable.mark_check)!!.mutate()

    private var ringWidth = resources.displayMetrics.density * 8
    private val disc = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val ring = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = ringWidth
    }
    private val light = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = ringWidth
        strokeCap = Paint.Cap.ROUND
        color = ContextCompat.getColor(context, R.color.green)
    }
    private val oval = RectF()

    /** Where the light is round the ring, in degrees from the top. */
    private var turned = 0f
    private var turning: ValueAnimator? = null

    fun show(look: Look, @DrawableRes icon: Int) {
        this.look = look
        this.icon = ContextCompat.getDrawable(context, icon)!!.mutate().apply {
            setTint(ContextCompat.getColor(context, look.tint))
        }
        disc.color = ContextCompat.getColor(context, look.fill)
        ring.color = ContextCompat.getColor(context, look.ring)
        val moving = look == Look.SYNCING && !Kit.calm()
        if (moving && turning == null) {
            turning = ValueAnimator.ofFloat(0f, 360f).apply {
                duration = 1600
                repeatCount = ValueAnimator.INFINITE
                interpolator = LinearInterpolator()
                addUpdateListener { turned = it.animatedValue as Float; invalidate() }
                start()
            }
        } else if (!moving) {
            turning?.cancel()
            turning = null
            turned = 0f
        }
        invalidate()
    }

    override fun onDetachedFromWindow() {
        turning?.cancel()
        turning = null
        super.onDetachedFromWindow()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        ringWidth = w * 8f / 168f
        ring.strokeWidth = ringWidth
        light.strokeWidth = ringWidth
    }

    override fun onDraw(canvas: Canvas) {
        val half = ringWidth / 2
        oval.set(half, half, width - half, height - half)
        canvas.drawOval(oval, disc)
        canvas.drawOval(oval, ring)
        if (look == Look.SYNCING) {
            // A third of the ring, from wherever the light has got to.
            canvas.drawArc(oval, turned - 90f, 120f, false, light)
        }
        val size = minOf(icon.intrinsicWidth, (width * 0.36f).toInt())
        val left = (width - size) / 2
        val top = (height - size) / 2
        icon.setBounds(left, top, left + size, top + size)
        icon.draw(canvas)
    }

    init {
        show(Look.SYNCED, R.drawable.mark_check)
    }
}
