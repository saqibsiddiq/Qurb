package com.qurb

import android.app.Application
import android.content.Context
import androidx.appcompat.app.AppCompatDelegate

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
        SYSTEM("As your phone is set", AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM),
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
}

/** The process: sets the theme before the first screen draws. */
class QurbApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        Appearance.apply(this)
    }
}
