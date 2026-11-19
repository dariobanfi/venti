package fly.venti.app.fly

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

    private fun waitFor(what: SemanticsMatcher, seconds: Int = 20) {
        screen.waitUntil(seconds * 1000L) { screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty() }
    }

    private fun exists(what: SemanticsMatcher) = screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty()

    private fun tap(what: SemanticsMatcher) {
        screen.onAllNodes(what).onFirst().performClick()
    }

    /** What an instrument shows, as a screen reader would read it: "1,472 m". */
    private fun reading(instrument: String): String? =
        screen.onAllNodes(hasContentDescription(instrument)).fetchSemanticsNodes().firstOrNull()
            ?.config?.getOrNull(SemanticsProperties.StateDescription)

    private fun recordingNotificationIsShown(): Boolean =
        checkNotNull(context.getSystemService(NotificationManager::class.java)).activeNotifications
            .any { it.notification.extras.getString("android.title") == "Recording your flight" }

    @Test
    fun aFlightIsRecordedFromThePhonesPositionsAndSavedToTheLogbook() {
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(LaunchOptions.UI_TESTING, true)
            .putExtra(LaunchOptions.BUNDLED_MAP, true)
        app = ActivityScenario.launch(intent)
        waitFor(hasContentDescription("Show my position"), seconds = 90)
        tap(hasText("Fly") and isSelectable())
        waitFor(hasText("Start flight"))

        tap(hasText("Start flight"))
        waitFor(hasText("Finish flight"))
        assertTrue(exists(hasText("Recording")))
        // Nothing has come from the receiver yet, and the cockpit says so.
        assertTrue(exists(hasText("No position")))

        fly()
        // Once positions arrive the warning goes and the instruments fill.
        screen.waitUntil(30_000) { !exists(hasText("No position")) && reading("Speed")?.endsWith("km/h") == true }
        // Recording carries on with the screen off: the service is running
        // and shows its notification.
        screen.waitUntil(15_000) { recordingNotificationIsShown() }

        // The receiver reported height above the ellipsoid; the cockpit
        // shows height above the sea, some 46 m less in this valley.
        Thread.sleep(5000)
        val altitude = checkNotNull(reading("Altitude")).filter { it.isDigit() }.toInt()
        assertTrue("altitude $altitude", altitude in 1440..1470)
        screen.waitForIdle()
        TestDevice.keepScreenshot("light-fly-5-recording")

        tap(hasText("Finish flight"))
        waitFor(hasText("Save flight"))
        tap(hasText("Save flight"))
        isFlying = false

        // The flight is in the logbook and opens; the service has stopped.
        waitFor(hasContentDescription("Flight details"), seconds = 30)
        screen.waitUntil(15_000) { !recordingNotificationIsShown() }
        tap(hasContentDescription("Back"))
        waitFor(hasTestTag("logbook-row"))
    }
}
