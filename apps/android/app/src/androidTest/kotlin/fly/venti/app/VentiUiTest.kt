package fly.venti.app

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.click
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.system.Os
import android.system.OsConstants
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isNotEnabled
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestWatcher
import org.junit.runner.Description
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
    private val context: Context = ApplicationProvider.getApplicationContext()
    private var app: ActivityScenario<MainActivity>? = null

    /**
     * Keeps a picture of the screen a test failed on, then closes the app.
     * It stands outside the rule below, so the app is still open when it
     * looks.
     */
    @get:Rule(order = 0)
    val closing = object : TestWatcher() {
        override fun failed(e: Throwable, description: Description) {
            runCatching { TestDevice.keepScreenshot("failed-${description.methodName}") }
        }

        override fun finished(description: Description) {
            app?.close()
        }
    }

    @get:Rule(order = 1)
    val screen = createEmptyComposeRule()

    @Before
    fun prepareTheDevice() {
        TestDevice.keepSystemDialogsAway()
    }

    private fun launch(
        bundledMap: Boolean = true,
        dark: Boolean = false,
        /** Whether the test begins where the app opens, on Today, and not on the map. */
        onToday: Boolean = false,
        with: Intent.() -> Unit = {},
    ) {
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(LaunchOptions.UI_TESTING, true)
            .putExtra(LaunchOptions.BUNDLED_MAP, bundledMap)
            .putExtra(LaunchOptions.DARK, dark)
            .apply(with)
        app = ActivityScenario.launch(intent)
        // The app opens on Today; most tests begin on the map. A file
        // handed over from another app opens what it brought instead.
        if (intent.action == null && !onToday) {
            waitFor(tab("Map"))
            tap(tab("Map"))
        }
    }

    private fun keepScreenshot(name: String) {
        TestDevice.keepScreenshot(screen, name, app)
    }

    /**
     * Waits for something to be on screen. It returns as soon as it is, so
     * the patience costs nothing; and it has to be long. On the emulator's
     * software renderer, with the cockpit redrawing five times a second, a
     * single look at the screen can take many seconds, and a wait gives up
     * after the first look that outlasts it.
     */
    private fun waitFor(what: SemanticsMatcher, seconds: Int = TestDevice.PATIENCE_SECONDS) {
        screen.waitUntil(seconds * 1000L) { screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty() }
    }

    /**
     * Waits for several things to be on screen at the same moment, with
     * one look at the screen for all of them. A replay goes on while a
     * test looks, and on the emulator a look can take seconds: looked for
     * one after the other, the first may be gone when the last is found.
     */
    private fun waitForAll(vararg what: SemanticsMatcher, seconds: Int = TestDevice.PATIENCE_SECONDS) {
        val any = what.reduce { one, other -> one or other }
        screen.waitUntil(seconds * 1000L) {
            val found = screen.onAllNodes(any).fetchSemanticsNodes()
            what.all { wanted -> found.any(wanted::matches) }
        }
    }

    private fun exists(what: SemanticsMatcher) = screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty()

    private fun tap(what: SemanticsMatcher) {
        screen.onAllNodes(what).onFirst().performClick()
    }

    /** One of the sections in the tab bar; not a day of the Today tab's switch, which says "Today" too. */
    private fun tab(name: String) = hasText(name) and isSelectable() and !hasAnyAncestor(hasTestTag("today-days"))

    /**
     * The settings or the assistant: pages of the Today tab, opened from
     * its corner. A tap on the tab while it is in front goes back to Today
     * itself, whatever page lay over it.
     */
    private fun openFromToday(name: String) {
        val opener = described(name) and hasClickAction()
        tap(tab("Today"))
        if (!exists(opener)) {
            tap(tab("Today"))
        }
        waitFor(opener)
        tap(opener)
    }

    private fun described(text: String) = hasContentDescription(text)

    /** Something whose state, as a screen reader says it, contains this text. */
    private fun saying(text: String) = SemanticsMatcher("says \"$text\"") { node ->
        node.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    /** Lets the app run and the map finish drawing; see [TestDevice.letTheAppRun]. */
    private fun letTheMapDraw(seconds: Int = 4) {
        TestDevice.letTheAppRun(screen, seconds)
    }

    /**
     * Ends the cockpit as a pilot does: a finger held on "Hold to end" for
     * longer than the second it takes. Nothing asks again.
     */
    private fun holdToEnd() {
        waitFor(hasTestTag("cockpit-end"))
        screen.onNode(hasTestTag("cockpit-end")).performTouchInput { longClick(durationMillis = 1_600) }
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("cockpit-end")) }
    }

    /** Turns the cockpit's pages with "Next page" until this one shows: "Instruments", "Map" or "Flight". */
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

    /** From an empty logbook to a flight on the map. */
    private fun addSampleFlightAndOpenIt(prefix: String) {
        waitFor(described("Show my position"))
        letTheMapDraw()
        keepScreenshot("$prefix-1-map")

        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        keepScreenshot("$prefix-2-logbook-empty")

        tap(hasText("Add a sample flight"))
        // A single imported flight opens straight away.
        waitFor(described("Flight details"))
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
        waitFor(described("Show my position"))
        tap(tab("Fly"))
        waitFor(hasText("Open the cockpit"))
        keepScreenshot("$prefix-fly-1-ready")

        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"))

        // Circling in lift, the cockpit changes to its thermal numbers by
        // itself, and its map pictures the thermal: the last circles as
        // dots around the circle to fly, in the thermal view beside the
        // map, and under it the words about where the lift is stronger. The sample's thermal lifts evenly all around.
        // At the map's other corner, the landing to think of first: the
        // one landing among the sample waypoints, well within reach from
        // up here, with the height the glider would arrive with. Under
        // the altitude stands the height over the ground: the map is
        // drawn from the region that comes with the app, and the ground
        // is read from that region's elevation. All of it is looked for
        // at one look: the flight goes on meanwhile.
        val landing = hasTestTag("landing-badge")
        waitForAll(
            described("Altitude") and saying(" m over ground"),
            described("Gained"),
            described("Thermal"),
            // Beside the map the thermal view, with the core's words about
            // the centre under it; the map keeps to the glide.
            hasTestTag("thermal-view") and saying("The lift of the circles flown"),
            hasTestTag("thermal-words") and saying("Centred"),
            landing and saying("Sample landing field, arriving +"),
        )
        letTheMapDraw(3)
        keepScreenshot("$prefix-fly-2-thermal")

        // A replay can be held at a moment, to look at it, and run on: the
        // word at the top is a menu of speeds.
        tap(hasTestTag("replay-speed"))
        waitFor(hasText("Paused"))
        tap(hasText("Paused"))
        waitFor(hasTestTag("replay-speed") and saying("Replay paused"))
        val held = screen.onNode(hasTestTag("flight-time")).fetchSemanticsNode().config
            .getOrNull(SemanticsProperties.Text)?.joinToString()
        letTheMapDraw(4)
        assertTrue(exists(hasTestTag("flight-time") and hasText(checkNotNull(held))))
        keepScreenshot("$prefix-fly-2a-paused")
        tap(hasTestTag("replay-speed"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(hasTestTag("replay-speed") and saying("Replay ×20"))

        // On the glide after the thermal it changes back, now with the wind
        // it worked out while circling. Beside the glide it is making, the
        // glide it needs to get home.
        waitForAll(
            described("Glide") and saying("Needs "),
            described("Ground speed"),
            described("Wind"),
            // The thermal view shows the track behind the glider now.
            hasTestTag("thermal-view") and saying("The track behind the glider"),
        )
        letTheMapDraw(3)
        keepScreenshot("$prefix-fly-3-glide")

        // The wind of every band of height the glider circled in, from a
        // tap on the wind over the map.
        tap(hasTestTag("wind-reading"))
        waitFor(hasText("Wind by height"))
        waitFor(hasTestTag("wind-by-height-row"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-3b-wind-by-height")
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasText("Wind by height")) }

        // The second page: the map as large as it gets. The whole flight
        // at a glance, with the scored route and where to go for an FAI
        // triangle; and back to the view that follows.
        turnTo("Map")
        letTheMapDraw(2)
        keepScreenshot("$prefix-fly-3a-map-page")
        tap(described("Show the whole flight"))
        waitFor(described("Follow the glider"))
        letTheMapDraw(3)
        keepScreenshot("$prefix-fly-3c-whole-flight")
        tap(described("Follow the glider"))
        waitFor(described("Show the whole flight"))

        // What lies ahead, seen from the side, from the second round
        // button: the ground of the region that comes with the app, the
        // glide drawn over it, and what the glide does over the ground
        // in words. The sample has no airspace, and the view says so.
        tap(described("Ahead, from the side"))
        waitForAll(
            hasTestTag("ahead-chart"),
            hasTestTag("ahead-words") and hasText("No airspace in the app"),
            hasTestTag("ahead-ground") and hasText("Your line", substring = true),
        )
        assertTrue(exists(hasText("it knows no trees, cables or buildings", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-3d-ahead")
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("ahead-chart")) }

        // A tap on the landing's badge lists the places to fly to.
        waitFor(landing)
        tap(landing)
        waitFor(hasText("Fly to"))
        tap(hasText("Cancel"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasText("Fly to")) }

        // The cockpit shows the way back to the take-off until the pilot
        // chooses a place; the waypoints are listed nearest first, each
        // with the height the glider is expected to arrive with, and the
        // list says how that is worked out. The summit sixty kilometres
        // away is out of reach.
        val strip = hasTestTag("target-strip")
        assertTrue(exists(strip and saying("Take-off")))
        tap(strip)
        waitFor(hasText("Fly to"))
        waitFor(described("Sample landing field"))
        assertTrue(exists(described("Sample summit far away") and saying("Out of reach")))
        assertTrue(exists(hasTestTag("reach-footer")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-4-fly-to")
        tap(described("Sample landing field"))
        waitFor(strip and saying("Sample landing field"))
        letTheMapDraw(2)
        keepScreenshot("$prefix-fly-5-to-landing")

        // The third page: the flight, and what can be switched.
        turnTo("Flight")
        waitFor(described("Flight time"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-5a-flight-page")

        // Locked, the screen answers no touch: a press on "Next page"
        // turns no page. Holding the lock opens it.
        tap(hasTestTag("cockpit-lock"))
        waitFor(hasTestTag("cockpit-unlock"))
        tap(hasTestTag("cockpit-next-page"))
        letTheMapDraw(2)
        assertTrue(exists(hasTestTag("cockpit-next-page") and saying("Flight")))
        keepScreenshot("$prefix-fly-5b-locked")
        screen.onNode(hasTestTag("cockpit-unlock")).performTouchInput { longClick(durationMillis = 1_600) }
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("cockpit-unlock")) }

        // Held for a second, "Hold to end" ends the replay; nothing asks again.
        holdToEnd()
        waitFor(hasText("Open the cockpit"))

        // Settings, and the credits the map data's licences ask for.
        openFromToday("Settings")
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
        waitFor(described("Show my position"))
        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"))
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))

        openFromToday("Assistant")
        val question = hasText("How was my flying this year?")
        waitFor(question)
        keepScreenshot("$prefix-assistant-1-start")
        tap(question)

        waitFor(hasText("Your logbook has one flight", substring = true))
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

        waitFor(described("Wind stations"))
        // The wind's credit appears with the server's first answer about
        // wind. The airspace's has a line of its own in the same box and
        // may be there before it.
        waitFor(hasText("Wind: Deutscher Wetterdienst and GeoSphere Austria"))
        assertTrue(exists(hasTestTag("wind-credits")))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { (TestDevice.drawnOnTheMap(scenario, "wind-stations") ?: 0) > 0 }
        letTheMapDraw()
        keepScreenshot("$prefix-wind-1-map")

        // A tap on a station's arrow opens the station, with the map still
        // in view. The Hohenpeißenberg stands alone, north of the others.
        val hill = checkNotNull(TestDevice.onScreen(scenario, lat = 47.8009, lon = 11.0108))
        TestDevice.tap(hill)
        waitFor(hasText("Hohenpeißenberg"))
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
        waitFor(hasTestTag("wind-chart"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-wind-4-station")
        screen.onNode(hasTestTag("station-page"))
            .performScrollToNode(hasText("Deutscher Wetterdienst", substring = true))
        assertTrue(exists(hasText("CC BY 4.0", substring = true)))

        // The forecast for the station's place, from the same server. On a
        // summit the model's ground lies well below the station, and the
        // forecast says so.
        screen.onNode(hasTestTag("station-page")).performScrollToNode(hasTestTag("station-forecast"))
        tap(hasTestTag("station-forecast"))
        waitFor(hasTestTag("forecast-table"))
        assertTrue(exists(hasText("The model's ground there is at", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-wind-4b-station-forecast")
        tap(described("Back"))
        waitFor(hasTestTag("station-page"))

        // Back to the list, and from there to the map.
        tap(described("Back"))
        waitFor(described("Zugspitze"))
        tap(hasText("Done"))
        waitFor(described("Wind stations"))

        // The assistant looks up the same wind on the same server.
        openFromToday("Assistant")
        val question = hasText("How is the wind on the Zugspitze right now?")
        waitFor(question)
        tap(question)
        waitFor(hasText("Zugspitze: 54 km/h", substring = true))
        assertTrue(exists(hasText("Looked up the wind stations around a place")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-wind-5-assistant")

        // The settings hold the server's address, and the credits name
        // whose stations it delivers.
        openFromToday("Settings")
        waitFor(hasText("Server address"))
        screen.onNode(hasText("Data and licences")).performScrollTo()
        keepScreenshot("$prefix-wind-6-settings")
        tap(hasText("Data and licences"))
        waitFor(hasText("GeoSphere Austria"))
    }

    /**
     * Airspace: a made-up control zone lies beside the valley the sample
     * flight crosses. Replayed at twenty times its speed, the glider comes
     * near it after about half a minute, and the cockpit says so. From then
     * on it stays near until it lands, and the last warning stays when the
     * replay has run out, so that this test finds a warning to look at
     * however long the emulator takes over each step.
     */
    private fun flyNearAirspace(prefix: String, dark: Boolean) {
        launch(dark = dark) { putExtra(LaunchOptions.SAMPLE_AIRSPACE, true) }
        waitFor(described("Show my position"))

        // Before take-off the Fly tab says what the cockpit will warn from.
        tap(tab("Fly"))
        waitFor(hasText("Open the cockpit"))
        assertTrue(exists(hasTestTag("airspace-before-takeoff") and hasText("1 file")))
        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"))

        val scenario = checkNotNull(app)
        val banner = hasTestTag("airspace-banner")
        waitFor(banner and saying("SAMPLE ZONE (NOT REAL)"), seconds = TestDevice.WHOLE_FLIGHT_SECONDS)
        // The zone itself is on the cockpit's map: the map is asked, since
        // a screenshot does not fail a test. The map has stepped back as
        // far as it takes to show the zone the warning is about.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { airspaceOnTheMap(scenario) > 0 }
        letTheMapDraw(2)
        keepScreenshot("$prefix-fly-8-airspace")

        // The line ahead from the side, with the zone as a block if it
        // lies on the line just now; which it does is the flight's to say.
        // Its button is over the map of the second page.
        turnTo("Map")
        tap(described("Ahead, from the side"))
        waitForAll(hasTestTag("ahead-chart"), hasTestTag("ahead-words"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-8b-ahead")
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("ahead-chart")) }

        tap(banner)
        waitFor(hasText("Seen: quiet for 10 minutes"))
        assertTrue(exists(hasText("Class D control zone", substring = true)))
        // The warning says which of the pilot's files it is from.
        assertTrue(exists(hasText("From your file “sample-zones-not-real”", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-9-airspace-warnings")
        // "Seen" would speak again as the glider comes closer; "does not
        // apply today" stays quiet, which is what this test can rely on.
        tap(hasText("Does not apply today"))
        waitFor(described("Airspace warning, quieted"))

        holdToEnd()
        waitFor(hasText("Open the cockpit"))

        // The files behind the warnings.
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        tap(hasText("Airspace"))
        waitFor(hasText("sample-zones-not-real"))
        keepScreenshot("$prefix-settings-4-airspace")
        tap(described("Back"))
        waitFor(hasText("Vario sound"))

        // And the zone on the map: with the sample flight in the logbook
        // the map shows the valley the flight ends in, beside the zone.
        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"))

        // The finished flight is checked against the same airspace: its
        // map draws the zone, and its details say when it was inside and
        // for how long. The check arrives a moment after the flight.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { airspaceOnTheMap(scenario) > 0 }
        tap(described("Flight details"))
        waitFor(hasTestTag("flight-details"))
        screen.onNode(hasTestTag("flight-details")).performTouchInput { swipeUp() }
        val crossing = hasTestTag("airspace-crossing-row")
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            runCatching { screen.onNode(hasTestTag("flight-details")).performScrollToNode(crossing) }.isSuccess
        }
        assertTrue(exists(crossing and hasText("SAMPLE ZONE (NOT REAL)")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-flight-airspace")
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("flight-details")) }

        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))
        tap(tab("Map"))
        waitFor(described("Show my position"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { airspaceOnTheMap(scenario) > 0 }
        letTheMapDraw(5)
        keepScreenshot("$prefix-map-airspace")

        // With the whole zone in view the map names it, with its floor and
        // its ceiling under the name. The name stands in the middle of the
        // zone, which the map of the flight beside it does not reach.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.686, lon = 11.728, zoom = 11.5))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "airspace-names") ?: 0) > 0
        }
        letTheMapDraw(3)
        keepScreenshot("$prefix-map-airspace-named")
    }

    /** How many closed airspaces the map on screen has drawn: their washes and their edges. */
    private fun airspaceOnTheMap(scenario: ActivityScenario<MainActivity>): Int =
        (TestDevice.drawnOnTheMap(scenario, "airspace-wash-2") ?: 0) +
            (TestDevice.drawnOnTheMap(scenario, "airspace-edge-2") ?: 0)

    /** How many airspaces of any kind the map on screen has drawn the edge of. */
    private fun airspaceEdgesOnTheMap(scenario: ActivityScenario<MainActivity>): Int =
        (0..2).sumOf { rank -> TestDevice.drawnOnTheMap(scenario, "airspace-edge-$rank") ?: 0 }

    /**
     * Airspace from the pilot's server: with a server set, the app fetches
     * the airspace around the map by itself and keeps it. The map draws it
     * and names whose it is; the Fly tab says before take-off what the
     * cockpit will warn from; and the list shows it with its provider's
     * credit and notice, and the switch that turns it off. Skipped without
     * a server, like the wind.
     */
    private fun takeAirspaceFromTheServer(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))

        // The map asked for the airspace around what it shows.
        waitFor(hasText("Airspace: openAIP, not certified"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { airspaceEdgesOnTheMap(scenario) > 0 }
        letTheMapDraw(3)
        keepScreenshot("$prefix-airspace-1-map")

        tap(tab("Fly"))
        val line = hasTestTag("airspace-before-takeoff")
        waitFor(line and hasText("openAIP", substring = true))
        keepScreenshot("$prefix-airspace-2-before-takeoff")

        tap(line)
        waitFor(hasTestTag("airspace-from-server"))
        assertTrue(exists(hasText("OpenAIP data is not certified", substring = true)))
        assertTrue(exists(hasText("Licence: CC BY-NC 4.0.", substring = true)))
        keepScreenshot("$prefix-airspace-3-from-server")

        // Switched off, what was kept is gone, and the Fly tab says so.
        screen.onNode(hasText("Airspace from Venti")).performScrollTo()
        tap(hasText("Airspace from Venti"))
        waitFor(hasText("Switched off: only your own files are used."))
        assertFalse(exists(hasTestTag("airspace-from-server")))
        tap(described("Back"))
        waitFor(line and hasText("None"))
        assertTrue(exists(hasText("Without airspace the cockpit warns of none.", substring = true)))
    }

    /**
     * A task: the made-up one along the sample flight is kept, the Fly tab
     * says so before take-off and shows it. Replayed at twenty times its
     * speed, the flight takes the start a few seconds in, passes the
     * turnpoint and reaches the goal after a minute and a half; the
     * cockpit's line counts along. Then a route of the pilot's own
     * waypoints replaces the task.
     */
    private fun flyATask(prefix: String, dark: Boolean) {
        launch(dark = dark) {
            putExtra(LaunchOptions.SAMPLE_TASK, true)
            putExtra(LaunchOptions.SAMPLE_WAYPOINTS, true)
        }
        waitFor(described("Show my position"))
        tap(tab("Fly"))
        waitFor(hasText("Open the cockpit"))
        val line = hasTestTag("task-before-takeoff")
        assertTrue(exists(line and hasText("4 turnpoints", substring = true)))
        tap(line)
        waitFor(hasTestTag("task-sketch"))
        assertTrue(exists(hasContentDescription("Sample turnpoint west", substring = true)))
        assertTrue(exists(hasText("along the shortest way through the edges of its cylinders", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-task-1-task")
        tap(described("Back"))
        waitFor(hasText("Open the cockpit"))

        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"))

        // The start is taken two minutes into the flight; from then on the
        // line runs the clock and says which turnpoint is next.
        val strip = hasTestTag("task-strip")
        // The task's line is on the second page, over the line that says
        // where to fly.
        turnTo("Map")
        waitFor(strip)
        waitFor(strip and saying("On task"))
        letTheMapDraw(2)
        keepScreenshot("$prefix-task-2-cockpit")

        // Every turnpoint, with whether it is reached, and the two buttons
        // for when the task and the flight have come apart.
        tap(strip)
        waitFor(hasTestTag("task-progress"))
        assertTrue(exists(hasContentDescription("Sample turnpoint west", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-task-3-progress")
        // The buttons stand below the turnpoints, where a list that is
        // longer than the screen has not drawn them yet.
        screen.onNode(hasTestTag("task-progress")).performScrollToNode(hasTestTag("task-skip"))
        assertTrue(exists(hasText("Skip to the next turnpoint")))
        assertTrue(exists(hasText("Back to the one before")))
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("task-progress")) }

        // The flight reaches the goal, and the line says so.
        waitFor(strip and saying("In goal"), seconds = TestDevice.WHOLE_FLIGHT_SECONDS)
        letTheMapDraw(2)
        keepScreenshot("$prefix-task-4-in-goal")
        holdToEnd()
        waitFor(hasText("Open the cockpit"))

        // A route of the pilot's own waypoints, tapped in order, replaces it.
        tap(line)
        waitFor(hasTestTag("task-sketch"))
        screen.onNode(hasTestTag("task-plan-route")).performScrollTo()
        tap(hasTestTag("task-plan-route"))
        waitFor(described("Sample landing field"))
        tap(described("Sample landing field"))
        tap(described("Sample turnpoint"))
        waitFor(hasTestTag("route-keep") and isEnabled())
        letTheMapDraw(1)
        keepScreenshot("$prefix-task-5-route-planner")
        tap(hasTestTag("route-keep"))
        waitFor(hasText("through its waypoints", substring = true))
        letTheMapDraw(1)
        keepScreenshot("$prefix-task-6-route")
        tap(described("Back"))
        waitFor(line and hasText("2 waypoints", substring = true))
    }

    /**
     * The wind over the cockpit's map says how old it is once that is five
     * minutes, and under it stands a line while it leaves the glider little
     * way into it. The sample flight's wind is 11 km/h, which no glider the
     * settings offer is slow enough for: the test flies one that trims at
     * 18 km/h.
     */
    private fun flyASlowGliderInTheWind(prefix: String, dark: Boolean) {
        launch(dark = dark) { putExtra(LaunchOptions.SLOW_GLIDER, true) }
        waitFor(described("Show my position"))
        tap(tab("Fly"))
        waitFor(hasText("Open the cockpit"))
        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"))

        // With the first wind, measured in the thermal, the line is there,
        // in the core's words.
        waitForAll(hasTestTag("strong-wind-badge"), described("Strong wind") and saying("Into the wind "))
        // On a glide, five minutes after the wind was last measured, the
        // wind says its age, and the line still stands under it. The glide
        // after the second thermal is the long one, and its wind leaves
        // 7 km/h; should the emulator be too slow to look in time, both
        // stand again once the flight has ended.
        val old = hasTestTag("wind-reading") and saying(" old")
        val strong = hasTestTag("strong-wind-badge") and saying("Into the wind 7 km/h")
        waitForAll(old, strong, seconds = TestDevice.WHOLE_FLIGHT_SECONDS)
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-7-old-and-strong-wind")

        holdToEnd()
        waitFor(hasText("Open the cockpit"))
    }

    /**
     * A flight of the logbook once more, in the cockpit: from the flight's
     * menu, at one of three speeds, played from the flight's own file.
     * Nothing is recorded: when the cockpit closes the pilot is back on
     * the flight, and the logbook has it once.
     */
    @Test
    fun aFlightOfTheLogbookIsReplayedInTheCockpit() {
        launch()
        waitFor(described("Show my position"))
        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"))

        // Material's menus do not open one out of another: the menu changes
        // into the choice of speeds.
        tap(described("More"))
        waitFor(hasText("Replay in the cockpit"))
        tap(hasText("Replay in the cockpit"))
        waitFor(hasText("10 times faster"))
        assertTrue(exists(hasText("In real time")))
        assertFalse(exists(hasText("Delete flight")))
        letTheMapDraw(1)
        keepScreenshot("replay-1-speeds")
        tap(hasText("20 times faster"))

        // The cockpit a pilot flies with. What is replayed is not shared:
        // there is no button for live tracking.
        waitForAll(described("Altitude"), hasTestTag("replay-speed") and saying("Replay ×20"))
        assertFalse(exists(hasTestTag("live-button")))
        // The picture is taken once the thermal is centred and stays so.
        // At twenty times the speed the map on the emulator is drawn a
        // good part of a circle behind the numbers; while the circle is
        // still being found, the ring would stand beside what the badge
        // says of it.
        waitForAll(described("Gained"), described("Lift") and saying("Centred"))
        letTheMapDraw(2)
        keepScreenshot("replay-2-cockpit")

        holdToEnd()
        // Back on the flight it was started from.
        waitFor(described("Flight details"))
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))
        assertTrue(screen.onAllNodes(hasTestTag("logbook-row")).fetchSemanticsNodes().size == 1)
    }

    @Test
    fun theCockpitSaysHowOldTheWindIsAndWhenItLeavesLittleWay() = flyASlowGliderInTheWind("light", dark = false)

    @Test
    fun theOldAndStrongWindInDarkAppearance() = flyASlowGliderInTheWind("dark", dark = true)

    @Test
    fun theCockpitFliesATask() = flyATask("light", dark = false)

    @Test
    fun theTaskInDarkAppearance() = flyATask("dark", dark = true)

    /**
     * A task file opened from another app is kept, said so, and found on
     * the Fly tab; a file of that name that holds no task is said too.
     */
    @Test
    fun aTaskFileOpenedFromAnotherAppIsKept() {
        val file = File(context.cacheDir, "sample-task.xctsk")
        file.writeText(LaunchOptions.SAMPLE_TASK_FILE)
        launch {
            action = Intent.ACTION_VIEW
            data = Uri.fromFile(file)
        }
        waitFor(hasText("Task kept"))
        keepScreenshot("task-7-opened-from-another-app")
        tap(hasText("OK") and hasClickAction())
        waitFor(hasTestTag("task-before-takeoff") and hasText("4 turnpoints", substring = true))
        app?.close()

        file.writeText("hello")
        launch {
            action = Intent.ACTION_VIEW
            data = Uri.fromFile(file)
        }
        waitFor(hasText("The task could not be read"))
        tap(hasText("OK") and hasClickAction())
    }

    /** From the map to the account screen, by way of the settings. */
    private fun openTheAccount() {
        waitFor(described("Show my position"))
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        screen.onNode(hasTestTag("account-link")).performScrollTo()
        assertTrue(exists(hasTestTag("account-link") and hasText("Not signed in")))
        tap(hasTestTag("account-link"))
        waitFor(hasTestTag("account-handle"))
    }

    /**
     * An account on a Venti server: signing in as the development server's
     * demo pilot brings that pilot's three flights into the logbook, and
     * one of them is shared with a link. Skipped without a server, like
     * the wind.
     */
    private fun signInAndShareAFlight(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) {
            putExtra(LaunchOptions.SERVER, server)
            putExtra(LaunchOptions.SAMPLE_WAYPOINTS, true)
        }
        openTheAccount()
        // The form says what an account does before it asks for anything.
        assertTrue(exists(hasText("A flight stays private until you share it.", substring = true)))
        keepScreenshot("$prefix-account-1-sign-in")

        // The demo pilot of the server's development mode; neither the
        // handle nor the password is a secret.
        screen.onNode(hasTestTag("account-handle")).performTextInput("demo")
        screen.onNode(hasTestTag("account-password")).performTextInput("demo-pilot-password")
        tap(hasTestTag("account-submit"))

        // Signed in, and the sync that follows has fetched the flights.
        waitFor(hasText("@demo"))
        waitFor(hasText("Sync now"))
        waitFor(hasText("In step since"))
        assertTrue(exists(hasText("Flights on Venti")))
        assertTrue(exists(hasTestTag("account-last-sync")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-account-2-signed-in")
        tap(described("Back"))
        waitFor(hasTestTag("account-link") and hasText("@demo"))

        tap(tab("Logbook"))
        waitFor(hasTestTag("logbook-row"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-account-3-logbook")

        // Share the newest flight with those who have its link. Whatever
        // the demo pilot had chosen for it before, that is the choice now.
        tap(hasTestTag("logbook-row"))
        waitFor(described("Flight details"))
        tap(described("More"))
        waitFor(hasText("Share with a link"))
        tap(hasText("Share with a link"))
        val withTheLink = hasTestTag("share-unlisted")
        waitFor(withTheLink)
        tap(withTheLink)
        waitFor(withTheLink and isSelected())
        waitFor(hasTestTag("share-link") and hasText("/f/", substring = true))
        letTheMapDraw(1)
        keepScreenshot("$prefix-account-4-share")
        tap(hasText("Done"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(withTheLink) }

        // With an account, deleting a flight reaches the server and the
        // pilot's other devices, and the question says so. Not answered
        // with yes: the next test needs the demo pilot's flights.
        tap(described("More"))
        waitFor(hasText("Delete flight"))
        tap(hasText("Delete flight"))
        waitFor(hasText("from Venti and from your other devices", substring = true))
        keepScreenshot("$prefix-account-5-delete")
        tap(hasText("Cancel"))
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))

        // Something another pilot shares that should not be there is
        // reported to whoever runs the server, from the account signed in
        // above: from the flight's menu, with a reason. The server's
        // development mode keeps a report until it is stopped, and a
        // second report about the same flight is not a new one, so the two
        // appearances report two pilots' flights.
        tap(tab("League"))
        val theirs = hasTestTag("public-flight-row") and described(if (dark) "Mara Muster" else "Jonas Beispiel")
        waitFor(theirs)
        screen.onAllNodes(theirs).onFirst().performClick()
        waitFor(described("Flight details"))
        tap(described("More"))
        waitFor(hasText("Report this flight"))
        tap(hasText("Report this flight"))
        val reason = hasTestTag("report-reason") and hasText("Not a real flight")
        waitFor(reason)
        // Nothing can be sent before a reason is chosen.
        assertTrue(exists(hasTestTag("report-send") and isNotEnabled()))
        assertFalse(exists(hasTestTag("report-obstacle")))
        tap(reason)
        waitFor(reason and isSelected())
        letTheMapDraw(1)
        keepScreenshot("$prefix-account-6-report")
        // Under the field for the pilot's own words stands what happens
        // to a report, and under that the button that sends it.
        screen.onNode(hasTestTag("report-send")).performScrollTo()
        assertTrue(exists(hasTestTag("report-notice") and hasText("sees that it came from you", substring = true)))
        waitFor(hasTestTag("report-send") and isEnabled())
        letTheMapDraw(1)
        keepScreenshot("$prefix-account-7-report-notice")
        tap(hasTestTag("report-send"))
        waitFor(hasTestTag("report-outcome"))
        assertTrue(
            exists(hasText("Whoever runs Venti will read it", substring = true)) ||
                exists(hasText("You reported this before", substring = true)),
        )
        assertFalse(exists(hasTestTag("report-send")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-account-8-reported")
        tap(hasTestTag("report-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("report-outcome")) }
        tap(described("Back"))
        waitFor(hasTestTag("public-flight-row"))
        tap(tab("Logbook"))
        waitFor(hasTestTag("logbook-row"))

        // Who flies where: the plans of the pilot's groups, from the map.
        // The development mode's demo pilot keeps a made-up group, whose
        // made-up plans are for the days around that mode's standing
        // clock and not for today: the test posts one for today.
        tap(tab("Map"))
        waitFor(described("Flying plans"))
        tap(described("Flying plans"))
        waitFor(hasTestTag("plans-groups"))
        tap(described("New plan"))
        waitFor(hasTestTag("plan-place"))
        val place = if (dark) "Musterberg Nord" else "Musterberg Ost"
        // Where it is on the map comes from one of the pilot's waypoints;
        // the name may then be the pilot's own.
        tap(hasTestTag("plan-from-waypoints"))
        waitFor(hasText("Sample landing field"))
        tap(hasText("Sample landing field"))
        waitFor(hasText("On the map"))
        screen.onNode(hasTestTag("plan-place")).performTextClearance()
        screen.onNode(hasTestTag("plan-place")).performTextInput(place)
        screen.onNode(hasTestTag("plan-save")).performScrollTo()
        waitFor(hasTestTag("plan-save") and isEnabled())
        tap(hasTestTag("plan-save"))
        val posted = hasTestTag("plan-row") and described(place)
        waitFor(posted)
        assertTrue(exists(hasText("Today")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-plans-1-board")

        // The plan itself: its author is in from the start, and can step
        // out again.
        screen.onAllNodes(posted).onFirst().performClick()
        waitFor(hasTestTag("plan-in") and hasText("I am out"))
        // Among those who are in, the pilot of this phone is "You".
        assertTrue(exists(hasTestTag("plan-pilot") and hasText("You")))
        assertTrue(exists(hasText("Change the plan")))
        // Its place is on the map: the forecast for it is a tap away.
        assertTrue(exists(hasTestTag("plan-forecast")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-plans-2-plan")
        tap(described("Back"))

        // The groups, and one of them with its members and what its
        // keeper can do.
        waitFor(hasTestTag("plans-groups"))
        screen.onNode(hasTestTag("plans-groups")).performScrollTo()
        tap(hasTestTag("plans-groups"))
        val group = hasTestTag("group-row") and described("Mustergruppe")
        waitFor(group)
        assertTrue(exists(hasText("Make a group")))
        assertTrue(exists(hasText("Join with a code")))
        tap(group)
        waitFor(hasTestTag("group-member"))
        assertTrue(exists(hasTestTag("group-member") and described("Demo Pilot (you)") and saying("Keeper")))
        assertTrue(exists(hasTestTag("group-member") and described("Mara Muster")))
        assertTrue(exists(hasText("New invite code")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-plans-3-group")
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("group")) }

        // Today's plan with a place on the map is marked there.
        val scenario = checkNotNull(app)
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { (TestDevice.drawnOnTheMap(scenario, "plan-pins") ?: 0) > 0 }
        letTheMapDraw(2)
        keepScreenshot("$prefix-plans-4-on-the-map")

        // Signing out leaves the flights on the phone.
        openFromToday("Settings")
        waitFor(hasTestTag("account-link"))
        screen.onNode(hasTestTag("account-link")).performScrollTo()
        tap(hasTestTag("account-link"))
        waitFor(hasTestTag("account-sign-out"))
        screen.onNode(hasTestTag("account-sign-out")).performScrollTo()
        tap(hasTestTag("account-sign-out"))
        waitFor(hasText("Sign out?"))
        assertTrue(exists(hasText("Your flights stay on this phone and on Venti.")))
        tap(hasTestTag("account-sign-out-confirm"))
        waitFor(hasTestTag("account-handle"))
        tap(described("Back"))
        waitFor(hasTestTag("account-link") and hasText("Not signed in"))
        tap(tab("Logbook"))
        waitFor(hasTestTag("logbook-row"))
    }

    @Test
    fun aPilotSignsInAndSharesAFlight() = signInAndShareAFlight("light", dark = false)

    @Test
    fun theAccountInDarkAppearance() = signInAndShareAFlight("dark", dark = true)

    /**
     * A new account, made on the server and deleted again: both ask more
     * of the phone's own networking than reading does, a body sent with a
     * request that deletes among them. The development mode takes any
     * registration without an invite. Skipped without a server.
     */
    @Test
    fun aNewAccountIsMadeAndDeletedAgain() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        openTheAccount()

        // A handle nobody has: an earlier run may have left its own behind.
        val handle = "t${System.currentTimeMillis() % 1_000_000_000}"
        val password = "a password for this test"
        tap(hasTestTag("account-new"))
        waitFor(hasTestTag("account-invite"))
        screen.onNode(hasTestTag("account-handle")).performTextInput(handle)
        screen.onNode(hasTestTag("account-name")).performTextInput("Test Pilot (not real)")
        screen.onNode(hasTestTag("account-password")).performTextInput(password)
        // Where a password is chosen it says that a forgotten one cannot be recovered.
        assertTrue(exists(hasText("A forgotten password cannot be recovered", substring = true)))
        keepScreenshot("account-6-new-account")
        screen.onNode(hasTestTag("account-submit")).performScrollTo()
        tap(hasTestTag("account-submit"))

        waitFor(hasText("@$handle"))
        waitFor(hasText("Sync now"))
        assertTrue(exists(hasText("Test Pilot (not real)")))

        // Deleting asks for the password, not only for a yes.
        screen.onNode(hasTestTag("account-delete")).performScrollTo()
        tap(hasTestTag("account-delete"))
        waitFor(hasText("Delete your account?"))
        keepScreenshot("account-7-delete-account")
        screen.onNode(hasTestTag("account-delete-password")).performTextInput(password)
        tap(hasTestTag("account-delete-confirm"))
        waitFor(hasTestTag("account-handle"))
        assertFalse(exists(hasText("@$handle")))
    }

    /**
     * Live tracking: who may watch a flight is chosen on the Fly tab and
     * kept; a flight that is started is shared from its start, the cockpit
     * says how the sharing stands, and the track can be deleted again. The
     * emulator does not fly, so the flight stays before its take-off and
     * no position is sent. The pilot is a new one, made for this test: a
     * flight is recorded here, and the demo pilot's logbook is other
     * tests'. Skipped without a server.
     */
    private fun shareAFlightLive(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        waitFor(described("Show my position"))

        // Nobody watches until the pilot says who may. Without an account
        // there is nobody to send positions to, and the page says so first.
        tap(tab("Fly"))
        waitFor(hasTestTag("live-before-takeoff") and hasText("Off"))
        tap(hasTestTag("live-before-takeoff"))
        waitFor(hasTestTag("live-needs-account"))
        assertTrue(exists(hasTestTag("live-nobody") and isSelected()))
        keepScreenshot("$prefix-live-1-no-account")
        tap(described("Back"))
        waitFor(hasText("Open the cockpit"))

        // A new account on the server.
        val handle = "l${System.currentTimeMillis() % 1_000_000_000}"
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        screen.onNode(hasTestTag("account-link")).performScrollTo()
        tap(hasTestTag("account-link"))
        waitFor(hasTestTag("account-handle"))
        tap(hasTestTag("account-new"))
        waitFor(hasTestTag("account-invite"))
        screen.onNode(hasTestTag("account-handle")).performTextInput(handle)
        screen.onNode(hasTestTag("account-name")).performTextInput("Test Pilot (not real)")
        screen.onNode(hasTestTag("account-password")).performTextInput("a password for this test")
        screen.onNode(hasTestTag("account-submit")).performScrollTo()
        tap(hasTestTag("account-submit"))
        waitFor(hasText("@$handle"))
        waitFor(hasText("Sync now"))
        tap(described("Back"))

        // Everyone may watch: the choice is made once and shown on the Fly
        // tab before every start.
        tap(tab("Fly"))
        waitFor(hasTestTag("live-before-takeoff"))
        tap(hasTestTag("live-before-takeoff"))
        waitFor(hasTestTag("live-everyone"))
        assertFalse(exists(hasTestTag("live-needs-account")))
        tap(hasTestTag("live-everyone"))
        waitFor(hasTestTag("live-everyone") and isSelected())
        // Before a flight there is no flight to say anything about.
        assertFalse(exists(hasTestTag("live-status")))
        keepScreenshot("$prefix-live-2-who-can-watch")
        tap(described("Back"))
        waitFor(hasTestTag("live-before-takeoff") and hasText("Everyone"))
        keepScreenshot("$prefix-live-3-fly-tab")

        // A flight, started as a pilot starts one. It is shared from the
        // start, and waits for the take-off: the cockpit's button says so.
        TestDevice.allowToRecordAFlight()
        tap(hasText("Open the cockpit"))
        try {
            waitFor(hasTestTag("cockpit-next-page"))
            // An emulator's location may be switched off; the app says so,
            // and the flight goes on without a position.
            if (exists(hasText("Flying")) && exists(hasText("OK"))) {
                tap(hasText("OK"))
            }
            // Ready on the ground: recording starts by itself at take-off.
            assertTrue(exists(hasTestTag("cockpit-phase") and described("Ready for take-off")))
            // Live tracking is on the flight's page.
            turnTo("Flight")
            waitFor(hasTestTag("live-button"))
            waitFor(hasTestTag("live-button") and saying("Begins at take-off"))
            letTheMapDraw(2)
            keepScreenshot("$prefix-live-4-cockpit")

            // The page again, over the cockpit, now with this flight: how
            // it stands, the link to send, and the way to take it back.
            tap(hasTestTag("live-button"))
            waitFor(hasTestTag("live-status"))
            assertTrue(exists(hasTestTag("live-status") and hasText("Begins at take-off")))
            assertTrue(exists(hasTestTag("live-everyone") and isSelected()))
            assertTrue(exists(hasTestTag("live-send")))
            letTheMapDraw(1)
            keepScreenshot("$prefix-live-5-this-flight")

            // Taking the flight away from everyone is asked first: it
            // deletes the track. After it nobody is the choice again.
            screen.onNode(hasTestTag("live-delete")).performScrollTo()
            tap(hasTestTag("live-delete"))
            waitFor(hasText("Delete the track?"))
            keepScreenshot("$prefix-live-6-delete-the-track")
            tap(hasTestTag("live-delete-confirm"))
            waitFor(hasTestTag("live-status") and hasText("You deleted the track."))
            assertTrue(exists(hasTestTag("live-nobody") and isSelected()))
            assertFalse(exists(hasTestTag("live-send")))
            tap(hasText("Done"))
            waitFor(hasTestTag("live-button") and saying("You deleted the track."))
        } finally {
            // Whatever happened, the flight is ended: its sensors and its
            // recording belong to the app, not to this test's screen.
            if (exists(hasTestTag("cockpit-end"))) {
                holdToEnd()
            }
        }
    }

    /**
     * Who is in the air: the server's development mode has two pilots who
     * show their flight to everyone, over the valley east of the Tegernsee,
     * one in the air and one who landed nine minutes ago. They are on the
     * map and in a list; one's page opens at half height, and while it is
     * open the map draws that flight. Skipped without a server, like the
     * wind.
     */
    private fun lookAtWhoIsInTheAir(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))

        // The valley, close enough to tell the two apart.
        fun lookAtTheValley() {
            assertTrue(TestDevice.lookAt(scenario, lat = 47.678, lon = 11.755, zoom = 11.6))
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
                (TestDevice.drawnOnTheMap(scenario, "live-gliders") ?: 0) >= 2
            }
        }
        lookAtTheValley()
        // The round button for the list is there once somebody is.
        waitFor(described("Pilots in the air"))
        letTheMapDraw()
        keepScreenshot("$prefix-air-1-map")

        // Both as a list: the pilot, and how high, how it climbs and how
        // old the position is.
        tap(described("Pilots in the air"))
        waitFor(hasTestTag("live-row"))
        assertTrue(exists(hasText("In the air")))
        val landed = hasTestTag("live-row") and saying("Landed 9 min ago")
        val flying = hasTestTag("live-row") and !saying("Landed")
        assertTrue(exists(landed and described("Demo Pilot")))
        assertTrue(exists(flying and saying("1596 m")))
        assertTrue(exists(hasText("it is not known to have landed", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-air-2-list")

        // Choosing the one in the air closes the list and opens that
        // pilot's page at half height. The flight so far is fetched, and
        // the map above the page draws it and moves to show all of it.
        tap(flying)
        waitFor(hasTestTag("live-age"))
        assertFalse(exists(hasTestTag("live-row")))
        assertTrue(exists(hasText("@demo")))
        waitFor(hasTestTag("live-duration"))
        assertTrue(exists(hasText("Follow the flight in the browser")))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "live-track-edge") ?: 0) > 0
        }
        letTheMapDraw(3)
        keepScreenshot("$prefix-air-3-flight")
        tap(hasText("Done"))
        // With the page the line goes.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            !exists(hasTestTag("live-age")) && TestDevice.drawnOnTheMap(scenario, "live-track-edge") == 0
        }

        // A tap on a glider on the map opens its page as well: the one on
        // the ground, whose page says when it landed and nothing of a climb.
        lookAtTheValley()
        letTheMapDraw(1)
        val disc = checkNotNull(TestDevice.onScreen(scenario, lat = 47.692168, lon = 11.767487))
        TestDevice.tap(disc)
        waitFor(hasTestTag("live-age") and hasText("Landed 9 min ago"))
        assertFalse(exists(hasText("Climb")))
        waitFor(hasTestTag("live-duration"))
        letTheMapDraw(3)
        keepScreenshot("$prefix-air-4-landed")
        // At the foot of the page: the way to report a track that should
        // not be there. Its form opens over the page, and without an
        // account it has nothing to fill in.
        screen.onNode(hasTestTag("live-glider")).performTouchInput { swipeUp() }
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            runCatching { screen.onNode(hasTestTag("report-track")).performScrollTo() }.isSuccess
        }
        tap(hasTestTag("report-track"))
        waitFor(hasTestTag("report-obstacle"))
        assertFalse(exists(hasTestTag("report-send")))
        tap(hasTestTag("report-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("report-obstacle")) }
        tap(hasText("Done"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("live-age")) }
    }

    /**
     * Everyone's flights: the League tab lists what the pilots of Venti
     * share with everyone, the newest first or the best, and ranks them.
     * The development mode has five public flights of three made-up
     * pilots. One opens like a flight of the logbook, from its file, and
     * nothing of it is kept. Skipped without a server.
     */
    private fun lookAtEveryonesFlights(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        waitFor(described("Show my position"))
        // Plans for a flying day are shared in groups on the server and
        // need an account: signed out, the board says so and offers
        // nothing else.
        tap(described("Flying plans"))
        waitFor(hasTestTag("plans-signed-out"))
        assertFalse(exists(described("New plan")))
        assertFalse(exists(hasTestTag("plans-groups")))
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("plans-signed-out")) }
        tap(tab("League"))
        waitFor(hasTestTag("league-everyone") and isSelected())

        // The newest first: the one of September, then August's.
        val row = hasTestTag("public-flight-row")
        waitFor(row)
        assertTrue(exists(row and described("Jonas Beispiel")))
        assertTrue(exists(row and described("Mara Muster")))
        // The development mode's demo pilot shares flights too, unless a
        // test that ran before took them back: not looked for.
        assertTrue(screen.onAllNodes(row).fetchSemanticsNodes().first().config[SemanticsProperties.ContentDescription] == listOf("Jonas Beispiel"))
        // Under the list: whose reckoning the distances and points are.
        screen.onNode(hasTestTag("public-flights")).performScrollToNode(hasTestTag("public-flights-reckoning"))
        assertTrue(exists(hasText("not a league's", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-everyone-1-newest")

        // The best by points, from the top.
        screen.onNode(hasTestTag("public-flights")).performScrollToNode(hasTestTag("public-flights-best"))
        tap(hasTestTag("public-flights-best"))
        waitFor(hasTestTag("public-flights-best") and isSelected())
        waitFor(row)
        letTheMapDraw(1)
        keepScreenshot("$prefix-everyone-2-best")

        // A flight opens like one of the pilot's own, with its pilot's
        // name over it.
        val pilot = screen.onAllNodes(row).fetchSemanticsNodes().first()
            .config[SemanticsProperties.ContentDescription].first()
        screen.onAllNodes(row).onFirst().performClick()
        waitFor(hasTestTag("public-flight-pilot") and hasText(pilot))
        waitFor(described("Flight details"))
        assertTrue(exists(described("Airtime")))
        // Another pilot's flight is not the pilot's to send to a league.
        assertFalse(exists(hasTestTag("send-to-league")))
        letTheMapDraw(6)
        keepScreenshot("$prefix-everyone-3-flight")

        // Its menu is not the menu of a flight of the logbook: there is
        // nothing of the pilot's to share, replay or delete. It hands on
        // the flight's page on the server, and leads to everything else
        // that pilot shares with everyone.
        tap(described("More"))
        waitFor(hasText("Flights of $pilot"))
        assertTrue(exists(hasText("Share the flight's page")))
        assertTrue(exists(hasText("Report this flight")))
        assertFalse(exists(hasText("Delete flight")))
        assertFalse(exists(hasText("Replay in the cockpit")))
        tap(hasText("Flights of $pilot"))
        val theirs = hasTestTag("pilot-flight-row")
        waitFor(theirs)
        assertTrue(exists(hasText(pilot)))
        assertTrue(screen.onAllNodes(theirs).fetchSemanticsNodes().all {
            it.config[SemanticsProperties.ContentDescription] == listOf(pilot)
        })
        letTheMapDraw(1)
        keepScreenshot("$prefix-everyone-4-pilot")
        // The pilot's page has the way to tell whoever runs the server
        // that it should not be there. A report is sent from an account,
        // and nobody is signed in here: the form says so and offers
        // nothing to fill in.
        tap(described("More"))
        waitFor(hasText("Report this pilot"))
        tap(hasText("Report this pilot"))
        waitFor(hasTestTag("report-obstacle") and hasText("Sign in under Settings, Account.", substring = true))
        assertFalse(exists(hasTestTag("report-reason")))
        assertFalse(exists(hasTestTag("report-text")))
        assertFalse(exists(hasTestTag("report-send")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-everyone-5-report-needs-account")
        tap(hasTestTag("report-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("report-obstacle")) }
        // One of them opens like the first, and the way back leads through
        // the pilot's page.
        screen.onAllNodes(theirs).onFirst().performClick()
        waitFor(hasTestTag("public-flight-pilot") and hasText(pilot))
        tap(described("Back"))
        waitFor(theirs)
        tap(described("Back"))
        waitFor(hasTestTag("public-flight-pilot"))

        // Back to the list, in the order it was left in.
        tap(described("Back"))
        waitFor(hasTestTag("public-flights-best") and isSelected())

        // The ranking: the pilots by the points of their best flights, in
        // the newest season, and in all years. A flight that counts opens
        // like any public flight.
        tap(hasTestTag("league-ranking"))
        waitFor(hasTestTag("ranking-season"))
        val allYears = hasText("All years") and hasAnyAncestor(hasTestTag("ranking-season"))
        tap(allYears)
        waitFor(hasTestTag("ranked-pilot"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-everyone-6-ranking")
        screen.onAllNodes(hasTestTag("ranked-flight")).onFirst().performClick()
        waitFor(hasTestTag("public-flight-pilot"))
        waitFor(described("Flight details"))
        tap(described("Back"))
        waitFor(hasTestTag("ranked-pilot"))

        // The pilot's own logbook is as empty as before.
        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
    }

    @Test
    fun theLeagueTabListsEveryonesFlights() = lookAtEveryonesFlights("light", dark = false)

    @Test
    fun everyonesFlightsInDarkAppearance() = lookAtEveryonesFlights("dark", dark = true)

    @Test
    fun theMapShowsWhoIsInTheAir() = lookAtWhoIsInTheAir("light", dark = false)

    @Test
    fun whoIsInTheAirInDarkAppearance() = lookAtWhoIsInTheAir("dark", dark = true)

    @Test
    fun aFlightIsSharedWhileItIsUnderWay() = shareAFlightLive("light", dark = false)

    @Test
    fun liveTrackingInDarkAppearance() = shareAFlightLive("dark", dark = true)

    /**
     * Flying sites: the same server knows the take-offs and landings its
     * source has. They are drawn once the map shows a valley and not a
     * country, here the one east of the Tegernsee, which the map shipped
     * with the app still covers. A sign opens its site; the round button
     * lists them; a site is kept as a waypoint. Skipped without a server,
     * like the wind.
     */
    private fun lookAtTheFlyingSites(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))
        // A map of the whole northern Alps has no signs and no button for them.
        assertFalse(exists(described("Flying sites")))

        // The take-off stands left of the round buttons along the right
        // edge, so that a tap on its pin is a tap on the pin.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.69, lon = 11.875, zoom = 11.0))
        waitFor(described("Flying sites"))
        waitFor(hasText("Sites: OpenStreetMap contributors"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) > 0
        }
        letTheMapDraw()
        keepScreenshot("$prefix-sites-1-map")

        // A tap on a take-off's sign opens the site, with the map still in view.
        val takeoff = "Fluggelände Jägerkamp/Nagelspitz"
        val sign = checkNotNull(TestDevice.onScreen(scenario, lat = 47.6791413, lon = 11.9051032))
        TestDevice.tap(sign)
        waitFor(hasText(takeoff))
        // The pack's sheet: the site in a line, what the forecast says of
        // its first day with flying hours with the status beside the name,
        // and the two things to do from here.
        waitFor(hasTestTag("site-in-a-line") and hasText("Take-off", substring = true))
        waitFor(hasTestTag("site-outlook"))
        assertTrue(exists(hasTestTag("site-status")))
        assertTrue(exists(hasTestTag("site-plan-route")))
        assertTrue(exists(hasTestTag("site-start-flight")))
        letTheMapDraw(2)
        keepScreenshot("$prefix-sites-2-site-from-the-map")
        // "Plan route": the route map, with the site as its first place.
        tap(hasTestTag("site-plan-route"))
        waitFor(hasText("Draw a route"))
        letTheMapDraw(2)
        keepScreenshot("$prefix-sites-2b-plan-route")
        tap(described("Cancel"))
        waitFor(described("Flying sites"))

        // The chips switch the sites off and on again, and the map
        // remembers.
        tap(hasTestTag("layer-sites"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(described("Flying sites")) }
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-2c-sites-off")
        tap(hasTestTag("layer-sites"))
        waitFor(described("Flying sites"))

        // The same signs as a list: those with a name first.
        tap(described("Flying sites"))
        waitFor(described(takeoff))
        assertTrue(exists(described("Landeplatz Jägerkamp/Nagelspitz")))
        assertTrue(exists(hasText("A site on the map is not permission to fly there.", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-3-list")

        // One site: what its source says, and what that is worth. Kept as
        // a waypoint, the cockpit can navigate to it.
        tap(described(takeoff))
        waitFor(hasText("Add to my waypoints"))
        assertTrue(exists(hasText("Marked as an official site")))
        tap(hasText("Add to my waypoints"))
        waitFor(hasText("In your waypoints"))
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasText("See or correct it at the source"))
        assertTrue(exists(hasText("© OpenStreetMap contributors", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-4-site")

        // What was flown from here, where the server has public flights
        // that took off at this site: the development mode's flights took
        // off at the sample's take-off, which may or may not be this one.
        letTheMapDraw(2)
        if (runCatching { screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-flight")) }.isSuccess) {
            letTheMapDraw(1)
            keepScreenshot("$prefix-sites-4b-flown-from-here")
        }

        // The same server's forecast for the place: the wind at each
        // height through the day, and the sky, with what it is worth.
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-forecast"))
        tap(hasTestTag("site-forecast"))
        waitFor(hasTestTag("forecast-table"))
        assertTrue(exists(hasContentDescription("hPa", substring = true)))
        assertTrue(exists(hasText("Nothing here says whether the day can be flown.", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-5-forecast")

        // The day after, and the other weather model: one at a time.
        tap(hasTestTag("forecast-day-1"))
        waitFor(hasTestTag("forecast-day-1") and isSelected())
        screen.onNode(hasTestTag("forecast-model")).performScrollTo()
        assertTrue(exists(hasTestTag("forecast-model") and hasText("ICON-D2")))
        assertTrue(exists(hasText("The model's ground here")))
        assertTrue(exists(hasText("Weather data by Open-Meteo.com", substring = true)))
        tap(hasTestTag("forecast-model"))
        waitFor(hasText("ICON-EU, 7 km cells"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-6-forecast-models")
        tap(hasText("ICON-EU, 7 km cells"))
        waitFor(hasTestTag("forecast-model") and hasText("ICON-EU"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-7-forecast-other-model")

        // Back on the site's page, which still knows that it was kept.
        tap(described("Back"))
        waitFor(hasText("In your waypoints"))
        tap(described("Back"))
        waitFor(described(takeoff))
        tap(hasText("Done"))
        waitFor(described("Flying sites"))

        // A take-off whose source says which wind it works in, on the Wank
        // above Garmisch. The map shipped with the app ends before it; the
        // sites are the server's all the same.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.5067, lon = 11.10, zoom = 10.5))
        tap(described("Flying sites"))
        val south = "Startplatz Wank Süd"
        // The list is the old part's until the server has answered for
        // this one, and then longer than the screen.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            runCatching { screen.onNode(hasTestTag("site-list")).performScrollToNode(described(south)) }.isSuccess
        }
        tap(described(south))
        waitFor(hasText("Wind it works in"))
        assertTrue(exists(hasText("E to WSW")))
        assertTrue(exists(hasText("Where")))

        // Under it, the wind measured nearby: the station in the valley,
        // 7 km away, with whether its wind comes from a direction the
        // site works in. The map has that station once it has asked for
        // the wind of this part.
        val page = hasTestTag("site-page")
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            runCatching { screen.onNode(page).performScrollToNode(hasTestTag("site-station")) }.isSuccess
        }
        assertTrue(exists(hasText("Wind measured nearby")))
        assertTrue(exists(hasTestTag("site-station") and hasText("Garmisch-Partenkirchen")))
        assertTrue(exists(hasText("direction the site works in", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-8-site-with-wind")

        // The station opens in the same sheet, and leads back.
        tap(hasTestTag("site-station"))
        waitFor(hasText("Last 24 hours"))
        tap(described("Back"))
        waitFor(hasText("Wind it works in"))
        tap(hasText("Done"))
        waitFor(described("Flying sites"))

        // The site that was kept is among the pilot's waypoints.
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        tap(hasText("Waypoints"))
        waitFor(described(takeoff))
    }

    /**
     * Today, against Venti in development mode: the app opens on it, with
     * the take-offs around the Tegernsee and what the recorded forecast, a
     * stormy day, says about flying from each. Another day, and a
     * take-off's own page. Skipped without a server, like the wind.
     */
    private fun lookAtToday(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark, onToday = true) { putExtra(LaunchOptions.SERVER, server) }

        // The best take-off as a card, with its status in words once its
        // forecast is read, its readings, and the others as rows.
        waitFor(hasTestTag("today-card"))
        val status = described("Flyable") or described("Marginal") or described("Not flyable")
        waitFor(hasTestTag("today-card") and status)
        assertTrue(exists(hasTestTag("today-card") and described("Cloud base")))
        assertTrue(exists(hasTestTag("today-row")))
        // Whose forecast it is, and that it is a reading of a model.
        screen.onNode(hasTestTag("today-foot")).performScrollTo()
        waitFor(hasText("the take-off decides", substring = true))
        assertTrue(exists(hasText("They are not advice to fly", substring = true)))
        // Back to the top of the page, the valley and its corner in view.
        screen.onNode(hasTestTag("today-head")).performScrollTo()
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-today-1")

        // Another day.
        val anotherDay = hasAnyAncestor(hasTestTag("today-days")) and isSelectable() and isSelected().not()
        if (exists(anotherDay)) {
            screen.onAllNodes(anotherDay).onLast().performClick()
            waitFor(hasTestTag("today-card") and status)
            TestDevice.letTheAppRun(screen, 1)
            keepScreenshot("$prefix-today-2-another-day")
        }

        // A take-off's own page, over Today.
        tap(hasTestTag("today-card"))
        waitFor(hasText("Wind it works in"))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-today-3-site")
        tap(hasText("Done"))
        waitFor(hasTestTag("today-card"))
    }

    @Test
    fun todaySaysWhatTheForecastSaysOfTheTakeoffs() = lookAtToday("light", dark = false)

    @Test
    fun todayInDarkAppearance() = lookAtToday("dark", dark = true)

    /**
     * Today without a Venti to ask: the app opens on it, says what is
     * missing and leads to where it is set. The settings are a page of the
     * Today tab and keep the tab bar, and their place while the pilot looks
     * at another tab.
     */
    @Test
    fun todayWithoutVentiSaysWhatIsMissing() {
        launch(onToday = true)
        waitFor(hasText("Venti is not connected"))
        assertTrue(exists(hasTestTag("today-headline") and hasText("No forecast yet")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-0-not-connected")

        tap(hasText("Open the settings"))
        waitFor(hasText("Vario sound"))
        tap(tab("Fly"))
        waitFor(hasText("Open the cockpit"))
        tap(tab("Today"))
        waitFor(hasText("Vario sound"))
        // Tapped again, the tab goes back to Today itself.
        tap(tab("Today"))
        waitFor(hasText("Venti is not connected"))
    }

    /**
     * A long press on the map shows the server's forecast for the place
     * under the finger, named by where it is. Skipped without a server.
     */
    @Test
    fun aLongPressOnTheMapShowsTheForecastThere() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))
        letTheMapDraw(2)

        // Above Garmisch, inside the server's first region.
        val place = checkNotNull(TestDevice.onScreen(scenario, lat = 47.5, lon = 11.1))
        TestDevice.longPress(place)
        waitFor(hasTestTag("forecast-table"))
        assertTrue(exists(hasText("° N, ", substring = true)))
        assertTrue(exists(hasContentDescription("hPa", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("map-forecast")
        tap(hasText("Done"))
        waitFor(described("Show my position"))
        assertFalse(exists(hasTestTag("forecast-table")))
    }

    @Test
    fun theMapShowsFlyingSitesFromAServer() = lookAtTheFlyingSites("light", dark = false)

    @Test
    fun theFlyingSitesInDarkAppearance() = lookAtTheFlyingSites("dark", dark = true)

    @Test
    fun airspaceComesFromTheServerByItself() = takeAirspaceFromTheServer("light", dark = false)

    @Test
    fun theServersAirspaceInDarkAppearance() = takeAirspaceFromTheServer("dark", dark = true)

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
        waitFor(described("Show my position"))
        val scenario = checkNotNull(app)
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { (TestDevice.drawnOnTheMap(scenario, "place") ?: 0) > 0 }
    }

    @Test
    fun theAssistantAnswersFromTheLogbook() = askTheAssistant("light", dark = false)

    @Test
    fun theAssistantInDarkAppearance() = askTheAssistant("dark", dark = true)

    @Test
    fun theSameInDarkAppearance() = importInspectAndDelete("dark", dark = true)

    /**
     * The other gliders on the cockpit's map, and the wind on the ground in
     * the cockpit, from Venti's wind stations: the
     * nearest in a badge, and all of them in the wind's sheet, which begins
     * with them when it is opened from that badge. The development mode's
     * stations stand fifty kilometres off and more, and the badge says so.
     */
    private fun lookAtTheWindOnTheGround(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        waitFor(described("Show my position"))
        tap(tab("Fly"))
        waitFor(hasText("Replay the sample flight"))
        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"))

        val badge = hasTestTag("ground-wind-badge")
        waitFor(badge and saying(" km/h"), seconds = TestDevice.WHOLE_FLIGHT_SECONDS)
        letTheMapDraw(2)
        keepScreenshot("$prefix-fly-ground-wind-1-badge")

        // Who else is in the air: the development mode's demo gliders fly
        // the sample flight too, and the whole flight's view has them.
        val scenario = checkNotNull(app)
        turnTo("Map")
        tap(described("Show the whole flight"))
        waitFor(described("Follow the glider"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "cockpit-gliders") ?: 0) > 0
        }
        letTheMapDraw(2)
        keepScreenshot("$prefix-fly-others-in-the-air")
        tap(described("Follow the glider"))
        waitFor(described("Show the whole flight"))

        tap(badge)
        waitForAll(hasText("Wind on the ground"), hasText("On the ground"), hasTestTag("ground-wind-row"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-ground-wind-2-sheet")
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasText("Wind on the ground")) }

        holdToEnd()
        waitFor(hasText("Open the cockpit"))
    }

    @Test
    fun theCockpitShowsTheWindOnTheGround() = lookAtTheWindOnTheGround("light", dark = false)

    @Test
    fun theWindOnTheGroundInDarkAppearance() = lookAtTheWindOnTheGround("dark", dark = true)

    /**
     * "Send to a league" on a flight: what both leagues ask about, XContest's
     * way with the file and its page, and the DHV-XC's form. Nothing is ever
     * sent: the test fills the form with made-up words and never presses
     * "Send to the DHV-XC".
     */
    private fun sendAFlightToALeague(prefix: String, dark: Boolean) {
        launch(dark = dark)
        waitFor(described("Show my position"))
        tap(tab("Logbook"))
        waitFor(hasText("Add a sample flight"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"))
        // On the pilot's own flight the panel has the button, under the
        // flight in one number.
        waitFor(hasTestTag("send-to-league"))
        assertTrue(exists(hasTestTag("flight-headline") and (described("Distance") or described("Track"))))
        tap(hasTestTag("send-to-league"))
        // The sample was recorded by Venti and flown in July: unsigned, and late.
        waitForAll(hasTestTag("league-deadline"), hasTestTag("league-signature"))
        assertTrue(exists(hasText("Venti recorded this flight and does not sign its files yet", substring = true)))
        assertTrue(exists(hasTestTag("xcontest-claim")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-league-1-before")

        // Without a user name and a password nothing can be sent.
        val send = hasTestTag("dhvxc-send")
        screen.onNode(send).performScrollTo()
        assertTrue(exists(send and isNotEnabled()))
        screen.onNode(hasTestTag("dhvxc-user")).performTextInput("made-up-pilot")
        screen.onNode(hasTestTag("dhvxc-password")).performTextInput("made-up-password")
        waitFor(send and isEnabled())
        screen.onNode(send).performScrollTo()
        letTheMapDraw(1)
        keepScreenshot("$prefix-league-2-dhvxc")

        tap(hasTestTag("league-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("league-sheet")) }
        assertTrue(exists(described("Flight details")))
    }

    @Test
    fun aFlightIsOfferedToTheLeagues() = sendAFlightToALeague("light", dark = false)

    @Test
    fun theLeaguesInDarkAppearance() = sendAFlightToALeague("dark", dark = true)

    /**
     * Two flights side by side: the sample flight and the same flight ten
     * minutes later on the same day, brought in together. They were in
     * the air together, so they are laid over each other by the clock.
     */
    private fun layTwoFlightsSideBySide(prefix: String, dark: Boolean) {
        val sample = context.assets.open(AppFiles.SAMPLE_FLIGHT).use { it.readBytes() }.decodeToString()
        val later = sample.lines().joinToString("\n") { line ->
            if (!line.startsWith("B")) return@joinToString line
            val time = line.substring(1, 7)
            val total = time.substring(0, 2).toInt() * 3600 + time.substring(2, 4).toInt() * 60 +
                time.substring(4, 6).toInt() + 600
            "B%02d%02d%02d".format(total / 3600, total / 60 % 60, total % 60) + line.substring(7)
        }
        val folder = File(context.cacheDir, "compare-${System.nanoTime()}").apply { mkdirs() }
        val files = listOf(File(folder, "sample.igc").apply { writeText(sample) }, File(folder, "later.igc").apply { writeText(later) })
        launch(dark = dark) {
            action = Intent.ACTION_SEND_MULTIPLE
            type = "application/octet-stream"
            putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList(files.map(Uri::fromFile)))
        }
        waitFor(hasText("2 flights added"))
        tap(hasText("OK") and hasClickAction())
        waitFor(hasTestTag("logbook-row"))
        tap(hasTestTag("logbook-row"))
        waitFor(described("Flight details"))

        tap(described("Flight details"))
        waitFor(hasTestTag("compare-row"))
        tap(hasTestTag("compare-row"))
        waitFor(hasTestTag("compare-own-row"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-compare-1-choose")
        tap(hasTestTag("compare-own-row"))

        // The other flight is on the map and in the profile, and a line in
        // the panel says whose it is and how the two lie to each other.
        val line = hasTestTag("compare-line")
        waitFor(line and saying("In the air together for"))
        letTheMapDraw(4)
        keepScreenshot("$prefix-compare-2-beside")

        tap(line)
        waitForAll(hasText("Side by side"), hasTestTag("comparison-rows"))
        assertTrue(exists(hasText("Go to where they were nearest")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-compare-3-side-by-side")
        tap(hasText("Go to where they were nearest"))
        waitFor(line and saying(" m away"))
        letTheMapDraw(2)
        keepScreenshot("$prefix-compare-4-nearest")

        tap(line)
        waitFor(hasTestTag("compare-end"))
        tap(hasTestTag("compare-end"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(line) }
        folder.deleteRecursively()
    }

    @Test
    fun twoFlightsAreLaidSideBySide() = layTwoFlightsSideBySide("light", dark = false)

    @Test
    fun twoFlightsSideBySideInDarkAppearance() = layTwoFlightsSideBySide("dark", dark = true)

    /**
     * A route drawn on the map: three taps, back to the start, and the
     * panel says what it comes to; kept, it is the task.
     */
    private fun drawARouteOnTheMap(prefix: String, dark: Boolean) {
        launch(dark = dark)
        waitFor(described("Show my position"))
        tap(tab("Fly"))
        waitFor(hasTestTag("task-before-takeoff"))
        tap(hasTestTag("task-before-takeoff"))
        waitFor(hasTestTag("task-draw-route"))
        screen.onNode(hasTestTag("task-draw-route")).performScrollTo()
        tap(hasTestTag("task-draw-route"))
        val words = hasTestTag("route-words")
        waitFor(words and hasText("Tap the map where the route begins."))
        letTheMapDraw(2)

        val map = screen.onNode(hasTestTag("route-map"))
        // Each tap waits for the map to say it was taken: a tap the map
        // takes for the first of a double tap waits for the second.
        map.performTouchInput { click(Offset(width * 0.35f, height * 0.30f)) }
        waitFor(words and hasText("Tap the next place to fly to."))
        map.performTouchInput { click(Offset(width * 0.65f, height * 0.30f)) }
        waitFor(words and hasText("along the route", substring = true))
        letTheMapDraw(2)
        keepScreenshot("$prefix-route-1-one-leg")
        map.performTouchInput { click(Offset(width * 0.50f, height * 0.55f)) }
        waitFor(hasText("Back to the start") and isEnabled())
        tap(hasText("Back to the start"))
        waitFor(words and hasText("triangle", substring = true))
        letTheMapDraw(2)
        keepScreenshot("$prefix-route-2-triangle")

        tap(hasText("Keep") and isEnabled())
        waitFor(hasTestTag("task-sketch"))
        assertTrue(exists(hasContentDescription("Start", substring = true)))
    }

    @Test
    fun aRouteIsDrawnOnTheMap() = drawARouteOnTheMap("light", dark = false)

    @Test
    fun aRouteOnTheMapInDarkAppearance() = drawARouteOnTheMap("dark", dark = true)

    /**
     * A map for flying without a network: the development mode hands out
     * the region that comes with the app. Downloaded, checked and kept,
     * the Fly tab names it, and the cockpit over the Tegernsee draws from
     * it and says so.
     */
    private fun downloadAMapForFlying(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        waitFor(described("Show my position"))
        tap(tab("Fly"))
        val line = hasTestTag("maps-before-takeoff")
        waitFor(line and hasText("None"))
        tap(line)
        waitFor(hasTestTag("map-region") and hasContentDescription("Tegernsee valley", substring = true))
        letTheMapDraw(1)
        keepScreenshot("$prefix-maps-1-offered")

        // The pilot is told the size before anything is fetched.
        tap(hasTestTag("map-download"))
        waitFor(hasTestTag("map-download-confirm"))
        assertTrue(exists(hasText("counts against your plan", substring = true)))
        tap(hasTestTag("map-download-confirm"))
        // Seven megabytes from the same machine: fetched, checked and kept.
        waitFor(hasTestTag("map-delete"), seconds = 120)
        waitFor(hasTestTag("map-region") and saying("On this device"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-maps-2-kept")

        tap(described("Back"))
        waitFor(line and hasText("Tegernsee valley", substring = true))

        // The sample flight is over the Tegernsee, inside the region.
        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"))
        waitFor(hasTestTag("map-kept"), seconds = TestDevice.WHOLE_FLIGHT_SECONDS)
        waitFor(described("Altitude") and saying("over ground"))
        letTheMapDraw(6)
        keepScreenshot("$prefix-maps-3-cockpit")
        holdToEnd()
        waitFor(hasText("Open the cockpit"))
    }

    @Test
    fun aMapIsDownloadedForFlyingWithoutANetwork() = downloadAMapForFlying("light", dark = false)

    @Test
    fun theOfflineMapsInDarkAppearance() = downloadAMapForFlying("dark", dark = true)

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

        waitFor(described("Flight details"))
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

        waitFor(hasText("1 file could not be imported"))
        assertTrue(exists(hasText("notes.igc", substring = true) and hasText("not a flight file", substring = true)))
        keepScreenshot("import-1-not-a-flight")
        tap(hasText("OK") and hasClickAction())
        // Nothing was added.
        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
    }

    /**
     * Many flights at once: shared to the app from another, as a folder of
     * them picked in the logbook is imported. The logbook counts them as
     * they come in, fills its list on the way, and says at the end what it
     * did.
     *
     * The last but one file is a pipe that holds nothing until the test
     * writes to it, so the import stands still at a known count for the
     * screenshot, however fast or slow the device is.
     */
    private fun shareManyFlights(prefix: String, dark: Boolean) {
        val sample = context.assets.open(AppFiles.SAMPLE_FLIGHT).use { it.readBytes() }.decodeToString()
        // The sample flight as flown on another day: another flight.
        fun flown(day: Int) = sample.replace("HFDTEDATE:040726", "HFDTEDATE:%02d0626".format(day))
        val folder = File(context.cacheDir, "shared-${System.nanoTime()}").apply { mkdirs() }
        val files = (1..22).map { day -> File(folder, "2026-06-%02d.igc".format(day)) }
        val held = files[20]
        for ((index, file) in files.withIndex()) {
            if (file == held) {
                Os.mkfifo(file.path, OsConstants.S_IRUSR or OsConstants.S_IWUSR)
            } else {
                file.writeText(flown(index + 1))
            }
        }

        launch(dark = dark) {
            action = Intent.ACTION_SEND_MULTIPLE
            type = "application/octet-stream"
            putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList(files.map(Uri::fromFile)))
        }

        // Twenty are in and listed; the twenty-first is being waited for.
        waitFor(hasTestTag("import-progress") and saying("20 of 22"))
        waitFor(hasTestTag("logbook-row"))
        // The logbook came to the front a moment ago: without this the
        // picture is of the map the app started on. The import stands
        // still meanwhile, so there is no hurry.
        letTheMapDraw(2)
        assertTrue(exists(hasTestTag("import-progress") and saying("20 of 22")))
        keepScreenshot("$prefix-import-2-progress")

        held.writeText(flown(21))
        waitFor(hasText("22 flights added"))
        assertTrue(exists(hasText("They are in your logbook, on this phone.")))
        assertFalse(exists(hasTestTag("import-progress")))
        keepScreenshot("$prefix-import-3-summary")
        tap(hasText("OK") and hasClickAction())
        screen.onNode(hasTestTag("logbook-totals")).assertIsDisplayed()
        folder.deleteRecursively()
    }

    @Test
    fun manyFlightsSharedAtOnceAreCountedAsTheyComeIn() = shareManyFlights("light", dark = false)

    @Test
    fun manyFlightsInDarkAppearance() = shareManyFlights("dark", dark = true)

    /**
     * The same screens with tiles fetched from the public servers, as a
     * pilot's phone draws them. Only the screenshots matter here: the test
     * does not fail when the network is slow.
     */
    @Test
    fun theMapWithTilesFromTheNetwork() {
        launch(bundledMap = false)
        waitFor(described("Show my position"))
        letTheMapDraw(12)
        keepScreenshot("online-1-map")

        // The aerial view: photographs from above under the names and
        // everything Venti draws. Off until the pilot switches it on.
        val aerial = hasTestTag("map-aerial")
        waitFor(aerial and isSelected().not())
        tap(aerial)
        waitFor(aerial and isSelected())
        letTheMapDraw(12)
        keepScreenshot("online-1b-map-aerial")

        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"))
        // One choice for every map: the flight's map shows the photographs too.
        waitFor(aerial and isSelected())
        letTheMapDraw(12)
        keepScreenshot("online-2b-flight-aerial")
        // Off again, as the test found it.
        tap(aerial)
        waitFor(aerial and isSelected().not())
        letTheMapDraw(12)
        keepScreenshot("online-2-flight")
    }
}
