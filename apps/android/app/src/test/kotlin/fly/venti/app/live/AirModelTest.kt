package fly.venti.app.live

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.map.AirLayers
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import java.io.IOException
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon

/**
 * Climbs now, where thermals have been and the pilots at a site, against
 * the real core, with the server played by the answers the real server's
 * own tests keep.
 */
class AirModelTest {
    private class Server {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var isReachable = true

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReachable) throw IOException("no route to host")
            return when {
                "/v1/live/climbs?" in url -> HttpAnswer(200, recorded("live_climbs.json"))
                "/v1/live/sites?" in url -> HttpAnswer(200, recorded("live_sites.json"))
                "/v1/thermals/flown?" in url -> HttpAnswer(200, recorded("thermals_flown.json"))
                else -> HttpAnswer(404, ByteArray(0))
            }
        }
    }

    private fun model(server: Server, month: Int = 7) =
        AirModel(transport = server::answer, month = { month }).also { it.server = { SERVER } }

    @Test
    fun nothingIsAskedWhileTheLayersAreOff() = runBlocking {
        val server = Server()
        val air = model(server)
        air.loadClimbs(TEGERNSEE)
        air.loadFlown(TEGERNSEE)
        assertTrue(server.asked.isEmpty())
        assertNull(air.climbs)
        assertNull(air.flown)
    }

    @Test
    fun theClimbsOfTheViewAreReadWithTheirWordsAndForgottenWhenSwitchedOff() = runBlocking {
        val server = Server()
        val air = model(server)
        air.climbsOn = true
        air.loadClimbs(TEGERNSEE)
        val climbs = checkNotNull(air.climbs)
        assertEquals("2 climbs in the last 10 minutes", climbs.countWords)
        assertEquals("+1.3", climbs.climbs[0].label)
        assertTrue(climbs.climbs[0].line.startsWith("Climbing now: +1.3 m/s"))
        assertEquals(1, climbs.credits.size)
        assertFalse(air.climbsTooWide)
        // A server that does not answer: a climb is of this moment, and none is kept.
        server.isReachable = false
        air.loadClimbs(TEGERNSEE)
        assertNull(air.climbs)
        server.isReachable = true
        air.loadClimbs(TEGERNSEE)
        assertNotNull(air.climbs)
        air.climbsOn = false
        air.loadClimbs(TEGERNSEE)
        assertNull(air.climbs)
    }

    @Test
    fun aMapThatShowsTooMuchAsksForNeither() = runBlocking {
        val server = Server()
        val air = model(server)
        air.climbsOn = true
        air.flownOn = true
        air.loadClimbs(THE_ALPS)
        air.loadFlown(THE_ALPS)
        assertTrue(server.asked.isEmpty())
        assertTrue(air.climbsTooWide)
        assertTrue(air.flownTooWide)
        assertNull(air.flown)
    }

    @Test
    fun whereThermalsHaveBeenIsAskedForThisMonthAtAnyHour() = runBlocking {
        val server = Server()
        val air = model(server, month = 7)
        air.flownOn = true
        air.loadFlown(TEGERNSEE)
        assertTrue(server.asked.single().contains("month=7"))
        assertFalse(server.asked.single().contains("hours"))
        val flown = checkNotNull(air.flown)
        assertEquals(2, flown.cells.size)
        assertTrue(flown.note.contains("once 3 pilots climbed there"))
        assertFalse(air.flownTooWide)
        // The past does not change by the minute: what was read stays while the server is silent.
        server.isReachable = false
        air.loadFlown(TEGERNSEE)
        assertNotNull(air.flown)
    }

    @Test
    fun aSitesPageSaysWhoFliesThereAndNothingWhereNobodyDoes() = runBlocking {
        val server = Server()
        val air = model(server)
        val sites = fly.venti.core.liveSites(200u, recorded("live_sites.json"))
        val first = sites.sites.first()
        val words = air.siteAir(first.siteId, "Wallberg", Coordinate(47.664, 11.797))
        assertTrue(words, words.contains("in the air at Wallberg") || words.contains("took off in the last hour"))
        assertTrue(server.asked.single().contains("/v1/live/sites?"))
        assertEquals("", air.siteAir("no-such-site", "Nowhere", Coordinate(47.664, 11.797)))
        server.isReachable = false
        assertEquals("", air.siteAir(first.siteId, "Wallberg", Coordinate(47.664, 11.797)))
    }

    @Test
    fun aClimbIsADotWithTheCoresLabelAndACellARectangleOfTheAnswersSide() {
        val climbs = fly.venti.core.liveClimbs(200u, recorded("live_climbs.json"))
        val marks = AirLayers.climbFeatures(climbs.climbs)
        assertEquals(climbs.climbs.size, marks.size)
        assertEquals(climbs.climbs[0].label, marks[0].getStringProperty("label"))
        assertEquals("0", marks[0].getStringProperty(AirLayers.INDEX))
        assertEquals(climbs.climbs[0].position.lat, (marks[0].geometry() as Point).latitude(), 1e-9)
        // The colour of the rate, from the pack's vario scale.
        assertTrue(marks[0].getStringProperty("colour").matches(Regex("#[0-9A-F]{6}")))

        val flown = fly.venti.core.flownThermals(200u, recorded("thermals_flown.json"))
        val cells = AirLayers.flownFeatures(flown)
        assertEquals(flown.cells.size, cells.size)
        val ring = (cells[0].geometry() as Polygon).coordinates().single()
        assertEquals(5, ring.size)
        assertEquals(flown.cellDeg, ring[1].longitude() - ring[0].longitude(), 1e-9)
        assertEquals(flown.cells[0].weight, cells[0].getNumberProperty("weight").toDouble(), 1e-9)
        assertTrue(AirLayers.flownFeatures(null).isEmpty())
    }

    private companion object {
        const val SERVER = "https://venti.example.org"
        val TEGERNSEE = Bounds(south = 47.55, west = 11.6, north = 47.8, east = 11.95)
        val THE_ALPS = Bounds(south = 40.0, west = 0.0, north = 50.0, east = 20.0)

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()
    }
}
