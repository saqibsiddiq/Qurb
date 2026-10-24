package com.qurb

import android.app.Activity
import android.app.Application
import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.BitmapDrawable
import android.view.Window
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.content.ContextCompat

/**
 * Light, dark, or as the system is set (decision 0056). Applied when the
 * process starts, before any screen draws, and again the moment it is chosen
 * in Settings -- AppCompat then recreates what is on screen in the new one.
 *
 * Kept in this phone's preferences: it is how this screen looks, not
 * something the person's other devices need to know.
 */
object Appearance {
    private const val PREFS = "appearance"
    private const val THEME = "theme"

    enum class Theme(val words: String, val mode: Int) {
        SYSTEM("Auto", AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM),
        LIGHT("Light", AppCompatDelegate.MODE_NIGHT_NO),
        DARK("Dark", AppCompatDelegate.MODE_NIGHT_YES),
    }

    fun chosen(context: Context): Theme {
        val name = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(THEME, null)
        return Theme.entries.firstOrNull { it.name == name } ?: Theme.SYSTEM
    }

    fun choose(context: Context, theme: Theme) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(THEME, theme.name).apply()
        AppCompatDelegate.setDefaultNightMode(theme.mode)
    }

    fun apply(context: Context) = AppCompatDelegate.setDefaultNightMode(chosen(context).mode)

    /**
     * The environment behind the glass (`drawable/environment`) as `activity`'s
     * window background, drawn once.
     *
     * Drawn as it is written -- a fill and four radial gradients, each across
     * the whole screen -- it was painted under every frame: on 2026-10-15 the
     * S23's overdraw view showed every pixel painted four times or more, and
     * its GPU took 14 ms for one frame in ten, against the 8.3 ms a frame has
     * at 120 Hz. Drawn once into a bitmap a quarter of the screen across and
     * stretched, it is one layer; the gradients are far too soft for the
     * stretch to show. Kept for the process, per size and light or dark.
     */
    fun paintEnvironment(activity: Activity) = paintEnvironment(activity, activity.window)

    /** [paintEnvironment], for a window of [context]'s that is not an
     *  activity's own: a page over the app, as joining from Devices is. */
    fun paintEnvironment(context: Context, window: Window) {
        val metrics = context.resources.displayMetrics
        val width = maxOf(1, metrics.widthPixels / ENVIRONMENT_SCALE)
        val height = maxOf(1, metrics.heightPixels / ENVIRONMENT_SCALE)
        val night = (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
            Configuration.UI_MODE_NIGHT_YES
        val key = Triple(width, height, night)
        val bitmap = environment?.takeIf { it.first == key }?.second ?: run {
            val layers = ContextCompat.getDrawable(context, R.drawable.environment) ?: return
            Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).also { drawn ->
                layers.setBounds(0, 0, width, height)
                layers.draw(Canvas(drawn))
                environment = key to drawn
            }
        }
        window.setBackgroundDrawable(BitmapDrawable(context.resources, bitmap).apply { isFilterBitmap = true })
    }

    /** How much smaller than the screen the environment is drawn. */
    private const val ENVIRONMENT_SCALE = 4

    private var environment: Pair<Triple<Int, Int, Boolean>, Bitmap>? = null
}

/** The process: sets the theme before the first screen draws. */
class QurbApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        Appearance.apply(this)
    }
}
