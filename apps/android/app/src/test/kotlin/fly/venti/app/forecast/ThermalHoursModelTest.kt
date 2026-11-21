package fly.venti.app.forecast

import fly.venti.app.HttpAnswer
import fly.venti.core.ThermalHoursLayer
import fly.venti.core.thermalHoursPath
import fly.venti.core.thermalMapPath
import java.net.URI
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The forecast map by the hour against the real core, with the server
 * played by answers in the shape of `GET /v1/thermals` and
 * `GET /v1/thermals/hours`: a grid of squares, two days, eleven hours a
 * day from 06 to 16 UTC, on a clock two hours ahead of UTC at noon UTC of
 * the first day.
 */
class ThermalHoursModelTest {
    /** The server, and a list of what it was asked. */
    private class Server(val rows: Int = 1, val columns: Int = 3) {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var refusesHours = false

        /** The hour of the morning the model's run began that the map is of. */
        @Volatile
        var run = 3

        /** What was asked for hours, without the server's address. */
        val askedForHours: List<String> get() = asked.filter { "/hours" in it }.map { it.removePrefix(BASE) }

        fun answer(url: String): HttpAnswer {
            asked += url
            val address = URI(url)
            if (!address.path.endsWith("/hours")) return HttpAnswer(200, map(rows, columns, run).toByteArray())
            if (refusesHours) return HttpAnswer(503, ByteArray(0))
            val query = address.query.split("&").associate { it.substringBefore("=") to it.substringAfter("=") }
            return HttpAnswer(
                200,
                hours(checkNotNull(query["date"]), checkNotNull(query["layer"]), query["height"]?.toInt(), rows, columns).toByteArray(),
            )
        }
    }

    private fun model(server: Server) =
        ThermalMapModel(
            transport = server::answer,
            // Two hours east of Greenwich, as in summer in the Alps.
            utcOffsetSeconds = { 7200 },
        ).also {
            it.server = { BASE }
            it.clock = { NOON }
        }

    private fun view(forecast: ThermalMapModel) = (forecast.hours.state as ThermalHoursState.There).view

    @Test
    fun thermalsOpenOnTheDayAsAWhole() = runBlocking {
        val server = Server()
        val forecast = model(server)
        forecast.load()
        // The map names heights, so there are hours to ask for and layers to choose.
        assertEquals(listOf("Thermals", "Wind", "Gusts", "Cloud", "Rain"), checkNotNull(forecast.map).layers.map { it.name })
        forecast.showHours()

        assertEquals(listOf(thermalHoursPath(TODAY, ThermalHoursLayer.THERMALS, null)), server.askedForHours)
        val hours = forecast.hours
        // "Day" first: the hours are read, and the map still shows the day's squares.
        assertNull(hours.hour)
        assertFalse(hours.isOn)
        assertNull(hours.shapes)
        assertEquals(11, view(forecast).hours.size)
        assertEquals("Thermals, Today", ThermalMapWords.said(view(forecast), hours.hour, checkNotNull(forecast.day)))

        // An hour of the line: the hour's squares, and the words say it by the pilot's clock.
        hours.hour = 5
        assertTrue(hours.isOn)
        assertTrue(checkNotNull(hours.shapes).squares.contains("\"cell\":0"))
        assertEquals("Thermals, 13:00", ThermalMapWords.said(view(forecast), hours.hour, checkNotNull(forecast.day)))
        // And back to the day.
        hours.hour = null
        assertFalse(hours.isOn)
        assertNull(hours.shapes)
    }

    @Test
    fun theWindAtTheGroundAndTheGustsAreOneAnswer() = runBlocking {
        val server = Server()
        val forecast = model(server)
        forecast.load()
        val hours = forecast.hours
        hours.layer = ThermalHoursLayer.WIND
        forecast.showHours()
        assertEquals("Wind at the ground", view(forecast).title)
        // Without an hour chosen, a layer opens at the hour the core names: today the hour that has begun.
        assertEquals(view(forecast).firstHour.toInt(), hours.hour)
        assertEquals("14:00", view(forecast).hours[checkNotNull(hours.hour)].label)
        val windArrows = checkNotNull(hours.shapes).arrows

        hours.layer = ThermalHoursLayer.GUSTS
        forecast.showHours()
        assertEquals("Gusts at the ground", view(forecast).title)
        // Asked once, read twice: the gusts come with the wind at the ground.
        assertEquals(listOf(thermalHoursPath(TODAY, ThermalHoursLayer.WIND, null)), server.askedForHours)
        assertEquals(1 to 2, hours.kept)
        // The gusts' arrows carry the gusts, not the wind's "speed/gusts".
        assertFalse(windArrows == checkNotNull(hours.shapes).arrows)

        // The wind at a height is another answer; back at the ground nothing is asked again.
        hours.layer = ThermalHoursLayer.WIND
        hours.heightM = 2000
        forecast.showHours()
        assertEquals("Wind at 2000 m", view(forecast).title)
        hours.heightM = null
        forecast.showHours()
        assertEquals(
            listOf(thermalHoursPath(TODAY, ThermalHoursLayer.WIND, null), thermalHoursPath(TODAY, ThermalHoursLayer.WIND, 2000)),
            server.askedForHours,
        )
        assertEquals(2 to 3, hours.kept)
    }

