package fly.venti.app.forecast

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindModel
import fly.venti.app.wind.ink
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.ForecastView
import fly.venti.core.WindBand
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import java.time.ZoneOffset
import java.util.Locale

/**
 * The forecast of a place: the wind model asks the server, the real core
 * reads the answer, and the table lays it out.
 */
class ForecastTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    private val tegernsee = Coordinate(lat = 47.7, lon = 11.8)

    /** The clock of a summer day in the valley: two hours ahead of UTC. */
    private val summer = ZoneOffset.ofHours(2)

    /** What a server that is not ready, or has no forecasts, answers. */
    private fun refusal(code: String) = """{"error": {"code": "$code", "message": "x"}}""".toByteArray()

    private fun model(asked: MutableList<String> = mutableListOf(), answer: (String) -> HttpAnswer) = WindModel(
        remembered = null,
        address = BASE,
        scope = CoroutineScope(Job().apply { cancel() }),
        transport = { url ->
            asked += url
            answer(url)
        },
        utcOffsetSeconds = { summer.totalSeconds },
    )

    private fun fetched(asked: MutableList<String> = mutableListOf()): ForecastView = runBlocking {
        model(asked) { HttpAnswer(200, CANNED.toByteArray()) }.forecast(tegernsee)
    }

    @Test
    fun theForecastOfAPlaceIsAskedOfTheServerAndReadModelByModel() {
        val asked = mutableListOf<String>()
        val forecast = fetched(asked)

        assertEquals(listOf("$BASE/v1/forecast?lat=47.7&lon=11.8"), asked)
        // The finest model first, whatever order the server gave.
        assertEquals(listOf("ICON-D2", "ICON-EU"), forecast.models.map { it.name })
        assertEquals(listOf("Weather data by Open-Meteo.com"), forecast.credits.map { it.text })
        val caveat = ForecastRows.caveat(forecast.models[0])
        assertTrue(caveat, "812 m" in caveat)
        assertTrue(caveat, "2 km cells" in caveat)
        assertTrue(caveat, caveat.endsWith("Nothing here says whether the day can be flown."))
    }

    @Test
    fun theServersOwnRecordedAnswerIsRead() = runBlocking {
        val recorded = TestSupport.recordedAnswers.resolve("forecast.json").readBytes()
        val forecast = model { HttpAnswer(200, recorded) }.forecast(tegernsee)
        assertEquals(listOf("ICON-D2", "ICON-EU"), forecast.models.map { it.name })
        assertEquals(1069.0, checkNotNull(forecast.models[0].cellElevationM), 0.5)
        assertTrue(forecast.credits.single().text.startsWith("Weather data by Open-Meteo.com"))
        // The recorded answer is one hour of a morning, 07:00 UTC on 21
        // November, with the sun up: one day of one hour.
        val days = ForecastRows.days(forecast.models[0])
        assertEquals(listOf("2026-11-21"), days.map { it.date })
        assertEquals(1, ForecastRows.hours(days.single()).size)
    }

    @Test
    fun aServerWithoutForecastsIsSaidInWords() = runBlocking {
        val wind = model { HttpAnswer(501, refusal("no_forecasts")) }
        try {
            wind.forecast(tegernsee)
            fail("A refusal was read as a forecast")
        } catch (error: CoreException) {
            assertEquals("This server has no forecasts.", error.pilotMessage)
        }
    }

    @Test
    fun aForecastTheServerIsStillFetchingIsAskedForAgain() = runBlocking {
        // "Not yet" twice, then the forecast.
        val asked = CopyOnWriteArrayList<String>()
        val wind = model(asked) {
            if (asked.size < 3) HttpAnswer(503, refusal("unavailable")) else HttpAnswer(200, CANNED.toByteArray())
        }
        wind.forecastPauseMilliseconds = 10
        assertEquals(2, wind.forecast(tegernsee).models.size)
        assertEquals(3, asked.size)

        // A server that never has it is given up on, in words.
        val never = model(asked) { HttpAnswer(503, refusal("unavailable")) }
        never.forecastPauseMilliseconds = 1
        try {
            never.forecast(tegernsee)
            fail("A server that is never ready was read as a forecast")
        } catch (error: CoreException) {
            assertEquals("The server is busy or being restarted. Try again in a moment.", error.pilotMessage)
        }
        assertEquals(3 + WindModel.FORECAST_TRIES, asked.size)
    }

    @Test
    fun theTableHasTheWindFromTheHighestLevelDownAndThenTheSky() {
        val model = fetched().models[0]
        // Every hour of the answer, the one of the night included.
        val hours = model.days.flatMap { it.hours }
        assertEquals(3, hours.size)

        val rows = ForecastRows.of(model, hours)
        assertEquals(
            listOf("700 hPa", "850 hPa", "80 m above ground", "ground", "base", "cloud", "rain", "temperature", "freezing"),
            rows.map { it.id },
        )
        // A pressure surface is named by about how high it lies.
        assertEquals("3100 m", rows[0].name)
        assertEquals("700 hPa", rows[0].detail)
        assertEquals("80 m", rows[2].name)
        assertEquals("above ground", rows[2].detail)
        assertEquals("3100 m, 700 hPa", ForecastRows.spokenName(rows[0]))

        // An hour without a level is a gap in its row, not a shift.
        val high = (rows[0].cells[0] as ForecastCell.Wind).wind
        assertEquals("34", high.label)
        assertEquals(WindBand.STRONG, high.band)
        assertEquals(ForecastCell.Empty, rows[0].cells[1])
        assertTrue(rows.all { it.cells.size == 3 })

        // The ground wind carries its gusts.
        assertEquals("9/25", (rows[3].cells[0] as ForecastCell.Wind).wind.label)

        assertEquals(listOf(ForecastCell.Number("2400"), ForecastCell.Empty, ForecastCell.Empty), rows[4].cells)
        assertEquals(listOf(ForecastCell.Cloud(30.0), ForecastCell.Cloud(80.0), ForecastCell.Empty), rows[5].cells)
        // A dry hour is a dot, so that the wet ones stand out.
        assertEquals(
            listOf(ForecastCell.Number(ForecastRows.DRY), ForecastCell.Number("1.2"), ForecastCell.Empty),
            rows[6].cells,
        )
        assertEquals(listOf(ForecastCell.Number("24"), ForecastCell.Number("25"), ForecastCell.Empty), rows[7].cells)
        assertEquals(listOf(ForecastCell.Number("3900"), ForecastCell.Empty, ForecastCell.Empty), rows[8].cells)
    }

    @Test
    fun theThermalsHaveTwoRowsLastWhereSomeHourHasAnEstimate() {
        val model = fetched().models[0]
        val recorded = model.days.flatMap { it.hours }
        // The recorded November hours have nothing to climb in: no rows, no note.
        assertFalse(ForecastRows.hasThermals(recorded))
        assertFalse(ForecastRows.of(model, recorded).any { it.id.startsWith("thermal") })

        // The core's estimate for the hours: a climb under cumulus, one in the blue, none.
        val hours = listOf(
            recorded[0].copy(thermalClimbMs = 1.8, thermalTopM = 3250.0, thermalCloud = true),
            recorded[1].copy(thermalClimbMs = 0.4, thermalTopM = 1800.0, thermalCloud = false),
            recorded[2].copy(thermalClimbMs = null, thermalTopM = null, thermalCloud = false),
        )
        assertTrue(ForecastRows.hasThermals(hours))
        val rows = ForecastRows.of(model, hours)
        assertEquals(listOf("thermal-climb", "thermal-top"), rows.takeLast(2).map { it.id })
        val (climb, top) = rows.takeLast(2)
        assertEquals("Thermals, climb", climb.name)
        assertEquals("m/s", climb.detail)
        assertEquals(listOf(ForecastCell.Number("1.8"), ForecastCell.Number("0.4"), ForecastCell.Empty), climb.cells)
        assertEquals("Thermals, top", top.name)
        // "base" under a top that is the cloud base.
        assertEquals(
            listOf(ForecastCell.Number("3250", under = "base"), ForecastCell.Number("1800"), ForecastCell.Empty),
            top.cells,
        )
        assertTrue(ForecastRows.spoken(top, hours, java.time.ZoneOffset.UTC).contains("3250, base"))
        assertTrue(ForecastRows.THERMALS_NOTE.contains("not a value of the model"))
    }

    @Test
    fun aModelWithOnlyAGroundWindHasOnlyThatRow() {
        val coarse = fetched().models[1]
        assertEquals(listOf("ground"), ForecastRows.of(coarse, coarse.days.flatMap { it.hours }).map { it.id })
    }

    @Test
    fun onlyTheHoursOfDaylightGetAColumnUnlessThereAreNone() {
        val model = fetched().models[0]
        // By the valley's clock the answer's last hour is one in the morning of the next day.
        assertEquals(2, model.days.size)
        val days = ForecastRows.days(model)
        assertEquals(listOf(model.days[0]), days)

        val day = days.single()
        val light = ForecastRows.hours(day)
        assertEquals(2, light.size)
        assertTrue(light.all { it.isDaylight })
        // Noon and one o'clock by the valley's clock.
        assertEquals(listOf("12", "13"), light.map { ForecastRows.clock(it.timeUnixS, summer) })

        // A night has no hour with the sun up, and shows all it has.
        val night = model.days[1]
        assertFalse(night.hours.any { it.isDaylight })
        assertEquals(night.hours, ForecastRows.hours(night))
    }

    @Test
    fun aForecastFromASquareOpensOnTheMapsDayFromTheMapsModel() {
        val (d2, eu) = fetched().models
        // Both models have the 4th of July; only the coarser one has the 5th.
        val euFifth = eu.days.first().copy(startUnixS = eu.days.first().startUnixS + 86_400, date = "2026-07-05")
        val models = listOf(d2, eu.copy(days = eu.days + euFifth))
        assertEquals("2026-07-04", d2.days.first().date)

        // The named model has the day: it is that model's.
        val named = ForecastRows.opening(models, ForecastOpening("2026-07-04", "ICON-EU"))
        assertEquals("ICON-EU", named?.first?.name)
        assertEquals("2026-07-04", named?.second?.date)

        // The named model lacks it, or there is no model of that name: the finest that has it.
        val finest = ForecastRows.opening(models, ForecastOpening("2026-07-04", "AROME"))
        assertEquals("ICON-D2", finest?.first?.name)
        val coarser = ForecastRows.opening(models, ForecastOpening("2026-07-05", "ICON-D2"))
        assertEquals("ICON-EU", coarser?.first?.name)
        assertEquals(euFifth.startUnixS, coarser?.second?.startUnixS)

        // Nobody has it: the forecast opens as from anywhere else.
        assertEquals(null, ForecastRows.opening(models, ForecastOpening("2026-07-09", "ICON-EU")))
    }

    @Test
    fun aDayIsCalledTodayOnlyWhileItIs() {
        val days = ForecastRows.days(fetched().models[0])
        val day = days.single()
        val noon = day.startUnixS + 12 * 3600

        assertEquals("Today", ForecastRows.name(day, now = noon, zone = summer))
        // A forecast looked at the week after is not taken for that week's.
        assertEquals("Sat 4", ForecastRows.name(day, now = noon + 7 * 86_400, zone = summer))

        assertEquals(day, ForecastRows.today(days, now = noon))
        // With no day that holds this moment, the first one.
        assertEquals(day, ForecastRows.today(days, now = noon + 7 * 86_400))
        assertEquals(null, ForecastRows.today(emptyList(), now = noon))
    }

    @Test
    fun aRowIsReadOutEveryThirdHour() {
        val model = fetched().models[0]
        val hours = model.days.flatMap { it.hours }
        val rows = ForecastRows.of(model, hours).associateBy { it.id }

        assertEquals("at 12 9 gusting 25 from W", ForecastRows.spoken(rows.getValue("ground"), hours, summer))
        assertEquals("at 12 34 from W", ForecastRows.spoken(rows.getValue("700 hPa"), hours, summer))
        assertEquals("at 12 30 percent", ForecastRows.spoken(rows.getValue("cloud"), hours, summer))
        // A dry hour is not worth saying.
        assertEquals("", ForecastRows.spoken(rows.getValue("rain"), hours, summer))
    }

    @Test
    fun onADarkSurfaceTheWindScaleRunsFromDimToBright() {
        val light = WindBand.entries.map { it.ink(dark = false) }
        val dark = WindBand.entries.map { it.ink(dark = true) }
        assertEquals(4, dark.toSet().size)
        WindBand.entries.forEachIndexed { index, _ -> assertNotEquals(light[index], dark[index]) }
        // Stronger wind is darker on a light surface and brighter on a dark one.
        fun brightness(colour: androidx.compose.ui.graphics.Color) = colour.red + colour.green + colour.blue
        assertEquals(light.sortedByDescending(::brightness), light)
        assertEquals(dark.sortedBy(::brightness), dark)
    }

    private companion object {
        const val BASE = "http://127.0.0.1:8080"

        /**
         * A summer day's answer for the Tegernsee valley in the server's
         * own format: two models, the coarser one first, as the server may
         * list them.
         */
        val CANNED = """
            {"point": {"lat": 47.7, "lon": 11.8}, "fetched_at": "2026-07-04T09:40:00Z",
             "layers": [{"source": "open-meteo", "series": [
               {"source": "open-meteo", "model": "icon-eu", "run": "2026-07-04T06:00:00Z",
                "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.6875, "lon": 11.8125},
                "cell_elevation_m": 905.0,
                "hours": [{"time": "2026-07-04T10:00:00Z", "wind_speed_ms": 3.0}]},
               {"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T06:00:00Z",
                "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.7, "lon": 11.8},
                "cell_elevation_m": 812.0,
                "hours": [
                  {"time": "2026-07-04T10:00:00Z", "wind_speed_ms": 2.5, "wind_gust_ms": 6.9,
                   "wind_direction_deg": 250.0, "temperature_c": 24.3, "cloud_cover_pct": 30.0,
                   "precipitation_mm": 0.0, "freezing_level_m": 3900.0, "convective_cloud_base_m": 2440.0,
                   "winds_aloft": [
                     {"above_ground_m": 80.0, "wind_speed_ms": 3.6, "wind_direction_deg": 255.0},
                     {"pressure_hpa": 850.0, "altitude_m": 1532.0, "wind_speed_ms": 5.6, "wind_direction_deg": 270.0},
                     {"pressure_hpa": 700.0, "altitude_m": 3140.0, "wind_speed_ms": 9.4, "wind_direction_deg": 290.0}
                   ]},
                  {"time": "2026-07-04T11:00:00Z", "wind_speed_ms": 3.0, "wind_direction_deg": 260.0,
                   "temperature_c": 25.4, "cloud_cover_pct": 80.0, "precipitation_mm": 1.24,
                   "winds_aloft": [
                     {"pressure_hpa": 850.0, "altitude_m": 1548.0, "wind_speed_ms": 4.0}
                   ]},
                  {"time": "2026-07-04T23:00:00Z", "wind_speed_ms": 0.8, "wind_direction_deg": 180.0}
                ]}
             ]}],
             "attributions": [{"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                               "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}]}
        """.trimIndent()
    }
}
