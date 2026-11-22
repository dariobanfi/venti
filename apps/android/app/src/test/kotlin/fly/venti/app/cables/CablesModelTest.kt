package fly.venti.app.cables

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.map.lineString
import fly.venti.core.Coordinate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The cables of the maps and the cockpit, fetched and read by the real core. */
class CablesModelTest {
    private val recorded: ByteArray get() = TestSupport.recordedAnswers.resolve("cables.json").readBytes()

    /** Near the Tegernsee, where the server's recorded cables are. */
    private val tegernsee = Coordinate(lat = 47.70, lon = 11.76)

    private class Server(private val answer: () -> HttpAnswer) {
        val asked = mutableListOf<String>()

        suspend fun get(url: String): HttpAnswer {
            asked += url
            return answer()
        }
    }

    @Test
    fun theCablesAroundAPlaceAreFetchedAndReadForTheMaps() = runBlocking {
        val server = Server { HttpAnswer(200, recorded) }
        val cables = CablesModel(background = Dispatchers.Unconfined, transport = server::get)
        cables.server = { "https://venti.test" }
        assertNull(cables.set)
        assertTrue(cables.lines.isEmpty())
        assertNull(cables.creditLine)

        cables.refresh(tegernsee)

        val url = server.asked.single()
        assertTrue(url, url.startsWith("https://venti.test/v1/cables"))
        assertNotNull(cables.set)
        assertEquals(8, cables.lines.size)
        // The core's words, and its sentence on what the layer lacks.
        assertTrue(cables.lines.any { it.words == "Power line, 110 kV, height not mapped" })
        assertTrue(checkNotNull(cables.layer).limits.contains("Not every cable is mapped"))
        assertTrue(checkNotNull(cables.creditLine).contains("OpenStreetMap"))
        val first = cables.lines.first()
        assertEquals(first, cables.line(first.id))
        // Each cable is a line on the map, dashed where its height is not mapped.
        val features = CableLayers.features(cables.lines)
        assertEquals(cables.lines.size, features.size)
        assertEquals(first.id, features.first().getStringProperty(CableLayers.ID))
        assertEquals(first.points.lineString(), features.first().geometry())
    }

    @Test
    fun aPlaceNearTheSetFetchedAsksNothingAndOneFartherAsksAgain() = runBlocking {
        val server = Server { HttpAnswer(200, recorded) }
        val cables = CablesModel(background = Dispatchers.Unconfined, transport = server::get)
        cables.server = { "https://venti.test" }

        cables.refresh(tegernsee)
        cables.refresh(Coordinate(lat = tegernsee.lat + 0.05, lon = tegernsee.lon - 0.05))
        assertEquals(1, server.asked.size)

        cables.refresh(Coordinate(lat = tegernsee.lat + 0.4, lon = tegernsee.lon))
        assertEquals(2, server.asked.size)
    }

    @Test
    fun aServerThatFailsIsSaidAndLeftAloneForAWhile() = runBlocking {
        val server = Server { HttpAnswer(503, ByteArray(0)) }
        var now = 1_000L
        val cables = CablesModel(background = Dispatchers.Unconfined, transport = server::get, now = { now })
        cables.server = { "https://venti.test" }

        cables.refresh(tegernsee)
        assertNull(cables.set)
        assertNotNull(cables.problem)

        cables.refresh(tegernsee)
        assertEquals(1, server.asked.size)
        now += CablesModel.RETRY_AFTER_SECONDS
        cables.refresh(tegernsee)
        assertEquals(2, server.asked.size)
    }

    @Test
    fun withoutAServerNothingIsAsked() = runBlocking {
        val server = Server { HttpAnswer(200, recorded) }
        val cables = CablesModel(background = Dispatchers.Unconfined, transport = server::get)
        cables.refresh(tegernsee)
        assertTrue(server.asked.isEmpty())
        assertNull(cables.set)
    }
}
