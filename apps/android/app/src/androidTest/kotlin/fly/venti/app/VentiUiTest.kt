package fly.venti.app

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Drives the app the way a pilot would, on an emulator, and keeps a
 * screenshot of every screen in light and dark appearance. The CI workflow
 * publishes the screenshots, so a change to the interface can be looked at
 * without a phone.
 *
 * The app is started with an empty logbook in a throwaway folder, and draws
 * its maps from the region shipped inside it.
 */
@RunWith(AndroidJUnit4::class)
class VentiUiTest {
    @get:Rule
    val screen = createEmptyComposeRule()

    private val context: Context = ApplicationProvider.getApplicationContext()
    private var app: ActivityScenario<MainActivity>? = null

    @Before
    fun prepareTheDevice() {
        TestDevice.keepSystemDialogsAway()
    }

    @After
    fun close() {
        app?.close()
    }

    private fun launch(bundledMap: Boolean = true, dark: Boolean = false, with: Intent.() -> Unit = {}) {
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(LaunchOptions.UI_TESTING, true)
            .putExtra(LaunchOptions.BUNDLED_MAP, bundledMap)
            .putExtra(LaunchOptions.DARK, dark)
            .apply(with)
        app = ActivityScenario.launch(intent)
    }

    private fun keepScreenshot(name: String) {
        screen.waitForIdle()
        TestDevice.keepScreenshot(name)
    }

    private fun waitFor(what: SemanticsMatcher, seconds: Int = 15) {
        screen.waitUntil(seconds * 1000L) { screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty() }
    }

    private fun exists(what: SemanticsMatcher) = screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty()

    private fun tap(what: SemanticsMatcher) {
        screen.onAllNodes(what).onFirst().performClick()
    }

    /** One of the four sections in the tab bar. */
    private fun tab(name: String) = hasText(name) and isSelectable()

    private fun described(text: String) = hasContentDescription(text)