    @Test
    fun theHourChosenStaysAcrossAChangeOfLayerAndNotOfDay() = runBlocking {
        val server = Server()
        val forecast = model(server)
        forecast.load()
        val hours = forecast.hours
        forecast.showHours()
        hours.hour = 3

        // A layer asked for the first time, one kept, and a height: the hour stays.
        for (layer in listOf(ThermalHoursLayer.WIND, ThermalHoursLayer.CLOUD, ThermalHoursLayer.RAIN, ThermalHoursLayer.WIND)) {
            hours.layer = layer
            forecast.showHours()
            assertEquals(layer.name, 3, hours.hour)
            assertTrue(hours.isOn)
        }
        hours.heightM = 1000
        forecast.showHours()
        assertEquals(3, hours.hour)
        assertEquals("Wind at 1000 m, 11:00", ThermalMapWords.said(view(forecast), hours.hour, checkNotNull(forecast.day)))
        // Back on "Thermals" it is that hour too, not the day.
        hours.layer = ThermalHoursLayer.THERMALS
        forecast.showHours()
        assertEquals(3, hours.hour)

        // Another day opens by itself: "Thermals" on the day as a whole,
        // the others at the hour nearest one o'clock.
        forecast.choose(TOMORROW)
        forecast.showHours()
        assertEquals(TOMORROW, view(forecast).date)
        assertNull(hours.hour)
        hours.layer = ThermalHoursLayer.CLOUD
        forecast.showHours()
        assertEquals("13:00", view(forecast).hours[checkNotNull(hours.hour)].label)

        // "Day" chosen on "Thermals" is no hour: the next layer opens at the hour the core names.
        forecast.choose(TODAY)
        hours.layer = ThermalHoursLayer.THERMALS
        forecast.showHours()
        hours.hour = 2
        hours.hour = null
        hours.layer = ThermalHoursLayer.RAIN
        forecast.showHours()
        assertEquals(view(forecast).firstHour.toInt(), hours.hour)
    }

    @Test
    fun aRefusalFallsBackToTheDayAndTryAgainAsksAnew() = runBlocking {
        val server = Server()
        val forecast = model(server)
        forecast.load()
        val hours = forecast.hours
        server.refusesHours = true
        forecast.showHours()
        val failed = hours.state as ThermalHoursState.Failed
        // The core's words for a server that is busy, not "not reached".
        assertEquals("The server is busy or being restarted. Try again in a moment.", failed.message)
        assertFalse(hours.isOn)
        assertNull(hours.shapes)
        assertEquals(0 to 0, hours.kept)

        server.refusesHours = false
        forecast.showHours()
        assertTrue(hours.state is ThermalHoursState.There)
        assertEquals(2, server.askedForHours.size)
    }

    @Test
    fun theCoresObjectsAreClosedWhenTheyAreLetGo() = runBlocking<Unit> {
        val server = Server()
        val forecast = model(server)
        forecast.load()
        val hours = forecast.hours
        forecast.showHours()
        hours.layer = ThermalHoursLayer.CLOUD
        forecast.showHours()
        val cloud = (hours.state as ThermalHoursState.There).hours
        assertEquals(2 to 2, hours.kept)
        assertNotNull(cloud.view())

        // The layer is switched off, or the server changed.
        hours.close()
        assertEquals(0 to 0, hours.kept)
        assertSame(ThermalHoursState.Days, hours.state)
        assertNull(hours.shapes)
        // The core's object is gone: it answers no more.
        assertThrows(IllegalStateException::class.java) { cloud.view() }

        // The same map fetched again is the map at hand: what was kept for it stays.
        forecast.showHours()
        val before = (hours.state as ThermalHoursState.There).hours
        forecast.load()
        forecast.showHours()
        assertSame(before, (hours.state as ThermalHoursState.There).hours)
        assertEquals(1, server.askedForHours.count { "cloud" in it } - 1)
        // A new map of the days, of a later run, lets go of what belonged to the one before.
        server.run = 6
        forecast.load()
        forecast.showHours()
        assertEquals(1 to 1, hours.kept)
        assertThrows(IllegalStateException::class.java) { before.view() }
        assertNotNull((hours.state as ThermalHoursState.There).hours.view())
    }

