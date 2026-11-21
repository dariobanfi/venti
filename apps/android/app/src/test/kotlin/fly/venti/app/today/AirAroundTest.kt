package fly.venti.app.today

import fly.venti.core.Flyability
import fly.venti.core.OutlookHour
import fly.venti.core.SiteOutlook
import fly.venti.core.WindSpan
import fly.venti.core.siteOutlook
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The air around and above a take-off, from the real core: a made-up day
 * with an inversion at the ground and a stable layer higher up in every
 * hour, and in one hour gusts near by that are much stronger than at the
 * place. What is much stronger, where a layer lies and every sentence are
 * the core's and tested there; here it is what the screens make of them.
 */
class AirAroundTest {
    /** An hour over ground at 812 m: 3 °C warmer 180 m up, and only 5 °C colder over the 1600 m above 1500 m. */
    private fun hour(utc: Int): String =
        """{"time": "2026-07-04T%02d:00:00Z", "wind_speed_ms": 3.0, "wind_gust_ms": 5.0, "wind_direction_deg": 0.0,
            "precipitation_mm": 0.0, "temperature_c": 2.0, "dew_point_c": -4.0, "winds_aloft": [
            {"above_ground_m": 80.0, "wind_speed_ms": 4.0, "temperature_c": 4.0},
            {"above_ground_m": 180.0, "wind_speed_ms": 4.0, "temperature_c": 5.0},
            {"pressure_hpa": 850.0, "altitude_m": 1500.0, "wind_speed_ms": 4.0, "temperature_c": 2.0},
            {"pressure_hpa": 700.0, "altitude_m": 3100.0, "wind_speed_ms": 4.0, "temperature_c": -3.0},
            {"pressure_hpa": 600.0, "altitude_m": 4300.0, "wind_speed_ms": 4.0, "temperature_c": -12.0}]}""".format(utc)

    /** The neighbourhood of an hour: its strongest wind in the place's own square, its strongest gust 22 km to the south. */
    private fun near(utc: Int, gustKmh: Int): String =
        """{"time": "2026-07-04T%02d:00:00Z", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
            "wind": {"kmh": 14, "at": {"lat": 47.7, "lon": 11.8}, "from_deg": 250, "ground_m": 812},
            "gust": {"kmh": $gustKmh, "at": {"lat": 47.5, "lon": 11.8}, "from_deg": 180, "ground_m": 2400}}""".format(utc)

    private fun answer(nearby: String): String = """{
        "point": {"lat": 47.7, "lon": 11.8},
        "fetched_at": "2026-07-04T05:40:00Z",
        "layers": [{"source": "open-meteo", "series": [
            {"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
             "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.7, "lon": 11.8}, "cell_elevation_m": 812.0,
             "hours": [${(6..17).joinToString(",") { hour(it) }}]}
        ]}],
        $nearby
        "attributions": [{"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                          "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}]
    }"""

    private fun outlook(nearby: String): SiteOutlook = siteOutlook(
        200u,
        answer(nearby).toByteArray(),
        2 * 3600,
        listOf(WindSpan(fromDeg = 337.5, toDeg = 22.5)),
        1550.0,
        1_783_137_600,
    )

    /** Gusts of 55 km/h near by at 12:00 UTC, which is 14:00 at the place, and of 20 km/h in the hour after. */
    private val around = outlook(
        """"nearby": {"source": "open-meteo", "radius_km": 30.0, "cells": 34, "hours": [${near(12, 55)}, ${near(13, 20)}]},""",
    )
    private val alone = outlook("")

    private fun hourAt(outlook: SiteOutlook, clock: String): OutlookHour = outlook.days.first().hours.first { it.hour == clock }

    @Test
    fun theDayHasItsLineAndKeepsItsVerdict() {
        val day = around.days.first()
        assertTrue(day.nearby, day.nearby.startsWith("Much stronger near by at 14:00: gusts to 55 km/h"))
        // It informs: the day with the line is judged as the day without it.
        assertEquals(alone.days.first().verdict, day.verdict)
        assertEquals(Flyability.FLYABLE, day.verdict)
        assertEquals("", alone.days.first().nearby)
        // What the wind near by is and is not, only where the server gave one.
        assertTrue(around.nearbyNote.contains("changes no verdict"))
        assertEquals("", alone.nearbyNote)
    }

    @Test
    fun anHoursDetailHasTheTwoFiguresWithWhereTheyAre() {
        val strong = hourAt(around, "14")
        assertEquals("14:00", AirAroundWords.clock(strong))
        assertEquals(
            listOf(
                AirAroundWords.WIND_NEAR_BY to "14 km/h at the place over ground at 812 m",
                AirAroundWords.GUSTS_NEAR_BY to "55 km/h 20 km to the south over ground at 2400 m",
            ),
            AirAroundWords.figures(strong.nearby),
        )
        // The hour after has its figures and no line: 20 km/h is not much stronger.
        val calm = hourAt(around, "15")
        assertEquals("", calm.nearby?.line)
        assertEquals(2, AirAroundWords.figures(calm.nearby).size)
        // An hour the neighbourhood does not have, and a server without one: no figures.
        assertNull(hourAt(around, "11").nearby)
        assertEquals(emptyList<Pair<String, String>>(), AirAroundWords.figures(hourAt(around, "11").nearby))
        assertEquals(emptyList<Pair<String, String>>(), AirAroundWords.figures(hourAt(alone, "14").nearby))
    }

    @Test
    fun anHourIsReadOutAsItsLineItsLayersAndItsFigures() {
        val strong = hourAt(around, "14")
        assertEquals(listOf(true, false), strong.layers.map { it.inversion })
        val said = AirAroundWords.spoken(strong)
        val line = said.indexOf("Gusts to 55 km/h")
        val inversion = said.indexOf("Inversion between 800 and 1000 m")
        val stable = said.indexOf("Stable layer between 1500 and 3100 m")
        val figure = said.indexOf("Gusts near by: 55 km/h 20 km to the south")
        assertTrue(said, line == 0 && inversion > line && stable > inversion && figure > stable)
        // Without a neighbourhood the layers alone; they are a reason to show the hours.
        assertTrue(AirAroundWords.spoken(hourAt(alone, "14")).startsWith("Inversion between"))
        assertTrue(AirAroundWords.hasDetail(hourAt(alone, "14")))
        assertFalse(AirAroundWords.hasDetail(hourAt(alone, "14").copy(layers = emptyList())))
    }

    @Test
    fun aLayerIsCutWhereThePictureEnds() {
        val day = around.days.first()
        val scale = checkNotNull(AirScale.of(day, 1550.0))
        // The picture of this day reaches from 500 m to 4000 m.
        assertEquals(500.0 to 4000.0, scale.floorM to scale.ceilingM)
        val (inversion, stable) = hourAt(around, "14").layers
        // The inversion at the ground lies within it; a layer reaching above it ends at its head.
        assertEquals(800.0 to 1000.0, scale.within(inversion.bottomM, inversion.topM))
        assertEquals(1500.0 to 3100.0, scale.within(stable.bottomM, stable.topM))
        assertEquals(3000.0 to 4000.0, scale.within(3000.0, 4300.0))
        assertEquals(500.0 to 700.0, scale.within(300.0, 700.0))
        // One that lies over the picture, or under it, is not drawn.
        assertNull(scale.within(4000.0, 4300.0))
        assertNull(scale.within(200.0, 500.0))
    }
}
