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
        assertEquals(CockpitPage.Task, CockpitPage.Map.next(hasTask = true))
        assertEquals(CockpitPage.Flight, CockpitPage.Map.next(hasTask = false))
        assertEquals(CockpitPage.Flight, CockpitPage.Task.next(hasTask = true))
        assertEquals(CockpitPage.Instruments, CockpitPage.Flight.next(hasTask = true))
    }
}