    /** Something whose state, as a screen reader says it, contains this text. */
    private fun saying(text: String) = SemanticsMatcher("says \"$text\"") { node ->
        node.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    /** Lets the app run and the map finish drawing; see [TestDevice.letTheAppRun]. */
    private fun letTheMapDraw(seconds: Int = 4) {
        TestDevice.letTheAppRun(screen, seconds)
    }

    /** From an empty logbook to a flight on the map. */
    private fun addSampleFlightAndOpenIt(prefix: String) {
        waitFor(described("Show my position"), seconds = 90)
        letTheMapDraw()
        keepScreenshot("$prefix-1-map")

        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        keepScreenshot("$prefix-2-logbook-empty")

        tap(hasText("Add a sample flight"))
        // A single imported flight opens straight away.
        waitFor(described("Flight details"), seconds = 30)
        assertTrue(exists(described("Airtime")))
        letTheMapDraw(6)
        keepScreenshot("$prefix-3-flight")
    }

    /** A pilot imports a flight, inspects it and deletes it. */
    private fun importInspectAndDelete(prefix: String, dark: Boolean) {
        launch(dark = dark)
        addSampleFlightAndOpenIt(prefix)

        // Dragging along the altitude chart marks a moment, which stays.
        screen.onNode(hasTestTag("altitude-chart")).performTouchInput {
            down(percentOffset(0.15f, 0.5f))
            moveTo(percentOffset(0.2f, 0.5f))
            moveTo(percentOffset(0.3f, 0.5f))
            up()
        }
        waitFor(described("Show flight totals"))
        assertTrue(exists(described("In the air")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-4-flight-moment")

        tap(described("Show flight totals"))
        waitFor(described("Best climb"))

        tap(described("Flight details"))
        waitFor(hasText("Cross-country"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-5-flight-details")

        // Pulling the sheet up opens it fully; further down it lists every
        // thermal. Choosing one marks it on the map.
        screen.onNode(hasTestTag("flight-details")).performTouchInput { swipeUp() }
        screen.onNode(hasTestTag("flight-details")).performScrollToNode(hasTestTag("thermal-row"))
        waitFor(hasTestTag("thermal-row"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-5b-flight-thermals")
        tap(hasTestTag("thermal-row"))
        waitFor(described("Show flight totals"))
        assertTrue(exists(described("In the air")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-5c-flight-thermal-marked")
        tap(described("Show flight totals"))

        // Back to the logbook, where the flight is now listed.
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6-logbook")

        // The totals lead to the statistics.
        tap(hasTestTag("logbook-totals"))
        waitFor(hasText("Your bests"))
        assertTrue(exists(hasText("Longest flight")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6b-statistics")
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))

        // The map tab now shows the flight too.
        tap(tab("Map"))
        waitFor(described("Show my position"))
        letTheMapDraw()
        keepScreenshot("$prefix-7-map-with-flight")
        tap(tab("Logbook"))
        waitFor(hasTestTag("logbook-row"))

        // Pressing and holding a flight offers to delete it, and asks first.
        screen.onNode(hasTestTag("logbook-row")).performTouchInput { longClick() }
        waitFor(hasText("Delete"))
        tap(hasText("Delete"))
        waitFor(hasText("Delete flight"))
        keepScreenshot("$prefix-8-delete")
        tap(hasText("Delete flight"))
        waitFor(hasText("No flights yet"))
    }

    /**
     * The cockpit, fed by a replay of the sample flight at twenty times its
     * speed: about ninety seconds into the flight the glider starts to
     * circle in lift, and four minutes later it glides on. There the pilot
     * chooses a place to fly to. Then the settings, the waypoints and the
     * credits.
     */
    private fun flyAReplayAndVisitTheSettings(prefix: String, dark: Boolean) {
        launch(dark = dark) { putExtra(LaunchOptions.SAMPLE_WAYPOINTS, true) }
        waitFor(described("Show my position"), seconds = 90)
        tap(tab("Fly"))
        waitFor(hasText("Start flight"), seconds = 30)
        keepScreenshot("$prefix-fly-1-ready")

        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"), seconds = 20)

        // Circling in lift, the cockpit changes to its thermal numbers by
        // itself.
        waitFor(described("Gained"), seconds = 40)
        assertTrue(exists(described("Thermal")))
        letTheMapDraw(3)
        keepScreenshot("$prefix-fly-2-thermal")

        // On the glide after the thermal it changes back, now with the wind
        // it worked out while circling.
        waitFor(described("Glide"), seconds = 40)
        assertTrue(exists(described("Speed")))
        assertTrue(exists(described("Wind")))
        letTheMapDraw(3)
        keepScreenshot("$prefix-fly-3-glide")

        // The cockpit shows the way back to the take-off until the pilot
        // chooses a place; the waypoints are listed nearest first.
        val strip = hasTestTag("target-strip")
        assertTrue(exists(strip and saying("Take-off")))
        tap(strip)
        waitFor(hasText("Fly to"))
        waitFor(described("Sample landing field"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-4-fly-to")
        tap(described("Sample landing field"))
        waitFor(strip and saying("Sample landing field"), seconds = 10)
        assertTrue(exists(described("Needs")))
        letTheMapDraw(2)
        keepScreenshot("$prefix-fly-5-to-landing")

        tap(hasText("End replay"))
        waitFor(hasText("Close the cockpit"))
        keepScreenshot("$prefix-fly-6-end")
        tap(hasText("Close the cockpit"))
        waitFor(hasText("Start flight"))

        // Settings, and the credits the map data's licences ask for.
        tap(tab("Settings"))
        waitFor(hasText("Vario sound"))
        keepScreenshot("$prefix-settings-1")

        // The pilot's waypoints.
        tap(hasText("Waypoints"))
        waitFor(described("Sample turnpoint"))
        keepScreenshot("$prefix-settings-3-waypoints")
        tap(described("Back"))
        waitFor(hasText("Vario sound"))

        screen.onNode(hasText("Data and licences")).performScrollTo()
        tap(hasText("Data and licences"))
        waitFor(hasText("Base map"))
        keepScreenshot("$prefix-settings-2-credits")
        tap(described("Back"))
        waitFor(hasText("Vario sound"))
    }

    /**
     * The assistant. First the pilot's key is asked for; then, with the
     * provider played by a script, a question is answered: the real tool
     * loop runs against the logbook, but no key and no network are used.
     */
    private fun askTheAssistant(prefix: String, dark: Boolean) {
        launch(dark = dark) { putExtra(LaunchOptions.SCRIPTED_ASSISTANT, true) }
        waitFor(described("Show my position"), seconds = 90)
        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"), seconds = 30)
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))

        tap(tab("Assistant"))
        val question = hasText("How was my flying this year?")
        waitFor(question)
        keepScreenshot("$prefix-assistant-1-start")
        tap(question)

        waitFor(hasText("Your logbook has one flight", substring = true), seconds = 20)
        assertTrue(exists(hasText("Looked up your list of flights")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-assistant-2-answer")

        // The menu holds the models, a new conversation and the key.
        tap(described("Assistant options"))
        waitFor(hasText("Remove my key"))
        keepScreenshot("$prefix-assistant-3-options")
        tap(hasText("Remove my key"))

        // Without a key the screen asks for one and says what that means.
        waitFor(hasText("Save key"))
        assertTrue(exists(hasText("Get a key from Anthropic")))
        keepScreenshot("$prefix-assistant-4-key")
        screen.onNode(hasSetTextAction()).performTextInput("a-key-for-the-test")
        tap(hasText("Save key"))
        waitFor(question)
    }

    /**
     * The wind map, against a Venti server that answers from recorded data.
     * The workflow starts one and names it in the `ventiServer` argument;
     * without it the test is skipped. Here: `cargo run -p venti-server --
     * dev`, then the tests with
     * `-Pandroid.testInstrumentationRunnerArguments.ventiServer=http://10.0.2.2:8080`,
     * which is the address an emulator reaches its host by.
     */
    private fun lookAtTheWind(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        // The model provider is played by a script; the Venti server is real.
        launch(dark = dark) {
            putExtra(LaunchOptions.SERVER, server)
            putExtra(LaunchOptions.SCRIPTED_ASSISTANT, true)
        }
        val scenario = checkNotNull(app)

        waitFor(described("Wind stations"), seconds = 90)
        // The credit appears with the first answer from the server.
        waitFor(hasTestTag("wind-credits"), seconds = 60)
        assertTrue(exists(hasText("Wind: Deutscher Wetterdienst and GeoSphere Austria")))
        screen.waitUntil(30_000) { (TestDevice.drawnOnTheMap(scenario, "wind-stations") ?: 0) > 0 }
        letTheMapDraw()
        keepScreenshot("$prefix-wind-1-map")

        // A tap on a station's arrow opens the station, with the map still
        // in view. The Hohenpeißenberg stands alone, north of the others.
        val hill = checkNotNull(TestDevice.onScreen(scenario, lat = 47.8009, lon = 11.0108))
        TestDevice.tap(hill)
        waitFor(hasText("Hohenpeißenberg"), seconds = 15)
        waitFor(hasText("Last 24 hours"))
        letTheMapDraw(2)
        keepScreenshot("$prefix-wind-2-station-from-the-map")
        tap(hasText("Done"))
        waitFor(described("Wind stations"))

        // The same stations as a list, by name.
        tap(described("Wind stations"))
        waitFor(described("Zugspitze"))
        // A name the provider writes in capitals is shown in ordinary case.
        assertTrue(exists(described("Patscherkofel")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-wind-3-stations")

        // One station: what it measures, its last day, and whose data it is.
        tap(described("Zugspitze"))
        waitFor(hasTestTag("wind-chart"), seconds = 30)
        letTheMapDraw(1)
        keepScreenshot("$prefix-wind-4-station")
        screen.onNode(hasTestTag("station-page"))
            .performScrollToNode(hasText("Deutscher Wetterdienst", substring = true))
        assertTrue(exists(hasText("CC BY 4.0", substring = true)))

        // Back to the list, and from there to the map.
        tap(described("Back"))
        waitFor(described("Zugspitze"))
        tap(hasText("Done"))
        waitFor(described("Wind stations"))

        // The assistant looks up the same wind on the same server.
        tap(tab("Assistant"))
        val question = hasText("How is the wind on the Zugspitze right now?")
        waitFor(question)
        tap(question)
        waitFor(hasText("Zugspitze: 54 km/h", substring = true), seconds = 30)
        assertTrue(exists(hasText("Looked up the wind stations around a place")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-wind-5-assistant")

        // The settings hold the server's address, and the credits name
        // whose stations it delivers.
        tap(tab("Settings"))
        waitFor(hasText("Live wind"))
        screen.onNode(hasText("Data and licences")).performScrollTo()
        keepScreenshot("$prefix-wind-6-settings")
        tap(hasText("Data and licences"))
        waitFor(hasText("GeoSphere Austria"))
    }

    /**
     * Airspace: a made-up control zone lies beside the end of the sample
     * flight. Replayed at twenty times its speed, the glider comes near it
     * after about a minute, and the cockpit says so.
     */
    private fun flyNearAirspace(prefix: String, dark: Boolean) {
        launch(dark = dark) { putExtra(LaunchOptions.SAMPLE_AIRSPACE, true) }
        waitFor(described("Show my position"), seconds = 90)

        tap(tab("Fly"))
        waitFor(hasText("Start flight"), seconds = 30)
        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"), seconds = 20)

        val banner = hasTestTag("airspace-banner")
        waitFor(banner, seconds = 120)
        assertTrue(exists(banner and saying("SAMPLE ZONE (NOT REAL)")))
        letTheMapDraw(2)
        keepScreenshot("$prefix-fly-8-airspace")

        tap(banner)
        waitFor(hasText("Seen: quiet for 10 minutes"), seconds = 10)
        assertTrue(exists(hasText("Class D control zone", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-9-airspace-warnings")
        // "Seen" would speak again as the glider comes closer; "does not
        // apply today" stays quiet, which is what this test can rely on.
        tap(hasText("Does not apply today"))
        waitFor(described("Airspace warning, quieted"), seconds = 10)

        tap(hasText("End replay"))
        waitFor(hasText("Close the cockpit"))
        tap(hasText("Close the cockpit"))
        waitFor(hasText("Start flight"))

        // The files behind the warnings, and the zone on the map.
        tap(tab("Settings"))
        waitFor(hasText("Vario sound"))
        tap(hasText("Airspace"))
        waitFor(hasText("sample-zones-not-real"), seconds = 15)
        keepScreenshot("$prefix-settings-4-airspace")
        tap(described("Back"))
        waitFor(hasText("Vario sound"))
        tap(tab("Map"))
        waitFor(described("Show my position"))
        letTheMapDraw()
        keepScreenshot("$prefix-map-airspace")
    }

    @Test
    fun theCockpitWarnsOfAirspace() = flyNearAirspace("light", dark = false)

    @Test
    fun theAirspaceWarningsInDarkAppearance() = flyNearAirspace("dark", dark = true)

    @Test
    fun theMapShowsLiveWindFromAServer() = lookAtTheWind("light", dark = false)

    @Test
    fun theWindInDarkAppearance() = lookAtTheWind("dark", dark = true)

    @Test
    fun aPilotImportsAFlightInspectsItAndDeletesIt() = importInspectAndDelete("light", dark = false)

    /**
     * Towns and villages are named on the map, in the font shipped inside
     * the app. The map itself is asked which names it has placed: a
     * screenshot does not always tell, because one of the emulator's
     * software renderers draws none of the map's text.
     */
    @Test
    fun theMapNamesPlaces() {
        launch()
        waitFor(described("Show my position"), seconds = 90)
        val scenario = checkNotNull(app)
        screen.waitUntil(60_000) { (TestDevice.drawnOnTheMap(scenario, "place") ?: 0) > 0 }
    }

    @Test
    fun theAssistantAnswersFromTheLogbook() = askTheAssistant("light", dark = false)

    @Test
    fun theAssistantInDarkAppearance() = askTheAssistant("dark", dark = true)

    @Test
    fun theSameInDarkAppearance() = importInspectAndDelete("dark", dark = true)

    @Test
    fun theCockpitShowsAReplayedFlight() = flyAReplayAndVisitTheSettings("light", dark = false)

    @Test
    fun theCockpitInDarkAppearance() = flyAReplayAndVisitTheSettings("dark", dark = true)

    /** A flight file opened from another app goes into the logbook and is shown. */
    @Test
    fun aFlightFileOpenedFromAnotherAppIsShown() {
        val file = File(context.cacheDir, "opened-from-outside.igc")
        file.writeBytes(context.assets.open(AppFiles.SAMPLE_FLIGHT).use { it.readBytes() })

        launch {
            action = Intent.ACTION_VIEW
            data = Uri.fromFile(file)
        }

        waitFor(described("Flight details"), seconds = 90)
        assertTrue(exists(described("Airtime")))
        // It is in the logbook, under the flight that was opened.
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))
        screen.onNode(hasTestTag("logbook-totals")).assertIsDisplayed()
    }

    /** A file that is not a flight is named, with the core's reason. */
    @Test
    fun aFileThatIsNotAFlightIsReportedByName() {
        val file = File(context.cacheDir, "notes.igc")
        file.writeText("hello")

        launch {
            action = Intent.ACTION_VIEW
            data = Uri.fromFile(file)
        }

        waitFor(hasText("1 file could not be imported"), seconds = 90)
        assertTrue(exists(hasText("notes.igc", substring = true) and hasText("not a flight file", substring = true)))
        keepScreenshot("import-1-not-a-flight")
        tap(hasText("OK") and hasClickAction())
        // Nothing was added.
        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
    }

    /**
     * The same screens with tiles fetched from the public servers, as a
     * pilot's phone draws them. Only the screenshots matter here: the test
     * does not fail when the network is slow.
     */
    @Test
    fun theMapWithTilesFromTheNetwork() {
        launch(bundledMap = false)
        waitFor(described("Show my position"), seconds = 90)
        letTheMapDraw(12)
        keepScreenshot("online-1-map")

        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"), seconds = 30)
        letTheMapDraw(12)
        keepScreenshot("online-2-flight")
    }
}
