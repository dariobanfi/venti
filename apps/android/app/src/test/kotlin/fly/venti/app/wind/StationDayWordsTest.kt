package fly.venti.app.wind

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.core.windMap
import fly.venti.core.stationAir
import fly.venti.core.stationDays
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A date picker's moments and the core's dates, one into the other. */
class StationDayWordsTest {
    @Test
    fun aPickersMomentIsTheDateTheCoreNamesAndBack() {
        // Midnight UTC of 17 November 2026, as Material's picker gives it.
        val millis = 1_794_873_600_000L
        assertEquals("2026-11-17", StationDayWords.date(millis))
        assertEquals(millis, StationDayWords.millis("2026-11-17"))
        // Any moment of that day in UTC is the same date.
        assertEquals("2026-11-17", StationDayWords.date(millis + 23 * 3_600_000L))
    }

    @Test
    fun thePickerOffersTheCoresSpanAYearBackToToday() {
        // 18 November 2026, 19:05 UTC, an hour ahead of UTC.
        val days = stationDays(1_795_028_700L, 3600)
        assertEquals("2026-11-18", days.lastDate)
        assertEquals("2025-11-18", days.firstDate)
        // The picker's days in that span are those strings, compared as text.
        assertEquals(true, StationDayWords.date(StationDayWords.millis(days.firstDate)) >= days.firstDate)
    }

    private fun recorded(name: String) = TestSupport.recordedAnswers.resolve(name).readBytes()

    /** The server's recorded stations, read on its own day, 18 November 2026 at 19:05 UTC. */
    private val stations = windMap(200u, recorded("stations.json"), 1_795_028_700L).stations

    @Test
    fun aChosenDayIsAskedOfTheServerAndReadByTheCore() = runBlocking {
        val asked = mutableListOf<String>()
        val wind = WindModel(
            remembered = null,
            address = "http://127.0.0.1:8080",
            scope = CoroutineScope(Job().apply { cancel() }),
            transport = { url ->
                asked += url
                HttpAnswer(200, recorded("observations.json"))
            },
            utcOffsetSeconds = { 3600 },
        )
        val zugspitze = stations.first { it.name == "Zugspitze" }

        // The day the recording is of: its readings, over the whole day.
        val day = wind.day(zugspitze, "2026-11-18")
        assertTrue(asked.single(), asked.single().contains("observations"))
        assertEquals("2026-11-18", day.date)
        assertEquals(3, day.readings.size)
        assertEquals(86_400L, day.endUnixS - day.startUnixS)
        assertNull(day.emptyWords)
        assertEquals(3, WindSample.of(day.readings).size)

        // A day the answer has nothing of says so in the core's words.
        val empty = wind.day(zugspitze, "2026-11-17")
        assertNotNull(empty.emptyWords)
    }

    @Test
    fun theAirIsReadFromTheStationsOnTheMap() {
        // The valley under the Patscherkofel, after dark: the step, no cloud base.
        val innsbruck = stations.first { it.id == "geosphere-tawes:11320" }
        val air = stationAir(innsbruck, stations)
        assertNull(air.cloudBase)
        val step = checkNotNull(air.temperatureStep)
        assertEquals(
            "Patscherkofel, 8.2 km away and 1673 m higher: −0.60 °C per 100 m",
            StationAirWords.between(step),
        )
    }
}
