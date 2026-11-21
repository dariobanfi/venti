package fly.venti.app.task

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.map.MapStyle
import fly.venti.app.terrain.TerrainModel
import fly.venti.core.Coordinate
import java.time.ZoneId
import java.util.Locale
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * The weather along a drawn route against the real core, with the server
 * played by the development mode's answer for the Wallberg, a place
 * south-east of it and the Brauneck on 22 November 2026, as the server's
 * own tests keep it, and the ground from the region that comes with the app.
 */
class RouteWeatherTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    private val berlin = ZoneId.of("Europe/Berlin")

    private val places = listOf(
        Coordinate(47.6642, 11.7967),
        Coordinate(47.62, 11.91),
        Coordinate(47.6603, 11.5197),
    )

    private class Server {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var refuses = false

        suspend fun answer(url: String): HttpAnswer {
            asked += url.removePrefix(BASE)
            if (refuses) return HttpAnswer(503, ByteArray(0))
            return HttpAnswer(200, TestSupport.recordedAnswers.resolve("route_weather.json").readBytes())
        }
    }

    private fun model(server: Server, ground: TerrainModel? = null) =
        RouteWeatherModel(
            transport = { url -> server.answer(url) },
            ground = ground?.let { terrain -> { area -> terrain.covering(area) } },
        ).also { it.server = { BASE } }

    @Test
    fun theForecastIsAskedOnceForARouteAndADay() = runBlocking {
        val server = Server()
        val weather = model(server)
        weather.show(places, DAY)
        assertEquals(listOf("/v1/route/weather?date=2026-11-22&points=47.6642,11.7967;47.62,11.91;47.6603,11.5197"), server.asked)
        assertNotNull(weather.forecast)
        assertFalse(weather.isAsking)
        assertNull(weather.problem)

        // The same route and day again, as after a later start: the answer held serves.
        weather.show(places, DAY)
        assertEquals(1, server.asked.size)

        val along = weather.along(places, NINE_UTC, speedKmh = 20, glideRatio = 8.0, zone = berlin)
        assertEquals(
            "The route takes 1 h 58 at 20 km/h, from 10:00 to 11:58. Point 3 lies outside the forecast's map. " +
                "Nothing in the forecast rules out an hour on the way.",
            along.words,
        )
        assertTrue(along.points[0].words, along.points[0].words.startsWith("Point 1 at 10:00: "))
        assertEquals("ICON-D2, run of 07:00", along.model)
        assertEquals(1, along.credits.size)
        // The speed is the core's to read again: nothing is asked for it.
        val faster = weather.along(places, NINE_UTC, speedKmh = 30, glideRatio = 8.0, zone = berlin)
        assertTrue(faster.words, faster.words.contains("at 30 km/h"))
        assertEquals(1, server.asked.size)

        // Another day is another question; and a shorter route too.
        weather.show(places, "2026-11-23")
        assertEquals(2, server.asked.size)
        weather.show(places.take(2), "2026-11-23")
        assertEquals(3, server.asked.size)
        // A route of one place asks nothing and holds nothing.
        weather.show(places.take(1), DAY)
        assertEquals(3, server.asked.size)
        assertNull(weather.forecast)
        weather.close()
    }

    @Test
    fun withoutAnAnswerTheTimesAndTheStillAirAreSaid() = runBlocking {
        val server = Server().apply { refuses = true }
        val weather = model(server)
        weather.show(places, DAY)
        assertNull(weather.forecast)
        assertNotNull(weather.problem)
        val along = weather.along(places, NINE_UTC, speedKmh = 20, glideRatio = 8.0, zone = berlin)
        assertTrue(along.words, along.words.contains("No forecast along it"))
        assertEquals("Point 1 at 10:00.", along.points[0].words)
        // Without the ground the height to leave at is not said, and why.
        assertTrue(along.points[0].departureWords, along.points[0].departureWords.contains("is not loaded"))

        // "Try again" asks anew, and an answer clears the problem.
        server.refuses = false
        weather.forgetForecast()
        weather.show(places, DAY)
        assertEquals(2, server.asked.size)
        assertNotNull(weather.forecast)
        assertNull(weather.problem)
        weather.close()

        // Without a server nothing is asked, and the reason is said.
        val alone = RouteWeatherModel(transport = { url -> server.answer(url) })
        alone.show(places, DAY)
        assertEquals(RouteWeatherWords.NO_SERVER, alone.problem)
        assertEquals(2, server.asked.size)
    }

    @Test
    fun overTheGroundThatCameWithTheAppAPlaceSaysHowHighToLeaveIt() = runBlocking {
        val archive = TestSupport.assets.resolve("MapDemo").resolve(MapStyle.BUNDLED_TERRAIN)
        val terrain = TerrainModel(source = { TerrainModel.Source.Archive(archive) }, folder = null)
        // From the Wallberg's top to Rottach-Egern on the lake, 4 km north.
        val short = listOf(Coordinate(47.6642, 11.7967), Coordinate(47.6960, 11.7700))
        val weather = model(Server(), terrain)
        weather.show(short, DAY)
        assertNotNull(weather.heights)
        val along = weather.along(short, NINE_UTC, speedKmh = 20, glideRatio = 8.0, zone = berlin)
        val first = along.points[0]
        assertNotNull(first.departureM)
        assertTrue(first.departureWords, first.departureWords.startsWith("Leave Point 1 at "))
        assertTrue(along.points[1].departureWords.isEmpty())
        weather.close()
        assertNull(weather.heights)
    }

    @Test
    fun theSettingsMoveByTheirStepsAndStayWithinTheirBounds() {
        // 09:20 UTC is 10:20 in Berlin in November: the next full hour is 11:00.
        assertEquals(11 * 60, RouteWeatherWords.nextFullHour(NINE_UTC + 20 * 60, berlin))
        // Late in the evening the last hour of the day.
        assertEquals(23 * 60, RouteWeatherWords.nextFullHour(NINE_UTC + 13 * 3600 + 30 * 60, berlin))
        assertEquals(NINE_UTC, RouteWeatherWords.startUnixS(DAY, 10 * 60, berlin))
        assertEquals(DAY, RouteWeatherWords.date(NINE_UTC, berlin))
        assertEquals("10:30", RouteWeatherWords.clock(RouteWeatherWords.stepStart(10 * 60, 1)))
        assertEquals(0, RouteWeatherWords.stepStart(0, -1))
        assertEquals(23 * 60 + 30, RouteWeatherWords.stepStart(23 * 60 + 30, 1))
        assertEquals(25, RouteWeatherWords.stepSpeed(20, 1))
        assertEquals(5, RouteWeatherWords.stepSpeed(5, -1))
        assertEquals(80, RouteWeatherWords.stepSpeed(80, 1))
        assertEquals(8.5, RouteWeatherWords.stepGlide(8.0, 1), 1e-9)
        assertEquals(4.0, RouteWeatherWords.stepGlide(4.0, -1), 1e-9)
        assertEquals(14.0, RouteWeatherWords.stepGlide(14.0, 1), 1e-9)
        // The ground is asked for a little wider than the places reach.
        val area = RouteWeatherWords.around(places)
        assertTrue(area.south < 47.62 && area.north > 47.6642 && area.west < 11.5197 && area.east > 11.91)
    }

    companion object {
        const val BASE = "http://venti.test"
        const val DAY = "2026-11-22"

        /** 22 November 2026, 09:00 UTC: 10:00 on a clock of Central Europe. */
        const val NINE_UTC = 1_795_338_000L
    }
}
