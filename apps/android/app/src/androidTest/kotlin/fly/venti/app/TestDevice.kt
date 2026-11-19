package fly.venti.app

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.RectF
import android.os.ParcelFileDescriptor
import android.view.View
import android.view.ViewGroup
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.maplibre.android.maps.MapView
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** What the UI tests ask of the phone or emulator they run on, rather than of the app. */
object TestDevice {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    /** Runs a command as the device's shell and waits for it to finish. */
    fun shell(command: String): String =
        ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(command))
            .use { it.readBytes().decodeToString() }

    /**
     * Keeps Android's own "isn't responding" dialogs off the screen. An
     * emulator that has just started is slow enough for its launcher to earn
     * one, and it would sit in the middle of every screenshot.
     */
    fun keepSystemDialogsAway() {
        shell("settings put global hide_error_dialogs 1")
        shell("am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS")
    }

    /**
     * Keeps a picture of the whole screen, the map and the system's bars
     * included, in the folder the build collects after the tests.
     */
    fun keepScreenshot(name: String) {
        val picture = checkNotNull(instrumentation.uiAutomation.takeScreenshot()) { "no screenshot for $name" }
        File(kept(), "$name.png").outputStream().use { picture.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    /** Keeps a line of text beside the screenshots. */
    fun keepNote(name: String, text: String) {
        File(kept(), "$name.txt").writeText(text + "\n")
    }

    /** The folder the build collects after the tests. */
    private fun kept(): File {
        val folder = InstrumentationRegistry.getArguments().getString("additionalTestOutputDir")?.let(::File)
            ?: checkNotNull(instrumentation.targetContext.getExternalFilesDir("screenshots"))
        folder.mkdirs()
        return folder
    }

    /**
     * How many features of one of the style's layers the map on screen has
     * drawn. For a layer of names that is the names it found room for,
     * which it can only place once their font has loaded. `null` if no map
     * is on screen.
     */
    fun <A : Activity> drawnOnTheMap(app: ActivityScenario<A>, layer: String): Int? {
        var count: Int? = null
        val asked = CountDownLatch(1)
        app.onActivity { activity ->
            val view = mapIn(activity.window.decorView)
            if (view == null) {
                asked.countDown()
            } else {
                view.getMapAsync { map ->
                    val whole = RectF(0f, 0f, view.width.toFloat(), view.height.toFloat())
                    count = map.queryRenderedFeatures(whole, layer).size
                    asked.countDown()
                }
            }
        }
        asked.await(10, TimeUnit.SECONDS)
        return count
    }

    private fun mapIn(view: View): MapView? = when (view) {
        is MapView -> view
        is ViewGroup -> (0 until view.childCount).firstNotNullOfOrNull { mapIn(view.getChildAt(it)) }
        else -> null
    }
}
