package fly.venti.app.forecast

import app.venti.design.VentiLight
import fly.venti.app.HttpAnswer
import fly.venti.core.Bounds
import fly.venti.core.Flyability
import fly.venti.core.thermalMapPath
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
import org.maplibre.geojson.Polygon
import java.io.IOException
import java.util.Locale

/**
 * The forecast map of the map tab against the real core, with the server
 * played by an answer in the shape of `GET /v1/thermals`: three squares in
 * a row for four days, the first of them gone by.
 */
class ThermalMapModelTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    /** The server, and a list of what it was asked. */
    private class Server {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var isReachable = true

        @Volatile
        var hasNoMap = false

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReachable) throw IOException("no route to host")
            return when {
                hasNoMap -> HttpAnswer(404, """{"error":{"code":"not_found","message":"No thermal map."}}""".toByteArray())
                else -> HttpAnswer(200, BODY.toByteArray())
            }
        }
    }

    /** A clock the test moves by hand, in milliseconds since 1970. */
    private class Clock(var now: Long = 1_795_000_000_000) {
        fun pass(minutes: Int) {
            now += minutes * 60_000L
        }
    }

    private fun model(server: Server, clock: Clock = Clock(), address: () -> String? = { BASE }) =
        ThermalMapModel(
            transport = server::answer,
            deviceClock = { clock.now },
            // Two hours east of Greenwich, as in summer in the Alps.
            utcOffsetSeconds = { 7200 },
        ).also {
            it.server = address
            it.clock = { NOON }
        }

    @Test
    fun theMapIsAskedOfTheServerAndItsDaysAreTheCores() = runBlocking {
        val server = Server()
        val forecast = model(server)
        assertEquals(ThermalMapState.Asking, forecast.state)
        forecast.load()

        assertEquals(listOf(BASE + thermalMapPath()), server.asked.toList())
        assertNull(forecast.problem)
        val state = forecast.state as ThermalMapState.There
        // The day before today is left out by the core, and the days are named by the pilot's clock.
        assertEquals(listOf("Today", "Tomorrow", "Tue"), state.map.days.map { it.name })
        assertEquals("Today", state.day.name)
        assertEquals("ICON-D2", state.map.model)
        assertEquals("Forecast: Open-Meteo", forecast.creditLine)
        // The key from the best kind of day down, then the day that is not flyable.
        assertEquals(
            listOf("Big day", "Cross-country day", "Local thermals", "Weak thermals", "Not flyable"),
            state.map.legend.map { it.words },
        )
        assertNull(state.map.legend.last().scaleStep)
        assertTrue(ThermalMapWords.run(state.day).startsWith("ICON-D2, run of "))
    }

    @Test
    fun anotherDayIsChosenByItsDate() = runBlocking {
        val forecast = model(Server())
        forecast.load()
        forecast.choose("2026-07-13")
        assertEquals("Tomorrow", forecast.day?.name)
        // A day the map no longer has gives way to the first it has.
        forecast.choose("2026-07-11")
        assertEquals("Today", forecast.day?.name)
    }

    @Test
    fun theMapIsAskedForAgainOnceItIsHalfAnHourOld() = runBlocking {
        val server = Server()
        val clock = Clock()
        val forecast = model(server, clock)
        assertTrue(forecast.isDue())
        forecast.load()
        assertFalse(forecast.isDue())
        clock.pass(minutes = 29)
        assertFalse(forecast.isDue())
        clock.pass(minutes = 1)
        assertTrue(forecast.isDue())
    }

    @Test
    fun anotherServersMapIsNotThisOnes() = runBlocking {
        var address = BASE
        val forecast = model(Server(), address = { address })
        forecast.load()
        assertNotNull(forecast.map)
        address = "http://10.0.2.2:9090"
        assertTrue(forecast.isDue())
        // Without a server there is nothing to ask and nothing to show.
        val none = model(Server(), address = { null })
        assertFalse(none.isDue())
        none.load()
        assertNull(none.map)
    }

    @Test
    fun aMapFetchedEarlierStaysWhenTheServerCannotBeReached() = runBlocking {
        val server = Server()
        val clock = Clock()
        val forecast = model(server, clock)
        forecast.load()
        val before = forecast.map

        clock.pass(minutes = 31)
        server.isReachable = false
        forecast.load()
        // The map stays, with its run's time; the card shows it as it was.
        assertEquals(before, forecast.map)
        assertTrue(forecast.state is ThermalMapState.There)
        assertTrue(checkNotNull(forecast.problem).startsWith(ThermalMapWords.UNREACHABLE))
        // Not asked again by itself at once, and again after a few minutes.
        assertFalse(forecast.isDue())
        clock.pass(minutes = 5)
        assertTrue(forecast.isDue())
    }

    @Test
    fun withoutAnyMapTheCardSaysWhyInTheCoresWords() = runBlocking {
        val server = Server()
        val clock = Clock()
        val forecast = model(server, clock)

        server.isReachable = false
        forecast.load()
        val unreachable = forecast.state as ThermalMapState.Failed
        assertTrue(unreachable.message, unreachable.message.startsWith("Venti could not be reached for the forecast map."))

        server.isReachable = true
        server.hasNoMap = true
        forecast.load()
        val refused = forecast.state as ThermalMapState.Failed
        // A refusal is said in the core's words, not as an unreachable server.
        assertFalse(refused.message, refused.message.startsWith("Venti could not be reached"))
        assertTrue(refused.message.isNotEmpty())

        // "Try again" asks at once, and a map clears the problem.
        server.hasNoMap = false
        forecast.load()
        assertTrue(forecast.state is ThermalMapState.There)
        assertNull(forecast.problem)
    }

    @Test
    fun theSquaresAreFilledByTheCoresStepOrAsNotFlyable() = runBlocking {
        val forecast = model(Server())
        forecast.load()
        val today = checkNotNull(forecast.day)
        // Today: a cross-country day, and a square that is not flyable
        // whatever its thermals; the third square has no verdict.
        assertEquals(listOf(Flyability.FLYABLE, Flyability.NOT_FLYABLE), today.cells.map { it.verdict })
        val features = ThermalMapLayer.features(today.cells).features().orEmpty()
        assertEquals(listOf(5, ThermalMapLayer.NOT_FLYABLE), features.map { it.getNumberProperty(ThermalMapLayer.FILL).toInt() })
        assertEquals(listOf(0, 1), features.map { it.getNumberProperty(ThermalMapLayer.CELL).toInt() })

        // A square is a closed ring around its cell's point, half a step each way.
        val ring = (features.first().geometry() as Polygon).coordinates().single()
        assertEquals(5, ring.size)
        assertEquals(ring.first(), ring.last())
        assertEquals(10.95, ring.minOf { it.longitude() }, 1e-9)
        assertEquals(11.05, ring.maxOf { it.longitude() }, 1e-9)
        assertEquals(46.95, ring.minOf { it.latitude() }, 1e-9)
        assertEquals(47.05, ring.maxOf { it.latitude() }, 1e-9)

        // Tuesday's third square is flyable without an estimate: it is not drawn.
        forecast.choose("2026-07-14")
        val tuesday = checkNotNull(forecast.day)
        assertEquals(3, tuesday.cells.size)
        assertNull(ThermalMapLayer.fill(tuesday.cells[2]))
        val drawn = ThermalMapLayer.features(tuesday.cells).features().orEmpty()
        assertEquals(listOf(2, 2), drawn.map { it.getNumberProperty(ThermalMapLayer.FILL).toInt() })
        assertEquals(listOf(0, 1), drawn.map { it.getNumberProperty(ThermalMapLayer.CELL).toInt() })
    }

    @Test
    fun theColoursAreThePacksThermalStepsAndAVeilForADayThatIsNotFlyable() = runBlocking {
        val forecast = model(Server())
        forecast.load()
        val map = checkNotNull(forecast.map)
        val swatches = map.legend.map { ThermalMapLayer.colour(it.scaleStep?.toInt() ?: ThermalMapLayer.NOT_FLYABLE) }
        // Not the danger colour, which looks like the scale's darkest step.
        assertEquals(VentiLight.textMuted, swatches.last())
        assertEquals(VentiLight.thermal[5], swatches.first())
        // Never the palest step, which is the colour of the drawn map's land.
        assertFalse(VentiLight.thermal[0] in swatches)
        // Every line of the key has a colour of its own.
        assertEquals(swatches.size, swatches.toSet().size)
    }

    @Test
    fun aSquareSaysItsKindOfDayAndWhereItWasJudged() = runBlocking {
        val forecast = model(Server())
        forecast.load()
        val square = checkNotNull(forecast.day).cells.first()
        assertEquals("Cross-country day", square.thermalWords)
        assertEquals("1.8 m/s to a cloud base of 3250 m", square.thermalLine)
        assertEquals("Judged at the model's ground, 612 m", ThermalMapWords.judgedAt(checkNotNull(square.groundM)))
        // It leads to the forecast for its middle, named by where that is.
        val place = ThermalMapWords.place(square)
        assertEquals(47.0, place.position.lat, 1e-9)
        assertEquals(11.0, place.position.lon, 1e-9)
        assertEquals("47.0000° N, 11.0000° E", place.name)
        assertTrue(checkNotNull(forecast.map).note.isNotEmpty())
        // Today was not judged with all of its daylight hours, and says so.
        val today = checkNotNull(forecast.day)
        assertFalse(today.whole)
        assertTrue(today.note.isNotEmpty())
    }

    @Test
    fun aSquareSaysWhatSpeaksAgainstItsDayWithItsStatus() = runBlocking {
        val forecast = model(Server())
        forecast.load()
        val today = checkNotNull(forecast.day).cells
        // A flyable day has nothing against it.
        assertEquals("", today[0].why)
        assertEquals("Flyable", ThermalMapWords.status(today[0]))
        assertEquals("Showers or thunderstorms can grow", today[1].why)
        assertEquals("Not flyable, Showers or thunderstorms can grow", ThermalMapWords.status(today[1]))
        forecast.choose("2026-07-13")
        assertEquals("Marginal, Gusts", ThermalMapWords.status(checkNotNull(forecast.day).cells[0]))
        // A day the server gives no reasons for has none.
        forecast.choose("2026-07-14")
        assertTrue(checkNotNull(forecast.day).cells.all { it.why.isEmpty() })
    }

    @Test
    fun theCardKnowsWhenThePartInViewHasNoSquares() = runBlocking {
        val forecast = model(Server())
        forecast.load()
        val today = checkNotNull(forecast.day)
        assertTrue(ThermalMapModel.hasSquaresIn(today, Bounds(south = 46.9, west = 10.9, north = 47.1, east = 11.3)))
        assertFalse(ThermalMapModel.hasSquaresIn(today, Bounds(south = 47.6, west = 11.7, north = 47.75, east = 11.95)))
    }

    companion object {
        private const val BASE = "http://10.0.2.2:8080"

        /** Noon on 12 July 2026 by UTC. */
        private const val NOON = 1_783_857_600L

        /** As the core's own tests have it. */
        private val BODY = """
            {
                "source": "open-meteo", "model": "icon-d2",
                "run": "2026-07-12T03:00:00Z", "computed_at": "2026-07-12T04:41:00Z",
                "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 3},
                "ground_m": [612, 1240, null],
                "days": [
                    {"date": "2026-07-11", "whole": true,
                     "verdict": ["flyable", "flyable", "flyable"],
                     "thermals": ["local", "local", "local"],
                     "climb_ms": [0.7, 0.7, 0.7], "top_m": [1800, 1800, 1800],
                     "cloud": [false, false, false]},
                    {"date": "2026-07-12", "whole": false,
                     "verdict": ["flyable", "not_flyable", null],
                     "thermals": ["cross_country", "big", null],
                     "climb_ms": [1.8, 2.4, null], "top_m": [3250, 3400, null],
                     "cloud": [true, false, false],
                     "why": [null, "storms", null]},
                    {"date": "2026-07-13", "whole": true,
                     "verdict": ["marginal", "flyable", "flyable"],
                     "thermals": ["weak", "weak", null],
                     "climb_ms": [0.3, 0.0, null], "top_m": [1800, null, null],
                     "cloud": [false, false, false],
                     "why": ["gusts", null, null]},
                    {"date": "2026-07-14", "whole": true,
                     "verdict": ["flyable", "flyable", "flyable"],
                     "thermals": ["weak", "weak", null],
                     "climb_ms": [0.2, 0.0, null], "top_m": [1500, null, null],
                     "cloud": [false, false, false]}
                ],
                "attributions": [{"source": "open-meteo", "provider": "Open-Meteo",
                    "licence": "CC BY 4.0", "text": "Weather data by Open-Meteo.com",
                    "url": "https://open-meteo.com/"}]
            }
        """.trimIndent()
    }
}
