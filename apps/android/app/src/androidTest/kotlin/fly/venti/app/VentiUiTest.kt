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
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isNotEnabled
import androidx.compose.ui.test.isOff
import androidx.compose.ui.test.isOn
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeUp
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import fly.venti.app.radar.RadarLayer
import fly.venti.core.siteConfirmationsPath
import org.junit.Assert.assertEquals
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
import java.time.LocalDate
import java.time.temporal.ChronoUnit

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

    /**
     * The layers that are rows of the layers sheet only. The map's chips
     * switch four: "layer-wind", "layer-airspace", "layer-sites", "layer-pilots".
     */
    private val SHEET_LAYERS =
        setOf("layer-forecast", "layer-radar", "layer-thermals", "layer-cables", "map-aerial", "layer-climbs", "layer-flown")

    private fun exists(what: SemanticsMatcher) = screen.onAllNodes(what).fetchSemanticsNodes().isNotEmpty()

    private fun tap(what: SemanticsMatcher) {
        screen.onAllNodes(what).onFirst().performClick()
    }

    /**
     * Switches a layer of the map: one of the four chips along its top,
     * scrolled into view first, or a row of the layers sheet for every
     * other layer, behind the layers button.
     */
    private fun tapChip(tag: String) {
        if (tag in SHEET_LAYERS) {
            switchLayer(tag)
        } else {
            screen.onNode(hasTestTag(tag)).performScrollTo().performClick()
        }
    }

    /** Opens the one sheet of the map's layers and lists, behind the layers button. */
    private fun openLayers() {
        waitFor(hasTestTag("map-layers"))
        tap(hasTestTag("map-layers"))
        waitFor(hasTestTag("layers-sheet"))
    }

    /** Closes the layers sheet and waits until the map is free again. */
    private fun closeLayers() {
        tap(hasTestTag("layers-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("layers-sheet")) }
    }

    /**
     * Taps a row of the layers sheet that leads elsewhere (the lists of
     * stations, sites and pilots, the plans): the sheet opens first, and
     * closes behind what the row opens.
     */
    private fun tapInLayers(row: SemanticsMatcher) {
        openLayers()
        waitFor(row)
        screen.onNode(row).performScrollTo().performClick()
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("layers-sheet")) }
    }

    /**
     * Switches a layer by its row of the layers sheet, and closes the
     * sheet. With `on`, the row was the other way before and is so after.
     */
    private fun switchLayer(tag: String, on: Boolean? = null) {
        openLayers()
        val row = hasTestTag(tag)
        waitFor(row)
        screen.onNode(row).performScrollTo()
        if (on != null) waitFor(row and (if (on) isOff() else isOn()))
        screen.onNode(row).performClick()
        if (on != null) waitFor(row and (if (on) isOn() else isOff()))
        closeLayers()
    }

    /**
     * Opens every credit in full behind the map's (i), waits for these
     * among them, and closes them. The credits are one element, its lines
     * its texts.
     */
    private fun seeInCredits(vararg lines: SemanticsMatcher) {
        tap(hasTestTag("map-credits"))
        val credits = hasTestTag("wind-credits")
        waitFor(credits)
        waitForAll(*lines.map { it and credits }.toTypedArray())
        tap(hasTestTag("credits-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(credits) }
    }

    /**
     * Unfolds "Key and notes" on the forecast map's card, which opens with
     * the key, the model's run and the notes folded away.
     */
    private fun unfoldTheKeyAndNotes(tag: String = "forecast-key-and-notes") {
        val fold = hasTestTag(tag)
        waitFor(fold and saying("Collapsed"))
        screen.onNode(fold).performScrollTo().performClick()
        waitFor(fold and saying("Expanded"))
    }

    /** One of the sections in the tab bar; not a day of Today's switch or of the forecast map's, which say "Today" too. */
    private fun tab(name: String) =
        hasText(name) and isSelectable() and !hasAnyAncestor(hasTestTag("today-days")) and !hasAnyAncestor(hasTestTag("forecast-days"))

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

    /** Something whose state, as a screen reader says it, holds this pattern. */
    private fun sayingLike(pattern: String) = SemanticsMatcher("says something like \"$pattern\"") { node ->
        node.config.getOrNull(SemanticsProperties.StateDescription)?.let { Regex(pattern).containsMatchIn(it) } == true
    }

    /** Something whose words, written or as a screen reader says them, hold this pattern. */
    private fun worded(pattern: String) = SemanticsMatcher("is worded like \"$pattern\"") { node ->
        val words = node.config.getOrNull(SemanticsProperties.Text).orEmpty().map { it.text } +
            node.config.getOrNull(SemanticsProperties.ContentDescription).orEmpty()
        words.any { Regex(pattern).containsMatchIn(it) }
    }

    /** Lets the app run and the map finish drawing; see [TestDevice.letTheAppRun]. */
    private fun letTheMapDraw(seconds: Int = 4) {
        TestDevice.letTheAppRun(screen, seconds)
    }

    /**
     * Ends the cockpit as a pilot does: a finger held on "End" in the top line for
     * longer than the second it takes. Nothing asks again.
     */
    private fun holdToEnd() {
        waitFor(hasTestTag("cockpit-end"))
        screen.onNode(hasTestTag("cockpit-end")).performTouchInput { longClick(durationMillis = 1_600) }
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("cockpit-end")) }
    }

    /**
     * Turns the cockpit's pages as a pilot does, by a swipe to the left across
     * the cockpit, until the dots in the top line say this one shows: "Instruments", "Map" or "Flight".
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
        // The profile is drawn over the ground the flight was flown over,
        // from the region that comes with the app, and the moment says how
        // high over it the glider was.
        waitFor(described("Over ground"))
        assertFalse(exists(described("In the air")))
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
        assertTrue(exists(described("Over ground")))
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
        waitFor(hasText("Last twelve months"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6b-statistics")
        // Further down: the years side by side, the months of the year,
        // a typical flight, the kinds of flight, and the bests.
        val list = screen.onNode(hasTestTag("statistics"))
        list.performScrollToNode(hasTestTag("statistics-seasons"))
        assertTrue(exists(hasText("Years")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6c-statistics-years")
        list.performScrollToNode(hasTestTag("statistics-route"))
        assertTrue(exists(hasText("A typical flight")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6d-statistics-typical-and-kinds")
        // The places flown from: the sample flight's, without a name.
        list.performScrollToNode(hasTestTag("statistics-site"))
        assertTrue(exists(hasText("By take-off")))
        assertTrue(exists(hasTestTag("statistics-site") and hasText("1 flight")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6e-statistics-by-take-off")
        list.performScrollToNode(hasText("Longest flight"))
        assertTrue(exists(hasText("Your bests")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6e-statistics-bests")
        // At the end, "How you fly": the sample flight alone has two
        // thermals, too few to say anything yet. With enough flights it
        // says something: `seeHowYouFly`.
        list.performScrollToNode(hasTestTag("statistics-how-you-fly"))
        tap(hasTestTag("statistics-how-you-fly"))
        waitFor(hasTestTag("tendencies-nothing"))
        assertTrue(exists(hasTestTag("tendencies-note")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-6f-how-you-fly-empty")
        tap(described("Back"))
        waitFor(hasTestTag("statistics"))
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
        )
        // The thermal view is gone with the circling, and the map has its room.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("thermal-view")) }
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

        // The aerial view, one choice with every other map: off until it
        // is switched on, and the photographs' makers named on the map
        // while it is on. The development server may have no photographs:
        // the button's state is what is checked, not the picture.
        val aerial = hasTestTag("cockpit-aerial")
        waitFor(aerial and isSelected().not())
        tap(aerial)
        waitFor(aerial and isSelected())
        waitFor(hasTestTag("photos-credit"))
        letTheMapDraw(3)
        if (!dark) keepScreenshot("cockpit-aerial")
        tap(aerial)
        waitFor(aerial and isSelected().not())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("photos-credit")) }

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

        // On the map's page the line under the map names the nearest
        // landing, in the core's words; a tap on it lists the places to fly to.
        val nearest = hasTestTag("nearest-landing")
        waitFor(nearest and saying("Sample landing field"))
        assertFalse(exists(landing))
        tap(nearest)
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

        // The readings that have no place on a page, in the core's words:
        // behind "Details" on this page.
        tap(hasTestTag("cockpit-details"))
        waitFor(hasTestTag("cockpit-details-sheet"))
        waitFor(hasTestTag("cockpit-detail") and described("Altitude"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-5c-details")
        tap(hasTestTag("cockpit-details-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("cockpit-details-sheet")) }

        // Locked, the screen answers no touch: neither a swipe nor a tap
        // on the dots turns a page. Holding the lock opens it.
        tap(hasTestTag("cockpit-lock"))
        waitFor(hasTestTag("cockpit-unlock"))
        screen.onNode(hasTestTag("cockpit")).performTouchInput { swipeLeft() }
        tap(hasTestTag("cockpit-pages"))
        letTheMapDraw(2)
        assertTrue(exists(hasTestTag("cockpit-pages") and saying("Flight")))
        keepScreenshot("$prefix-fly-5b-locked")
        screen.onNode(hasTestTag("cockpit-unlock")).performTouchInput { longClick(durationMillis = 1_600) }
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("cockpit-unlock")) }

        // A tap on the dots goes to the next page, round to the first.
        tap(hasTestTag("cockpit-pages"))
        waitFor(hasTestTag("cockpit-pages") and saying("Instruments"))

        // Held for a second, "End" in the top line ends the replay; nothing asks again.
        holdToEnd()
        waitFor(hasText("Open the cockpit"))

        // Settings, and the credits the map data's licences ask for.
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        keepScreenshot("$prefix-settings-1")
        // The glider: its class is none and "Never land" off until the pilot chooses.
        screen.onNode(hasTestTag("never-land")).performScrollTo()
        assertTrue(exists(hasTestTag("glider-class") and hasText("None")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-settings-1b-glider")

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

        // Another service, by its address: one without encryption is
        // refused in the core's words, beside the fields. Nothing is sent.
        tap(described("Assistant options"))
        waitFor(hasText("Another service"))
        tap(hasText("Another service"))
        waitFor(hasTestTag("assistant-service-form"))
        screen.onNode(hasTestTag("assistant-service-address")).performTextInput("http://api.example.org/v1")
        screen.onNode(hasTestTag("assistant-service-model")).performTextInput("some-model")
        screen.onNode(hasTestTag("assistant-service-key")).performTextInput("a-made-up-key")
        // The button lies below the keyboard: scrolled to first.
        screen.onNode(hasTestTag("assistant-service-use")).performScrollTo().performClick()
        waitFor(hasTestTag("assistant-service-problem"))
        screen.onNode(hasTestTag("assistant-service-problem")).performScrollTo()
        assertTrue(exists(hasText("encrypted connection", substring = true) and hasAnyAncestor(hasTestTag("assistant-service-problem"))))
        letTheMapDraw(1)
        keepScreenshot("$prefix-assistant-5-another-service")
        tap(described("Back"))
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

        waitFor(hasTestTag("layer-wind"))
        // The wind's credit appears with the server's first answer about
        // wind. The airspace's has a line of its own in the same box and
        // may be there before it.
        // Every provider whose stations are in view is named.
        seeInCredits(hasText("Wind: ", substring = true) and hasText("Deutscher Wetterdienst", substring = true) and hasText("GeoSphere Austria", substring = true))
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
        waitFor(hasTestTag("layer-wind"))

        // The same stations as a list, by name.
        tapInLayers(described("Wind stations"))
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
        waitFor(hasTestTag("layer-wind"))

        // The assistant looks up the same wind on the same server.
        openFromToday("Assistant")
        val question = hasText("How is the wind on the Zugspitze right now?")
        waitFor(question)
        tap(question)
        waitFor(hasText("Zugspitze: 54 km/h", substring = true))
        assertTrue(exists(hasText("Looked up the wind stations around a place")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-wind-5-assistant")

        // The settings have no address: the app has its server built in.
        // The credits name whose stations it delivers.
        openFromToday("Settings")
        waitFor(hasTestTag("account-link"))
        assertFalse(exists(hasText("Server address")))
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
        // While a warning is up the airspace that matters next stands over
        // the first page's map too, small, in the core's words; it may be
        // about another airspace than the band, but there is one.
        waitForAll(banner, hasTestTag("airspace-proximity"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-8-airspace-proximity")

        // On the map's page it is large: the name, where and how high, and
        // the height line; beside the map the distance flown so far.
        turnTo("Map")
        // The height line is inside the reading, which is one element for
        // TalkBack: it is found by the reading's tag.
        waitFor(hasTestTag("airspace-proximity-map"))
        assertTrue(exists(hasTestTag("airspace-proximity-map") and saying("SAMPLE ZONE (NOT REAL)")))
        waitFor(hasTestTag("score-badge"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-fly-8b-airspace-proximity-map")

        // The line ahead from the side, with the zone as a block if it
        // lies on the line just now; which it does is the flight's to say.
        // Its button is over the map of the second page.
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
        seeInCredits(hasText("Airspace: openAIP, not certified"))
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
        // The airspace's own page says what that means; the Fly tab's row says "None".
        assertTrue(exists(hasText("without airspace the cockpit warns of none", substring = true)))
        tap(described("Back"))
        waitFor(line and hasText("None"))
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
    /**
     * Waits for the account's sign-in form and opens the handle and
     * password form, which stands behind "Sign in with a handle and
     * password" where the server offers Google.
     */
    private fun waitForThePasswordForm() {
        waitFor(hasTestTag("account-handle") or hasTestTag("account-with-password"))
        if (exists(hasTestTag("account-with-password"))) tap(hasTestTag("account-with-password"))
        waitFor(hasTestTag("account-handle"))
    }

    private fun openTheAccount() {
        waitFor(described("Show my position"))
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        screen.onNode(hasTestTag("account-link")).performScrollTo()
        assertTrue(exists(hasTestTag("account-link") and hasText("Not signed in")))
        tap(hasTestTag("account-link"))
        waitForThePasswordForm()
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
        tapInLayers(described("Flying plans"))
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
        // Pressed by its action, not by a touch: the keyboard can still lie
        // over the button, and a touch then lands on the keyboard (seen on
        // the Mac with four emulators at once).
        screen.onNode(hasTestTag("plan-save")).performSemanticsAction(SemanticsActions.OnClick)
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
        // One's own plan is changed or deleted, not reported. Another
        // pilot's would have "Report this plan": the development server's
        // group has no plan of another pilot to show it on.
        assertFalse(exists(hasTestTag("plan-report")))
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
        // A group one keeps is not reported: its keeper renames or deletes
        // it. A member who does not keep a group has "Report this group".
        assertFalse(exists(hasTestTag("group-report")))
        tap(described("Back"))

        // The development server has a second made-up group, which another
        // pilot keeps: the demo pilot comes into it with its invite code,
        // which is no secret, and can then report it. The form is looked
        // at and not sent. The pilot leaves the group again: the other
        // appearance's test finds the server as this one did.
        waitFor(hasTestTag("group-join"))
        tap(hasTestTag("group-join"))
        waitFor(hasTestTag("group-typed"))
        screen.onNode(hasTestTag("group-typed")).performTextInput("5555-5555-5555-5555")
        press(hasText("Join") and hasClickAction())
        val theirGroup = hasTestTag("group-row") and described("Beispielgruppe")
        waitFor(theirGroup)
        tap(theirGroup)
        waitFor(hasTestTag("group-report"))
        assertTrue(exists(hasTestTag("group-member") and described("Mara Muster") and saying("Keeper")))
        screen.onNode(hasTestTag("group-report")).performScrollTo().performClick()
        waitFor(hasTestTag("report-reason"))
        assertTrue(exists(hasText("Report this group")))
        assertTrue(exists(hasTestTag("report-send") and isNotEnabled()))
        letTheMapDraw(1)
        keepScreenshot("$prefix-groups-1-report")
        tap(hasTestTag("report-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("report-reason")) }
        screen.onNode(hasText("Leave the group")).performScrollTo().performClick()
        waitFor(hasTestTag("group-confirm"))
        tap(hasTestTag("group-confirm"))
        waitFor(hasTestTag("group-join"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(theirGroup) }
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("groups")) }

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
        waitForThePasswordForm()
        tap(described("Back"))
        waitFor(hasTestTag("account-link") and hasText("Not signed in"))
        tap(tab("Logbook"))
        waitFor(hasTestTag("logbook-row"))
    }

    @Test
    fun aPilotSignsInAndSharesAFlight() = signInAndShareAFlight("light", dark = false)

    @Test
    fun theAccountInDarkAppearance() = signInAndShareAFlight("dark", dark = true)

    /** Signs in as the development server's demo pilot, from the map, and stays on the account's page. */
    private fun signInAsTheDemoPilot() {
        openTheAccount()
        screen.onNode(hasTestTag("account-handle")).performTextInput("demo")
        screen.onNode(hasTestTag("account-password")).performTextInput("demo-pilot-password")
        tap(hasTestTag("account-submit"))
        waitFor(hasText("@demo"))
        waitFor(hasText("Sync now"))
    }

    /**
     * The pilot's alarms: under the account the demo pilot's two, and what
     * the wind alarm fired; then a wind alarm made from a station's page,
     * whose fault the core names at its field until a direction is chosen,
     * and which the list then shows. Once more in the dark appearance, the
     * list and an alarm's form.
     */
    @Test
    fun thePilotSetsAnAlarmAndSeesWhatFired() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        TestDevice.allowNotifications()
        launch { putExtra(LaunchOptions.SERVER, server) }
        signInAsTheDemoPilot()
        screen.onNode(hasTestTag("account-alerts")).performScrollTo()
        tap(hasTestTag("account-alerts"))
        waitFor(hasTestTag("alerts-page"))
        waitForAll(
            hasTestTag("alert-row") and hasText("Patscherkofel north"),
            hasTestTag("alert-row") and hasText("Nordkette cross-country"),
        )
        assertTrue(exists(hasText("A cross-country day or better at Nordkette", substring = true)))
        waitFor(hasText("Patscherkofel: 15 km/h from the north", substring = true))
        assertTrue(exists(hasTestTag("alerts-fired-title")))
        letTheMapDraw(1)
        keepScreenshot("light-alerts-1-list")

        // A wind alarm from a station's page, its name filled in.
        tap(described("Back"))
        waitFor(hasTestTag("account-alerts"))
        tap(described("Back"))
        waitFor(tab("Map"))
        tap(tab("Map"))
        waitFor(hasTestTag("layer-wind"))
        tapInLayers(described("Wind stations"))
        waitFor(described("Zugspitze"))
        tap(described("Zugspitze"))
        waitFor(hasTestTag("station-page"))
        screen.onNode(hasTestTag("station-page")).performScrollToNode(hasTestTag("station-alert-me"))
        tap(hasTestTag("station-alert-me"))
        waitFor(hasTestTag("alert-form"))
        assertTrue(exists(hasTestTag("alert-name") and hasText("Zugspitze")))
        // Nothing chosen: the core says what is missing, at the directions.
        screen.onNode(hasTestTag("alert-save")).performScrollTo().performClick()
        waitFor(hasTestTag("alert-fault") and hasText("The directions:", substring = true))
        letTheMapDraw(1)
        keepScreenshot("light-alerts-2-form-fault")
        tapChip("alert-from-N")
        tapChip("alert-to-NE")
        screen.onNode(hasTestTag("alert-min-more")).performScrollTo().performClick()
        waitFor(hasTestTag("alert-line") and hasText("Wind at Zugspitze from N to NE, at least 10 km/h"))
        assertFalse(exists(hasTestTag("alert-fault")))
        screen.onNode(hasTestTag("alert-line")).performScrollTo()
        letTheMapDraw(1)
        keepScreenshot("light-alerts-3-form")
        screen.onNode(hasTestTag("alert-save")).performScrollTo().performClick()
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("alert-form")) }
        tap(hasText("Done"))
        waitFor(hasTestTag("layer-wind"))

        // The list has it.
        openFromToday("Settings")
        screen.onNode(hasTestTag("account-link")).performScrollTo()
        tap(hasTestTag("account-link"))
        screen.onNode(hasTestTag("account-alerts")).performScrollTo()
        tap(hasTestTag("account-alerts"))
        waitFor(hasTestTag("alert-row") and hasText("Wind at Zugspitze from N to NE, at least 10 km/h"))
        letTheMapDraw(1)
        keepScreenshot("light-alerts-4-list-with-the-new-alarm")
        app?.close()

        // Dark: the list, and an alarm's form opened from it.
        launch(dark = true) { putExtra(LaunchOptions.SERVER, server) }
        signInAsTheDemoPilot()
        screen.onNode(hasTestTag("account-alerts")).performScrollTo()
        tap(hasTestTag("account-alerts"))
        waitFor(hasText("Patscherkofel: 15 km/h from the north", substring = true))
        letTheMapDraw(1)
        keepScreenshot("dark-alerts-1-list")
        tap(hasTestTag("alert-row") and hasText("Patscherkofel north"))
        waitFor(hasTestTag("alert-form"))
        waitFor(hasTestTag("alert-line") and hasText("Wind at Patscherkofel from N to NE, 10 to 20 km/h", substring = true))
        letTheMapDraw(1)
        keepScreenshot("dark-alerts-2-form")
    }

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
        waitForThePasswordForm()
        assertFalse(exists(hasText("@$handle")))
    }

    /**
     * "Continue with Google" without Google: the development server's
     * made-up Google gives the ID token (`LaunchOptions.GOOGLE_TEST_ACCOUNT`).
     * A first sign-in goes through the handle step, with the server's
     * suggestion and the name from Google filled in, and lands signed in
     * with the Google email on the account; after signing out the same
     * Google account signs in at once. Then once more in the dark.
     * Skipped without a server.
     */
    @Test
    fun continueWithGoogleMakesTheAccountOnceAndThenSignsIn() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        val withGoogle: Intent.() -> Unit = {
            putExtra(LaunchOptions.SERVER, server)
            putExtra(LaunchOptions.GOOGLE_TEST_ACCOUNT, GOOGLE_TEST_EMAIL)
        }
        launch(with = withGoogle)
        openTheGoogleSignIn()
        assertTrue(exists(hasTestTag("account-google") and hasText("Continue with Google")))
        assertTrue(exists(hasTestTag("account-with-password") and hasText("Sign in with a handle and password")))
        assertFalse(exists(hasTestTag("account-handle")))
        keepScreenshot("account-google-1-signed-out")

        // The first time: the handle step, filled in from the server and Google.
        tap(hasTestTag("account-google"))
        waitFor(hasTestTag("account-choose-handle"))
        assertTrue(exists(hasTestTag("account-choose-handle") and hasText("Choose your handle")))
        waitFor(hasTestTag("account-google-handle") and hasText("ui_test", substring = true))
        assertTrue(exists(hasTestTag("account-google-name") and hasText("UI Test", substring = true)))
        assertTrue(exists(hasText("@ui_test is free.", substring = true)))
        keepScreenshot("account-google-2-handle")
        screen.onNode(hasTestTag("account-google-create")).performScrollTo()
        tap(hasTestTag("account-google-create"))
        waitFor(hasText("@ui_test"))
        waitFor(hasText(GOOGLE_TEST_EMAIL) and hasAnyAncestor(hasTestTag("account-google-email")))
        waitFor(hasText("Sync now"))
        assertTrue(exists(hasTestTag("account-unlink-google") and hasText("Unlink Google")))
        keepScreenshot("account-google-3-signed-in")

        // Signed out and in again: Venti knows the Google account now.
        screen.onNode(hasTestTag("account-sign-out")).performScrollTo()
        tap(hasTestTag("account-sign-out"))
        tap(hasTestTag("account-sign-out-confirm"))
        waitFor(hasTestTag("account-google"))
        tap(hasTestTag("account-google"))
        waitFor(hasText(GOOGLE_TEST_EMAIL) and hasAnyAncestor(hasTestTag("account-google-email")))
        assertFalse(exists(hasTestTag("account-choose-handle")))
        assertTrue(exists(hasText("@ui_test")))

        // In the dark, on a phone that has not signed in yet: at once.
        app?.close()
        launch(dark = true, with = withGoogle)
        openTheGoogleSignIn()
        keepScreenshot("account-google-dark-1-signed-out")
        tap(hasTestTag("account-google"))
        waitFor(hasText(GOOGLE_TEST_EMAIL) and hasAnyAncestor(hasTestTag("account-google-email")))
        assertFalse(exists(hasTestTag("account-choose-handle")))
        waitFor(hasText("Sync now"))
        keepScreenshot("account-google-dark-3-signed-in")
    }

    /** Settings, then the account signed out, with "Continue with Google" at its head. */
    private fun openTheGoogleSignIn() {
        waitFor(described("Show my position"))
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        screen.onNode(hasTestTag("account-link")).performScrollTo()
        tap(hasTestTag("account-link"))
        waitFor(hasTestTag("account-google"))
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
        waitForThePasswordForm()
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
            waitFor(hasTestTag("cockpit-pages"))
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
        letTheMapDraw()
        keepScreenshot("$prefix-air-1-map")

        // Both as a list: the pilot, and how high, how it climbs and how
        // old the position is.
        tapInLayers(described("Pilots in the air"))
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
     * The development mode has eleven public flights of six made-up
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
        tapInLayers(described("Flying plans"))
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
        // Over the flights is what they add up to.
        waitFor(hasTestTag("pilot-totals"))
        bringThePilotsFlightsIntoView()
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
        bringThePilotsFlightsIntoView()
        screen.onAllNodes(theirs).onFirst().performClick()
        waitFor(hasTestTag("public-flight-pilot") and hasText(pilot))
        tap(described("Back"))
        waitFor(hasTestTag("pilot-flights"))
        bringThePilotsFlightsIntoView()
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

    /**
     * Finding a pilot, their totals, and the ranking of a month. The field
     * over everyone's flights asks the server for pilots by name once two
     * letters stand in it: the development mode's Lena Probe is found by
     * "le", in the core's line. Her page has what her three public flights
     * add up to, the take-off she flew from most, which opens its page, and
     * the twelve months as bars. The ranking offers the months of its year
     * that have flights, and May 2026 is Lena's. Skipped without a server.
     */
    private fun findAPilotAndAMonth(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        waitFor(described("Show my position"))
        tap(tab("League"))
        waitFor(hasTestTag("league-everyone") and isSelected())
        val field = hasTestTag("pilot-search")
        waitFor(field)
        waitFor(hasTestTag("public-flight-row"))

        // One letter is not asked for: the core says how many it takes.
        screen.onNode(field).performTextInput("l")
        waitFor(hasTestTag("pilots-found-words") and hasText("Type at least two letters", substring = true))
        assertFalse(exists(hasTestTag("pilot-found")))
        // Two are, once the typing has paused.
        screen.onNode(field).performTextInput("e")
        val lena = hasText("Lena Probe (@lena), 3 public flights")
        waitFor(lena)
        assertEquals(1, screen.onAllNodes(hasTestTag("pilot-found")).fetchSemanticsNodes().size)
        assertTrue(exists(hasTestTag("pilots-found-words") and hasText("1 pilot")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-pilots-1-found")

        // Her page: the totals of all years over her flights.
        tap(lena)
        waitFor(hasTestTag("pilot-totals"))
        assertTrue(exists(hasText("Lena Probe")))
        assertTrue(exists(hasText("Points, best 3 flights")))
        assertTrue(exists(hasText("28.19")))
        assertTrue(exists(hasTestTag("pilot-site") and hasText("Musterkogel West, 2 flights")))
        assertTrue(exists(described("May: 1 flight") and hasAnyAncestor(hasTestTag("pilot-months"))))
        assertTrue(exists(hasTestTag("pilot-totals-note")))
        assertTrue(exists(hasTestTag("pilot-totals-terms")))
        val season = hasText("2026") and hasAnyAncestor(hasTestTag("pilot-seasons"))
        tap(season)
        waitFor(season and isSelected())
        assertTrue(exists(hasText("28.19")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-pilots-2-totals")
        // Under the totals are the flights they are of.
        bringThePilotsFlightsIntoView()
        assertTrue(
            screen.onAllNodes(hasTestTag("pilot-flight-row")).fetchSemanticsNodes().all {
                it.config[SemanticsProperties.ContentDescription] == listOf("Lena Probe")
            },
        )
        screen.onNode(hasTestTag("pilot-flights")).performScrollToNode(hasTestTag("pilot-totals"))

        // The longest flight is a row that opens that flight.
        tap(hasTestTag("pilot-total-flight") and hasText("Longest flight"))
        waitFor(hasTestTag("public-flight-pilot") and hasText("Lena Probe"))
        tap(described("Back"))
        waitFor(hasTestTag("pilot-totals"))
        // The take-off opens its page, in a sheet over hers.
        tap(hasTestTag("pilot-site"))
        waitFor(hasTestTag("site-page"))
        assertTrue(exists(hasText("Musterkogel West")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-pilots-3-takeoff")
        tap(hasText("Done") and hasClickAction())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("site-page")) }

        // Back to the search as it was left; emptied, the list is everyone's flights again.
        tap(described("Back"))
        waitFor(lena)
        tap(described("Clear"))
        waitFor(hasTestTag("public-flight-row"))

        // The ranking: the year's months that have flights, the newest
        // first, and May's ranking under its name.
        tap(hasTestTag("league-ranking"))
        waitFor(hasTestTag("ranking-months"))
        waitFor(hasTestTag("ranking-title") and hasText("2026"))
        val may = described("May 2026: 1 flight")
        screen.onNode(may).performScrollTo().performClick()
        waitFor(hasTestTag("ranking-title") and hasText("May 2026"))
        waitFor(hasTestTag("ranked-pilot"))
        assertEquals(
            listOf("Place 1, Lena Probe"),
            screen.onAllNodes(hasTestTag("ranked-pilot")).fetchSemanticsNodes().first().config[SemanticsProperties.ContentDescription],
        )
        assertTrue(exists(may and isSelected()))
        letTheMapDraw(1)
        keepScreenshot("$prefix-pilots-4-month")
        // Tapped again, the month gives the whole year back.
        screen.onNode(may).performScrollTo().performClick()
        waitFor(hasTestTag("ranking-title") and hasText("2026"))
        assertFalse(exists(may and isSelected()))
    }

    /**
     * A pilot's flights stand under what they add up to, and a list makes
     * only the rows that are in view: waits until the server has answered
     * with the flights and brings the first of them into view.
     */
    private fun bringThePilotsFlightsIntoView() {
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            runCatching {
                screen.onNode(hasTestTag("pilot-flights")).performScrollToNode(hasTestTag("pilot-flight-row"))
            }.isSuccess
        }
    }

    @Test
    fun aPilotIsFoundByNameAndTheRankingHasMonths() = findAPilotAndAMonth("light", dark = false)

    @Test
    fun findingAPilotInDarkAppearance() = findAPilotAndAMonth("dark", dark = true)

    @Test
    fun theMapShowsWhoIsInTheAir() = lookAtWhoIsInTheAir("light", dark = false)

    @Test
    fun whoIsInTheAirInDarkAppearance() = lookAtWhoIsInTheAir("dark", dark = true)

    /**
     * Others in the air from the Open Glider Network: the development
     * server's made-up aircraft over the Tegernsee are drawn beside Venti's
     * own pilots with the "Pilots" layer on, the network's credit is with
     * the map's, and a tap on the paraglider over the Wallberg opens its
     * sheet with the core's line. Light, then the same in the dark
     * appearance. Skipped without a server.
     */
    @Test
    fun othersInTheAirFromTheOpenGliderNetwork() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        for (dark in listOf(false, true)) {
            val prefix = if (dark) "dark" else "light"
            launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
            val scenario = checkNotNull(app)
            waitFor(described("Show my position"))
            waitFor(hasTestTag("layer-pilots") and isOn())
            assertTrue(TestDevice.lookAt(scenario, lat = 47.6642, lon = 11.7967, zoom = 11.0))
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
                (TestDevice.drawnOnTheMap(scenario, "live-others") ?: 0) > 0
            }
            seeInCredits(hasText("Positions: Open Glider Network", substring = true))
            letTheMapDraw(2)
            keepScreenshot("$prefix-others-1-map")

            TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.664083, lon = 11.7967)))
            waitFor(hasTestTag("other-line") and hasText("Paraglider at 2,140 m", substring = true))
            assertTrue(exists(hasTestTag("other-kind") and hasText("Paraglider")))
            // The line ends with the position's age; it is not said again under it.
            assertFalse(exists(hasTestTag("other-age")))
            assertTrue(exists(hasTestTag("other-credit")))
            letTheMapDraw(1)
            keepScreenshot("$prefix-others-2-sheet")
            tap(hasText("Done"))
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("other-line")) }
            app?.close()
        }
    }

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
        assertFalse(exists(hasTestTag("sites-filter-open")))

        // The take-off stands left of the round buttons along the right
        // edge, so that a tap on its pin is a tap on the pin.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.69, lon = 11.875, zoom = 11.0))
        waitFor(hasTestTag("sites-filter-open"))
        seeInCredits(hasText("Sites: OpenStreetMap contributors"))
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
        waitFor(hasTestTag("sites-filter-open"))

        // The chips switch the sites off and on again, and the map
        // remembers.
        tapChip("layer-sites")
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("sites-filter-open")) }
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-2c-sites-off")
        tapChip("layer-sites")
        waitFor(hasTestTag("sites-filter-open"))

        // The same signs as a list: those with a name first.
        tapInLayers(described("Flying sites"))
        waitFor(described(takeoff))
        assertTrue(exists(described("Landeplatz Jägerkamp/Nagelspitz")))
        assertTrue(exists(hasText("A site on the map is not permission to fly there.", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-3-list")

        // One site: what its source says, and what that is worth. Kept as
        // a waypoint, the cockpit can navigate to it.
        tap(described(takeoff))
        // The page is a lazy list: what lies under the head and the picture
        // of the day's air is there only once it is scrolled to, and goes
        // again when it is scrolled past. So the official mark is found by
        // scrolling to it (finding it is the check), and the keep row after.
        waitFor(hasTestTag("site-page"))
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasText("Marked as an official site"))
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-keep"))
        waitFor(hasText("Add to my waypoints"))
        tap(hasText("Add to my waypoints"))
        waitFor(hasText("In your waypoints"))
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasText("See or correct it at the source"))
        unfoldHowASiteIsRead()
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

        // Back on the site's page, which still knows that it was kept. The
        // page is a lazy list and comes back at its head: the kept row
        // exists again only once it is scrolled to.
        tap(described("Back"))
        waitFor(hasTestTag("site-page"))
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-keep"))
        waitFor(hasText("In your waypoints"))
        tap(described("Back"))
        waitFor(described(takeoff))
        tap(hasText("Done"))
        waitFor(hasTestTag("sites-filter-open"))

        // A take-off whose source says which wind it works in, on the Wank
        // above Garmisch. The map shipped with the app ends before it; the
        // sites are the server's all the same.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.5067, lon = 11.10, zoom = 10.5))
        tapInLayers(described("Flying sites"))
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
        waitFor(hasTestTag("sites-filter-open"))

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
        waitFor(hasTestTag("site-outlook"))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-today-3-site")
        // How it is read, whose data it is and the licence: behind a row at the page's end.
        unfoldHowASiteIsRead()
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-today-3b-site-how-read")
        tap(hasText("Done"))
        waitFor(hasTestTag("today-card"))
    }

    /**
     * A day with thermals, against the stand-in in front of the development
     * server whose forecast is a made-up summer day (tools/made-up-day): the
     * recorded forecasts are of late November and have none. The day is
     * made up, and so is everything the screenshots say of it. Skipped
     * without the stand-in.
     */
    @Test
    fun todaySaysWhatKindOfDayTheThermalsMake() {
        val server = InstrumentationRegistry.getArguments().getString("madeUpDayServer")
        assumeTrue("No stand-in with a made-up day was named in the madeUpDayServer argument.", !server.isNullOrEmpty())
        launch(onToday = true) { putExtra(LaunchOptions.SERVER, server) }

        // The best take-off is the one with the best thermals: the head
        // says its kind of day over the valley, its card the line.
        waitFor(hasTestTag("today-thermals") and hasText("Big day"))
        waitFor(hasTestTag("today-card") and described("Big day") and saying("m/s to a cloud base of"))
        screen.onNode(hasTestTag("today-head")).performScrollTo()
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-5-big-day")

        // Further down: the hours with their climbs, and the other
        // take-offs with their kinds of day.
        screen.onAllNodes(hasTestTag("today-row")).onLast().performScrollTo()
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-6-kinds-of-day")

        // The take-off's own page says it too.
        screen.onNode(hasTestTag("today-card")).performScrollTo()
        tap(hasTestTag("today-card"))
        waitFor(hasTestTag("site-status"))
        waitFor(hasTestTag("thermal-line"))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-7-site-on-a-big-day")
        // The air over the take-off through the day, as a picture: the
        // sheet pulled up to its full height first.
        screen.onNode(hasTestTag("site-page")).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("air-chart"))
        waitFor(hasTestTag("air-chart") and saying("m/s to a cloud base of"))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-8-air-through-the-day")

        // The forecast for the take-off has the thermals' two rows under
        // the sky, and says under the table what they are.
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-forecast"))
        tap(hasTestTag("site-forecast"))
        waitFor(hasTestTag("forecast-table"))
        waitForAll(described("Thermals, climb, m/s"), described("Thermals, top, m"))
        screen.onNode(hasTestTag("forecast-thermals-note")).performScrollTo()
        assertTrue(exists(hasText("not a value of the model", substring = true)))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("forecast-thermals")
        tap(hasText("Done"))
        waitFor(hasTestTag("today-card"))
    }

    /**
     * The forecast map on the map tab: the kind of day square by square,
     * against the stand-in with a made-up day (`madeUpDayServer`, as for
     * Today's kinds of day); without it the test is skipped. Its map has
     * every kind of day in clear patches around 47.5 to 47.9 N and 11.5 to
     * 12.3 E, and two days; the middle of the map, 47.7 N 11.9 E, is weak
     * thermals on both. What its screenshots show is made up. The forecast
     * a square leads to opens on the day the map showed.
     */
    @Test
    fun theMapShowsTheKindOfDaySquareBySquare() {
        val server = InstrumentationRegistry.getArguments().getString("madeUpDayServer")
        assumeTrue("No stand-in with a made-up day was named in the madeUpDayServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(hasTestTag("layer-wind"))
        // Nothing else of Venti's under the finger where a square is tapped.
        // The chips' row scrolls: a chip beyond its edge is scrolled to
        // first. A test's choices are its own: every launch starts a new
        // session with the chips as a new phone has them.
        for (layer in listOf("layer-wind", "layer-sites", "layer-pilots")) {
            tapChip(layer)
            screen.onNode(hasTestTag(layer)).assertIsOff()
        }
        assertTrue(TestDevice.lookAt(scenario, lat = 47.7, lon = 11.9, zoom = 8.6))
        // Off until the pilot switches it on, in the layers sheet.
        switchLayer("layer-forecast", on = true)

        // The card: the days and, behind "Key and notes", the key from the
        // best kind of day down to "Not flyable" and the model with its run.
        waitFor(hasTestTag("forecast-days"))
        assertFalse(exists(hasTestTag("forecast-key")))
        unfoldTheKeyAndNotes()
        waitFor(hasTestTag("forecast-key"))
        // A line of the key is read as its words: it has a description and
        // no text of its own for a test to find.
        waitForAll(hasTestTag("forecast-days"), described("Big day"), described("Not flyable"), hasTestTag("forecast-map-run"))
        // The first day the map has is shown first. Its name is the core's,
        // by the server's clock; the stand-in dates its map from that
        // clock, so the days are "Today", "Tomorrow" and a third. The test
        // goes by their place in the switch all the same, since which day
        // is today also hangs on the emulator's time zone.
        val days = isSelectable() and hasAnyAncestor(hasTestTag("forecast-days"))
        waitFor(days)
        assertTrue(screen.onAllNodes(days)[0].fetchSemanticsNode().config.getOrNull(SemanticsProperties.Selected) == true)
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "forecast-squares") ?: 0) > 0
        }
        // Whose forecast it is, with the map's other credits.
        seeInCredits(hasText("Forecast: ", substring = true))
        letTheMapDraw()
        keepScreenshot("map-forecast-1-today")

        // Another day swaps the squares: the second the map has.
        val second = screen.onAllNodes(days)[1]
        val secondName = checkNotNull(
            second.fetchSemanticsNode().config.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text },
        )
        second.performClick()
        waitFor(days and hasText(secondName) and isSelected())
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "forecast-squares") ?: 0) > 0
        }
        letTheMapDraw()
        keepScreenshot("map-forecast-2-tomorrow")

        // A square tapped opens the pack's sheet with the core's words.
        val square = checkNotNull(TestDevice.drawnInTheMiddle(scenario, "forecast-squares")) { "no square in the middle of the map" }
        TestDevice.tap(square)
        val sheet = hasTestTag("forecast-square")
        waitFor(sheet)
        waitForAll(
            hasText(secondName) and hasAnyAncestor(sheet),
            hasTestTag("square-status"),
            hasText("Judged at the model's ground, ", substring = true),
            hasTestTag("square-note"),
        )
        // The status, read with what speaks against the day where something does: "Marginal, Gusts".
        val statuses = listOf("Flyable", "Marginal", "Not flyable").map { hasContentDescription(it, substring = true) }
        assertTrue(exists(hasTestTag("square-status") and statuses.reduce { one, other -> one or other }))
        // A square with a kind of day has the mark and its line; one that is not flyable has none.
        if (!exists(hasTestTag("square-status") and hasContentDescription("Not flyable", substring = true))) {
            assertTrue(exists(hasTestTag("thermal-line") and hasAnyAncestor(sheet)))
        }
        letTheMapDraw(2)
        keepScreenshot("map-forecast-3-square")

        // The square leads to the forecast for its middle, in the same sheet,
        // on the day the map showed: its second. The stand-in's forecast has
        // the map's days from its first on, so that is the forecast's second
        // day too; from anywhere else the forecast opens on today, or on its
        // first day when it has no today.
        screen.onNode(hasTestTag("forecast-square-forecast")).performScrollTo()
        tap(hasTestTag("forecast-square-forecast"))
        waitFor(hasTestTag("forecast-table"))
        waitFor(hasTestTag("forecast-day-1") and isSelected())
        letTheMapDraw(1)
        keepScreenshot("map-forecast-4-forecast")
        tap(hasText("Done"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("forecast-table")) }

        // Switched off, the card goes and the squares with it.
        tapChip("layer-forecast")
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("forecast-map-card")) }
    }

    /** A layer of the forecast map's card, by its name: one of the chips of which one is on. */
    private fun forecastLayer(name: String) = isSelectable() and hasAnyAncestor(hasTestTag("forecast-layers")) and hasText(name)

    /** Chooses a layer of the forecast map: its chips scroll across the card, so the chip is scrolled to first. */
    private fun chooseForecastLayer(name: String) {
        waitFor(forecastLayer(name))
        screen.onAllNodes(forecastLayer(name)).onFirst().performScrollTo().performClick()
        waitFor(forecastLayer(name) and isSelected())
    }

    /** Sets the forecast map's line of hours to a place in it: the strip is one value to adjust. */
    private fun toHour(place: Int) {
        screen.onNode(hasTestTag("forecast-hours")).performSemanticsAction(SemanticsActions.SetProgress) { it(place.toFloat()) }
        screen.waitForIdle()
    }

    /** How many places the forecast map's line of hours has. */
    private fun placesOfTheHours(): Int = placesOf("forecast-hours")

    /**
     * Steps the forecast map's line of hours to an hour of the pilot's
     * clock, "13:00". The hours are named by the emulator's clock, so the
     * place of an hour in the line is looked for, not counted.
     */
    private fun toClock(label: String) {
        val strip = hasTestTag("forecast-hours")
        waitFor(strip)
        for (place in 0 until placesOfTheHours()) {
            toHour(place)
            if (exists(strip and saying(label))) return
        }
        throw AssertionError("The line of hours has no $label.")
    }

    /** Presses something by its action: a menu is a window of its own, where a touch went astray. */
    private fun press(what: SemanticsMatcher) {
        waitFor(what)
        screen.onAllNodes(what).onFirst().performSemanticsAction(SemanticsActions.OnClick)
    }

    /** Chooses the height of the forecast map's wind from its menu: "ground", "1000", "2000". */
    private fun chooseForecastHeight(level: String, shown: String) {
        press(hasTestTag("forecast-level"))
        press(hasTestTag("forecast-level-$level"))
        waitFor(hasTestTag("forecast-level") and hasText(shown))
    }

    /** Where something on screen stands, in the window, by its tag. */
    private fun boundsOf(tag: String) = screen.onNode(hasTestTag(tag)).fetchSemanticsNode().boundsInRoot

    /**
     * Shows the made-up day's map so that something of every layer stands
     * clear between the chips and the card. Its grid has its squares
     * around 47.5 to 47.9 N and 11.5 to 12.3 E, each a tenth of a degree:
     * every kind of day in patches from west to east, the gusts and the
     * cloud in the east, and the rain in the south-east corner, 47.5 and
     * 47.6 N, 12.2 and 12.3 E. The view is of 11.6 to 12.4 E, with the
     * grid's south just above the card; the northernmost row lies under
     * the chips, and the two westernmost columns outside the view.
     */
    private fun lookAtTheMadeUpDay(scenario: ActivityScenario<MainActivity>) {
        assertTrue(TestDevice.lookAt(scenario, lat = 47.5, lon = 12.0, zoom = 8.6))
    }

    /**
     * The forecast map by the hour, against the made-up day: every layer
     * of its days, eleven hours each, a wind at each height and none at
     * 1000 m where its made-up ground lies higher. Everything its
     * screenshots show is made up. Skipped without the stand-in.
     *
     * The layers are looked at at 13:00: late in the day the made-up day
     * has little cloud and no rain. Its rain and its gusts are in the
     * hours from 10 to 15 UTC, which 13:00 is on a clock from two hours
     * behind UTC to three ahead of it.
     */
    private fun lookAtTheForecastByTheHour(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("madeUpDayServer")
        assumeTrue("No stand-in with a made-up day was named in the madeUpDayServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(hasTestTag("map-layers"))
        for (layer in listOf("layer-wind", "layer-sites", "layer-pilots")) {
            tapChip(layer)
        }
        lookAtTheMadeUpDay(scenario)
        tapChip("layer-forecast")
        waitFor(hasTestTag("forecast-layers"))
        val strip = hasTestTag("forecast-hours")
        val said = hasTestTag("forecast-said")
        val fold = hasTestTag("forecast-key-and-notes")
        fun drawn(layer: String) = screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { (TestDevice.drawnOnTheMap(scenario, layer) ?: 0) > 0 }

        // "Thermals" opens on the day as a whole: the days' squares. Under
        // the line of hours the card says what the map shows, on "Day"
        // with the day's name where an hour's would stand.
        waitFor(forecastLayer("Thermals") and isSelected())
        waitFor(strip)
        assertTrue(exists(strip and saying("Day")))
        val days = isSelectable() and hasAnyAncestor(hasTestTag("forecast-days"))
        val firstDay = checkNotNull(
            screen.onAllNodes(days)[0].fetchSemanticsNode().config.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text },
        )
        waitFor(said and hasText("Thermals, $firstDay"))
        // The key, the model's run and the notes are folded away when the
        // card opens, and TalkBack is told so; the height is the wind's.
        assertTrue(exists(fold and saying("Collapsed")))
        assertFalse(exists(hasTestTag("forecast-key")))
        assertFalse(exists(hasTestTag("forecast-map-run")))
        assertFalse(exists(hasTestTag("forecast-level")))
        drawn("forecast-squares")
        letTheMapDraw()
        keepScreenshot("$prefix-map-hours-1-thermals-day")

        // An hour of the thermals: the strip is one value to adjust.
        toClock("13:00")
        drawn("forecast-hours-squares")
        assertFalse(exists(strip and saying("Day")))
        waitFor(said and hasText("Thermals, 13:00"))
        letTheMapDraw()
        keepScreenshot("$prefix-map-hours-2-thermals-hour")

        // The other layers keep the hour the pilot chose, also when they
        // are asked for the first time. The height's menu stands on the
        // line of the words, with the wind alone.
        val titles = listOf("Wind" to "Wind at the ground", "Gusts" to "Gusts at the ground", "Cloud" to "Cloud", "Rain" to "Rain")
        for ((index, layer) in titles.withIndex()) {
            val (name, title) = layer
            chooseForecastLayer(name)
            waitFor(strip and saying("13:00"))
            waitFor(said and hasText("$title, 13:00"))
            drawn("forecast-hours-squares")
            assertTrue((name == "Wind") == exists(hasTestTag("forecast-level")))
            assertFalse(exists(hasTestTag("forecast-key")))
            letTheMapDraw()
            keepScreenshot("$prefix-map-hours-${index + 3}-${name.lowercase()}")
        }

        // The wind at 2000 m, from the height's menu.
        chooseForecastLayer("Wind")
        chooseForecastHeight("2000", "2000 m")
        waitFor(said and hasText("Wind at 2000 m, 13:00"))
        drawn("forecast-hours-arrows")
        // The menu stands at the right of the words, clear of them.
        assertTrue(boundsOf("forecast-said").right <= boundsOf("forecast-level").left)
        letTheMapDraw()
        keepScreenshot("$prefix-map-hours-7-wind-2000")

        // "Key and notes" unfolded: the layer's key, the model with its
        // run, and the layer's note in the core's words.
        unfoldTheKeyAndNotes()
        waitForAll(hasTestTag("forecast-key"), described("20 to 30 km/h"), hasTestTag("forecast-map-run"), hasTestTag("forecast-layer-note"))
        assertTrue(exists(hasTestTag("forecast-layer-note") and hasText("The model's mean wind", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-map-hours-8-key-and-notes")
        screen.onNode(fold).performScrollTo().performClick()
        waitFor(fold and saying("Collapsed"))
        assertFalse(exists(hasTestTag("forecast-key")))

        // At 1000 m the made-up ground lies higher under some squares,
        // which then have no wind and are not drawn: the one around
        // 47.8 N 11.9 E has its ground at 1150 m. A tap there still opens
        // the square, which says so.
        chooseForecastHeight("1000", "1000 m")
        waitFor(said and hasText("Wind at 1000 m, 13:00"))
        drawn("forecast-hours-squares")
        letTheMapDraw(2)
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.8, lon = 11.9)))
        val sheet = hasTestTag("forecast-hour-square")
        waitFor(sheet)
        waitFor(hasTestTag("square-hour") and described("No wind at 1000 m here") and saying("The model's ground lies higher, at 1150 m."))
        assertTrue(exists(hasTestTag("square-days")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-map-hours-9-square")
        tap(hasText("Done") and hasAnyAncestor(sheet))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(sheet) }

        // How long the core takes for the squares and the arrows of an
        // hour, as the app's model times it: every hour of the wind at the
        // ground, each with its arrows. Said in the test's log.
        chooseForecastHeight("ground", "Ground")
        waitFor(said and hasText("Wind at the ground, 13:00"))
        val steps = (0 until placesOfTheHours()).map { place ->
            toHour(place)
            var took = -1L
            scenario.onActivity { activity ->
                val session = (activity.application as VentiApplication).testSession as? Session.Ready
                took = session?.thermalMap?.hours?.lastStepMillis ?: -1L
            }
            took
        }
        assertTrue("A step of an hour took ${steps.max()} ms.", steps.all { it in 0..500 })
        TestDevice.say("forecastHourStepMillis", steps.joinToString(" "))

        // Back to the thermals on the day, as a new phone has them.
        chooseForecastLayer("Thermals")
        toHour(0)
        waitFor(strip and saying("Day"))
    }

    /**
     * The forecast map's card while its hours do not come, and when they
     * are refused. The map of the days comes from the made-up day's
     * stand-in; its hours pass through a server the test plays in front of
     * it ([HoursHeldBack]), which first keeps the app waiting, then
     * answers as a server that is being restarted does, and at last hands
     * the hours on. Skipped without the stand-in.
     */
    @Test
    fun theForecastMapWaitsForItsHoursAndSaysWhenTheyDoNotCome() {
        val server = InstrumentationRegistry.getArguments().getString("madeUpDayServer")
        assumeTrue("No stand-in with a made-up day was named in the madeUpDayServer argument.", !server.isNullOrEmpty())
        HoursHeldBack(checkNotNull(server)).use { held ->
            launch { putExtra(LaunchOptions.SERVER, held.address) }
            val scenario = checkNotNull(app)
            waitFor(hasTestTag("map-layers"))
            for (layer in listOf("layer-wind", "layer-sites", "layer-pilots")) {
                tapChip(layer)
            }
            lookAtTheMadeUpDay(scenario)
            tapChip("layer-forecast")

            // The days are there and their squares are drawn; where the
            // hours will stand the card says that they are asked for.
            waitFor(hasTestTag("forecast-hours-asking"))
            assertTrue(exists(hasText("Asking Venti for the hours")))
            waitFor(hasTestTag("forecast-days"))
            assertFalse(exists(hasTestTag("forecast-hours")))
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { (TestDevice.drawnOnTheMap(scenario, "forecast-squares") ?: 0) > 0 }
            // Kept at once: the app gives a server twenty seconds to answer.
            keepScreenshot("map-hours-0-waiting")
            assertTrue(exists(hasTestTag("forecast-hours-asking")))

            // Refused: the card says so in the core's words, with "Try
            // again", and the layer falls back to the day.
            held.hours = HoursHeldBack.Hours.REFUSED
            val problem = hasTestTag("forecast-hours-problem")
            waitFor(problem)
            assertTrue(exists(hasText("The server is busy or being restarted. Try again in a moment.")))
            assertFalse(exists(hasTestTag("forecast-hours")))
            assertTrue((TestDevice.drawnOnTheMap(scenario, "forecast-squares") ?: 0) > 0)
            letTheMapDraw(1)
            keepScreenshot("map-hours-0-refused")

            // "Try again" asks anew, and the hours come.
            held.hours = HoursHeldBack.Hours.HANDED_ON
            tap(hasText("Try again") and hasAnyAncestor(problem))
            waitFor(hasTestTag("forecast-hours"))
            assertTrue(exists(hasTestTag("forecast-hours") and saying("Day")))
            assertFalse(exists(problem))
        }
    }

    /**
     * The rain radar on the map, against Venti in development mode, which
     * has two layers recorded on 2026-11-21 by a clock that stands at
     * 13:19 UTC: Germany's radars with frames of 13:10 and 13:15, the
     * newest four minutes old, and under them Austria's analysis, of which
     * both moments take the frame of 13:00. The layer is off until it is
     * switched on; its card has the two moments with the newest chosen and
     * the moment's line, what the two pictures are and the key behind
     * "Key and notes", a step to the earlier moment, and the moments
     * played through to the newest. The map is turned far enough out to
     * show the whole region: the analysis fills what the radars do not
     * see, and the veil lies only beyond both. Skipped without a server.
     */
    @Test
    fun theMapShowsTheRainRadar() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(hasTestTag("layer-wind"))
        assertFalse(showsARadarLayer(scenario))
        lookAtTheWholeRadar(scenario)
        // Off until the pilot switches it on, in the layers sheet.
        switchLayer("layer-radar", on = true)

        // The card: two moments, the newest chosen, and its line in the
        // core's words, which says of the analysis under it what it is of.
        // The hours are the emulator's clock's, so only their pattern is held.
        val strip = hasTestTag("radar-frames")
        val said = hasTestTag("radar-said")
        waitFor(strip)
        assertEquals(2, placesOf("radar-frames"))
        waitFor(strip and saying(", 4 minutes ago"))
        waitFor(said and worded("""^Rain radar, \d\d:\d\d, 4 minutes ago\. Austria's analysis is of \d\d:\d\d\.$"""))
        // Both layers are on the map, each a layer of its own.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            showsBothRadarLayers(scenario)
        }
        // Whose rain it is, at the data: both sources with the map's other
        // credits, which a tap on their line shows in full.
        seeInCredits(
            hasText("Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben"),
            hasText("Data source: GeoSphere Austria", substring = true),
        )
        assertFalse(exists(hasTestTag("radar-key")))
        assertFalse(exists(hasTestTag("radar-stale")))
        letTheMapDraw()
        keepScreenshot("map-radar-1-newest")

        // "Key and notes": what the two pictures are, the key from the
        // most rain down, what was not seen, the note and the credits.
        unfoldTheKeyAndNotes("radar-key-and-notes")
        val layers = hasAnyAncestor(hasTestTag("radar-layers"))
        val credited = hasAnyAncestor(hasTestTag("radar-credit"))
        waitForAll(
            hasText("Germany's radars, every 5 minutes") and layers,
            hasText("Austria's analysis of radar and gauges, every 15 minutes") and layers,
            hasText("Deutscher Wetterdienst", substring = true) and credited,
            hasText("GeoSphere Austria", substring = true) and credited,
            hasTestTag("radar-key"),
            described("Cloudburst, 30 mm an hour and more"),
            described("A trace, under 0.2 mm an hour"),
            described("Not seen by the radar"),
            hasTestTag("radar-note"),
        )
        letTheMapDraw(1)
        keepScreenshot("map-radar-2-key")
        screen.onNode(hasTestTag("radar-key-and-notes")).performScrollTo().performClick()
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("radar-key")) }

        // A step to the earlier moment swaps the radars' picture and the
        // line; the analysis under it is the same one.
        screen.onNode(strip).performSemanticsAction(SemanticsActions.SetProgress) { it(0f) }
        waitFor(said and worded(""", 9 minutes ago\. Austria's analysis is of \d\d:\d\d\.$"""))
        assertTrue(showsBothRadarLayers(scenario))
        letTheMapDraw(1)
        keepScreenshot("map-radar-3-earlier-frame")

        // Played through, the moments end on the newest, and the button is the one to play again.
        tap(hasTestTag("radar-play"))
        waitFor(said and hasText(", 4 minutes ago", substring = true))
        waitFor(described("Play the radar pictures"))

        // Closer in, over the Tegernsee: a step of rain stays a step, not smoothed.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.7, lon = 11.75, zoom = 8.6))
        letTheMapDraw()
        keepScreenshot("map-radar-4-closer")

        // Switched off, the card goes and both pictures with it.
        tapChip("layer-radar")
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("radar-card")) }
        assertFalse(showsARadarLayer(scenario))
    }

    /**
     * The rain radar's two layers over the whole region with the moment's
     * line, and its card with what the layers are and the key unfolded,
     * in the dark appearance. Only the screenshots matter here.
     */
    @Test
    fun theRainRadarInDarkAppearance() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = true) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(hasTestTag("map-layers"))
        lookAtTheWholeRadar(scenario)
        tapChip("layer-radar")
        waitFor(hasTestTag("radar-said"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            showsBothRadarLayers(scenario)
        }
        letTheMapDraw()
        keepScreenshot("map-radar-5-dark-map")
        unfoldTheKeyAndNotes("radar-key-and-notes")
        waitForAll(hasTestTag("radar-layers"), hasTestTag("radar-key"))
        letTheMapDraw(1)
        keepScreenshot("map-radar-5-dark")
    }

    /**
     * A server without a rain radar: the test plays one in front of Venti
     * in development mode ([HoursHeldBack]) that answers the radar's list
     * as such a server does, with 501 and `no_radar`. The card says so in
     * the core's words and nothing is drawn; then the server hands the
     * list on, and "Try again" brings the frames. Skipped without a server.
     */
    @Test
    fun aServerWithoutARainRadarSaysSo() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        val none = 501 to """{"error":{"code":"no_radar","message":"This server collects no rain radar."}}"""
        HoursHeldBack(checkNotNull(server), path = "/v1/radar", refusal = none).use { held ->
            held.hours = HoursHeldBack.Hours.REFUSED
            launch { putExtra(LaunchOptions.SERVER, held.address) }
            val scenario = checkNotNull(app)
            waitFor(hasTestTag("map-layers"))
            lookAtTheWholeRadar(scenario)
            tapChip("layer-radar")

            val problem = hasTestTag("radar-problem")
            waitFor(problem)
            assertTrue(exists(hasText("This server has no rain radar.")))
            assertFalse(exists(hasTestTag("radar-frames")))
            assertFalse(showsARadarLayer(scenario))
            letTheMapDraw(1)
            keepScreenshot("map-radar-6-no-radar")

            // "Try again" asks anew, and the frames come.
            held.hours = HoursHeldBack.Hours.HANDED_ON
            tap(hasText("Try again") and hasAnyAncestor(problem))
            waitFor(hasTestTag("radar-frames"))
            assertFalse(exists(problem))
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
                showsBothRadarLayers(scenario)
            }
        }
    }

    /**
     * Whether the map shows a picture of each of the rain radar's two
     * layers, which the map names by their place in the core's list: the
     * analysis lowest, the radars over it. A picture is no feature to
     * count, so the map is asked whether its layers show.
     */
    private fun showsBothRadarLayers(scenario: ActivityScenario<MainActivity>): Boolean =
        (0..1).all { TestDevice.showsOnTheMap(scenario, RadarLayer.name(it)) }

    /** Whether the map shows any picture of the rain radar. */
    private fun showsARadarLayer(scenario: ActivityScenario<MainActivity>): Boolean =
        (0..1).any { TestDevice.showsOnTheMap(scenario, RadarLayer.name(it)) }

    /**
     * Turns the map to the whole of the radar's pictures, 9.4 to 17.2 E
     * and 46.3 to 48.5 N, with their edge in view on every side.
     */
    private fun lookAtTheWholeRadar(scenario: ActivityScenario<MainActivity>) {
        assertTrue(TestDevice.lookAt(scenario, lat = 46.6, lon = 13.3, zoom = 5.0))
    }

    /** How many places a strip has: the forecast map's hours, the rain radar's frames. */
    private fun placesOf(tag: String): Int {
        val strip = screen.onNode(hasTestTag(tag)).fetchSemanticsNode()
        return strip.config[SemanticsProperties.ProgressBarRangeInfo].range.endInclusive.toInt() + 1
    }

    /**
     * The forecast map's card on a screen 360 dp wide with text at 130 %:
     * the layers' row, four days, the line of hours with "Day" and eleven
     * hours, and the words with the height's menu beside them. The test
     * sets the device so and puts it back, whatever happens. Only the
     * screenshots and that nothing lies over another matter here. Skipped
     * without the stand-in.
     */
    @Test
    fun theForecastMapByTheHourOnANarrowScreenWithLargeText() {
        val server = InstrumentationRegistry.getArguments().getString("madeUpDayServer")
        assumeTrue("No stand-in with a made-up day was named in the madeUpDayServer argument.", !server.isNullOrEmpty())
        TestDevice.narrowWithLargeText()
        try {
            launch { putExtra(LaunchOptions.SERVER, server) }
            val scenario = checkNotNull(app)
            waitFor(hasTestTag("map-layers"))
            for (layer in listOf("layer-wind", "layer-sites", "layer-pilots")) {
                tapChip(layer)
            }
            lookAtTheMadeUpDay(scenario)
            tapChip("layer-forecast")
            val strip = hasTestTag("forecast-hours")
            val said = hasTestTag("forecast-said")
            waitFor(strip and saying("Day"))
            assertTrue(screen.onAllNodes(isSelectable() and hasAnyAncestor(hasTestTag("forecast-days"))).fetchSemanticsNodes().size == 4)
            assertTrue(placesOfTheHours() == 12)
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { (TestDevice.drawnOnTheMap(scenario, "forecast-squares") ?: 0) > 0 }
            letTheMapDraw()
            keepScreenshot("narrow-map-hours-1-thermals-day")

            // The wind: the words and, at their right, the height's menu.
            toClock("13:00")
            chooseForecastLayer("Wind")
            waitFor(said and hasText("Wind at the ground, 13:00"))
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { (TestDevice.drawnOnTheMap(scenario, "forecast-hours-arrows") ?: 0) > 0 }
            letTheMapDraw()
            keepScreenshot("narrow-map-hours-2-wind")

            chooseForecastHeight("2000", "2000 m")
            waitFor(said and hasText("Wind at 2000 m, 13:00"))
            // Nothing lies over another or leaves the card: the words end
            // where the menu begins, and both, the days and the hours
            // stand inside the card.
            val card = boundsOf("forecast-map-card")
            assertTrue(boundsOf("forecast-said").right <= boundsOf("forecast-level").left)
            for (tag in listOf("forecast-said", "forecast-level", "forecast-days", "forecast-hours", "forecast-key-and-notes")) {
                val part = boundsOf(tag)
                assertTrue("$tag leaves the card", part.left >= card.left && part.right <= card.right && part.top >= card.top && part.bottom <= card.bottom)
            }
            letTheMapDraw()
            keepScreenshot("narrow-map-hours-3-wind-2000")

            unfoldTheKeyAndNotes()
            waitFor(hasTestTag("forecast-layer-note"))
            letTheMapDraw(1)
            keepScreenshot("narrow-map-hours-4-key-and-notes")
        } finally {
            TestDevice.asItWasBuilt()
        }
    }

    @Test
    fun theForecastMapByTheHour() = lookAtTheForecastByTheHour("light", dark = false)

    @Test
    fun theForecastMapByTheHourInDarkAppearance() = lookAtTheForecastByTheHour("dark", dark = true)

    /**
     * The forecast map in the dark appearance, over the drawn map and over
     * the photographs from above, with tiles from the network as a phone
     * draws them. Only the screenshots matter here.
     */
    @Test
    fun theKindOfDayOnTheMapInDarkAppearance() {
        val server = InstrumentationRegistry.getArguments().getString("madeUpDayServer")
        assumeTrue("No stand-in with a made-up day was named in the madeUpDayServer argument.", !server.isNullOrEmpty())
        launch(bundledMap = false, dark = true) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(hasTestTag("map-layers"))
        assertTrue(TestDevice.lookAt(scenario, lat = 47.7, lon = 11.9, zoom = 8.6))
        tapChip("layer-forecast")
        waitFor(hasTestTag("forecast-days"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "forecast-squares") ?: 0) > 0
        }
        letTheMapDraw(12)
        keepScreenshot("map-forecast-4-dark")

        switchLayer("map-aerial", on = true)
        letTheMapDraw(12)
        keepScreenshot("map-forecast-5-dark-aerial")
        switchLayer("map-aerial", on = false)
    }

    @Test
    fun todaySaysWhatTheForecastSaysOfTheTakeoffs() = lookAtToday("light", dark = false)

    @Test
    fun todayInDarkAppearance() = lookAtToday("dark", dark = true)

    /**
     * The air around and above a take-off, against Venti in development
     * mode: its recorded forecast of late November has, with each place,
     * the strongest wind among the forecast map's squares near by, and on
     * its Sunday and Monday air that is layered in every hour. Today says
     * with a day when the wind near by is much stronger than at a
     * take-off, as a note and not as the status; the take-off's page marks
     * each hour's layers in the picture of the day's air, says an hour's
     * layers and the wind near by in words under it, and at its foot what
     * both are and are not.
     *
     * A take-off's page speaks of its first day with flying hours, and by
     * the development server's own clock that is the recorded Saturday, a
     * stormy day whose air is mixed all the way up. So the test plays a
     * server on the device in front of it that hands everything on but
     * the clock (`GET /v1/health`), which it sets to an evening: the page
     * then speaks of the day after. What is there is asserted by its
     * pattern, never by a figure: the words and the figures are the
     * core's. Skipped without a server.
     */
    private fun lookAtTheAirAroundATakeoff(
        prefix: String,
        dark: Boolean,
        /** The evening the server's clock is set to: the page speaks of the day after. */
        evening: String,
        /** Whether that day is the one whose wind near by is much stronger: then the page has the line too, and an hour of it. */
        theLineIsOnThePage: Boolean,
        /** Whether that day's air has an inversion besides its stable layer. */
        hasAnInversion: Boolean,
    ) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        val clock = """{"status":"ok","version":"0.1.0","time":"$evening","forecasts":"available","registration":"open","sources":[]}"""
        HoursHeldBack(checkNotNull(server), path = "/v1/health", refusal = 200 to clock).use { inFront ->
            inFront.hours = HoursHeldBack.Hours.REFUSED
            launch(dark = dark, onToday = true) { putExtra(LaunchOptions.SERVER, inFront.address) }
            val status = described("Flyable") or described("Marginal") or described("Not flyable")
            waitFor(hasTestTag("today-card") and status)

            // The day's line, on the card or a row of the first day of the
            // switch that has it: the hour, what is much stronger and
            // where, and what the take-off itself has.
            val words = """Much stronger near by at \d{1,2}:00: (gusts to|wind) \d+ km/h .+, \d+ km/h at the place"""
            val line = (hasTestTag("today-card") or hasTestTag("today-row")) and worded(words)
            val days = hasAnyAncestor(hasTestTag("today-days")) and isSelectable()
            fun hasTheLine() = runCatching { waitFor(line, seconds = 10) }.isSuccess
            var found = hasTheLine()
            var day = 0
            while (!found && day < screen.onAllNodes(days).fetchSemanticsNodes().size) {
                screen.onAllNodes(days)[day].performScrollTo().performClick()
                waitFor(hasTestTag("today-card") and status)
                found = hasTheLine()
                day += 1
            }
            assertTrue("No day of the recorded forecast has a much stronger wind near by.", found)
            // The take-off with the line keeps a status of its own beside it.
            assertTrue(exists(line and status))
            screen.onAllNodes(line).onFirst().performScrollTo()
            TestDevice.letTheAppRun(screen, 1)
            keepScreenshot("$prefix-air-around-1-day-with-the-line")
            // Another take-off as a row has it under what limits its day.
            val row = hasTestTag("today-row") and worded(words)
            if (runCatching { waitFor(row, seconds = 10) }.isSuccess) {
                screen.onAllNodes(row).onFirst().performScrollTo()
                TestDevice.letTheAppRun(screen, 1)
                keepScreenshot("$prefix-air-around-1b-row-with-the-line")
            }
            // What the wind near by is and is not, at the foot.
            screen.onNode(hasTestTag("today-nearby-note")).performScrollTo()
            assertTrue(exists(hasTestTag("today-nearby-note") and hasText("changes no verdict", substring = true)))

            // The take-off's own page, which speaks of its first day with
            // flying hours: the line under that day's, where it is that day's.
            screen.onAllNodes(line).onFirst().performScrollTo().performClick()
            val page = hasTestTag("site-page")
            waitFor(page)
            waitFor(hasTestTag("site-outlook"))
            assertEquals(theLineIsOnThePage, exists(hasAnyAncestor(hasTestTag("nearby-line")) and worded(words)))
            TestDevice.letTheAppRun(screen, 1)
            keepScreenshot("$prefix-air-around-2-site")
            // The picture of the day's air, the sheet pulled up to its full height first.
            screen.onNode(page).performTouchInput { swipeUp() }
            TestDevice.letTheAppRun(screen, 1)
            screen.onNode(page).performScrollToNode(hasTestTag("air-chart"))
            waitFor(hasTestTag("air-hour-0"))
            TestDevice.letTheAppRun(screen, 1)
            keepScreenshot("$prefix-air-around-3-picture-with-layers")

            // Hour by hour: a tap says the hour's layers and the strongest
            // wind and gust near by with where they are, and a screen
            // reader hears them on the hour itself.
            val layered = """(Stable layer|Inversion) between \d+ and \d+ m: \d+\.\d °C( warmer)? per 100 m"""
            val whereabouts = """^\d+ km/h (in the square around the place|\d+ km to the [a-z-]+)( over ground at \d+ m)?$"""
            val gusts = hasTestTag("air-hour-figure") and described("Gusts near by") and sayingLike(whereabouts)
            val wind = hasTestTag("air-hour-figure") and described("Wind near by") and sayingLike(whereabouts)
            var withLayers = -1
            var withAnInversion = -1
            var withLine = -1
            var hour = 0
            while (exists(hasTestTag("air-hour-$hour"))) {
                tap(hasTestTag("air-hour-$hour"))
                waitFor(hasTestTag("air-hour-$hour") and isSelected())
                if (exists(hasTestTag("air-hour-layer") and worded("^$layered$")) && exists(gusts) && exists(wind)) {
                    withLayers = hour
                    assertTrue(exists(hasTestTag("air-hour-$hour") and sayingLike("$layered.*\\. Gusts near by: \\d+ km/h")))
                }
                if (exists(hasTestTag("air-hour-layer") and worded("^Inversion between"))) withAnInversion = hour
                if (withLine < 0 && exists(hasAnyAncestor(hasTestTag("air-hour-line")) and worded("^(Gusts to|Wind) \\d+ km/h"))) {
                    withLine = hour
                }
                hour += 1
            }
            assertTrue("No hour of the day has both a layer and the wind near by.", withLayers >= 0)
            assertEquals(hasAnInversion, withAnInversion >= 0)
            assertEquals(theLineIsOnThePage, withLine >= 0)
            // The hour that is much stronger near by, where the day has
            // one; else one with an inversion, else the last with layers.
            val looked = listOf(withLine, withAnInversion, withLayers).first { it >= 0 }
            tap(hasTestTag("air-hour-$looked"))
            waitFor(hasTestTag("air-hour-$looked") and isSelected())
            screen.onNode(page).performScrollToNode(hasTestTag("air-hour"))
            TestDevice.letTheAppRun(screen, 1)
            keepScreenshot("$prefix-air-around-4-hour")

            // What the layers and the wind near by are and are not, at the foot.
            unfoldHowASiteIsRead()
            screen.onNode(page).performScrollToNode(hasTestTag("site-layers-note"))
            assertTrue(exists(hasTestTag("site-layers-note") and hasText("true sounding", substring = true)))
            assertTrue(exists(hasTestTag("site-nearby-note") and hasText("changes no verdict", substring = true)))
            TestDevice.letTheAppRun(screen, 1)
            keepScreenshot("$prefix-air-around-5-notes")
            tap(hasText("Done"))
            waitFor(hasTestTag("today-card"))
        }
    }

    /** The recorded Monday: gusts near by that are much stronger than at a take-off, and stable air in every hour. */
    @Test
    fun aTakeoffSaysTheAirAroundAndAboveIt() = lookAtTheAirAroundATakeoff(
        "light",
        dark = false,
        evening = "2026-11-22T16:00:00Z",
        theLineIsOnThePage = true,
        hasAnInversion = false,
    )

    /** The recorded Sunday: an inversion at the ground under stable air, and nothing much stronger near by. */
    @Test
    fun theAirAroundATakeoffInDarkAppearance() = lookAtTheAirAroundATakeoff(
        "dark",
        dark = true,
        evening = "2026-11-21T16:00:00Z",
        theLineIsOnThePage = false,
        hasAnInversion = true,
    )

    /**
     * A take-off of the pilot's own, against Venti in development mode:
     * kept on its page, where its source's fee and web page stand too; then
     * first on Today under "Your take-offs", with the others under "Within
     * 30 km"; listed in the settings with the core's note, and forgotten
     * there, after which Today is as it was. Another take-off has the
     * sentence about rules for guests, and a ramp on the Wallberg, opened
     * on the map, its mapper's description. Skipped without a server.
     */
    private fun keepATakeoff(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark, onToday = true) { putExtra(LaunchOptions.SERVER, server) }
        val takeoff = "Fluggelände Jägerkamp/Nagelspitz"
        val page = hasTestTag("site-page")
        fun top(what: SemanticsMatcher) = screen.onAllNodes(what).onFirst().fetchSemanticsNode().boundsInRoot.top

        // None kept: Today names no groups.
        waitFor(hasTestTag("today-card"))
        waitFor(hasText(takeoff) and hasClickAction())
        assertFalse(exists(hasTestTag("today-kept-title")))
        assertFalse(exists(hasTestTag("today-near-title")))

        // The take-off's page: the switch, and what its source says beyond
        // the name and the wind. The page is a lazy list in a sheet at half
        // height: pulled up, then scrolled to each row.
        screen.onAllNodes(hasText(takeoff) and hasClickAction()).onFirst().performScrollTo().performClick()
        waitFor(page)
        screen.onNode(page).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(page).performScrollToNode(hasTestTag("site-own"))
        waitFor(hasTestTag("site-own") and hasText("Keep as my take-off"))
        tap(hasTestTag("site-own"))
        waitFor(hasTestTag("site-own") and hasText("Kept"))
        screen.onNode(page).performScrollToNode(hasTestTag("site-more"))
        waitFor(hasText("A fee is asked for using this site."))
        screen.onNode(page).performScrollToNode(hasTestTag("site-website"))
        assertTrue(exists(hasTestTag("site-website") and hasText("https://www.dhv.de/")))
        screen.onNode(page).performScrollToNode(hasText("See or correct it at the source"))
        unfoldHowASiteIsRead()
        assertTrue(exists(hasText("© OpenStreetMap contributors", substring = true)))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-own-1-site")
        tap(hasText("Done"))

        // Today lists it first, under the core's heading, and the others
        // under theirs.
        waitFor(hasTestTag("today-kept-title") and hasText("Your take-offs"))
        waitFor(hasTestTag("today-near-title") and hasText("Within 30 km"))
        val own = top(hasText(takeoff) and hasClickAction())
        assertTrue(top(hasTestTag("today-kept-title")) < own)
        assertTrue(own < top(hasTestTag("today-near-title")))
        assertTrue(exists(hasTestTag("today-row")))
        screen.onNode(hasTestTag("today-head")).performScrollTo()
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-own-2-today")
        screen.onNode(hasTestTag("today-foot")).performScrollTo()
        assertTrue(exists(hasText("© OpenStreetMap contributors", substring = true)))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-own-2b-today-foot")

        // Another take-off of Today's, whose source names rules for guests:
        // the core's sentence about them.
        screen.onAllNodes(hasText("Vogelsang Ost") and hasClickAction()).onFirst().performScrollTo().performClick()
        waitFor(page)
        screen.onNode(page).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(page).performScrollToNode(hasTestTag("site-more"))
        waitFor(hasText("Guests have to read this site's rules for guests before they fly here."))
        tap(hasText("Done"))
        waitFor(hasTestTag("today-card"))

        // A ramp on the Wallberg, which has no name and is not among
        // Today's six: opened by its sign on the map, it has its mapper's
        // words, as they were written. Close enough that the development
        // server's made-up paraglider, some 160 m away and drawn over the
        // sites, is not under the finger too.
        val scenario = checkNotNull(app)
        tap(tab("Map"))
        waitFor(described("Show my position"))
        assertTrue(TestDevice.lookAt(scenario, lat = 47.6653, lon = 11.7954, zoom = 15.0))
        waitFor(hasTestTag("sites-filter-open"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) > 0
        }
        letTheMapDraw()
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.6652212, lon = 11.7954245)))
        waitFor(page)
        screen.onNode(page).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(page).performScrollToNode(hasTestTag("site-description"))
        waitFor(hasTestTag("site-description") and hasText("Startrampe (vermutlich für Drachenflieger)"))
        screen.onNode(page).performScrollToNode(hasText("See or correct it at the source"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-own-3-description")
        tap(hasText("Done"))
        waitFor(hasTestTag("sites-filter-open"))

        // All of the pilot's own, in the settings, with where they are kept.
        // Today comes back where it was scrolled to: the corner that opens
        // the settings is at its head.
        tap(tab("Today"))
        waitFor(hasTestTag("today-card"))
        screen.onNode(hasTestTag("today-head")).performScrollTo()
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        screen.onNode(hasTestTag("kept-sites-link")).performScrollTo().performClick()
        waitFor(hasTestTag("kept-site") and hasAnyDescendant(described(takeoff)))
        assertTrue(exists(hasText("Kept on this device only", substring = true)))
        assertTrue(exists(hasText("© OpenStreetMap contributors", substring = true)))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-own-4-list")

        // Forgotten there, after the app has asked.
        tap(hasTestTag("kept-forget"))
        waitFor(hasTestTag("kept-forget-confirm"))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-own-5-forget")
        tap(hasTestTag("kept-forget-confirm"))
        waitFor(hasText("No take-offs of your own yet"))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("$prefix-own-6-none")

        // And Today is as it was.
        tap(described("Back"))
        waitFor(hasText("Vario sound"))
        tap(tab("Today"))
        waitFor(hasTestTag("today-card"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("today-kept-title")) }
        assertFalse(exists(hasTestTag("today-near-title")))
    }

    @Test
    fun thePilotKeepsATakeoffOfTheirOwn() = keepATakeoff("light", dark = false)

    @Test
    fun thePilotsOwnTakeoffInDarkAppearance() = keepATakeoff("dark", dark = true)

    /**
     * Today when Venti cannot be reached: an address nothing listens on, as
     * a phone without a network or a server that is down would be. The app
     * opens on it and says so, with "Try again". The settings are a page of
     * the Today tab and keep the tab bar, and their place while the pilot
     * looks at another tab.
     */
    @Test
    fun todaySaysWhenVentiCannotBeReached() {
        launch(onToday = true) { putExtra(LaunchOptions.SERVER, "http://10.0.2.2:9") }
        waitFor(hasText("The forecast could not be read"))
        assertTrue(exists(hasText("Venti could not be reached", substring = true)))
        assertTrue(exists(hasText("Try again")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-0-not-reached")

        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        tap(tab("Fly"))
        waitFor(hasText("Open the cockpit"))
        tap(tab("Today"))
        waitFor(hasText("Vario sound"))
        // Tapped again, the tab goes back to Today itself.
        tap(tab("Today"))
        waitFor(hasText("The forecast could not be read"))
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

    /**
     * What a glide reaches from the Wallberg's top, over the ground of the
     * region that comes with the app: a long press there, the entry over
     * the forecast, the area on the map and the core's words on the card
     * under it; "Clear" takes it away. Skipped without a server, which the
     * long press needs for the forecast it opens.
     */
    private fun lookAtWhatAGlideReaches(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))
        assertTrue(TestDevice.lookAt(scenario, lat = 47.6642, lon = 11.7967, zoom = 11.0))
        letTheMapDraw(2)
        TestDevice.longPress(checkNotNull(TestDevice.onScreen(scenario, lat = 47.6642, lon = 11.7967)))
        waitFor(hasTestTag("glide-reach-here"))
        tap(hasTestTag("glide-reach-here"))
        waitFor(hasTestTag("glide-reach-words") and hasText("From ", substring = true))
        assertTrue(exists(hasTestTag("glide-reach-note")))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "glide-reach-area") ?: 0) > 0
        }
        letTheMapDraw(2)
        keepScreenshot("$prefix-glide-reach")
        tap(hasTestTag("glide-reach-clear"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("glide-reach-card")) }
    }

    @Test
    fun whatAGlideReachesFromAPlaceOnTheMap() = lookAtWhatAGlideReaches("light", dark = false)

    @Test
    fun whatAGlideReachesInDarkAppearance() = lookAtWhatAGlideReaches("dark", dark = true)

    /**
     * Choosing among the sites: the filter beside the list's button, a
     * wind chosen, what the core says it leaves, the take-offs whose source
     * names no wind dimmed on the map and apart in the list, and a site
     * mapped as an area drawn as one. The development server's sites
     * between the Tegernsee and the Achensee: a take-off on the
     * Vorderunnutz that works in a south-west wind, three east of the
     * Tegernsee that name none. In the dark appearance only the map is
     * looked at. Skipped without a server.
     */
    private fun chooseAmongTheSites(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))
        // The wind's arrows and the gliders off, so that the signs are seen.
        tapChip("layer-wind")
        tapChip("layer-pilots")
        assertTrue(TestDevice.lookAt(scenario, lat = 47.59, lon = 11.845, zoom = 10.0))
        waitFor(hasTestTag("sites-filter-open"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) > 0
        }
        // No filter yet: no line about one on the map.
        assertFalse(exists(hasTestTag("sites-filter-card")))
        val before = TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0

        // The filter, in the core's words: three groups of choices and its note.
        tap(hasTestTag("sites-filter-open"))
        waitFor(hasTestTag("sites-filter-note"))
        for (words in listOf("Show", "All sites", "Take-offs", "Landings", "Take-offs for a wind from", "Take-offs from", "1000 m", "1500 m", "2000 m")) {
            assertTrue(words, exists(hasText(words) and hasAnyAncestor(hasTestTag("sites-filter"))))
        }
        assertTrue(exists(hasTestTag("sites-wind-any") and isSelected()))
        assertTrue(exists(hasTestTag("sites-filter-note") and hasText("not permission to fly there", substring = true)))
        // The forecast's wind is not at hand on the map and is not offered.
        assertFalse(exists(hasText("Forecast now")))
        TestDevice.say("sites-measured-now", exists(hasTestTag("sites-wind-measured-now")).toString())
        if (!dark) {
            letTheMapDraw(1)
            keepScreenshot("$prefix-sites-filter-1-control")
        }

        // A wind from the south-west: what it leaves, in three lines.
        tapChip("sites-wind-SW")
        waitFor(hasTestTag("sites-wind-SW") and isSelected())
        val inTheSheet = hasAnyAncestor(hasTestTag("sites-filter"))
        waitFor(hasTestTag("sites-filter-words") and inTheSheet and hasText("All sites, take-offs for a wind from SW"))
        val fits = Regex("[1-9]\\d* of \\d+ take-offs fits? a wind from SW")
        val apart = Regex("\\d+ take-offs give no wind directions")
        waitFor(hasTestTag("sites-filter-leaves") and inTheSheet and written(fits))
        waitFor(hasTestTag("sites-filter-unsaid") and inTheSheet and written(apart))
        TestDevice.say("sites-filter-leaves", textOf(hasTestTag("sites-filter-leaves") and inTheSheet))
        TestDevice.say("sites-filter-unsaid", textOf(hasTestTag("sites-filter-unsaid") and inTheSheet))
        if (!dark) {
            letTheMapDraw(1)
            keepScreenshot("$prefix-sites-filter-2-wind-chosen")
            // Under "Landings" neither the wind nor the height is asked or offered.
            tap(hasText("Landings") and inTheSheet)
            waitFor(hasTestTag("sites-filter-words") and inTheSheet and hasText("Landings"))
            waitFor(hasTestTag("sites-filter-leaves") and inTheSheet and written(Regex("\\d+ landings")))
            assertFalse(exists(hasTestTag("sites-filter-winds")))
            assertFalse(exists(hasTestTag("sites-filter-heights")))
            assertFalse(exists(hasTestTag("sites-filter-unsaid") and inTheSheet))
            // Back to all sites: the wind is as it was chosen.
            tap(hasText("All sites") and inTheSheet)
            waitFor(hasTestTag("sites-wind-SW") and isSelected())
        }
        tap(hasTestTag("sites-filter-done"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("sites-filter-note")) }

        // The map: the line of the filter, the button as a chip that is
        // on, the take-offs that fit as ever, those that do not say
        // dimmed, and fewer signs than before.
        waitFor(hasTestTag("sites-filter-card"))
        // The card's lines are one element: the filter, what it leaves, what is shown apart.
        val lines = hasTestTag("sites-filter-lines")
        assertTrue(exists(lines and hasText("All sites, take-offs for a wind from SW")))
        assertTrue(exists(lines and written(fits)))
        assertTrue(exists(lines and written(apart)))
        assertTrue(exists(hasTestTag("sites-filter-open") and isSelected()))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: before) < before
        }
        letTheMapDraw()
        keepScreenshot("$prefix-sites-filter-3-map")
        if (dark) return

        // The list: the filter where its title stands, what it leaves, the
        // sites that fit first, and those whose source does not say apart
        // under the core's heading and sentence.
        tapInLayers(described("Flying sites"))
        waitFor(hasTestTag("site-list-title") and hasText("All sites, take-offs for a wind from SW"))
        assertTrue(exists(hasTestTag("site-list-leaves") and written(fits)))
        val list = screen.onNode(hasTestTag("site-list"))
        list.performScrollToNode(hasTestTag("site-row") and described("Startmöglichkeit Vorderunnutz"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-filter-4-list")
        list.performScrollToNode(hasTestTag("site-list-unsaid"))
        assertTrue(exists(written(apart) and hasAnyAncestor(hasTestTag("site-list-unsaid"))))
        assertTrue(exists(hasText("They are shown apart, not left out.", substring = true)))
        list.performScrollToNode(hasTestTag("site-row-unsaid") and described("Fluggelände Jägerkamp/Nagelspitz"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-sites-filter-5-list-shown-apart")
        // A take-off that faces another wind is in neither part.
        assertTrue(runCatching { list.performScrollToNode(described("Startmöglichkeit Heuberg")) }.isFailure)
        tap(hasText("Done"))
        waitFor(hasTestTag("sites-filter-card"))

        // A landing field as a field: its outline under its sign, close up.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.6924, lon = 11.8865, zoom = 14.5))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites-areas") ?: 0) > 0
        }
        letTheMapDraw()
        keepScreenshot("$prefix-sites-filter-6-landing-field")
        // And a take-off mapped as an area, one of those shown apart: dimmed, its outline too.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.6791, lon = 11.9051, zoom = 14.5))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites-areas") ?: 0) > 0
        }
        letTheMapDraw()
        keepScreenshot("$prefix-sites-filter-7-takeoff-area-shown-apart")
    }

    /** Something whose text is all of what this pattern describes. */
    private fun written(pattern: Regex) = SemanticsMatcher("is written as ${pattern.pattern}") { node ->
        node.config.getOrNull(SemanticsProperties.Text)?.any { pattern.matches(it.text) } == true
    }

    /** The first text of the first thing that matches, for the test's log. */
    private fun textOf(what: SemanticsMatcher): String =
        screen.onAllNodes(what).fetchSemanticsNodes().firstOrNull()
            ?.config?.getOrNull(SemanticsProperties.Text)?.firstOrNull()?.text.orEmpty()

    @Test
    fun thePilotChoosesAmongTheSites() = chooseAmongTheSites("light", dark = false)

    @Test
    fun choosingAmongTheSitesInDarkAppearance() = chooseAmongTheSites("dark", dark = true)

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
    /**
     * A station's page says what its readings tell of the air, and shows a
     * past day. The development server's clock stands after dark, when
     * nothing rises from a station: no cloud base then; the temperature
     * step is there, from Innsbruck in the valley to the Patscherkofel.
     */
    /** Unfolds the row at the end of a site's page: how it is read, whose data it is, the licence. */
    private fun unfoldHowASiteIsRead() {
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-how-read"))
        tap(hasTestTag("site-how-read"))
        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-footnote"))
    }

    private fun lookAtAStationsAirAndDays(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        waitFor(hasTestTag("layer-wind"))
        tapInLayers(described("Wind stations"))
        waitFor(described("Innsbruck/Universitaet"))
        tap(described("Innsbruck/Universitaet"))
        waitFor(hasTestTag("station-page"))
        val page = screen.onNode(hasTestTag("station-page"))
        // The air's card stands behind a row at the page's end.
        page.performScrollToNode(hasTestTag("station-more-air"))
        tap(hasTestTag("station-more-air"))
        page.performScrollToNode(hasTestTag("station-air"))
        waitFor(hasText("Patscherkofel, 8.2 km away and 1673 m higher", substring = true))
        assertTrue(exists(hasText("Cooler above, as in air at rest")))
        assertFalse(exists(hasTestTag("station-cloud-base")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-station-1-air")
        tap(hasText("Done"))
        waitFor(hasTestTag("layer-wind"))

        // Another day: the picker opens on today, the server's, and offers
        // a year back to it; the days after are not offered. Choosing a day
        // in it is not done here: a touch or a click action on a day of
        // Material's picker was not delivered in a UI test on the emulator.
        // What a chosen day shows is tested against the core in
        // StationDayWordsTest.
        tapInLayers(described("Wind stations"))
        waitFor(described("Zugspitze"))
        tap(described("Zugspitze"))
        waitFor(hasTestTag("station-page"))
        page.performScrollToNode(hasTestTag("station-day"))
        assertTrue(exists(hasTestTag("station-day") and hasText("2026-11-18", substring = true)))
        // What the days are, behind the row at the page's end.
        page.performScrollToNode(hasTestTag("station-more-air"))
        tap(hasTestTag("station-more-air"))
        waitFor(hasTestTag("station-days-note") and hasText("last year", substring = true))
        page.performScrollToNode(hasTestTag("station-day"))
        tap(hasTestTag("station-day"))
        waitFor(hasTestTag("station-day-ok"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-station-2-day-picker")
        tap(hasText("Cancel"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("station-day-ok")) }
        tap(hasText("Done"))
    }

    @Test
    fun aStationSaysWhatItsReadingsTellOfTheAir() = lookAtAStationsAirAndDays("light", dark = false)

    @Test
    fun aStationsAirInDarkAppearance() = lookAtAStationsAirAndDays("dark", dark = true)

    /**
     * A tap on the map where airspace is drawn and nothing else of Venti's
     * lies lists what lies over the place, with the filter by height at
     * the foot of the sheet. Without a server: no station, site or glider
     * under the finger; the made-up zone of the pilot's sample file.
     */
    private fun lookAtTheAirspaceOverAPlace(prefix: String, dark: Boolean) {
        launch(dark = dark) { putExtra(LaunchOptions.SAMPLE_AIRSPACE, true) }
        waitFor(described("Show my position"))
        val scenario = checkNotNull(app)
        assertTrue(TestDevice.lookAt(scenario, lat = 47.686, lon = 11.728, zoom = 11.5))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { airspaceOnTheMap(scenario) > 0 }
        letTheMapDraw(2)
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.686, lon = 11.728)))
        val sheet = hasTestTag("airspace-over")
        waitFor(sheet)
        waitForAll(
            hasTestTag("airspace-over-words"),
            hasTestTag("airspace-over-row") and hasAnyDescendant(hasText("SAMPLE ZONE (NOT REAL)")),
        )
        screen.onNode(hasTestTag("airspace-over-caveat")).performScrollTo()
        assertTrue(exists(hasText("not certified", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("$prefix-airspace-over-1-place")

        // The filter by height: the core's steps, the choice kept for the maps.
        val steps = isSelectable() and hasAnyAncestor(hasTestTag("airspace-height-filter"))
        // Pulled up to its full height, the sheet shows the filter at its foot.
        screen.onNode(sheet).performTouchInput { swipeUp() }
        screen.onNode(hasTestTag("airspace-height-filter")).performScrollTo()
        assertTrue(screen.onAllNodes(steps).fetchSemanticsNodes().size == 4)
        waitFor(steps and hasText("All") and isSelected())
        tap(steps and hasText("2500 m"))
        waitFor(steps and hasText("2500 m") and isSelected())
        letTheMapDraw(1)
        keepScreenshot("$prefix-airspace-over-2-filter")
        // Back to all, as the test found it.
        tap(steps and hasText("All"))
        waitFor(steps and hasText("All") and isSelected())
        tap(hasText("Done") and hasAnyAncestor(sheet))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(sheet) }
    }

    @Test
    fun aTapOnAirspaceSaysWhatLiesOverThePlace() = lookAtTheAirspaceOverAPlace("light", dark = false)

    @Test
    fun theAirspaceOverAPlaceInDarkAppearance() = lookAtTheAirspaceOverAPlace("dark", dark = true)

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

    /**
     * "How you fly" with something to say. The core observes once the
     * logbook holds ten thermals; the sample flight has two, so five
     * copies of it on other days of July 2026 are brought in together, as
     * files shared to the app. A flight the logbook already has is
     * recognised, so the copies differ in their date.
     */
    private fun seeHowYouFly(prefix: String, dark: Boolean) {
        val sample = context.assets.open(AppFiles.SAMPLE_FLIGHT).use { it.readBytes() }.decodeToString()
        check("HFDTEDATE:040726" in sample) { "the sample flight is no longer of 4 July 2026" }
        val folder = File(context.cacheDir, "tendencies-${System.nanoTime()}").apply { mkdirs() }
        val files = listOf(1, 2, 3, 5, 6).map { day ->
            File(folder, "2026-07-%02d.igc".format(day)).apply {
                writeText(sample.replace("HFDTEDATE:040726", "HFDTEDATE:%02d0726".format(day)))
            }
        }
        launch(dark = dark) {
            action = Intent.ACTION_SEND_MULTIPLE
            type = "application/octet-stream"
            putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList(files.map(Uri::fromFile)))
        }
        waitFor(hasText("5 flights added"))
        tap(hasText("OK") and hasClickAction())
        waitFor(hasTestTag("logbook-row"))
        tap(hasTestTag("logbook-totals"))
        waitFor(hasTestTag("statistics"))
        screen.onNode(hasTestTag("statistics")).performScrollToNode(hasTestTag("statistics-how-you-fly"))
        tap(hasTestTag("statistics-how-you-fly"))
        // At least one observation, and what they are and are not under them.
        waitFor(hasTestTag("tendency"))
        assertFalse(exists(hasTestTag("tendencies-nothing")))
        screen.onNode(hasTestTag("tendencies")).performScrollToNode(hasTestTag("tendencies-note"))
        waitFor(hasTestTag("tendencies-note"))
        screen.onNode(hasTestTag("tendencies")).performScrollToIndex(0)
        letTheMapDraw(1)
        keepScreenshot("$prefix-6f-how-you-fly")
        folder.deleteRecursively()
    }

    @Test
    fun howYouFlySaysWhatTheTracksShow() = seeHowYouFly("light", dark = false)

    @Test
    fun howYouFlyInDarkAppearance() = seeHowYouFly("dark", dark = true)

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
     * The weather along a drawn route: three places over the Tegernsee,
     * the development mode's forecast region, inside the ground that comes
     * with the app. Opened, the section has the day, the start, the speed
     * and the glide, the core's line for the route and, at the first
     * place, the height to leave it at. Skipped without a server.
     */
    private fun seeTheWeatherAlongARoute(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
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
        // The places stand in the upper half of the map, above the card.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.66, lon = 11.78, zoom = 12.0))
        letTheMapDraw(2)
        // The Wallberg, Rottach-Egern on the lake, and the valley east of it by Ellmau.
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.6642, lon = 11.7967)))
        waitFor(words and hasText("Tap the next place to fly to."))
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.6960, lon = 11.7700)))
        waitFor(hasTestTag("route-weather-toggle"))
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.685, lon = 11.815)))
        waitFor(described("Legs") and saying("2  "))

        tap(hasTestTag("route-weather-toggle"))
        waitFor(hasTestTag("route-weather-toggle") and saying("Expanded"))
        waitFor(hasTestTag("route-weather-words") and hasText("The route takes", substring = true))
        // The forecast came: its model and credit stand under the places.
        waitFor(hasTestTag("route-weather-credits"))
        assertFalse(exists(hasTestTag("route-weather-problem")))
        waitFor(hasTestTag("route-weather-departure"))
        val departure = screen.onAllNodes(hasTestTag("route-weather-departure")).fetchSemanticsNodes().first()
            .config[SemanticsProperties.Text].joinToString()
        assertTrue(departure, departure.startsWith("Leave Point 1 at "))
        assertTrue(exists(hasTestTag("route-weather-leg") and hasText("The first leg", substring = true)))
        assertTrue(exists(described("More: Speed over the ground")))
        screen.onNode(hasTestTag("route-card")).performScrollToNode(hasTestTag("route-weather-words"))
        letTheMapDraw(2)
        keepScreenshot("$prefix-route-weather-1-open")
        screen.onNode(hasTestTag("route-card")).performScrollToNode(hasTestTag("route-weather-credits"))
        letTheMapDraw(1)
        keepScreenshot("$prefix-route-weather-2-places")

        // A faster pilot is there sooner: the core reads the route again, nothing is asked.
        tap(described("More: Speed over the ground"))
        waitFor(hasTestTag("route-weather-words") and hasText("at 25 km/h", substring = true))
    }

    @Test
    fun theWeatherAlongADrawnRoute() = seeTheWeatherAlongARoute("light", dark = false)

    @Test
    fun theWeatherAlongARouteInDarkAppearance() = seeTheWeatherAlongARoute("dark", dark = true)

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
        switchLayer("map-aerial", on = true)
        // Whose photographs they are, by name, while they show: those in
        // view. The map opens zoomed out over the Alps, where only the
        // world's mosaic is drawn.
        seeInCredits(hasText("Photos: EOX"))
        letTheMapDraw(12)
        keepScreenshot("online-1b-map-aerial")

        tap(tab("Logbook"))
        waitFor(hasText("No flights yet"))
        tap(hasText("Add a sample flight"))
        waitFor(described("Flight details"))
        // One choice for every map: the flight's map shows the photographs too,
        // close over the Tegernsee, with Bavaria's among them.
        val aerial = hasTestTag("map-aerial")
        waitFor(aerial and isSelected())
        waitFor(hasTestTag("photos-credit"))
        waitFor(hasText("Bayerische Vermessungsverwaltung", substring = true))
        letTheMapDraw(12)
        keepScreenshot("online-2b-flight-aerial")
        // Off again, as the test found it.
        tap(aerial)
        waitFor(aerial and isSelected().not())
        letTheMapDraw(12)
        keepScreenshot("online-2-flight")
    }

    /**
     * The map over the development server's made-up sites of Venti's own
     * layers, near Garmisch, with the wind and the gliders off so that the
     * sites are what is seen. The region that comes with the app ends
     * before it: the ground is empty there, and the sites are not. Two of
     * them are a partner's and on every map; the third is a place from
     * pilots, which is there only once the pilot has switched those on.
     */
    private fun lookAtTheMadeUpSites(scenario: ActivityScenario<MainActivity>, atLeast: Int = 2) {
        waitFor(described("Show my position"))
        assertTrue(TestDevice.lookAt(scenario, lat = 47.285, lon = 10.9, zoom = 12.0))
        waitFor(hasTestTag("sites-filter-open"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) >= atLeast
        }
        letTheMapDraw(2)
    }

    /** Switches the places from pilots on in the sites' filter, and waits for them on the map. */
    private fun showThePlacesFromPilots(scenario: ActivityScenario<MainActivity>) {
        tap(hasTestTag("sites-filter-open"))
        waitFor(hasTestTag("sites-filter"))
        screen.onNode(hasTestTag("sites-from-pilots")).performScrollTo().performClick()
        waitFor(hasTestTag("sites-from-pilots") and isOn())
        waitFor(hasTestTag("sites-filter-from-pilots"))
        tap(hasTestTag("sites-filter-done"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) >= 3
        }
        letTheMapDraw(1)
    }

    /** Opens a site's page from the list of the sites on the map, which opens at full height. */
    private fun openFromTheListOfSites(name: String) {
        if (!exists(hasTestTag("site-list"))) {
            tapInLayers(described("Flying sites"))
            waitFor(hasTestTag("site-list"))
        }
        val row = hasTestTag("site-row") and described(name)
        screen.onNode(hasTestTag("site-list")).performScrollToNode(row)
        tap(row)
        waitFor(hasTestTag("site-page"))
    }

    /** Signed out, a long press where no site lies offers the way to the account as the core's sentence: taken, and an account made. */
    private fun signInFromTheMap(scenario: ActivityScenario<MainActivity>, handle: String) {
        TestDevice.longPress(checkNotNull(TestDevice.onScreen(scenario, lat = 47.295, lon = 10.875)))
        waitFor(hasTestTag("site-add-sign-in") and hasText("Sign in to add a place pilots take off from."))
        assertFalse(exists(hasTestTag("site-add-here")))
        tap(hasTestTag("site-add-sign-in"))
        makeAnAccountAndGoBackToTheMap(handle)
        lookAtTheMadeUpSites(scenario)
    }

    /** Makes a new account from the form that is open, and goes back to the map. */
    /**
     * What public flights say about where pilots take off: the section of
     * the made-up take-off "Musterkogel West" with the core's line and its
     * months; the layer "Take-offs without a site" switched on in the
     * sites' filter, with the place near 47.2612 N 10.9312 E as a ring; its
     * sheet, and the core's offer to add it to a pilot who took off there,
     * which a copy of the sample flight moved to that place makes this
     * test's pilot; and the section once in dark appearance. Against the
     * development server's made-up flights. Skipped without a server.
     */
    @Test
    fun whatPublicFlightsSayOfATakeoffAndOfThePlacesNoSiteNames() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        // A flight of this pilot's that began at the place no site names.
        val folder = File(context.cacheDir, "candidate-${System.nanoTime()}").apply { mkdirs() }
        val flight = File(folder, "candidate.igc").apply { writeText(movedTo(CANDIDATE_LAT, CANDIDATE_LON)) }
        launch {
            putExtra(LaunchOptions.SERVER, server)
            action = Intent.ACTION_SEND_MULTIPLE
            type = "application/octet-stream"
            putParcelableArrayListExtra(Intent.EXTRA_STREAM, arrayListOf(Uri.fromFile(flight)))
        }
        // One flight opens by itself; behind it, the logbook.
        waitFor(described("Flight details"))
        tap(described("Back"))
        waitFor(hasTestTag("logbook-row"))
        tap(tab("Map"))
        val scenario = checkNotNull(app)
        val page = hasTestTag("site-page")
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(scenario)
        signInFromTheMap(scenario, "c${System.currentTimeMillis() % 1_000_000_000}")

        // The take-off's page: under the confirmations, the core's title, its line and the months.
        openFromTheListOfSites("Musterkogel West")
        screen.onNode(page).performScrollToNode(hasTestTag("site-flights"))
        waitFor(hasText("Public flights from here"))
        waitFor(hasTestTag("site-flights-words") and hasText("began here", substring = true))
        screen.onNode(page).performScrollToNode(hasTestTag("site-flights-months"))
        assertTrue(exists(hasText("When it is flown")))
        assertTrue(exists(hasContentDescription("July:", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("takeoffs-1-site")
        screen.onNode(page).performScrollToNode(hasTestTag("site-flights-terms"))
        letTheMapDraw(1)
        keepScreenshot("takeoffs-2-site-best")
        tap(hasText("Done"))
        waitFor(described("Show my position"))

        // The layer's switch beside the places from pilots, off until switched on.
        tap(hasTestTag("sites-filter-open"))
        waitFor(hasTestTag("sites-filter"))
        val layer = hasTestTag("sites-candidates")
        screen.onNode(layer).performScrollTo()
        assertTrue(exists(layer and isOff() and hasText("Take-offs without a site")))
        tap(layer)
        waitFor(layer and isOn())
        tap(hasTestTag("sites-filter-done"))

        // The place as a ring, where the part of the map is small enough to ask about.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.26, lon = 10.935, zoom = 13.0))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "site-candidates") ?: 0) >= 1
        }
        letTheMapDraw(2)
        keepScreenshot("takeoffs-3-layer")

        // Its sheet: the core's title, line and height; and the offer, since this pilot took off there.
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = CANDIDATE_LAT, lon = CANDIDATE_LON)))
        waitFor(hasTestTag("candidate-sheet"))
        assertTrue(exists(hasText("Pilots take off here")))
        assertTrue(exists(hasTestTag("candidate-words") and hasText("No site names this place.", substring = true)))
        waitFor(hasTestTag("candidate-offer-words") and hasText("You took off here on", substring = true))
        assertTrue(exists(hasTestTag("candidate-add") and hasText("Add this place")))
        assertTrue(exists(hasTestTag("candidate-set-aside") and hasText("Not this one")))
        letTheMapDraw(1)
        keepScreenshot("takeoffs-4-candidate")
        // "Add this place" opens the add form on the core's draft.
        tap(hasTestTag("candidate-add"))
        waitFor(hasText("A place pilots take off from"))
        letTheMapDraw(1)
        keepScreenshot("takeoffs-5-form")
        folder.deleteRecursively()

        // The section once in dark appearance, from a new start of the app.
        scenario.close()
        launch(dark = true) { putExtra(LaunchOptions.SERVER, server) }
        val dark = checkNotNull(app)
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(dark)
        openFromTheListOfSites("Musterkogel West")
        screen.onNode(page).performScrollToNode(hasTestTag("site-flights-months"))
        waitFor(hasTestTag("site-flights-words") and hasText("began here", substring = true))
        letTheMapDraw(1)
        keepScreenshot("dark-takeoffs-1-site")
    }

    /** The development server's made-up place where pilots take off and no site stands. */
    private val CANDIDATE_LAT = 47.2612
    private val CANDIDATE_LON = 10.9312

    /** The sample flight with each fix moved by as much as takes its first to a place. */
    private fun movedTo(lat: Double, lon: Double): String {
        val sample = context.assets.open(AppFiles.SAMPLE_FLIGHT).use { it.readBytes() }.decodeToString()
        fun latOf(line: String) = line.substring(7, 9).toInt() + line.substring(9, 14).toInt() / 60_000.0
        fun lonOf(line: String) = line.substring(15, 18).toInt() + line.substring(18, 23).toInt() / 60_000.0
        val first = sample.lineSequence().first { it.startsWith("B") }
        val (byLat, byLon) = lat - latOf(first) to lon - lonOf(first)
        fun written(degrees: Double, width: Int): String {
            val thousandths = Math.round(degrees * 60_000)
            return "%0${width}d%05d".format(thousandths / 60_000, thousandths % 60_000)
        }
        return sample.lines().joinToString("\n") { line ->
            if (!line.startsWith("B") || line.length < 24) {
                line
            } else {
                line.substring(0, 7) + written(latOf(line) + byLat, 2) + "N" + written(lonOf(line) + byLon, 3) + "E" + line.substring(24)
            }
        }
    }

    private fun makeAnAccountAndGoBackToTheMap(handle: String) {
        waitForThePasswordForm()
        tap(hasTestTag("account-new"))
        waitFor(hasTestTag("account-invite"))
        screen.onNode(hasTestTag("account-handle")).performTextInput(handle)
        screen.onNode(hasTestTag("account-name")).performTextInput("Test Pilot (not real)")
        screen.onNode(hasTestTag("account-password")).performTextInput("a password for this test")
        screen.onNode(hasTestTag("account-submit")).performScrollTo()
        tap(hasTestTag("account-submit"))
        waitFor(hasText("@$handle"))
        waitFor(hasText("Sync now"))
        // The account's page has no tab bar: back to the settings, which have it.
        tap(described("Back"))
        waitFor(tab("Map"))
        tap(tab("Map"))
    }

    /** A long press on the map where no site lies, and the entry it offers over the forecast. */
    private fun pressWhereNoSiteLies(scenario: ActivityScenario<MainActivity>, lat: Double, lon: Double) {
        TestDevice.longPress(checkNotNull(TestDevice.onScreen(scenario, lat = lat, lon = lon)))
        waitFor(hasTestTag("site-add-here"))
    }

    /**
     * A pilot adds a place pilots take off from, which is on no map until
     * the places from pilots are switched on; corrects it, looks at its
     * history, undoes the correction and removes it; and is told when a
     * site stands at the place already. Against the development server's
     * made-up sites near Garmisch, with an account the test makes, which
     * is an ordinary pilot's: signed out, the entry is the core's sentence
     * and the way to the account. Everything said about the rule,
     * the tier, the licence and a refusal is the core's. Skipped without
     * a server.
     */
    @Test
    fun aPilotAddsAndCorrectsASite() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        val page = hasTestTag("site-page")
        val form = hasTestTag("site-form")
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(scenario)

        // Signed out: the core's sentence as the entry, which leads to the account.
        val handle = "s${System.currentTimeMillis() % 1_000_000_000}"
        signInFromTheMap(scenario, handle)

        // Signed in, an ordinary pilot: the entry over the forecast of the
        // place pressed, in the core's words for the crowdsourced tier.
        val before = TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0
        pressWhereNoSiteLies(scenario, lat = 47.295, lon = 10.875)
        assertTrue(exists(hasTestTag("site-add-here") and hasText("Add a place pilots take off from")))
        assertFalse(exists(hasTestTag("site-add-sign-in")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-1-entry")
        tap(hasTestTag("site-add-here"))

        // The form, empty: the tier's title, and the core's fields and helps.
        waitFor(form)
        assertTrue(exists(hasTestTag("site-form-title") and hasText("A place pilots take off from")))
        assertTrue(exists(hasText("What pilots call the place.")))
        assertTrue(exists(hasTestTag("site-field-place") and hasText("° N, ", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("site-edit-2-form-empty")

        // Sent with nothing in it: the faults at their fields, in the core's words.
        screen.onNode(hasTestTag("site-send")).performScrollTo().performClick()
        waitFor(hasText("Give the site a name.", substring = true))
        assertTrue(exists(hasTestTag("site-fault-uses")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-3-form-fault")

        screen.onNode(hasTestTag("site-field-name")).performTextInput("Testkogel (not real)")
        screen.onNode(hasTestTag("site-use-takeoff")).performScrollTo().performClick()
        screen.onNode(hasTestTag("site-field-height")).performScrollTo().performTextInput("1500")
        screen.onNode(hasTestTag("site-wind-SW")).performScrollTo().performClick()
        screen.onNode(hasTestTag("site-wind-WSW")).performScrollTo().performClick()
        screen.onNode(hasTestTag("site-wind-W")).performScrollTo().performClick()
        waitFor(hasTestTag("site-wind-words") and hasText("SW to W"))
        assertFalse(exists(hasTestTag("site-fault-uses")))
        assertFalse(exists(hasText("Give the site a name.", substring = true)))

        // What giving a site means, with the licence, over the button.
        screen.onNode(hasTestTag("site-send")).performScrollTo()
        assertTrue(
            exists(
                hasTestTag("site-giving") and
                    hasText("You give this site to everyone under CC0 1.0", substring = true) and
                    hasText("to pilots who switch on the places from pilots", substring = true),
            ),
        )
        assertTrue(exists(hasTestTag("site-giving-note") and hasText("not official and not checked by anyone", substring = true)))
        assertTrue(exists(hasTestTag("site-licence") and hasText("CC0 1.0")))
        assertTrue(exists(hasTestTag("site-send") and hasText("Add as a place from pilots")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-4-giving")
        tap(hasTestTag("site-send"))

        // Its page, with the core's line, which says where it is seen, and
        // marked as from pilots. The layer is not switched on behind the
        // pilot's back: the map has no sign more than before.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(form) }
        waitFor(page)
        waitFor(hasTestTag("site-said") and hasText("Added. Pilots who switch on the places from pilots see it on the map now."))
        assertTrue(exists(hasText("Testkogel (not real)")))
        assertTrue(exists(hasTestTag("site-tier") and hasText("From pilots")))
        letTheMapDraw(3)
        assertEquals(before, TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0)
        keepScreenshot("site-edit-5-new-site")
        screen.onNode(page).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(page).performScrollToNode(hasTestTag("site-by-pilots"))
        assertTrue(exists(hasText("Added by Test Pilot (not real) (@$handle)")))
        assertTrue(exists(hasTestTag("site-by-pilots-note") and hasText("not official and not checked by anyone", substring = true)))
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        waitFor(hasTestTag("site-correct") and hasText("Correct this site"))
        letTheMapDraw(1)
        keepScreenshot("site-edit-5b-site-page")

        // Corrected: another height, and why.
        tap(hasTestTag("site-correct"))
        waitFor(form)
        assertTrue(exists(hasTestTag("site-send") and hasText("Save for everyone")))
        screen.onNode(hasTestTag("site-field-height")).performScrollTo().performTextClearance()
        screen.onNode(hasTestTag("site-field-height")).performTextInput("1520")
        screen.onNode(hasTestTag("site-field-comment")).performScrollTo().performTextInput("measured again")
        screen.onNode(hasTestTag("site-send")).performScrollTo().performClick()
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(form) }
        screen.onNode(page).performScrollToNode(hasTestTag("site-said"))
        waitFor(hasTestTag("site-said") and hasText("Saved for everyone, as revision 2 of this site."))
        screen.onNode(page).performScrollToNode(hasTestTag("site-in-a-line"))
        waitFor(hasTestTag("site-in-a-line") and hasText("1520 m", substring = true))

        // The history: two revisions, the newest first, with the change in words.
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        tap(hasTestTag("site-history-open"))
        waitFor(hasTestTag("site-history"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            screen.onAllNodes(hasTestTag("site-revision-words")).fetchSemanticsNodes().size == 2
        }
        assertTrue(exists(hasTestTag("site-revision-change") and hasText("Height: 1500 m → 1520 m")))
        assertTrue(exists(hasTestTag("site-revision-words") and hasText("measured again", substring = true)))
        assertTrue(exists(hasTestTag("site-undo") and hasText("Undo the last change")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-6-history")

        // Undone: the site stands as it was added, at a third revision.
        tap(hasTestTag("site-undo"))
        waitFor(hasTestTag("site-why"))
        screen.onNode(hasTestTag("site-why")).performTextInput("the first height was right")
        tap(hasTestTag("site-why-send"))
        waitFor(page)
        screen.onNode(page).performScrollToNode(hasTestTag("site-said"))
        waitFor(hasTestTag("site-said") and hasText("revision 3", substring = true))
        screen.onNode(page).performScrollToNode(hasTestTag("site-in-a-line"))
        waitFor(hasTestTag("site-in-a-line") and hasText("1500 m", substring = true))
        tap(hasText("Done"))
        waitFor(described("Show my position"))

        // Seen once the places from pilots are switched on: its sign on the
        // map, and its line in the list, marked.
        showThePlacesFromPilots(scenario)
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) >= before + 2
        }
        letTheMapDraw(2)
        keepScreenshot("site-edit-5c-on-the-map")
        tapInLayers(described("Flying sites"))
        waitFor(hasTestTag("site-list"))
        screen.onNode(hasTestTag("site-list")).performScrollToNode(hasTestTag("site-row") and described("Testkogel (not real)"))
        assertTrue(exists(hasTestTag("site-row") and described("Testkogel (not real)") and saying("From pilots")))
        openFromTheListOfSites("Testkogel (not real)")

        // Removed: the core's question and what it means, its line after,
        // and the history, which brings the site back.
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        waitFor(hasTestTag("site-remove") and hasText("Remove this site"))
        tap(hasTestTag("site-remove"))
        waitFor(hasTestTag("site-why"))
        assertTrue(exists(hasText("Remove this site from the map?")))
        assertTrue(exists(hasTestTag("site-why-about") and hasText("It goes from the map for everyone.", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("site-edit-7-remove")
        tap(hasTestTag("site-why-send"))
        waitFor(hasTestTag("site-said") and hasText("Removed from the map. Its history stays, and any pilot who may correct the site can bring it back."))
        waitFor(hasTestTag("site-history"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            screen.onAllNodes(hasTestTag("site-revision-words")).fetchSemanticsNodes().size == 4
        }
        assertTrue(exists(hasTestTag("site-bring-back") and hasText("Bring this site back")))
        assertTrue(exists(hasTestTag("site-put-back") and hasText("Bring the site back as it stood here")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-8-removed")
        tap(hasText("Done"))
        waitFor(described("Show my position"))

        // On top of a site that stands: moved there on the map, refused in
        // the core's words by the tier of the site in the way, with the
        // offer to open that one. First a partner's, then a place from pilots.
        lookAtTheMadeUpSites(scenario, atLeast = 3)
        pressWhereNoSiteLies(scenario, lat = 47.275, lon = 10.92)
        tap(hasTestTag("site-add-here"))
        waitFor(form)
        screen.onNode(hasTestTag("site-field-name")).performTextInput("Beispieljoch again (not real)")
        screen.onNode(hasTestTag("site-use-takeoff")).performScrollTo().performClick()
        screen.onNode(hasTestTag("site-move")).performScrollTo().performClick()
        waitFor(hasTestTag("site-place-card"))
        assertTrue(exists(hasText("Move the map until the mark is where pilots stand or touch down.")))
        assertTrue(exists(hasTestTag("site-place-use") and hasText("Use this place")))
        // The partner's "Musterkogel West": no place from pilots is put on top of it.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.3, lon = 10.9, zoom = 15.0))
        letTheMapDraw(2)
        tap(hasTestTag("site-place-use"))
        waitFor(form)
        assertTrue(exists(hasTestTag("site-field-place") and hasText("47.3000° N, 10.9000° E")))
        screen.onNode(hasTestTag("site-send")).performScrollTo().performClick()
        waitFor(hasTestTag("site-problem") and hasText("stands at this place already: Musterkogel West.", substring = true))
        screen.onNode(hasTestTag("site-in-the-way")).performScrollTo()
        assertTrue(exists(hasTestTag("site-problem") and hasText("A site that a partner of Venti added", substring = true)))
        assertTrue(exists(hasTestTag("site-problem") and hasText("open it and report it", substring = true)))
        assertTrue(exists(hasTestTag("site-in-the-way") and hasText("Open Musterkogel West")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-10a-on-a-partners-site")

        // Then onto the place from pilots.
        screen.onNode(hasTestTag("site-move")).performScrollTo().performClick()
        waitFor(hasTestTag("site-place-card"))
        assertTrue(TestDevice.lookAt(scenario, lat = 47.29, lon = 10.91, zoom = 15.0))
        letTheMapDraw(2)
        keepScreenshot("site-edit-9-place")
        tap(hasTestTag("site-place-use"))
        waitFor(form)
        // What was entered is still there, at the new place.
        assertTrue(exists(hasTestTag("site-field-name") and hasText("Beispieljoch again (not real)")))
        assertTrue(exists(hasTestTag("site-field-place") and hasText("47.2900° N, 10.9100° E")))
        screen.onNode(hasTestTag("site-send")).performScrollTo().performClick()
        waitFor(hasTestTag("site-problem") and hasText("A place from pilots stands here already: Beispieljoch.", substring = true))
        screen.onNode(hasTestTag("site-in-the-way")).performScrollTo()
        assertTrue(exists(hasTestTag("site-in-the-way") and hasText("Open Beispieljoch")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-10-too-close")
        tap(hasTestTag("site-in-the-way"))
        waitFor(page)
        waitFor(hasText("Beispieljoch"))
        assertFalse(exists(form))

        // Another pilot's place from pilots: this pilot may correct it too,
        // and report it, as the core offers.
        assertTrue(exists(hasTestTag("site-tier") and hasText("From pilots")))
        screen.onNode(page).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        waitFor(hasTestTag("site-report") and hasText("Report this site"))
        assertTrue(exists(hasTestTag("site-correct") and hasText("Correct this site")))

        // Reporting it: the app's report form, with a site's reasons. Nothing is sent.
        tap(hasTestTag("site-report"))
        waitFor(hasTestTag("report-notice"))
        assertTrue(exists(hasText("Report this site")))
        assertTrue(exists(hasText("Wrong place or dangerous")))
        letTheMapDraw(1)
        keepScreenshot("site-edit-11-report")
        tap(hasTestTag("report-close"))
    }

    /**
     * The form for a site on a screen 360 dp wide with the text at 130 %:
     * the fields, the sixteen winds and what giving means over the button.
     * Skipped without a server.
     */
    @Test
    fun theFormForASiteOnANarrowScreenWithLargeText() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        TestDevice.narrowWithLargeText()
        try {
            launch { putExtra(LaunchOptions.SERVER, server) }
            val scenario = checkNotNull(app)
            tapChip("layer-wind")
            tapChip("layer-pilots")
            lookAtTheMadeUpSites(scenario)
            TestDevice.longPress(checkNotNull(TestDevice.onScreen(scenario, lat = 47.295, lon = 10.875)))
            waitFor(hasTestTag("site-add-sign-in"))
            letTheMapDraw(1)
            keepScreenshot("narrow-site-edit-1-entry-signed-out")
            tap(hasTestTag("site-add-sign-in"))
            makeAnAccountAndGoBackToTheMap("n${System.currentTimeMillis() % 1_000_000_000}")
            lookAtTheMadeUpSites(scenario)
            pressWhereNoSiteLies(scenario, lat = 47.295, lon = 10.875)
            tap(hasTestTag("site-add-here"))
            waitFor(hasTestTag("site-form"))
            screen.onNode(hasTestTag("site-send")).performScrollTo().performClick()
            waitFor(hasText("Give the site a name.", substring = true))
            letTheMapDraw(1)
            keepScreenshot("narrow-site-edit-2-form-fault")
            screen.onNode(hasTestTag("site-wind-NNW")).performScrollTo().performClick()
            screen.onNode(hasTestTag("site-field-description")).performScrollTo()
            letTheMapDraw(1)
            keepScreenshot("narrow-site-edit-3-winds")
            screen.onNode(hasTestTag("site-send")).performScrollTo()
            letTheMapDraw(1)
            keepScreenshot("narrow-site-edit-4-giving")
        } finally {
            TestDevice.asItWasBuilt()
        }
    }

    /**
     * The places from pilots on a screen 360 dp wide with the text at
     * 130 %: the filter's sheet with the switch, the page of a place from
     * pilots with the sentence to confirm, the form that confirms and the
     * list of who took off there. Skipped without a server.
     */
    @Test
    fun thePlacesFromPilotsOnANarrowScreenWithLargeText() = thePlacesFromPilotsOnANarrowScreen(dark = false)

    /** The same in the dark appearance, and the form for a new place with a fault. Skipped without a server. */
    @Test
    fun thePlacesFromPilotsOnANarrowScreenInDarkAppearance() = thePlacesFromPilotsOnANarrowScreen(dark = true)

    private fun thePlacesFromPilotsOnANarrowScreen(dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        val name = if (dark) "narrow-dark-sites-pilots" else "narrow-sites-pilots"
        TestDevice.narrowWithLargeText()
        try {
            launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
            val scenario = checkNotNull(app)
            val page = hasTestTag("site-page")
            tapChip("layer-wind")
            tapChip("layer-pilots")
            lookAtTheMadeUpSites(scenario)
            signInFromTheMap(scenario, "w${System.currentTimeMillis() % 1_000_000_000}")

            // The filter's sheet: the switch with the core's sentence, then what it leaves.
            tap(hasTestTag("sites-filter-open"))
            waitFor(hasTestTag("sites-filter"))
            screen.onNode(hasTestTag("sites-from-pilots")).performScrollTo().performClick()
            waitFor(hasTestTag("sites-from-pilots") and isOn())
            waitFor(hasTestTag("sites-filter-from-pilots"))
            letTheMapDraw(1)
            keepScreenshot("$name-1-filter")
            screen.onNode(hasTestTag("sites-filter-note")).performScrollTo()
            letTheMapDraw(1)
            keepScreenshot("$name-1b-filter-foot")
            // "Done" stands at the head, which scrolled away.
            screen.onNode(hasTestTag("sites-filter-done")).performScrollTo().performClick()
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("sites-filter")) }
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
                (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) >= 3
            }
            letTheMapDraw(1)
            keepScreenshot("$name-2-map")

            // The place's page: its tier, the notice once, and the sentence to confirm.
            openFromTheListOfSites("Beispieljoch")
            waitFor(hasTestTag("site-tier") and hasText("From pilots"))
            letTheMapDraw(1)
            keepScreenshot("$name-3-site")
            screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
            waitFor(hasTestTag("site-confirm") and hasText("I took off here and it was good"))
            letTheMapDraw(1)
            keepScreenshot("$name-4-site-confirm-entry")
            screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
            letTheMapDraw(1)
            keepScreenshot("$name-4b-site-entries")

            // The form that confirms.
            screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
            tap(hasTestTag("site-confirm"))
            waitFor(hasTestTag("site-confirm-form"))
            letTheMapDraw(1)
            keepScreenshot("$name-5-confirm")
            screen.onNode(hasTestTag("site-confirm-send")).performScrollTo()
            letTheMapDraw(1)
            keepScreenshot("$name-5b-confirm-foot")
            tap(hasTestTag("site-confirm-cancel"))
            screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("site-confirm-form")) }

            // Who took off there.
            tap(hasTestTag("site-confirmations-open"))
            waitFor(hasTestTag("site-confirmation"))
            letTheMapDraw(1)
            keepScreenshot("$name-6-confirmations")
            tap(hasText("Done"))
            waitFor(described("Show my position"))

            if (dark) {
                // The form for a new place, with a fault, and what giving means over the button.
                // The map followed the place whose page was open: back over the made-up sites.
                lookAtTheMadeUpSites(scenario, atLeast = 3)
                pressWhereNoSiteLies(scenario, lat = 47.295, lon = 10.875)
                letTheMapDraw(1)
                keepScreenshot("$name-7-entry")
                tap(hasTestTag("site-add-here"))
                waitFor(hasTestTag("site-form"))
                screen.onNode(hasTestTag("site-send")).performScrollTo().performClick()
                waitFor(hasText("Give the site a name.", substring = true))
                letTheMapDraw(1)
                keepScreenshot("$name-8-form-fault")
                screen.onNode(hasTestTag("site-send")).performScrollTo()
                letTheMapDraw(1)
                keepScreenshot("$name-9-giving")
            }
        } finally {
            TestDevice.asItWasBuilt()
        }
    }

    /**
     * The list of who took off at a place while it cannot be read: a
     * server in front of the development server ([HoursHeldBack]) first
     * keeps the list waiting, then refuses it, then hands it on, and at
     * last is gone, as a phone without a network finds it. The site's page
     * keeps the count the map's answer came with; the list's page waits,
     * then says why in the core's words with "Try again". On a screen
     * 360 dp wide with the text at 130 %. Skipped without a server.
     */
    @Test
    fun theListOfWhoTookOffSaysWhenItCannotBeRead() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        val path = siteConfirmationsPath("venti-pilots:0000000000000000000000s002")
        val page = hasTestTag("site-page")
        val problem = hasTestTag("site-confirmations-problem")
        TestDevice.narrowWithLargeText()
        try {
            HoursHeldBack(checkNotNull(server), path = path).use { inFront ->
                launch { putExtra(LaunchOptions.SERVER, inFront.address) }
                val scenario = checkNotNull(app)
                tapChip("layer-wind")
                tapChip("layer-pilots")
                lookAtTheMadeUpSites(scenario)
                showThePlacesFromPilots(scenario)

                // Held: the page has the count the map's answer came with, and the list waits.
                openFromTheListOfSites("Beispieljoch")
                screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
                waitFor(hasTestTag("site-confirmations-open") and hasText("1 pilot took off here", substring = true))
                tap(hasTestTag("site-confirmations-open"))
                waitFor(hasTestTag("site-confirmations"))
                assertFalse(exists(problem))
                assertFalse(exists(hasTestTag("site-confirmation")))
                letTheMapDraw(1)
                keepScreenshot("narrow-site-confirmations-1-waiting")

                // Refused: why, in the core's words, and "Try again".
                inFront.hours = HoursHeldBack.Hours.REFUSED
                waitFor(problem)
                assertTrue(exists(hasText("The confirmations could not be read")))
                assertTrue(exists(hasText("Try again")))
                letTheMapDraw(1)
                keepScreenshot("narrow-site-confirmations-2-refused")

                // Handed on: "Try again" reads it.
                inFront.hours = HoursHeldBack.Hours.HANDED_ON
                tap(hasText("Try again"))
                waitFor(hasTestTag("site-confirmation") and hasText("Mara Muster (@mara) on ", substring = true))
                assertFalse(exists(problem))
                tap(described("Back"))
                waitFor(page)
                tap(described("Back"))
                waitFor(hasTestTag("site-list"))
            }
            // The server is gone, as without a network: the page opens with
            // what the map's answer said of the place, and the list says why it is not there.
            openFromTheListOfSites("Beispieljoch")
            screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
            waitFor(hasTestTag("site-confirmations-open") and hasText("1 pilot took off here", substring = true))
            letTheMapDraw(1)
            keepScreenshot("narrow-site-confirmations-3-page-without-a-network")
            tap(hasTestTag("site-confirmations-open"))
            waitFor(problem)
            assertTrue(exists(hasText("Try again")))
            letTheMapDraw(1)
            keepScreenshot("narrow-site-confirmations-4-without-a-network")
        } finally {
            TestDevice.asItWasBuilt()
        }
    }

    /** The form for a new site in the dark appearance, with a fault at its field. Skipped without a server. */
    @Test
    fun theFormForASiteInDarkAppearance() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = true) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(scenario)
        signInFromTheMap(scenario, "d${System.currentTimeMillis() % 1_000_000_000}")
        pressWhereNoSiteLies(scenario, lat = 47.295, lon = 10.875)
        letTheMapDraw(1)
        keepScreenshot("dark-site-edit-1-entry")
        tap(hasTestTag("site-add-here"))
        waitFor(hasTestTag("site-form"))
        screen.onNode(hasTestTag("site-send")).performScrollTo().performClick()
        waitFor(hasText("Give the site a name.", substring = true))
        letTheMapDraw(1)
        keepScreenshot("dark-site-edit-2-form-fault")
        screen.onNode(hasTestTag("site-send")).performScrollTo()
        letTheMapDraw(1)
        keepScreenshot("dark-site-edit-3-giving")
    }

    /**
     * The places from pilots, the crowdsourced layer: off until the pilot
     * switches them on at the foot of the sites' filter, so the made-up
     * take-off that pilots added is neither on the map nor in the list;
     * switched on it is on both, marked "From pilots", and the filter's
     * line says how many of the sites are such places. Its page says what
     * a place from pilots is worth, and to a pilot who is not signed in
     * what signing in is for, in the core's line. A partner's site says
     * "From a partner" on its page only. Skipped without a server.
     */
    @Test
    fun thePlacesFromPilotsShowOnlyOnceSwitchedOn() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        val page = hasTestTag("site-page")
        val fromPilots = "from pilots: not official and not checked by anyone."
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(scenario)

        // Off: the partner's two sites, and no place from pilots, on the map and in the list.
        assertEquals(2, TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0)
        assertFalse(exists(hasTestTag("sites-filter-card")))
        tapInLayers(described("Flying sites"))
        waitFor(hasTestTag("site-list"))
        assertTrue(exists(hasTestTag("site-row") and described("Musterkogel West")))
        assertFalse(exists(described("Beispieljoch")))
        assertFalse(exists(hasTestTag("site-list-leaves")))
        tap(hasText("Done"))
        waitFor(described("Show my position"))

        // The switch at the foot of the filter, off, under the core's name and what it shows.
        tap(hasTestTag("sites-filter-open"))
        waitFor(hasTestTag("sites-filter"))
        screen.onNode(hasTestTag("sites-filter-note")).performScrollTo()
        assertTrue(exists(hasTestTag("sites-from-pilots") and isOff() and hasText("Places from pilots")))
        assertTrue(
            exists(
                hasTestTag("sites-from-pilots") and
                    hasText("a place on the map is not permission to fly there.", substring = true),
            ),
        )
        assertFalse(exists(hasTestTag("sites-filter-from-pilots")))
        letTheMapDraw(1)
        keepScreenshot("sites-pilots-1-filter-off")

        // On: the count says that a place from pilots is among the sites.
        tap(hasTestTag("sites-from-pilots"))
        waitFor(hasTestTag("sites-from-pilots") and isOn())
        waitFor(hasTestTag("sites-filter-from-pilots") and hasText(fromPilots, substring = true))
        screen.onNode(hasTestTag("sites-filter-note")).performScrollTo()
        letTheMapDraw(1)
        keepScreenshot("sites-pilots-2-filter-on")
        tap(hasTestTag("sites-filter-done"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) >= 3
        }
        // The switch counts as part of the filter being on: the button is
        // drawn as on, and the line on the map says what is shown.
        assertTrue(exists(hasTestTag("sites-filter-open") and isSelected()))
        waitFor(hasTestTag("sites-filter-card"))
        // The card's lines are one element for TalkBack and for a test.
        assertTrue(exists(hasTestTag("sites-filter-lines") and hasText(fromPilots, substring = true)))
        letTheMapDraw(2)
        keepScreenshot("sites-pilots-3-map")

        // In the list: marked in its line, and the partner's site is not.
        tapInLayers(described("Flying sites"))
        waitFor(hasTestTag("site-list"))
        assertTrue(exists(hasTestTag("site-list-leaves") and hasText(fromPilots, substring = true)))
        val row = hasTestTag("site-row") and described("Beispieljoch")
        screen.onNode(hasTestTag("site-list")).performScrollToNode(row)
        assertTrue(exists(row and saying("From pilots")))
        assertFalse(exists(hasTestTag("site-row") and described("Musterkogel West") and saying("From")))
        letTheMapDraw(1)
        keepScreenshot("sites-pilots-4-list")

        // Its page: the tier by its name, the core's notice, who took off
        // there; and, signed out, what signing in is for.
        tap(row)
        waitFor(page)
        waitFor(hasTestTag("site-tier") and hasText("From pilots"))
        screen.onNode(page).performScrollToNode(hasTestTag("site-by-pilots"))
        assertTrue(exists(hasTestTag("site-by-pilots-note") and hasText("not official and not checked by anyone", substring = true)))
        screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
        waitFor(hasTestTag("site-confirmations-open") and hasText("1 pilot took off here", substring = true))
        assertFalse(exists(hasTestTag("site-confirm")))
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        waitFor(
            hasTestTag("site-offers-note") and
                hasText("Sign in to correct this site, to say that you took off here and to report it."),
        )
        assertTrue(exists(hasTestTag("site-history-open") and hasText("History")))
        assertTrue(exists(hasTestTag("site-sign-in")))
        assertFalse(exists(hasTestTag("site-correct")))
        assertFalse(exists(hasTestTag("site-remove")))
        assertFalse(exists(hasTestTag("site-report")))
        letTheMapDraw(1)
        keepScreenshot("sites-pilots-5-site-signed-out")

        // Anyone reads who took off there, and the history.
        screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
        tap(hasTestTag("site-confirmations-open"))
        waitFor(hasTestTag("site-confirmations"))
        assertTrue(exists(hasText("Pilots who took off here")))
        waitFor(hasTestTag("site-confirmation") and hasText("Mara Muster (@mara) on ", substring = true))
        tap(described("Back"))
        waitFor(page)

        // A partner's site says whose it is on its page, and nowhere else.
        tap(described("Back"))
        openFromTheListOfSites("Musterkogel West")
        waitFor(hasTestTag("site-tier") and hasText("From a partner"))
        tap(hasText("Done"))
        waitFor(described("Show my position"))

        // Off again: the place from pilots is gone from the map, and the filter is off.
        tap(hasTestTag("sites-filter-open"))
        waitFor(hasTestTag("sites-filter"))
        screen.onNode(hasTestTag("sites-from-pilots")).performScrollTo().performClick()
        waitFor(hasTestTag("sites-from-pilots") and isOff())
        tap(hasTestTag("sites-filter-done"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) == 2
        }
        assertFalse(exists(hasTestTag("sites-filter-card")))
        assertTrue(exists(hasTestTag("sites-filter-open") and isSelected().not()))
    }

    /** A place from pilots in the dark appearance: its sign on the map with the others, and its page. Skipped without a server. */
    @Test
    fun aPlaceFromPilotsInDarkAppearance() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = true) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        val page = hasTestTag("site-page")
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(scenario)
        showThePlacesFromPilots(scenario)
        letTheMapDraw(1)
        keepScreenshot("dark-sites-pilots-1-map")
        openFromTheListOfSites("Beispieljoch")
        waitFor(hasTestTag("site-tier") and hasText("From pilots"))
        letTheMapDraw(1)
        keepScreenshot("dark-sites-pilots-2-site")
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        waitFor(hasTestTag("site-offers-note"))
        letTheMapDraw(1)
        keepScreenshot("dark-sites-pilots-3-site-foot")
    }

    /**
     * "I took off here and it was good": a pilot the test makes says it of
     * the made-up place from pilots, for the day before the server's, with
     * a line; the page then says so in the button's place and counts the
     * pilot, the list of who took off there has the line, and the pilot
     * takes it back. A partner's site offers the same pilot the sentence
     * too, and no correcting, with the core's line why. The development
     * server's clock stands at 18 November 2026, so the day is chosen from
     * the form's menu by its date. Skipped without a server.
     */
    @Test
    fun aPilotSaysTheyTookOffAtAPlaceFromPilots() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        // The day before the development server's own, which its clock takes whatever the emulator's says.
        val day = LocalDate.of(2026, 11, 17)
        val back = ChronoUnit.DAYS.between(day, LocalDate.now())
        assumeTrue("The form offers the last 90 days, and 17 November 2026 is not among them.", back in 0..89)
        val dayWords = when (back) {
            0L -> "Today"
            1L -> "Yesterday"
            else -> "Tue 17 Nov"
        }
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        val page = hasTestTag("site-page")
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(scenario)
        val handle = "c${System.currentTimeMillis() % 1_000_000_000}"
        signInFromTheMap(scenario, handle)
        showThePlacesFromPilots(scenario)

        // The place's page for a signed-in pilot: the sentence as a button.
        openFromTheListOfSites("Beispieljoch")
        waitFor(hasTestTag("site-tier") and hasText("From pilots"))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-1-page")
        screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
        waitFor(hasTestTag("site-confirm") and hasText("I took off here and it was good"))
        assertTrue(exists(hasTestTag("site-confirmations-open") and hasText("1 pilot took off here", substring = true)))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-1b-offer")
        tap(hasTestTag("site-confirm"))

        // The small form: today chosen, the days as a menu, the line with
        // its room, and no flights where the logbook has none.
        waitFor(hasTestTag("site-confirm-form"))
        assertTrue(exists(hasTestTag("site-confirm-day") and hasText("Today")))
        assertTrue(exists(hasText("On which day")))
        assertTrue(exists(hasTestTag("site-confirm-about") and hasText("Other pilots see your handle, the day and your line", substring = true)))
        assertFalse(exists(hasTestTag("site-confirm-flights")))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-2-form")
        tap(hasTestTag("site-confirm-day"))
        waitFor(hasTestTag("site-confirm-day-$day"))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-2b-days")
        screen.onNode(hasTestTag("site-confirm-day-$day")).performScrollTo().performClick()
        waitFor(hasTestTag("site-confirm-day") and hasText(dayWords))
        screen.onNode(hasTestTag("site-confirm-note")).performTextInput("West wind, easy launch (a test).")
        waitFor(hasText("32 / 200"))
        screen.onNode(hasTestTag("site-confirm-send")).performScrollTo()
        assertTrue(exists(hasTestTag("site-confirm-send") and hasText("Confirm")))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-2c-filled")
        tap(hasTestTag("site-confirm-send"))

        // Said: the core's line, the sentence about it in the button's
        // place with the two ways on, and the pilot counted.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("site-confirm-form")) }
        waitFor(page)
        screen.onNode(page).performScrollToNode(hasTestTag("site-said"))
        waitFor(hasTestTag("site-said") and hasText("You confirmed this site for Tue 17 Nov."))
        screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
        waitFor(hasTestTag("site-confirmed-words") and hasText("You confirmed this site for Tue 17 Nov 2026."))
        waitFor(hasTestTag("site-confirmations-open") and hasText("2 pilots took off here, last on 17 November 2026"))
        assertTrue(exists(hasTestTag("site-confirm-again") and hasText("Change the day")))
        assertTrue(exists(hasTestTag("site-withdraw") and hasText("Take my confirmation back")))
        assertFalse(exists(hasTestTag("site-confirm")))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-3-confirmed")

        // "Pilots who took off here": the pilot's line among them, the newest day first.
        tap(hasTestTag("site-confirmations-open"))
        waitFor(hasTestTag("site-confirmations"))
        assertTrue(exists(hasText("Pilots who took off here")))
        waitFor(
            hasTestTag("site-confirmation") and
                hasText("(@$handle) on Tue 17 Nov 2026", substring = true) and
                hasText("West wind, easy launch (a test).", substring = true),
        )
        assertEquals(2, screen.onAllNodes(hasTestTag("site-confirmation")).fetchSemanticsNodes().size)
        letTheMapDraw(1)
        keepScreenshot("site-confirm-4-list")
        tap(described("Back"))
        waitFor(page)

        // Taken back: asked first in the core's words, then its line, the button again, and one pilot fewer.
        screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
        tap(hasTestTag("site-withdraw"))
        waitFor(hasText("Take your confirmation of this site back?"))
        assertFalse(exists(hasTestTag("site-why")))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-5-take-back")
        tap(hasTestTag("site-why-send"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("site-why-send")) }
        screen.onNode(page).performScrollToNode(hasTestTag("site-said"))
        waitFor(hasTestTag("site-said") and hasText("Your confirmation of this site is taken back."))
        screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
        waitFor(hasTestTag("site-confirm") and hasText("I took off here and it was good"))
        waitFor(hasTestTag("site-confirmations-open") and hasText("1 pilot took off here", substring = true))
        assertFalse(exists(hasTestTag("site-confirmed-words")))

        // A partner's site: the same pilot may say the sentence of it, and
        // is offered no correcting and no removing, with the core's line why.
        tap(described("Back"))
        openFromTheListOfSites("Musterkogel West")
        waitFor(hasTestTag("site-tier") and hasText("From a partner"))
        assertFalse(exists(hasTestTag("site-said")))
        screen.onNode(page).performScrollToNode(hasTestTag("site-confirmed"))
        waitFor(hasTestTag("site-confirm") and hasText("I took off here and it was good"))
        assertTrue(exists(hasTestTag("site-confirmations-open") and hasText("2 pilots took off here", substring = true)))
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        waitFor(
            hasTestTag("site-offers-note") and
                hasText("A partner of Venti added this site, and only Venti's partners change it. If something about it is wrong, report it."),
        )
        assertFalse(exists(hasTestTag("site-correct")))
        assertFalse(exists(hasTestTag("site-remove")))
        assertTrue(exists(hasTestTag("site-report") and hasText("Report this site")))
        assertTrue(exists(hasTestTag("site-history-open")))
        assertFalse(exists(hasTestTag("site-sign-in")))
        letTheMapDraw(1)
        keepScreenshot("site-confirm-6-partners-site")
    }

    /**
     * A site's history and reporting it are reached from its page as Today
     * opens it, as from the map's: a partner's made-up take-off, kept as
     * the pilot's own so that Today lists it, by a pilot the test makes.
     * Skipped without a server.
     */
    @Test
    fun aSitesHistoryAndReportAreReachedFromToday() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        val page = hasTestTag("site-page")
        val takeoff = "Musterkogel West"
        tapChip("layer-wind")
        tapChip("layer-pilots")
        lookAtTheMadeUpSites(scenario)
        signInFromTheMap(scenario, "t${System.currentTimeMillis() % 1_000_000_000}")

        // Kept as the pilot's own from the map's page, so that Today lists it, however far away.
        openFromTheListOfSites(takeoff)
        screen.onNode(page).performScrollToNode(hasTestTag("site-own"))
        tap(hasTestTag("site-own"))
        waitFor(hasTestTag("site-own") and hasText("Kept"))
        tap(hasText("Done"))
        waitFor(described("Show my position"))
        backToToday()
        waitFor(hasTestTag("today-kept-title"))
        waitFor(hasText(takeoff) and hasClickAction())

        // Today's page of it: the tier, who took off there, and the core's entries at its foot.
        screen.onAllNodes(hasText(takeoff) and hasClickAction()).onFirst().performScrollTo().performClick()
        waitFor(page)
        waitFor(hasTestTag("site-tier") and hasText("From a partner"))
        screen.onNode(page).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        waitFor(hasTestTag("site-history-open") and hasText("History"))
        waitFor(hasTestTag("site-report") and hasText("Report this site"))
        assertFalse(exists(hasTestTag("site-correct")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-site-1-entries")

        // Its history: two revisions, the newest first, and no putting back for a pilot who may not correct it.
        tap(hasTestTag("site-history-open"))
        waitFor(hasTestTag("site-history"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            screen.onAllNodes(hasTestTag("site-revision-words")).fetchSemanticsNodes().size == 2
        }
        assertTrue(exists(hasTestTag("site-revision-words") and hasText("Wind directions added.", substring = true)))
        assertTrue(exists(hasTestTag("site-revision-report") and hasText("Report this revision")))
        assertFalse(exists(hasTestTag("site-put-back")))
        assertFalse(exists(hasTestTag("site-undo")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-site-2-history")
        tap(described("Back"))
        waitFor(page)

        // Reporting it: the app's report form, with a site's reasons. Nothing is sent.
        screen.onNode(page).performScrollToNode(hasTestTag("site-edits"))
        tap(hasTestTag("site-report"))
        waitFor(hasTestTag("report-notice"))
        assertTrue(exists(hasText("Wrong place or dangerous")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("today-site-3-report")
        tap(hasTestTag("report-close"))
    }

    /** From Today to the account's form, by way of the settings. */
    private fun openTheAccountFromToday() {
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        screen.onNode(hasTestTag("account-link")).performScrollTo().performClick()
        waitForThePasswordForm()
    }

    /** Today itself, from another tab: a page left over it is left with a second tap. */
    private fun backToToday() {
        tap(tab("Today"))
        if (exists(hasText("Vario sound"))) {
            tap(tab("Today"))
        }
        waitFor(hasTestTag("today-card"))
    }

    /** The list of the pilot's own take-offs, from Today, by way of the settings. */
    private fun openThePilotsTakeoffs() {
        screen.onNode(hasTestTag("today-head")).performScrollTo()
        openFromToday("Settings")
        waitFor(hasText("Vario sound"))
        screen.onNode(hasTestTag("kept-sites-link")).performScrollTo().performClick()
    }

    /**
     * A take-off kept while signed in is on the pilot's account, and the
     * list says so in the core's words; the app started again with nothing
     * on the device, and signed in to the same account, has it back from
     * the account. Against the development server, with an account the
     * test makes. Skipped without a server.
     */
    @Test
    fun aKeptTakeoffFollowsTheAccount() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(onToday = true) { putExtra(LaunchOptions.SERVER, server) }
        val takeoff = "Fluggelände Jägerkamp/Nagelspitz"
        val page = hasTestTag("site-page")
        val onTheAccount = hasTestTag("kept-sites-note") and
            hasText("Kept on your Venti account and on this device", substring = true)
        val handle = "k${System.currentTimeMillis() % 1_000_000_000}"

        waitFor(hasTestTag("today-card"))
        openTheAccountFromToday()
        makeAnAccountAndGoBackToTheMap(handle)

        // Signed in, the take-off is kept from its page as ever.
        backToToday()
        waitFor(hasText(takeoff) and hasClickAction())
        screen.onAllNodes(hasText(takeoff) and hasClickAction()).onFirst().performScrollTo().performClick()
        waitFor(page)
        screen.onNode(page).performTouchInput { swipeUp() }
        TestDevice.letTheAppRun(screen, 1)
        screen.onNode(page).performScrollToNode(hasTestTag("site-own"))
        waitFor(hasTestTag("site-own") and hasText("Keep as my take-off"))
        tap(hasTestTag("site-own"))
        waitFor(hasTestTag("site-own") and hasText("Kept"))
        tap(hasText("Done"))
        waitFor(hasTestTag("today-kept-title") and hasText("Your take-offs"))

        // Its list says where it lives, once the sync that followed is through.
        openThePilotsTakeoffs()
        waitFor(hasTestTag("kept-site") and hasAnyDescendant(described(takeoff)))
        waitFor(onTheAccount)
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("kept-account-1-on-the-account")
        app?.close()

        // Started again with an empty device: nothing is kept, until the
        // pilot signs in to the same account.
        launch(onToday = true) { putExtra(LaunchOptions.SERVER, server) }
        waitFor(hasTestTag("today-card"))
        waitFor(hasText(takeoff) and hasClickAction())
        assertFalse(exists(hasTestTag("today-kept-title")))
        openTheAccountFromToday()
        screen.onNode(hasTestTag("account-handle")).performTextInput(handle)
        screen.onNode(hasTestTag("account-password")).performTextInput("a password for this test")
        tap(hasTestTag("account-submit"))
        waitFor(hasText("@$handle"))
        waitFor(hasText("Sync now"))
        tap(described("Back"))
        waitFor(hasTestTag("kept-sites-link"))
        screen.onNode(hasTestTag("kept-sites-link")).performScrollTo().performClick()
        waitFor(hasTestTag("kept-site") and hasAnyDescendant(described(takeoff)))
        waitFor(onTheAccount)
        assertTrue(exists(hasText("© OpenStreetMap contributors", substring = true)))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("kept-account-2-back-from-the-account")

        // And Today lists it first again.
        tap(described("Back"))
        waitFor(hasText("Vario sound"))
        backToToday()
        waitFor(hasTestTag("today-kept-title") and hasText("Your take-offs"))

        // Forgotten here, it is forgotten on the account: the list is empty, and stays so.
        openThePilotsTakeoffs()
        waitFor(hasTestTag("kept-forget"))
        tap(hasTestTag("kept-forget"))
        waitFor(hasTestTag("kept-forget-confirm"))
        tap(hasTestTag("kept-forget-confirm"))
        waitFor(hasText("No take-offs of your own yet"))
        TestDevice.letTheAppRun(screen, 2)
        assertTrue(exists(hasText("No take-offs of your own yet")))
    }

    /** Makes a new account from the form that is open, and opens "Programs with access" from its page. */
    private fun makeAnAccountAndOpenTheProgramsWithAccess(handle: String) {
        tap(hasTestTag("account-new"))
        waitFor(hasTestTag("account-invite"))
        screen.onNode(hasTestTag("account-handle")).performTextInput(handle)
        screen.onNode(hasTestTag("account-name")).performTextInput("Test Pilot (not real)")
        screen.onNode(hasTestTag("account-password")).performTextInput("a password for this test")
        screen.onNode(hasTestTag("account-submit")).performScrollTo()
        tap(hasTestTag("account-submit"))
        waitFor(hasText("@$handle"))
        waitFor(hasText("Sync now"))
        screen.onNode(hasTestTag("account-tokens")).performScrollTo()
        assertTrue(exists(hasTestTag("account-tokens") and hasText("Programs with access")))
        tap(hasTestTag("account-tokens"))
        // A new account has given no program access: the core's line.
        waitFor(hasTestTag("tokens-empty") and hasText("No program has access to your account."))
        assertTrue(exists(hasText("without telling it your password", substring = true)))
    }

    /** The form that gives access, filled in for "My logger" with two things it may do. */
    private fun fillInTheFormForMyLogger() {
        tap(hasTestTag("tokens-give"))
        waitFor(hasTestTag("token-name"))
        screen.onNode(hasTestTag("token-name")).performTextInput("My logger")
        for (scope in listOf("flights:write", "live:write")) {
            screen.onNode(hasTestTag("token-scope-$scope")).performScrollTo().performClick()
            screen.onNode(hasTestTag("token-scope-$scope")).assertIsOn()
        }
        screen.onNode(hasTestTag("token-name")).performScrollTo()
        TestDevice.letTheAppRun(screen, 1)
    }

    /**
     * "Programs with access": a pilot gives a logger access to two things,
     * sees its secret once, finds it in the list, and withdraws it.
     * Against the development server, with an account the test makes.
     * Every word about a token and every refusal is the core's. Skipped
     * without a server.
     */
    @Test
    fun aPilotGivesAProgramAccessAndWithdrawsIt() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch { putExtra(LaunchOptions.SERVER, server) }
        openTheAccount()
        makeAnAccountAndOpenTheProgramsWithAccess("t${System.currentTimeMillis() % 1_000_000_000}")

        // A program without a name is refused before anything is sent.
        tap(hasTestTag("tokens-give"))
        waitFor(hasTestTag("token-submit"))
        screen.onNode(hasTestTag("token-submit")).performScrollTo().performClick()
        waitFor(hasText("Give the program a name, so that you know later what this access is for."))
        tap(described("Back"))
        waitFor(hasTestTag("tokens-give"))

        fillInTheFormForMyLogger()
        assertTrue(exists(hasText("Flights: write")))
        keepScreenshot("tokens-1-form")

        // The password is asked for, with the core's reason, and a wrong one is refused in its words.
        screen.onNode(hasTestTag("token-password")).performScrollTo().performTextInput("not the password at all")
        assertTrue(exists(hasText("nobody who picks up this device", substring = true)))
        screen.onNode(hasTestTag("token-submit")).performScrollTo().performClick()
        waitFor(hasTestTag("token-problem") and hasText("The password is wrong."))
        tap(hasText("OK") and hasClickAction())
        screen.onNode(hasTestTag("token-password")).performTextClearance()
        screen.onNode(hasTestTag("token-password")).performTextInput("a password for this test")
        screen.onNode(hasTestTag("token-submit")).performScrollTo().performClick()

        // The secret, once: with the core's words, and no way off the page but its own button.
        waitFor(hasTestTag("token-secret"))
        val secret = screen.onNode(hasTestTag("token-secret")).fetchSemanticsNode()
            .config.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text }.orEmpty()
        assertTrue(secret.startsWith("venti_pat_"))
        assertTrue(exists(hasText("Copy it now: Venti does not show it again.")))
        assertTrue(exists(hasText("give it to nobody else", substring = true)))
        assertFalse(exists(described("Back")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("tokens-2-secret-shown-once")

        // Copied with the clipboard told that it is a secret.
        tap(hasTestTag("token-copy"))
        waitFor(hasTestTag("token-copied"))
        val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
        var isMarkedSensitive = false
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            isMarkedSensitive = clipboard.primaryClipDescription?.extras
                ?.getBoolean(android.content.ClipDescription.EXTRA_IS_SENSITIVE) == true
            // Nothing of the test stays on the emulator's clipboard.
            clipboard.clearPrimaryClip()
        }
        assertTrue(isMarkedSensitive)
        tap(hasTestTag("token-done"))

        // The list has it, with what it may do in the core's line, and no secret anywhere.
        waitFor(hasTestTag("token") and hasAnyDescendant(described("My logger")))
        assertTrue(exists(saying("Flights: write · Live tracking: send")))
        assertTrue(exists(sayingLike("Made \\d+ \\w+ \\d{4}\nLast used never\nEnds never")))
        assertFalse(exists(hasText("venti_pat_", substring = true)))
        // Android's own note that something was copied has gone by then.
        TestDevice.letTheAppRun(screen, 7)
        keepScreenshot("tokens-3-list")

        // Withdrawing asks first, in the core's words.
        tap(hasTestTag("token-withdraw"))
        waitFor(hasTestTag("token-withdraw-confirm"))
        assertTrue(exists(hasText("Withdraw the access of \u201cMy logger\u201d?")))
        assertTrue(exists(hasText("Keep access")))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("tokens-4-withdraw")
        tap(hasTestTag("token-withdraw-confirm"))
        waitFor(hasTestTag("tokens-empty") and hasText("No program has access to your account."))
        TestDevice.letTheAppRun(screen, 1)
        keepScreenshot("tokens-5-none")

        // One step back is the account's page.
        tap(described("Back"))
        waitFor(hasTestTag("account-tokens"))
    }

    /** The form that gives a program access, in dark appearance. */
    @Test
    fun theFormThatGivesAProgramAccessInDarkAppearance() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = true) { putExtra(LaunchOptions.SERVER, server) }
        openTheAccount()
        makeAnAccountAndOpenTheProgramsWithAccess("u${System.currentTimeMillis() % 1_000_000_000}")
        fillInTheFormForMyLogger()
        keepScreenshot("dark-tokens-1-form")
    }

    /**
     * The map as it is since 2026-11-22: three round buttons at most, an
     * (i) for every credit in full, and one layers sheet with a switch
     * for every layer and a row for every list. In it "Climbs now", off
     * until switched on: the development server's two made-up climbs over
     * the Tegernsee valley, each a mark with the core's label, and the
     * core's line on a tap; and "Where thermals have been", its two
     * made-up cells. Skipped without a server.
     */
    @Test
    fun theLayersSheetShowsWhereOthersClimbNow() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch {
            putExtra(LaunchOptions.SERVER, server)
            // The development server's made-up flights are of August.
            putExtra(LaunchOptions.FLOWN_MONTH, 8)
        }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))
        assertTrue(TestDevice.lookAt(scenario, lat = 47.655, lon = 11.775, zoom = 11.0))
        waitFor(hasTestTag("sites-filter-open"))
        // Three round buttons, and none of the lists' or the plans'.
        assertTrue(exists(hasTestTag("map-layers")))
        for (gone in listOf("Wind stations", "Flying sites", "Pilots in the air", "Flying plans", "Aerial view")) {
            assertFalse(gone, exists(described(gone)))
        }
        letTheMapDraw()
        keepScreenshot("light-layers-1-map")

        // The (i): every credit in full, Venti's data and the base map's.
        tap(hasTestTag("map-credits"))
        val credits = hasTestTag("wind-credits")
        waitForAll(
            credits and hasText("Airspace: openAIP, not certified"),
            credits and hasText("Sites: OpenStreetMap contributors"),
            // The base map's own, which the map's button showed before.
            credits and hasText("© OpenStreetMap contributors", substring = true),
        )
        letTheMapDraw(1)
        keepScreenshot("light-layers-2-credits")
        tap(hasTestTag("credits-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(credits) }

        // The sheet: the chips' four in the chips' state, the layers that
        // have no chip, and the lists.
        openLayers()
        waitForAll(
            hasTestTag("layers-wind") and isOn(),
            hasTestTag("layers-sites") and isOn(),
            hasTestTag("layer-forecast") and isOff(),
            hasTestTag("layer-radar") and isOff(),
        )
        letTheMapDraw(1)
        keepScreenshot("light-layers-3-sheet")
        // What others found in the air: off until the pilot asks for it.
        val climbs = hasTestTag("layer-climbs")
        screen.onNode(climbs).performScrollTo()
        waitFor(climbs and isOff())
        assertTrue(exists(hasTestTag("layer-flown") and isOff()))
        for (list in listOf("Wind stations", "Flying sites", "Flying plans")) {
            assertTrue(list, exists(described(list)))
        }
        screen.onNode(climbs).performClick()
        waitFor(climbs and isOn())
        // How many, in the core's words, under the row.
        waitFor(climbs and hasText("2 climbs in the last 10 minutes"))
        screen.onNode(hasTestTag("layer-flown")).performScrollTo()
        letTheMapDraw(1)
        keepScreenshot("light-layers-4-sheet-climbs-on")
        closeLayers()

        // On the map: a mark for each, and the network's credit with the map's.
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "live-climbs") ?: 0) >= 2
        }
        letTheMapDraw()
        keepScreenshot("light-layers-5-climbs-on-the-map")
        seeInCredits(hasText("Positions: Open Glider Network", substring = true))

        // A tap on the climb east of the Wallberg: the core's line.
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.6675, lon = 11.8025)))
        waitFor(hasTestTag("climb-line") and hasText("Climbing now: +2.4 m/s between 1,600 and 2,100 m, 3 pilots", substring = true))
        letTheMapDraw(1)
        keepScreenshot("light-layers-6-climb")
        tap(hasTestTag("air-line-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("climb-line")) }

        // Where thermals have been: the two cells, with the note and the
        // flights' terms under the row. Nothing else under the finger.
        tapChip("layer-wind")
        tapChip("layer-sites")
        tapChip("layer-pilots")
        switchLayer("layer-climbs", on = false)
        openLayers()
        val flown = hasTestTag("layer-flown")
        screen.onNode(flown).performScrollTo().performClick()
        waitFor(flown and isOn())
        waitFor(flown and hasText("Where pilots climbed, from the public flights on Venti.", substring = true))
        closeLayers()
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flown-thermals") ?: 0) >= 2 &&
                (TestDevice.drawnOnTheMap(scenario, "live-climbs") ?: 0) == 0
        }
        letTheMapDraw()
        keepScreenshot("light-layers-7-thermals-flown")
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.6875, lon = 11.7525)))
        waitFor(hasTestTag("flown-line") and hasText(" climbs of 3 pilots, +1.5 m/s on average", substring = true))
        tap(hasTestTag("air-line-close"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("flown-line")) }

        // Zoomed out beyond a degree the core says to zoom in.
        assertTrue(TestDevice.lookAt(scenario, lat = 47.655, lon = 11.775, zoom = 7.0))
        waitFor(hasText("Zoom in to see where pilots climbed."))
        assertTrue(exists(hasTestTag("flown-too-wide")))
        letTheMapDraw(1)
        keepScreenshot("light-layers-8-thermals-zoom-in")
    }

    /**
     * Pilots in the air at a site: the line under the name of the
     * Wallberg's take-off, where the development server has made-up
     * paragliders in the air, and an alarm for it made from the take-off's
     * "Alert me", the second kind of the form, which the list then shows.
     * Skipped without a server.
     */
    @Test
    fun aPilotSetsAnAlarmForPilotsInTheAirAtATakeoff() {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        TestDevice.allowNotifications()
        launch { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        signInAsTheDemoPilot()
        tap(described("Back"))
        waitFor(tab("Map"))
        tap(tab("Map"))
        waitFor(described("Show my position"))
        // Only the sites under the finger.
        tapChip("layer-wind")
        tapChip("layer-pilots")
        assertTrue(TestDevice.lookAt(scenario, lat = 47.6652, lon = 11.7954, zoom = 13.0))
        waitFor(hasTestTag("sites-filter-open"))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "flying-sites") ?: 0) > 0
        }
        letTheMapDraw(2)
        TestDevice.tap(checkNotNull(TestDevice.onScreen(scenario, lat = 47.6652212, lon = 11.7954245)))
        waitFor(hasTestTag("site-page"))
        // Who flies there now, in the core's words, under the name.
        waitFor(hasTestTag("site-air") and hasText("in the air", substring = true))
        letTheMapDraw(1)
        keepScreenshot("light-air-site-1-pilots-line")

        screen.onNode(hasTestTag("site-page")).performScrollToNode(hasTestTag("site-alert-me"))
        tap(hasTestTag("site-alert-me"))
        waitFor(hasTestTag("alert-form"))
        // The form opens as the day's alarm; the second kind is the core's "Pilots in the air".
        val kinds = hasAnyAncestor(hasTestTag("alert-kind"))
        waitFor(hasText("Pilots in the air") and kinds)
        tap(hasText("Pilots in the air") and kinds)
        waitFor(hasTestTag("alert-line") and hasText("3 pilots or more in the air at ", substring = true) and hasText(", at most once in 6 hours", substring = true))
        assertTrue(exists(hasTestTag("alert-pilots-note") and hasText("Counts paragliders and hang gliders flying within 5 km of the place", substring = true)))
        // Five pilots, and quiet for four hours after.
        screen.onNode(hasTestTag("alert-pilots-more")).performScrollTo().performClick()
        screen.onNode(hasTestTag("alert-pilots-more")).performClick()
        screen.onNode(hasTestTag("alert-rest-less")).performScrollTo().performClick()
        screen.onNode(hasTestTag("alert-rest-less")).performClick()
        waitFor(hasTestTag("alert-line") and hasText("5 pilots or more in the air at ", substring = true) and hasText(", at most once in 4 hours", substring = true))
        screen.onNode(hasTestTag("alert-line")).performScrollTo()
        letTheMapDraw(1)
        keepScreenshot("light-air-site-2-alarm-form")
        screen.onNode(hasTestTag("alert-save")).performScrollTo().performClick()
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) { !exists(hasTestTag("alert-form")) }
        tap(hasText("Done"))
        waitFor(described("Show my position"))

        // The list has it, with the core's line.
        openFromToday("Settings")
        screen.onNode(hasTestTag("account-link")).performScrollTo()
        tap(hasTestTag("account-link"))
        screen.onNode(hasTestTag("account-alerts")).performScrollTo()
        tap(hasTestTag("account-alerts"))
        waitFor(hasTestTag("alert-row") and hasText("5 pilots or more in the air at ", substring = true))
        letTheMapDraw(1)
        keepScreenshot("light-air-site-3-alarm-in-the-list")
    }

    /**
     * Cables near the Tegernsee, which the development server serves from
     * its recorded answer: drawn on the map from close in, dashed where
     * their height is not mapped, and a tap on one says what it is, with
     * the core's sentence on what the layer does not promise.
     */
    private fun lookAtTheCables(prefix: String, dark: Boolean) {
        val server = InstrumentationRegistry.getArguments().getString("ventiServer")
        assumeTrue("No Venti server was named in the ventiServer argument.", !server.isNullOrEmpty())
        launch(dark = dark) { putExtra(LaunchOptions.SERVER, server) }
        val scenario = checkNotNull(app)
        waitFor(described("Show my position"))
        waitFor(hasTestTag("map-layers"))
        assertTrue(TestDevice.lookAt(scenario, lat = 47.7229, lon = 11.7112, zoom = 15.0))
        screen.waitUntil(TestDevice.PATIENCE_SECONDS * 1000L) {
            (TestDevice.drawnOnTheMap(scenario, "cables-dashed") ?: 0) > 0
        }
        letTheMapDraw()
        keepScreenshot("$prefix-cables-1-map")

        val point = checkNotNull(TestDevice.onScreen(scenario, lat = 47.7229503, lon = 11.7111713))
        TestDevice.tap(point)
        waitFor(hasTestTag("cable-sheet"))
        assertTrue(exists(hasTestTag("cable-words") and hasText("height not mapped", substring = true)))
        assertTrue(exists(hasTestTag("cable-limits")))
        letTheMapDraw(1)
        keepScreenshot("$prefix-cables-2-cable")
    }

    @Test
    fun theMapDrawsTheCables() = lookAtTheCables("light", dark = false)

    /**
     * The task's page: the sample flight replayed with the made-up task,
     * and the page says the next turnpoint with its readings once the start
     * is taken.
     */
    private fun lookAtTheTaskPage(prefix: String, dark: Boolean) {
        launch(dark = dark) { putExtra(LaunchOptions.SAMPLE_TASK, true) }
        waitFor(described("Show my position"))
        tap(tab("Fly"))
        waitFor(hasText("Replay the sample flight"))
        tap(hasText("Replay the sample flight"))
        waitFor(hasText("20 times faster"))
        tap(hasText("20 times faster"))
        waitFor(described("Altitude"))
        turnTo("Task")
        waitFor(hasTestTag("task-page"))
        waitFor(hasTestTag("task-next-name") and hasText("Sample turnpoint west"), seconds = TestDevice.WHOLE_FLIGHT_SECONDS)
        waitFor(hasTestTag("task-over-needed") and !saying("not available"))
        letTheMapDraw(2)
        keepScreenshot("$prefix-task-page-1")
        holdToEnd()
    }

    @Test
    fun theCockpitsTaskPageInDarkAppearance() = lookAtTheTaskPage("dark", dark = true)
}

/** The made-up Google account of the sign-in test; the development server makes its token. */
private const val GOOGLE_TEST_EMAIL = "ui.test@example.com"
