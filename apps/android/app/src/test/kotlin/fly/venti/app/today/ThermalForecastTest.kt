package fly.venti.app.today

import fly.venti.core.Flyability
import fly.venti.core.Thermals
import fly.venti.core.WindSpan
import fly.venti.core.siteOutlook
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What Today shows of thermals, from the real core: a strong day the
 * development server's late-November forecasts never have. The forecast
 * is made up, in the shape of the core's own test of a big day: a summer
 * day over the Tegernsee valley with cumulus at 3400 m.
 */
class ThermalForecastTest {
    private fun hour(local: Int): String {
        val heat = when (local) {
            8, 19 -> 90.0
            9, 18 -> 200.0
            10 -> 300.0
            else -> 400.0
        }
        return """{"time": "2026-07-04T%02d:00:00Z", "wind_speed_ms": 3.0, "wind_gust_ms": 5.0, "wind_direction_deg": 0.0,
            "precipitation_mm": 0.0, "cape_j_kg": 150.0, "convective_cloud_base_m": 3400.0, "temperature_c": 26.0,
            "dew_point_c": 10.0, "heat_to_air_w_m2": $heat, "winds_aloft": [
            {"above_ground_m": 80.0, "wind_speed_ms": 4.0, "temperature_c": 24.0},
            {"above_ground_m": 180.0, "wind_speed_ms": 4.0, "temperature_c": 23.0},
            {"pressure_hpa": 850.0, "altitude_m": 1500.0, "wind_speed_ms": 4.0, "temperature_c": 18.0},
            {"pressure_hpa": 700.0, "altitude_m": 3100.0, "wind_speed_ms": 4.0, "temperature_c": 3.0},
            {"pressure_hpa": 600.0, "altitude_m": 4300.0, "wind_speed_ms": 4.0, "temperature_c": -4.0}]}""".format(local - 2)
    }

    private val madeUpBigDay = """{
        "point": {"lat": 47.7, "lon": 11.8},
        "fetched_at": "2026-07-04T05:40:00Z",
        "layers": [{"source": "open-meteo", "series": [
            {"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
             "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.7, "lon": 11.8}, "cell_elevation_m": 812.0,
             "hours": [${(8..19).joinToString(",") { hour(it) }}]}
        ]}],
        "attributions": [{"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                          "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}]
    }"""

    @Test
    fun aBigDayHasItsWordsItsLineAndItsClimbHourByHour() {
        val outlook = siteOutlook(
            200u,
            madeUpBigDay.toByteArray(),
            2 * 3600,
            listOf(WindSpan(fromDeg = 337.5, toDeg = 22.5)),
            1550.0,
            1_783_137_600,
        )
        val day = outlook.days.first()
        assertEquals(Flyability.FLYABLE, day.verdict)
        assertEquals(Thermals.BIG, day.thermals)
        // What the card's thermal line and the head say, as the core words it.
        assertEquals("Big day", day.thermalWords)
        assertEquals("2.2 m/s to a cloud base of 3400 m, from 11:00 to 18:00", day.thermalLine)
        // Its mark: the fifth step of the scale, from 2 m/s.
        assertEquals(4, ThermalScale.step(checkNotNull(day.climbMs)))
        // Every hour has a climb, so the strip has its two rows of thermals.
        assertTrue(day.hours.all { it.climbMs != null })
        assertTrue(day.hours.first().climbMs!! < day.hours[5].climbMs!!)
    }

    @Test
    fun theScaleStepsAtItsBreaksAndTheBarGrowsWithIt() {
        assertEquals(0, ThermalScale.step(0.0))
        assertEquals(0, ThermalScale.step(0.4))
        assertEquals(1, ThermalScale.step(0.5))
        assertEquals(3, ThermalScale.step(1.8))
        assertEquals(5, ThermalScale.step(3.2))
        assertEquals(2, ThermalScale.barHeight(0))
        assertEquals(12, ThermalScale.barHeight(5))
    }

    @Test
    fun aFlyableDayWithBetterThermalsComesFirst() {
        val order = listOf(null, Thermals.WEAK, Thermals.LOCAL, Thermals.BIG, Thermals.CROSS_COUNTRY)
            .sortedBy(ThermalScale::rank)
        assertEquals(listOf(Thermals.BIG, Thermals.CROSS_COUNTRY, Thermals.LOCAL, null, Thermals.WEAK), order)
    }
}
