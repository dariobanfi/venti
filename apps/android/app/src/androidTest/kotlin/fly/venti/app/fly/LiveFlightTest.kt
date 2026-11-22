package fly.venti.app.fly

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.longClick
import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.location.Criteria
import android.location.Location
import android.location.LocationManager
import android.os.Build
import android.os.SystemClock
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import fly.venti.app.LaunchOptions
import fly.venti.app.MainActivity
import fly.venti.app.TestDevice
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.concurrent.thread

/**
 * A flight recorded from the phone's own positions, on an emulator.
 *
 * The positions are made up and handed to Android as if its satellite
 * receiver had found them; from there on everything is the real thing:
 * the permission, the location manager, the foreground service and its
 * notification, the conversion of the altitude, the core's recorder, and
 * the logbook the flight ends up in. What an emulator cannot show stays
 * unverified: a real receiver, the screen off for hours, a battery saver.
 */
@RunWith(AndroidJUnit4::class)
class LiveFlightTest {
    @get:Rule
    val screen = createEmptyComposeRule()

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val locations = checkNotNull(context.getSystemService(LocationManager::class.java))
    private var app: ActivityScenario<MainActivity>? = null

    @Volatile
    private var isFlying = false

    private companion object {
        const val ELLIPSOID_HEIGHT = 1500.0

        /** The ground at the made-up take-off above the Tegernsee, above the ellipsoid, as the map's terrain has it. */
        const val TAKEOFF_GROUND_ELLIPSOID_HEIGHT = 1120.0

        /** How long the made-up flight's one circle takes, seconds: within the core's minute for a circle. */
        const val CIRCLE_SECONDS = 36
    }

