package fly.venti.app

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.PointF
import android.graphics.RectF
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapView
import org.maplibre.android.style.layers.Property
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
     * Gives the app what it would otherwise ask the pilot for in a dialog
     * of Android's own, which a test cannot answer: to use the position and
     * to show a notification, as a flight being recorded needs them.
     */
    fun allowToRecordAFlight() {
        val app = instrumentation.targetContext.packageName
        for (permission in listOf("ACCESS_FINE_LOCATION", "ACCESS_COARSE_LOCATION", "POST_NOTIFICATIONS")) {
            shell("pm grant $app android.permission.$permission")
        }
    }

    /**
     * Lets the app show notifications without asking, as saving the first
     * alarm would ask in a dialog of Android's own that a test cannot answer.
     */
    fun allowNotifications() {
        shell("pm grant ${instrumentation.targetContext.packageName} android.permission.POST_NOTIFICATIONS")
    }

    /**
     * Lets the app run for so long, and the map draw: the map draws on its
     * own thread and tells nobody when it is done.
     *
     * A test that simply slept would look at a frozen screen. Compose draws
     * a test's frames when the test asks for them, so while a test sleeps
     * nothing the app has changed reaches the screen, the map included.
     * This wait keeps asking.
     */
    fun letTheAppRun(screen: ComposeTestRule, seconds: Int) {
        val until = SystemClock.uptimeMillis() + seconds * 1000L
        screen.waitUntil((seconds + PATIENCE_SECONDS) * 1000L) { SystemClock.uptimeMillis() >= until }
        // The last frame the app asked for, drawn by the map as well.
        screen.waitForIdle()
        Thread.sleep(MAP_FRAME_MILLISECONDS)
    }

    /** How long a test waits for something before it gives up. See `VentiUiTest.waitFor`. */
    const val PATIENCE_SECONDS = 120

    /**
     * How long a test waits for what comes late in a replayed flight. The
     * sample flight takes a minute and a half at twenty times its speed,
     * and longer on a slow emulator: a replay moves on by two seconds of
     * the clock at most at once, however long the app was held.
     */
    const val WHOLE_FLIGHT_SECONDS = 480

    /** What the map's own thread is given to draw the frame it was last asked for. */
    private const val MAP_FRAME_MILLISECONDS = 400L

    /**
     * Keeps a picture of the whole screen, the map and the system's bars
     * included, in the folder the build collects after the tests.
     */
    fun keepScreenshot(name: String) {
        val picture = checkNotNull(instrumentation.uiAutomation.takeScreenshot()) { "no screenshot for $name" }
        File(kept(), "$name.png").outputStream().use { picture.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    /**
     * Keeps a picture of the screen as the app last asked for it. The last
     * frame is asked for and the map's own thread is given its time to
     * draw it. Between that and the picture no frame is asked for, so the
     * map and what stands around it show the same moment: taken straight
     * after the frame, a picture of the cockpit in a replay could show the
     * numbers of one refresh over the map of the one before, with the
     * glider beside the line that starts at it.
     *
     * With `app`, the map is asked first whether it has drawn all it was
     * given: a long track takes the emulator longer than the time every
     * picture waits.
     */
    fun <A : Activity> keepScreenshot(screen: ComposeTestRule, name: String, app: ActivityScenario<A>? = null) {
        screen.waitForIdle()
        if (app != null) {
            runCatching { letTheMapFinish(app) }
        }
        Thread.sleep(MAP_FRAME_MILLISECONDS)
        keepScreenshot(name)
    }

    /**
     * Waits until the map on screen has drawn everything it was given, and
     * no longer than `milliseconds`. The map takes what it is given in its
     * own time, on threads of its own, and says with every frame it draws
     * whether that frame had all of it; a map at rest draws none, so one
     * is asked for. Does nothing where no map is on screen.
     */
    fun <A : Activity> letTheMapFinish(app: ActivityScenario<A>, milliseconds: Long = MAP_FINISH_MILLISECONDS) {
        val finished = CountDownLatch(1)
        var found: MapView? = null
        val listener = MapView.OnDidFinishRenderingFrameListener { fully, _, _ ->
            if (fully) {
                finished.countDown()
            }
        }
        app.onActivity { activity ->
            val view = mapIn(activity.window.decorView)
            if (view == null) {
                finished.countDown()
            } else {
                found = view
                view.addOnDidFinishRenderingFrameListener(listener)
                view.getMapAsync { map -> map.triggerRepaint() }
            }
        }
        finished.await(milliseconds, TimeUnit.MILLISECONDS)
        app.onActivity { found?.removeOnDidFinishRenderingFrameListener(listener) }
    }

    /** The longest a picture waits for the map to have drawn everything. */
    private const val MAP_FINISH_MILLISECONDS = 2_000L

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

    /**
     * Whether the map on screen has one of its style's layers and shows
     * it: for a picture, which is no feature to count. False if no map is
     * on screen.
     */
    fun <A : Activity> showsOnTheMap(app: ActivityScenario<A>, layer: String): Boolean {
        var shows = false
        val asked = CountDownLatch(1)
        app.onActivity { activity ->
            val view = mapIn(activity.window.decorView)
            if (view == null) {
                asked.countDown()
            } else {
                view.getMapAsync { map ->
                    shows = map.style?.getLayer(layer)?.visibility?.getValue() == Property.VISIBLE
                    asked.countDown()
                }
            }
        }
        asked.await(10, TimeUnit.SECONDS)
        return shows
    }

    /**
     * Where on the screen the map shows a place, in pixels, or `null` if no
     * map is on screen.
     */
    fun <A : Activity> onScreen(app: ActivityScenario<A>, lat: Double, lon: Double): Pair<Int, Int>? {
        var place: Pair<Int, Int>? = null
        val asked = CountDownLatch(1)
        app.onActivity { activity ->
            val view = mapIn(activity.window.decorView)
            if (view == null) {
                asked.countDown()
            } else {
                view.getMapAsync { map ->
                    val inMap = map.projection.toScreenLocation(LatLng(lat, lon))
                    val corner = IntArray(2).also(view::getLocationOnScreen)
                    place = Pair(corner[0] + inMap.x.toInt(), corner[1] + inMap.y.toInt())
                    asked.countDown()
                }
            }
        }
        asked.await(10, TimeUnit.SECONDS)
        return place
    }

    /**
     * Turns the map on screen to a place and a zoom, where a pilot would
     * drag and pinch it. Returns false if no map is on screen.
     */
    fun <A : Activity> lookAt(app: ActivityScenario<A>, lat: Double, lon: Double, zoom: Double): Boolean {
        var moved = false
        val asked = CountDownLatch(1)
        app.onActivity { activity ->
            val view = mapIn(activity.window.decorView)
            if (view == null) {
                asked.countDown()
            } else {
                view.getMapAsync { map ->
                    map.moveCamera(CameraUpdateFactory.newLatLngZoom(LatLng(lat, lon), zoom))
                    moved = true
                    asked.countDown()
                }
            }
        }
        asked.await(10, TimeUnit.SECONDS)
        return moved
    }

    /**
     * A place on the screen, in pixels, where the map on screen has drawn a
     * feature of one of its layers, looked for in the middle of the map
     * (between a fifth and seven tenths of its width, a quarter and a half of
     * its height), clear of the chips above and of what stands at its foot.
     * `null` if there is none there, or no map.
     */
    fun <A : Activity> drawnInTheMiddle(app: ActivityScenario<A>, layer: String): Pair<Int, Int>? {
        var place: Pair<Int, Int>? = null
        val asked = CountDownLatch(1)
        app.onActivity { activity ->
            val view = mapIn(activity.window.decorView)
            if (view == null) {
                asked.countDown()
            } else {
                view.getMapAsync { map ->
                    val corner = IntArray(2).also(view::getLocationOnScreen)
                    val step = 24 * view.resources.displayMetrics.density
                    var y = view.height * 0.25f
                    search@ while (y <= view.height * 0.5f) {
                        var x = view.width * 0.2f
                        while (x <= view.width * 0.7f) {
                            if (map.queryRenderedFeatures(PointF(x, y), layer).isNotEmpty()) {
                                place = Pair(corner[0] + x.toInt(), corner[1] + y.toInt())
                                break@search
                            }
                            x += step
                        }
                        y += step
                    }
                    asked.countDown()
                }
            }
        }
        asked.await(10, TimeUnit.SECONDS)
        return place
    }

    /**
     * Makes the screen 360 dp wide and the text 130 % of its size: what
     * `design/AGENTS.md` asks every screen to be looked at with. A Pixel 6
     * is 1080 pixels wide, which are 360 dp at 480 dots an inch. Put back
     * with [asItWasBuilt], in a `finally`: the emulator keeps both for
     * every test after this one.
     */
    fun narrowWithLargeText() {
        shell("wm density 480")
        shell("settings put system font_scale 1.3")
    }

    /** The screen's own density and text at its own size again. */
    fun asItWasBuilt() {
        shell("wm density reset")
        shell("settings put system font_scale 1.0")
    }

    /**
     * Says something in the test's log for whoever reads it: a figure the
     * test measured. `am instrument -r` prints it as a status line,
     * `INSTRUMENTATION_STATUS: <name>=<words>`.
     */
    fun say(name: String, words: String) {
        instrumentation.sendStatus(0, Bundle().apply { putString(name, words) })
    }

    /** A tap with a finger, as the phone itself takes one. */
    fun tap(place: Pair<Int, Int>) {
        shell("input tap ${place.first} ${place.second}")
    }

    /** A finger held down in one place, long enough for the phone to take it as a long press. */
    fun longPress(place: Pair<Int, Int>) {
        shell("input swipe ${place.first} ${place.second} ${place.first} ${place.second} 1500")
    }

    private fun mapIn(view: View): MapView? = when (view) {
        is MapView -> view
        is ViewGroup -> (0 until view.childCount).firstNotNullOfOrNull { mapIn(view.getChildAt(it)) }
        else -> null
    }
}
