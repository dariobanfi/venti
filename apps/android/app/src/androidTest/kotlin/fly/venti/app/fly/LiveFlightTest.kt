package fly.venti.app.fly

import androidx.compose.ui.test.performTouchInput
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

    /** Turns the cockpit's pages with "Next page" until this one shows. */
    private fun turnTo(page: String) {
        val next = hasTestTag("cockpit-next-page")
        waitFor(next)
        repeat(3) {
            if (exists(next and saying(page))) return
            tap(next)
            TestDevice.letTheAppRun(screen, 1)
        }
        waitFor(next and saying(page))
    }

    private fun saying(text: String) = SemanticsMatcher("says \"$text\"") { node ->
        node.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    /** What an instrument shows, as a screen reader would read it: "1472 m". */
    private fun reading(instrument: String): String? =
        screen.onAllNodes(hasContentDescription(instrument)).fetchSemanticsNodes().firstOrNull()
            ?.config?.getOrNull(SemanticsProperties.StateDescription)

    private fun recordingNotificationIsShown(): Boolean =
        checkNotNull(context.getSystemService(NotificationManager::class.java)).activeNotifications
            .any { it.notification.extras.getString("android.title") == "Recording your flight" }

    /** Seconds since the made-up positions began, as the thread that hands them over counts them. */
    @Volatile
    private var secondsFed = 0

    /**
     * Hands Android a made-up position every second, as `where` says for
     * that second: metres north of a take-off above the Tegernsee, the
     * height above the ellipsoid and the speed. A receiver's positions
     * wander while it stands, by half a metre here.
     */
    private fun feed(where: (Int) -> Triple<Double, Double, Float>) {
        isFlying = true
        thread(name = "made-up-flight") {
            var second = 0
            while (isFlying) {
                val (north, height, speed) = where(second)
                val wander = if (second % 2 == 0) 0.5 else 0.0
                val position = Location(LocationManager.GPS_PROVIDER).apply {
                    latitude = 47.664 + (north + wander) / 111_195.0
                    longitude = 11.772
                    altitude = height
                    this.speed = speed
                    bearing = 0f
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
        waitFor(hasContentDescription("Show my position"))
        tap(hasText("Fly") and isSelectable())
        waitFor(hasText("Open the cockpit"))
        // Eight seconds at the take-off, forty-five of flight at 10 m/s
        // sinking a metre a second, then standing in the landing field.
        feed { second ->
            val flown = (second - 8).coerceIn(0, 45).toDouble()
            Triple(10 * flown, ELLIPSOID_HEIGHT + 100 - flown, if (second in 9..53) 10f else 0f)
        }
        tap(hasText("Open the cockpit"))

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

    @Test
    fun aFlightIsRecordedFromThePhonesPositionsAndSavedToTheLogbook() {
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(LaunchOptions.UI_TESTING, true)
            .putExtra(LaunchOptions.BUNDLED_MAP, true)
        app = ActivityScenario.launch(intent)
        waitFor(hasContentDescription("Show my position"))
        tap(hasText("Fly") and isSelectable())
        waitFor(hasText("Open the cockpit"))

        tap(hasText("Open the cockpit"))
        // The cockpit waits on the ground and records from the take-off.
        waitFor(hasTestTag("cockpit-ready"))
        assertTrue(exists(hasTestTag("cockpit-phase") and hasContentDescription("Ready for take-off")))
        // Nothing has come from the receiver yet, and the cockpit says so.
        assertTrue(exists(hasText("No GPS")))
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
            !exists(hasText("No GPS")) && reading("Ground speed")?.endsWith("km/h") == true
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

        // Held for a second, "Hold to end" saves the flight; nothing asks again.
        screen.onNode(hasTestTag("cockpit-end")).performTouchInput { longClick(durationMillis = 1_600) }
        isFlying = false

        // The flight is in the logbook and opens; the service has stopped.
        waitFor(hasContentDescription("Flight details"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !recordingNotificationIsShown() }
        tap(hasContentDescription("Back"))
        waitFor(hasTestTag("logbook-row"))
    }
}
