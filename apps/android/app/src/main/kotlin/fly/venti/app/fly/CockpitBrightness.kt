package fly.venti.app.fly

import android.view.Window
import android.view.WindowManager

/**
 * How bright a screen is asked to be: from 0 for dark to 1 for all the way
 * up, or less than 0 for "as the phone is set".
 */
interface ScreenBrightness {
    var level: Float
}

/** The brightness a window asks for while it shows. It is the window's own: the phone's setting stays. */
class WindowBrightness(private val window: Window) : ScreenBrightness {
    override var level: Float
        get() = window.attributes.screenBrightness
        set(value) {
            window.attributes = window.attributes.apply { screenBrightness = value }
        }
}

/**
 * The cockpit's hold on the screen's brightness: all the way up while the
 * cockpit is open, and back to what it was before when it leaves.
 */
class CockpitBrightness(private val screen: ScreenBrightness) {
    /** How bright the screen was asked to be before the cockpit turned it up; `null` while it has not. */
    private var before: Float? = null

    /** Turns the screen all the way up. A second call keeps what the first found. */
    fun turnUp() {
        if (before == null) before = screen.level
        screen.level = FULL
    }

    /** Puts the brightness back to what it was. Does nothing where it was not turned up. */
    fun putBack() {
        before?.let { screen.level = it }
        before = null
    }

    companion object {
        /** All the way up. */
        const val FULL = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_FULL
    }
}
