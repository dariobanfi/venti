package fly.venti.app.today

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.core.Coordinate
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * The Today tab's take-offs against the real core, with the server played
 * by the answers the real server's own tests keep: the sites east of the
 * Tegernsee, and a forecast.
 */
class TodayModelTest {
    private val tegernsee = Coordinate(lat = 47.6, lon = 11.75)

    private fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

    private class Clock(var now: Long = 1_795_000_000_000)

    private fun model(clock: Clock = Clock(), answer: (String) -> HttpAnswer): TodayModel =
        TodayModel(
            transport = { url -> answer(url) },
            deviceClock = { clock.now },
            background = Dispatchers.Unconfined,
            utcOffsetSeconds = { 3600 },
        ).apply {
            server = { "https://venti.example" }
            // The moment of the recorded forecast's first hour.
            this.clock = { 1_795_000_000 }
            forecastPauseMilliseconds = 0
        }

    private fun recordedServer(url: String): HttpAnswer = when {
        "/v1/sites?" in url -> HttpAnswer(200, recorded("sites.json"))
        "/v1/forecast?" in url -> HttpAnswer(200, recorded("forecast.json"))
        else -> HttpAnswer(404, ByteArray(0))
    }

    @Test
    fun theTakeoffsAroundArePutInOrderWithANameFirst() = runBlocking {
        val today = model(answer = ::recordedServer)
        today.load(tegernsee)

        assertEquals(TodayState.Loaded, today.state)
        // The landings are left out; the named take-off comes before the
        // two without a name, which are nearer.
        val names = today.places.map { it.site.isNamed }
        assertEquals(listOf(true, false, false), names)
        assertTrue(today.places[1].distanceM <= today.places[2].distanceM)
        assertTrue(today.places.all { it.distanceM <= TodayModel.REACH_M })
        // Each with what the forecast says of it, and whose forecast it is.
        assertTrue(today.places.all { it.outlook != null })
        assertTrue(today.credits.isNotEmpty())
    }

    @Test
    fun aTakeoffFurtherThanThirtyKilometresIsNotAmongThem() = runBlocking {
        val today = model(answer = ::recordedServer)
        // Munich: the take-offs east of the Tegernsee are some 50 km away.
        today.load(Coordinate(lat = 48.14, lon = 11.58))

        assertEquals(TodayState.Loaded, today.state)
        assertTrue(today.places.isEmpty())
    }

    @Test
    fun aForecastTheServerIsStillFetchingIsAskedForAgain() = runBlocking {
        val asked = CopyOnWriteArrayList<String>()
        val today = model { url ->
            asked += url
            if ("/v1/forecast?" in url && asked.count { "/v1/forecast?" in it } <= 3) {
                HttpAnswer(503, ByteArray(0))
            } else {
                recordedServer(url)
            }
        }
        today.load(tegernsee)

        assertTrue(today.places.all { it.outlook != null })
    }

    @Test
    fun withoutVentiTheScreenSaysItCouldNotBeReached() = runBlocking {
        val today = model(answer = ::recordedServer)
        today.server = { null }
        today.load(tegernsee)

        assertEquals(TodayState.Failed("Venti could not be reached for today's forecast."), today.state)
        assertTrue(today.places.isEmpty())
    }

    @Test
    fun aServerThatCannotBeReachedIsSaidInWords() = runBlocking {
        val today = model { throw IOException("no route to host") }
        today.load(tegernsee)

        val state = today.state
        assertTrue(state is TodayState.Failed)
        assertTrue((state as TodayState.Failed).message.startsWith("Venti could not be reached"))
    }

    @Test
    fun anAnswerIsKeptForHalfAnHourAndForThePlaceItWasFor() = runBlocking {
        val clock = Clock()
        val today = model(clock, ::recordedServer)
        assertFalse(today.isFresh(tegernsee))
        today.load(tegernsee)
        assertNotNull(today.places.firstOrNull())

        assertTrue(today.isFresh(tegernsee))
        // A few hundred metres away is the same place; ten kilometres is not.
        assertTrue(today.isFresh(Coordinate(lat = 47.602, lon = 11.75)))
        assertFalse(today.isFresh(Coordinate(lat = 47.69, lon = 11.75)))
        // Nor is another server's answer.
        today.server = { "https://other.example" }
        assertFalse(today.isFresh(tegernsee))
        today.server = { "https://venti.example" }
        clock.now += 31 * 60_000L
        assertFalse(today.isFresh(tegernsee))
    }

    @Test
    fun distancesAreMeasuredOnARoundEarth() {
        // One degree of latitude is some 111 km.
        val metres = TodayModel.metresBetween(Coordinate(lat = 47.0, lon = 11.0), Coordinate(lat = 48.0, lon = 11.0))
        assertEquals(111_195.0, metres, 50.0)
    }
}