    @Before
    @Suppress("DEPRECATION")
    fun becomeTheSatelliteReceiver() {
        val permissions = buildList {
            add(Manifest.permission.ACCESS_FINE_LOCATION)
            add(Manifest.permission.ACCESS_COARSE_LOCATION)
            if (Build.VERSION.SDK_INT >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
        }
        for (permission in permissions) {
            instrumentation.uiAutomation.grantRuntimePermission(context.packageName, permission)
        }
        TestDevice.keepSystemDialogsAway()
        TestDevice.shell("appops set ${context.packageName} android:mock_location allow")
        locations.addTestProvider(
            LocationManager.GPS_PROVIDER,
            false, true, false, false, true, true, true,
            Criteria.POWER_LOW, Criteria.ACCURACY_FINE,
        )
        locations.setTestProviderEnabled(LocationManager.GPS_PROVIDER, true)
    }

    @After
    fun stop() {
        isFlying = false
        app?.close()
        runCatching { locations.removeTestProvider(LocationManager.GPS_PROVIDER) }
    }

    /**
     * A glider over the Tegernsee valley, flying north at 11 m/s, one
     * position a second. Its height is given above the ellipsoid, as a
     * receiver reports it: 1,500 m there is about 1,454 m above the sea.
     */
    private fun fly() {
        isFlying = true
        thread(name = "made-up-positions") {
            var second = 0
            while (isFlying) {
                val position = Location(LocationManager.GPS_PROVIDER).apply {
                    latitude = 47.66 + second * 0.0001
                    longitude = 11.78
                    altitude = ELLIPSOID_HEIGHT
                    speed = 11f
                    bearing = 0f
                    accuracy = 4f
                    time = System.currentTimeMillis()
                    elapsedRealtimeNanos = SystemClock.elapsedRealtimeNanos()
                }
                runCatching { locations.setTestProviderLocation(LocationManager.GPS_PROVIDER, position) }
                second += 1
                Thread.sleep(1000)
            }
        }
    }

    private fun waitFor(what: SemanticsMatcher, seconds: Int = TestDevice.PATIENCE_SECONDS) {
        screen.waitUntil(seconds * 1000L) { screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty() }
    }

    private fun exists(what: SemanticsMatcher) = screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty()

    private fun tap(what: SemanticsMatcher) {
        screen.onAllNodes(what).onFirst().performClick()
    }

    /**
     * Turns the cockpit's pages as a pilot does, by a swipe to the left across
     * the cockpit, until the dots in the top line say this one shows.
     */
    private fun turnTo(page: String) {
        val dots = hasTestTag("cockpit-pages")
        waitFor(dots)
        repeat(4) {
            if (exists(dots and saying(page))) return
            screen.onNode(hasTestTag("cockpit")).performTouchInput { swipeLeft() }
            TestDevice.letTheAppRun(screen, 1)
        }
        waitFor(dots and saying(page))
    }

    private fun saying(text: String) = SemanticsMatcher("says \"$text\"") { node ->
        node.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    /** What an instrument shows, as a screen reader would read it: "1472 m". */
    private fun reading(instrument: String): String? =
        screen.onAllNodes(hasContentDescription(instrument)).fetchSemanticsNodes().firstOrNull()
            ?.config?.getOrNull(SemanticsProperties.StateDescription)

    /** Whether a view of the window asks Android to keep the screen on. */
    private fun keepsTheScreenOn(view: android.view.View): Boolean =
        view.keepScreenOn || (view is android.view.ViewGroup && (0 until view.childCount).any { keepsTheScreenOn(view.getChildAt(it)) })

    private fun recordingNotificationIsShown(): Boolean =
        checkNotNull(context.getSystemService(NotificationManager::class.java)).activeNotifications
            .any { it.notification.extras.getString("android.title") == "Recording your flight" }

    /** Seconds since the made-up positions began, as the thread that hands them over counts them. */
    @Volatile
    private var secondsFed = 0

    /**
     * Where the made-up receiver is in one second: metres north and east of
     * a take-off above the Tegernsee, the height above the ellipsoid, the
     * speed and the course.
     */
    private data class Fed(
        val north: Double,
        val height: Double,
        val speed: Float,
        val east: Double = 0.0,
        val bearing: Float = 0f,
    )

    /**
     * Hands Android a made-up position every second, as `where` says for
     * that second. A receiver's positions wander while it stands, by half
     * a metre here.
     */
    private fun feed(where: (Int) -> Fed) {
        isFlying = true
        thread(name = "made-up-flight") {
            var second = 0
            while (isFlying) {
                val fed = where(second)
                val wander = if (second % 2 == 0) 0.5 else 0.0
                val position = Location(LocationManager.GPS_PROVIDER).apply {
                    latitude = 47.664 + (fed.north + wander) / 111_195.0
                    longitude = 11.772 + fed.east / (111_195.0 * cos(Math.toRadians(47.664)))
                    altitude = fed.height
                    speed = fed.speed
                    bearing = fed.bearing
                    accuracy = 4f
                    time = System.currentTimeMillis()
                    elapsedRealtimeNanos = SystemClock.elapsedRealtimeNanos()
                }
                runCatching { locations.setTestProviderLocation(LocationManager.GPS_PROVIDER, position) }
                second += 1
                secondsFed = second
                Thread.sleep(1000)
            }
        }
    }

    /**
     * A flight that records itself: the cockpit waits on the ground and
     * writes nothing, takes off when the phone moves as a glider does,
     * lands when it stops, and two minutes later the flight is in the
     * logbook without a touch.
     */
    @Test
    fun aFlightRecordsItself() {
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(LaunchOptions.UI_TESTING, true)
            .putExtra(LaunchOptions.BUNDLED_MAP, true)
        app = ActivityScenario.launch(intent)
        // The app opens on Today; the Fly tab is the cockpit.
        waitFor(hasText("Fly") and isSelectable())
        tap(hasText("Fly") and isSelectable())
        waitFor(hasTestTag("cockpit-settings"))
        // Eight seconds at the take-off, then at 10 m/s: 100 m north, one
        // full circle in 36 s (what shows the core a glider: a lift or a
        // car that comes to rest without having flown is taken back), and
        // 100 m south, then standing where it started. On the
        // ground: the core does not end a flight while the terrain the app
        // knows lies more than 50 m below (it waits five minutes there),
        // and the emulator's barometer holds the height of the first fix,
        // so the whole flight is at the height of the ground at the
        // take-off. The map's terrain has it at some 1,070 m above the sea
        // (a recorded flight: 482 m over ground at 1,553 m, 100 m north),
        // which is some 1,120 m above the ellipsoid here.
        val radius = 10.0 * CIRCLE_SECONDS / (2 * PI)
        feed { second ->
            val t = second - 8
            when {
                t <= 0 -> Fed(0.0, TAKEOFF_GROUND_ELLIPSOID_HEIGHT, 0f)
                t <= 10 -> Fed(10.0 * t, TAKEOFF_GROUND_ELLIPSOID_HEIGHT, 10f)
                t <= 10 + CIRCLE_SECONDS -> {
                    // Clockwise, from its western point, heading north.
                    val turned = 360.0 * (t - 10) / CIRCLE_SECONDS
                    val angle = Math.toRadians(turned)
                    Fed(
                        north = 100.0 + radius * sin(angle),
                        height = TAKEOFF_GROUND_ELLIPSOID_HEIGHT,
                        speed = 10f,
                        east = radius - radius * cos(angle),
                        bearing = (turned % 360).toFloat(),
                    )
                }
                t <= 20 + CIRCLE_SECONDS -> Fed(
                    100.0 - 10.0 * (t - 10 - CIRCLE_SECONDS),
                    TAKEOFF_GROUND_ELLIPSOID_HEIGHT,
                    10f,
                    bearing = 180f,
                )
                else -> Fed(0.0, TAKEOFF_GROUND_ELLIPSOID_HEIGHT, 0f)
            }
        }

        // Standing at the take-off: ready, and nothing is recorded.
        waitFor(hasTestTag("cockpit-phase") and hasContentDescription("Ready for take-off"))
        assertTrue(exists(hasTestTag("cockpit-ready")))
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-self-1-ready")
        // What can be switched before the take-off, with "Record now".
        turnTo("Flight")
        waitFor(hasTestTag("cockpit-record-now"))
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-self-2-ready-flight-page")
        turnTo("Instruments")

        // In the air: recording by itself.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { exists(hasText("Recording")) && !exists(hasTestTag("cockpit-phase")) }
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-self-3-in-the-air")

        // Standing in the landing field: landed, and the cockpit says when
        // the flight will be saved.
        waitFor(hasTestTag("cockpit-phase") and saying("The flight is saved in"), seconds = 180)
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-self-4-landed")

        // Two minutes after it came to rest the flight is in the logbook,
        // and its page is open.
        waitFor(hasContentDescription("Flight details"), seconds = 240)
        isFlying = false
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-self-5-saved")
        tap(hasContentDescription("Back"))
        waitFor(hasTestTag("logbook-row"))
    }

    /**
     * A take-off while the app is open on another tab (the owner,
     * 2026-11-22): the take-off is watched for from any tab while the app
     * is in front, the cockpit comes to the front by itself and records,
     * with the screen kept on and the recording's service running.
     */
    @Test
    fun aTakeoffSeenFromAnotherTabOpensTheCockpitAndRecords() {
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(LaunchOptions.UI_TESTING, true)
            .putExtra(LaunchOptions.BUNDLED_MAP, true)
        app = ActivityScenario.launch(intent)
        // On Today, the cockpit not opened.
        waitFor(hasText("Today") and isSelectable())
        assertTrue(!exists(hasTestTag("cockpit")))
        // Standing, then moving as a glider does.
        feed { second ->
            val flown = (second - 8).coerceIn(0, 120)
            Fed(10.0 * flown, TAKEOFF_GROUND_ELLIPSOID_HEIGHT, if (second >= 9) 10f else 0f)
        }
        // In the air: the cockpit has come to the front, recording.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            exists(hasTestTag("cockpit")) && exists(hasText("Recording"))
        }
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { recordingNotificationIsShown() }
        // The screen is kept on while the cockpit shows.
        var keptOn = false
        checkNotNull(app).onActivity { keptOn = keepsTheScreenOn(it.window.decorView) }
        assertTrue("the screen is kept on", keptOn)
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-self-0-takeoff-seen-from-today")

        // Held for a second, "End" saves what was recorded.
        screen.onNode(hasTestTag("cockpit-end")).performTouchInput { longClick(durationMillis = 1_600) }
        isFlying = false
        waitFor(hasContentDescription("Flight details"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !recordingNotificationIsShown() }
    }

    @Test
    fun aFlightIsRecordedFromThePhonesPositionsAndSavedToTheLogbook() {
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(LaunchOptions.UI_TESTING, true)
            .putExtra(LaunchOptions.BUNDLED_MAP, true)
        app = ActivityScenario.launch(intent)
        // The app opens on Today; the Fly tab is the cockpit.
        waitFor(hasText("Fly") and isSelectable())
        tap(hasText("Fly") and isSelectable())
        // The cockpit waits on the ground and records from the take-off.
        waitFor(hasTestTag("cockpit-ready"))
        assertTrue(exists(hasTestTag("cockpit-phase") and hasContentDescription("Ready for take-off")))
        // Nothing has come from the receiver yet, and the cockpit says so.
        assertTrue(exists(hasContentDescription("No GPS")))
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-fly-6b-ready")

        // "Record now", on the flight's page, for a pilot who would rather
        // not wait for the take-off to be noticed.
        turnTo("Flight")
        tap(hasTestTag("cockpit-record-now"))
        waitFor(hasText("Recording"))
        turnTo("Instruments")

        fly()
        // Once positions arrive the warning goes and the instruments fill.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            // The value, then the course under it: "37 km/h, 302° NW".
            !exists(hasContentDescription("No GPS")) && reading("Ground speed")?.substringBefore(",")?.endsWith("km/h") == true
        }
        // Recording carries on with the screen off: the service is running
        // and shows its notification.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { recordingNotificationIsShown() }

        // The receiver reported height above the ellipsoid; the cockpit
        // shows height above the sea, some 46 m less in this valley.
        TestDevice.letTheAppRun(screen, seconds = 5)
        // Read as "1453 m, 238 m over ground": the altitude comes first.
        val altitude = checkNotNull(reading("Altitude")).substringBefore(" m").filter { it.isDigit() }.toInt()
        assertTrue("altitude $altitude", altitude in 1440..1470)
        // Under it stands the height over the ground, also in a flight
        // that is recorded: the map is drawn from the region that comes
        // with the app, and the cockpit reads the ground from it.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            reading("Altitude")?.endsWith(" m over ground") == true
        }
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-fly-7-recording")

        // Held for a second, "End" in the top line saves the flight; nothing asks again.
        screen.onNode(hasTestTag("cockpit-end")).performTouchInput { longClick(durationMillis = 1_600) }
        isFlying = false

        // The flight is in the logbook and opens; the service has stopped.
        waitFor(hasContentDescription("Flight details"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !recordingNotificationIsShown() }
        tap(hasContentDescription("Back"))
        waitFor(hasTestTag("logbook-row"))
    }
}
