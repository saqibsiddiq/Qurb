package com.qurb

import android.content.res.ColorStateList
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import com.qurb.databinding.ScreenPageBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * This phone's profile, as WhatsApp's (the owner's review of 2026-11-02,
 * SE-2): its picture large, the camera on it to change it; its name under
 * it, the pencil to change it. What its computers show for this phone.
 */
class ProfileScreen(app: MainActivity) : Screen(app) {

    private val views = ScreenPageBinding.inflate(app.layoutInflater)
    override val view: View get() = views.root
    override val tab = R.id.tab_settings

    init {
        views.back.visibility = View.VISIBLE
        views.back.text = "Settings"
        views.back.setOnClickListener { app.onBackPressedDispatcher.onBackPressed() }
        views.title.text = "Profile"
        views.refresh.isEnabled = false
    }

    override fun refresh() {
        scope.launch {
            val profile = runCatching { withContext(Dispatchers.IO) { engine().ownProfile() } }.getOrNull()
            val name = profile?.name.orEmpty().ifEmpty { android.os.Build.MODEL ?: "Phone" }
            val picture = profile?.picture?.let { Devices.decode(it) }
            show(name, picture)
        }
    }

    private fun show(name: String, picture: android.graphics.Bitmap?) {
        val page = views.sections
        page.removeAllViews()
        val holder = FrameLayout(app)
        val big = FrameLayout(app)
        big.addView(kit.avatar(name, null, picture, 150), FrameLayout.LayoutParams(kit.dp(150), kit.dp(150)))
        big.addView(ImageView(app).apply {
            setImageResource(R.drawable.ic_camera)
            imageTintList = ColorStateList.valueOf(kit.color(R.color.surface))
            background = android.graphics.drawable.GradientDrawable().apply {
                shape = android.graphics.drawable.GradientDrawable.OVAL
                setColor(kit.color(R.color.green))
                setStroke(kit.dp(3), kit.color(R.color.bg))
            }
            setPadding(kit.dp(11), kit.dp(11), kit.dp(11), kit.dp(11))
            contentDescription = "Change picture"
            setOnClickListener { pictureSheet(picture != null) }
        }, FrameLayout.LayoutParams(kit.dp(46), kit.dp(46), Gravity.END or Gravity.BOTTOM))
        big.setOnClickListener { pictureSheet(picture != null) }
        holder.addView(big, FrameLayout.LayoutParams(kit.dp(150), kit.dp(150), Gravity.CENTER_HORIZONTAL))
        page.addView(holder, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = kit.dp(20) })

        val card = kit.card(page, top = 28)
        val row = kit.setting(card, R.drawable.ic_user_circle, "Name", name) { rename(name) }
        row.chevron.setImageResource(R.drawable.ic_pencil)
        row.chevron.imageTintList = ColorStateList.valueOf(kit.color(R.color.green))
        row.chevron.visibility = View.VISIBLE
        page.addView(TextView(app).apply {
            text = "Your computers show this name and picture for this phone."
            setTextAppearance(R.style.Text_Meta)
            setTextColor(kit.color(R.color.text_3))
            setPadding(kit.dp(6), kit.dp(10), kit.dp(6), 0)
        })
    }

    private fun rename(name: String) = kit.askName("Your name", name, "Save") { to ->
        change { it.setOwnName(to) }
    }

    private fun pictureSheet(has: Boolean) {
        val sheet = kit.sheet().title("Picture")
            .action(R.drawable.ic_camera, "Take a photo") {
                app.takePicture { bitmap -> change { it.setOwnPicture(Devices.encode(bitmap)) } }
            }
            .action(R.drawable.ic_image, "Choose from gallery") {
                app.pickPicture { uri ->
                    scope.launch {
                        val bytes = runCatching { withContext(Dispatchers.IO) { Devices.encode(app, uri) } }
                            .getOrElse { app.fail("Could not read that picture", it); return@launch }
                        change { it.setOwnPicture(bytes) }
                    }
                }
            }
        if (has) sheet.action(R.drawable.ic_trash_2, "Remove", danger = true) { change { it.setOwnPicture(null) } }
        sheet.show()
    }

    private fun change(work: (uniffi.qurb_mobile.Qurb) -> Unit) {
        scope.launch {
            runCatching { withContext(Dispatchers.IO) { work(engine()) } }
                .onSuccess { refresh() }
                .onFailure { app.fail("Could not change it", it) }
        }
    }
}
