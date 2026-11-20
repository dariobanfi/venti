package fly.venti.app.flight

import fly.venti.app.TestSupport
import fly.venti.core.ComparisonLead
import fly.venti.core.flightAnalysisFromFile
import fly.venti.core.flightFromFile
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

/** Two flights side by side, against the real core: the sample flight and the same flight moved in time. */
class ComparisonTest {
    /** The sample flight with every fix this many seconds later, on the same day. */
    private fun movedBy(seconds: Int): String = TestSupport.sampleFlight.lines().joinToString("\n") { line ->
        if (!line.startsWith("B")) return@joinToString line
        val time = line.substring(1, 7)
        val total = time.substring(0, 2).toInt() * 3600 + time.substring(2, 4).toInt() * 60 + time.substring(4, 6).toInt() + seconds
        "B%02d%02d%02d".format(total / 3600, total / 60 % 60, total % 60) + line.substring(7)
    }

    @Test
    fun flightsInTheAirTogetherAreLaidOverEachOtherByTheClock() = runBlocking {
        val first = flightFromFile(TestSupport.sampleFlight)
        val text = movedBy(600)
        val compared = ComparedFlight.of(
            first, flightAnalysisFromFile(TestSupport.sampleFlight), flightFromFile(text), flightAnalysisFromFile(text), "Mara Muster",
        )
        val comparison = compared.comparison
        assertTrue(comparison.byTheClock)
        assertEquals(600L, comparison.offsetS)
        assertTrue(comparison.words, comparison.words.startsWith("In the air together for"))
        assertTrue(comparison.rows.any { it.label == "Airtime" })
        // The same flight later: neither is ahead in anything.
        assertTrue(comparison.rows.all { it.lead == ComparisonLead.NEITHER })

        // Before the other took off it was not in the air.
        val early = first.track.first { it.seconds < 600u }
        assertNull(compared.point(early.seconds))
        assertNull(compared.words(early))
        // Later it was, ten minutes behind on the same track.
        val later = first.track.first { it.seconds >= 900u }
        val theirs = checkNotNull(compared.point(later.seconds))
        assertTrue(abs(later.seconds.toLong() - 600 - theirs.seconds.toLong()) <= 2)
        val words = checkNotNull(compared.words(later))
        assertTrue(words, Regex("""^[\d,]+ m · [\d,.]+ k?m away$""").matches(words))
        // Where they were nearest is a moment of the flight on screen.
        assertNotNull(compared.nearestIndex(first.track))
    }

    @Test
    fun flightsOfOtherDaysAreEachLaidOutFromTheirFirstMoment() = runBlocking {
        val first = flightFromFile(TestSupport.sampleFlight)
        val other = flightFromFile(TestSupport.sampleFlight.replace("HFDTEDATE:040726", "HFDTEDATE:070726"))
        val compared = ComparedFlight.of(first, null, other, null, CompareWords.own(other.startUnixS))
        assertFalse(compared.comparison.byTheClock)
        assertEquals(0L, compared.comparison.offsetS)
        assertTrue(compared.comparison.words, compared.comparison.words.endsWith("Each is laid out from its first moment."))
        assertNull(compared.nearestIndex(first.track))
        assertTrue(compared.name.startsWith("Your flight of "))
    }

    @Test
    fun distancesAreOverTheGround() {
        // A minute of latitude is a nautical mile.
        assertEquals(1852.0, CompareWords.metresBetween(47.0, 11.0, 47.0 + 1.0 / 60, 11.0), 5.0)
        assertEquals(0.0, CompareWords.metresBetween(47.0, 11.0, 47.0, 11.0), 0.0)
    }
}
