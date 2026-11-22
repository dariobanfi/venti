package fly.venti.app.fly

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.CableSet
import fly.venti.core.Coordinate
import fly.venti.core.cockpitOthersPath
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the cockpit is handed of what is around the flight: the cables of
 * its area, the other networks' aircraft every quarter of a minute in the
 * air, and the take-offs around the glider. Against the real core, fed by
 * a replay of the sample flight.
 */
class CockpitAroundTest {
    private val scope = CoroutineScope(Dispatchers.Default)

    @After
    fun stop() {
        scope.cancel()
    }

    private fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

    private class NoSensors : FlightSensors {
        override fun start(session: fly.venti.core.FlightSession, onProblem: (String) -> Unit) = Unit

        override fun stop() = Unit
    }

    @Test
    fun theCockpitAsksForTheOthersAndTheTakeoffsAroundAndWatchesTheCables() {
        val fly = FlySession(TestSupport.newFolder("around"), MemoryPreferences(), NoSensors(), SilentTone(), scope)
        val asked = CopyOnWriteArrayList<String>()
        val othersAnswer = recorded("live_others.json")
        fly.fetch = { path ->
            asked += path
            HttpAnswer(200, othersAnswer)
        }
        val takeoffsAskedAt = CopyOnWriteArrayList<Coordinate>()
        fly.takeoffsAround = { place ->
            takeoffsAskedAt += place
            null
        }
        val cables = CableSet.fromAnswer(200u, recorded("cables.json"))
        var cablesAsked = 0
        fly.cables = {
            cablesAsked += 1
            cables
        }

        fly.startReplay(TestSupport.sampleFlight, speed = 200.0)
        Thread.sleep(2_000)

        // The take-offs are asked for with the first position.
        assertTrue(takeoffsAskedAt.isNotEmpty())
        // In the air, the other networks around the glider, by the core's path.
        val position = checkNotNull(fly.view?.position)
        val path = asked.firstOrNull()
        assertNotNull("the others were not asked for", path)
        assertTrue(path!!, path.startsWith(checkNotNull(cockpitOthersPath(position.lat, position.lon)).substringBefore('?')))
        // What the core keeps of them, with its count in words.
        val near = checkNotNull(fly.othersNear)
        assertTrue(near.aircraft.isNotEmpty())
        assertTrue(near.countWords, near.countWords.isNotEmpty())
        // The cables are handed to the core at the start and asked for again with every refresh.
        assertTrue(cablesAsked > 1)

        fly.finish()
        assertNull(fly.othersNear)
        assertNull(fly.cableWarning)
        assertNull(fly.takeoffs)
        assertEquals(null, fly.view)
    }

    @Test
    fun theClimbsOfOthersAreAskedForAndDrawnOnlyWhileThePilotHasThemOn() {
        val preferences = MemoryPreferences()
        val fly = FlySession(TestSupport.newFolder("climbs"), preferences, NoSensors(), SilentTone(), scope)
        fly.wantsClimbs = { preferences.cockpitShowsClimbs }
        val asked = CopyOnWriteArrayList<String>()
        val climbs = recorded("live_climbs.json")
        val others = recorded("live_others.json")
        fly.fetch = { path ->
            asked += path
            HttpAnswer(200, if ("/v1/live/climbs" in path) climbs else others)
        }

        // Off, as a new phone has it: nothing asked, nothing to draw.
        fly.startReplay(TestSupport.sampleFlight, speed = 200.0)
        Thread.sleep(1_500)
        assertTrue(asked.none { "/v1/live/climbs" in it })
        assertNull(fly.climbsNear)
        fly.finish()

        // On: asked by the core's path around the glider, and the nearest first with the core's line.
        preferences.cockpitShowsClimbs = true
        asked.clear()
        fly.startReplay(TestSupport.sampleFlight, speed = 200.0)
        Thread.sleep(1_500)
        val path = asked.firstOrNull { "/v1/live/climbs" in it }
        assertNotNull("the climbs were not asked for", path)
        assertEquals(1, asked.count { "/v1/live/climbs" in it })
        val near = checkNotNull(fly.climbsNear)
        assertTrue(near.climbs.isNotEmpty())
        assertTrue(near.climbs.first().line, near.climbs.first().line.startsWith("Climb "))
        assertTrue(near.climbs.first().climb.label.startsWith("+"))

        // Switched off in the flight: gone with the next refresh.
        preferences.cockpitShowsClimbs = false
        Thread.sleep(500)
        assertNull(fly.climbsNear)
        fly.finish()
    }

    @Test
    fun aCableSeenIsQuietedInTheCore() {
        val fly = FlySession(TestSupport.newFolder("cable"), MemoryPreferences(), NoSensors(), SilentTone(), scope)
        fly.cables = { CableSet.fromAnswer(200u, recorded("cables.json")) }
        fly.startReplay(TestSupport.sampleFlight, speed = 200.0)
        Thread.sleep(500)
        // Without a cable ahead nothing is warned of, and "Seen" for one that is not there does nothing.
        fly.acknowledgeCable("osm-cables:way/1")
        fly.cableWarning?.let { warning -> assertTrue(warning.words.startsWith("Cable")) }
        fly.finish()
    }

    @Test
    fun theTaskPageIsAmongThePagesOnlyWithATask() {
        // Instruments, Map, Flight, and the task's page last, as on the iPhone.
        assertEquals(
            listOf(CockpitPage.Instruments, CockpitPage.Map, CockpitPage.Flight, CockpitPage.Task),
            CockpitPage.pages(hasTask = true),
        )
        assertEquals(listOf(CockpitPage.Instruments, CockpitPage.Map, CockpitPage.Flight), CockpitPage.pages(hasTask = false))
        assertEquals(CockpitPage.Flight, CockpitPage.Map.next(hasTask = true))
        assertEquals(CockpitPage.Flight, CockpitPage.Map.next(hasTask = false))
        assertEquals(CockpitPage.Task, CockpitPage.Flight.next(hasTask = true))
        assertEquals(CockpitPage.Instruments, CockpitPage.Flight.next(hasTask = false))
        assertEquals(CockpitPage.Instruments, CockpitPage.Task.next(hasTask = true))
        // A swipe the other way goes back, round to the last from the first.
        assertEquals(CockpitPage.Task, CockpitPage.Instruments.previous(hasTask = true))
        assertEquals(CockpitPage.Flight, CockpitPage.Instruments.previous(hasTask = false))
        assertEquals(CockpitPage.Map, CockpitPage.Flight.previous(hasTask = false))
        assertEquals(CockpitPage.Flight, CockpitPage.Task.previous(hasTask = true))
    }
}
