package fly.venti.app.fly

import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.waypoints.place
import fly.venti.core.FlightSession
import fly.venti.core.LandingOption
import fly.venti.core.Reach
import fly.venti.core.ReachView
import fly.venti.core.Replay
import fly.venti.core.WaypointKind
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.Locale

/**
 * The landing within reach: what the cockpit's badge and the list of places
 * to fly to say of what the core works out, and in which of three tones.
 */
class ReachTest {
    private val locale = Locale.getDefault()
    private val scope = CoroutineScope(Dispatchers.Default)

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
        scope.cancel()
    }

    private fun reach(kind: Reach, arrivalM: Double?) =
        ReachView(
            distanceM = 3_000.0, bearingDeg = 300.0, arrivalHeightM = arrivalM, reach = kind,
            groundInTheWayM = null,
        )

    private fun landing(kind: Reach, arrivalM: Double?) = LandingOption(
        index = 0u,
        name = "Sample landing field",
        reach = reach(kind, arrivalM),
        inReach = if (kind == Reach.WITHIN || kind == Reach.JUST) 1u else 0u,
    )

    @Test
    fun theBadgeNamesTheLandingWithTheHeightToSpareOrSaysThatNoneIsInReach() {
        // Height to spare, to ten metres: the prediction is no finer.
        val within = landing(Reach.WITHIN, 512.4)
        assertEquals("+510 m", ReachWords.height(within))
        assertEquals("Sample landing field, arriving +510 m above it", ReachWords.spoken(within))
        assertEquals(ReachTone.Plain, ReachTone.of(within.reach.reach))

        // Less than a hundred metres to spare: still named, on amber.
        val just = landing(Reach.JUST, 64.9)
        assertEquals("+60 m", ReachWords.height(just))
        assertEquals(ReachTone.Amber, ReachTone.of(just.reach.reach))

        // The glide reaches none: the badge says so, on red, and names no
        // height for the one that misses by least.
        val beyond = landing(Reach.BEYOND, -180.0)
        assertNull(ReachWords.height(beyond))
        assertEquals("No landing in reach", ReachWords.spoken(beyond))
        assertEquals(ReachTone.Red, ReachTone.of(beyond.reach.reach))

        // What is not known is not dressed up as either.
        assertNull(ReachWords.height(landing(Reach.UNKNOWN, null)))
        assertEquals(ReachTone.Quiet, ReachTone.of(Reach.UNKNOWN))

        // Halves go away from nothing, as on the iPhone.
        assertEquals("+10 m", ReachWords.height(5.0))
        assertEquals("0 m", ReachWords.height(4.9))
        assertEquals("−10 m", ReachWords.height(-5.0))
        assertEquals("+1,240 m", ReachWords.height(1_236.0))
    }

    @Test
    fun theListSaysTheHeightBesideEveryPlaceAlsoWhereTheGlideFallsShort() {
        assertEquals("+510 m", ReachWords.beside(reach(Reach.WITHIN, 512.4)))
        assertEquals("+60 m", ReachWords.beside(reach(Reach.JUST, 64.9)))
        // How far under the place the glide ends is worth knowing.
        assertEquals("−180 m", ReachWords.beside(reach(Reach.BEYOND, -183.0)))
        assertNull(ReachWords.beside(reach(Reach.UNKNOWN, null)))
        assertNull(ReachWords.beside(reach(Reach.UNKNOWN, 200.0)))
        assertNull(ReachWords.beside(null))

        // For someone who cannot see the row.
        assertEquals("Arriving +510 metres above it.", ReachWords.spoken(reach(Reach.WITHIN, 512.4)))
        assertEquals("Out of reach.", ReachWords.spoken(reach(Reach.BEYOND, -183.0)))
        assertEquals("Out of reach.", ReachWords.spoken(reach(Reach.BEYOND, null)))
        assertNull(ReachWords.spoken(reach(Reach.UNKNOWN, null)))
        assertNull(ReachWords.spoken(null as ReachView?))

        // The list says how the heights are worked out, what the
        // mountain means, and what is not looked at.
        assertTrue(ReachWords.HOW_IT_IS_WORKED_OUT.contains("A mountain marks a place with a ridge in the way"))
        assertTrue(ReachWords.HOW_IT_IS_WORKED_OUT.endsWith("where it has not, the ground in between is not looked at."))
    }

    @Test
    fun aPlaceBehindARidgeIsOutOfReachWhateverHeightTheGlideWouldArriveWith() {
        // The straight glide would arrive 240 m over the place, and
        // meets the ground 1.8 km from the glider.
        val behind = ReachView(
            distanceM = 3_000.0, bearingDeg = 300.0, arrivalHeightM = 240.0, reach = Reach.BEYOND,
            groundInTheWayM = 1_800.0,
        )
        // No height beside it: the row shows a mountain in red in its place.
        assertNull(ReachWords.beside(behind))
        assertEquals(ReachTone.Red, ReachTone.of(behind.reach))
        assertEquals("Out of reach: the ground is in the way.", ReachWords.spoken(behind))
        assertEquals("Ground in the way", ReachWords.GROUND_IN_THE_WAY)
        // A landing behind a ridge is none to name over the map.
        val landing = LandingOption(index = 0u, name = "Sample landing field", reach = behind, inReach = 0u)
        assertNull(ReachWords.height(landing))
        assertEquals(ReachWords.NO_LANDING, ReachWords.spoken(landing))
    }

    @Test
    fun theSampleFlightHasItsOneLandingWellWithinReachInTheFirstThermal() {
        val folder = TestSupport.newFolder("reach")
        val waypoints = LogbookModel.open(folder, TestSupport.files).makeWaypoints()
        waypoints.importText(LaunchOptions.SAMPLE_WAYPOINT_FILE)
        // What the app hands the cockpit: the waypoints that are places to land.
        val landings = waypoints.all.filter { it.kind == WaypointKind.LANDING }
        assertEquals(listOf("Sample landing field"), landings.map { it.name })

        val session = FlightSession(folder.path)
        val replay = Replay(TestSupport.sampleFlight)
        // On the ground there is nothing to think of yet.
        assertNull(session.landingOption(landings.map { it.place }, replay.now()))

        // Five minutes in, circling a few hundred metres over the take-off.
        replay.advance(session, 300.0)
        val option = checkNotNull(session.landingOption(landings.map { it.place }, replay.now()))
        assertEquals("Sample landing field", option.name)
        assertEquals(Reach.WITHIN, option.reach.reach)
        assertEquals(1u, option.inReach)
        val height = checkNotNull(ReachWords.height(option))
        assertTrue(height, Regex("""\+[\d,]+0 m""").matches(height))
        assertTrue(checkNotNull(option.reach.arrivalHeightM) > 300)

        // The list: every waypoint in its order, the summit sixty
        // kilometres away far out of reach.
        val all = waypoints.all
        val reaches = session.reach(all.map { it.place }, replay.now())
        assertEquals(all.size, reaches.size)
        val summit = reaches[all.indexOfFirst { it.name == "Sample summit far away" }]
        assertEquals(Reach.BEYOND, summit.reach)
        assertEquals("Out of reach.", ReachWords.spoken(summit))
    }

    @Test
    fun theCockpitAsksForTheLandingWhileAFlightRunsAndForgetsItAfter() {
        val folder = TestSupport.newFolder("reach")
        val waypoints = LogbookModel.open(folder, TestSupport.files).makeWaypoints()
        waypoints.importText(LaunchOptions.SAMPLE_WAYPOINT_FILE)
        val sensors = object : FlightSensors {
            override fun start(session: FlightSession, onProblem: (String) -> Unit) = Unit

            override fun stop() = Unit
        }
        val fly = FlySession(folder, MemoryPreferences(), sensors, SilentTone(), scope)
        fly.landings = { waypoints.all.filter { it.kind == WaypointKind.LANDING } }
        assertNull(fly.landing)
        assertTrue(fly.reach(waypoints.all).isEmpty())

        fly.startReplay(TestSupport.sampleFlight, speed = 100.0)
        var waited = 0
        // A few seconds on a machine at rest; a minute's patience for one that is building beside the test.
        while (fly.landing == null && waited < 60_000) {
            Thread.sleep(50)
            waited += 50
        }
        assertEquals("Sample landing field", fly.landing?.name)
        assertEquals(waypoints.all.size, fly.reach(waypoints.all).size)

        fly.finish()
        assertNull(fly.landing)
        assertTrue(fly.reach(waypoints.all).isEmpty())
    }
}