    /**
     * How long the core takes for the shapes of an hour, on the machine
     * the tests run on: the wind at the ground with its arrows, for a
     * region of 1,800 squares. The figure is printed, for the report; the
     * test only holds that a step is no wait.
     */
    @Test
    fun aStepOfAnHourIsNoWait() = runBlocking {
        val server = Server(rows = 30, columns = 60)
        val forecast = model(server)
        forecast.load()
        val hours = forecast.hours
        hours.layer = ThermalHoursLayer.WIND
        forecast.showHours()
        val steps = view(forecast).hours.indices.map { index ->
            hours.hour = index
            assertTrue(checkNotNull(hours.shapes).arrows.length > 100_000)
            hours.lastStepMillis
        }
        println("A step of an hour, 1800 squares with arrows, core of the build machine: ${steps.joinToString()} ms")
        assertTrue("a step took ${steps.max()} ms", steps.max() < 500)
    }

    companion object {
        private const val BASE = "http://10.0.2.2:8080"

        /** Noon on 12 July 2026 by UTC. */
        private const val NOON = 1_783_857_600L
        private const val TODAY = "2026-07-12"
        private const val TOMORROW = "2026-07-13"

        private const val CREDIT = """[{"source": "open-meteo", "provider": "Open-Meteo",
            "licence": "CC BY 4.0", "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}]"""

        private fun grid(rows: Int, columns: Int) =
            """{"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": $rows, "columns": $columns}"""

        /** A list with a value for each square. */
        private fun each(cells: Int, value: (Int) -> Any?) = (0 until cells).joinToString(",", "[", "]") { value(it).toString() }

        /** The map of the days, which names the heights its hours have the wind at. */
        private fun map(rows: Int, columns: Int, run: Int): String {
            val cells = rows * columns
            val days = listOf(TODAY, TOMORROW).joinToString(",") { date ->
                """{"date": "$date", "whole": true,
                    "verdict": ${each(cells) { "\"flyable\"" }}, "thermals": ${each(cells) { "\"local\"" }},
                    "climb_ms": ${each(cells) { 0.8 }}, "top_m": ${each(cells) { 2100 }}, "cloud": ${each(cells) { false }}}"""
            }
            return """{"source": "open-meteo", "model": "icon-d2",
                "run": "${TODAY}T0$run:00:00Z", "computed_at": "${TODAY}T0${run + 1}:41:00Z",
                "grid": ${grid(rows, columns)}, "ground_m": ${each(cells) { 612 }},
                "heights_m": [1000, 2000], "days": [$days], "attributions": $CREDIT}"""
        }

        /** One layer of the hours of a day, as `tools/made-up-day/serve.py` answers them. */
        private fun hours(date: String, layer: String, height: Int?, rows: Int, columns: Int): String {
            val cells = rows * columns
            val hours = (6..16).joinToString(",") { at ->
                val lists = when (layer) {
                    "thermals" -> """"verdict": ${each(cells) { "\"flyable\"" }}, "why": ${each(cells) { null }},
                        "thermals": ${each(cells) { "\"local\"" }}, "climb_ms": ${each(cells) { 0.8 }},
                        "top_m": ${each(cells) { 2100 }}, "cloud": ${each(cells) { false }}"""
                    "wind" -> """"wind_kmh": ${each(cells) { 5 + (it + at) % 30 }},
                        "wind_from_deg": ${each(cells) { (it * 10 + at * 20) % 360 }}""" +
                        if (height == null) """, "gust_kmh": ${each(cells) { 14 + (it + at) % 30 }}""" else ""
                    "cloud" -> """"cloud_cover_pct": ${each(cells) { (it * 7 + at * 5) % 101 }}"""
                    else -> """"precipitation_mm": ${each(cells) { (it + at) % 4 * 0.5 }}"""
                }
                """{"time": "${date}T${at.toString().padStart(2, '0')}:00:00Z", $lists}"""
            }
            return """{"source": "open-meteo", "date": "$date", "layer": "$layer", "heights_m": [1000, 2000],
                "model": "icon-d2", "run": "${TODAY}T03:00:00Z", "whole": true, "computed_at": "${TODAY}T04:41:00Z",
                "grid": ${grid(rows, columns)}, "ground_m": ${each(cells) { 612 }},
                "hours": [$hours], "attributions": $CREDIT${height?.let { """, "height_m": $it""" } ?: ""}}"""
        }
    }
}
