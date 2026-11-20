package fly.venti.app.design

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.ZoneId
import java.time.ZoneOffset
import java.util.Locale

/** How the core's numbers are set as text, and where a chart's labels go. */
class FormatTest {
    private val locale = Locale.getDefault()

    // Month names follow the phone's language; the tests fix one.
    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
        Format.uses24HourClock = true
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    @Test
    fun numbersAreWrittenTheEnglishWayOnAPhoneSetToGerman() {
        // The core words its own numbers with a point: "10.0 km NE". A phone
        // set to German once wrote "2,5 km" beside it.
        Locale.setDefault(Locale.GERMANY)

        assertEquals("2.5", Format.kilometres(2_500.0))
        assertEquals("3100", Format.metres(3_100.0))
        assertEquals("+2.4", Format.climb(2.4))
        assertEquals("1,745", Format.count(1_745))
        assertEquals("1.4", Format.plain(1.4))
        assertEquals("23%", Format.percent(0.23))
        assertEquals("47.6642° N, 11.7967° E", Format.place(47.6642, 11.7967))
    }

    @Test
    fun formattingUsesMetricUnits() {
        assertEquals("950", Format.metres(950.4))
        assertEquals("2117", Format.metres(2117.0))
        assertEquals("0:29", Format.hoursMinutes(1744))
        assertEquals("5:42", Format.hoursMinutes(5 * 3600 + 42 * 60))
        assertEquals("4:12", Format.minutesSeconds(252))
        assertEquals("0:12:34", Format.stopwatch(754))
        assertEquals("36", Format.kilometresPerHour(10.0))
        assertEquals("36 kilometres per hour", Format.spokenKilometresPerHour(10.0))
        assertEquals("1 kilometre per hour", Format.spokenKilometresPerHour(0.2))
        assertEquals("12.3", Format.kilometres(12_345.0))
        assertEquals("12", Format.wholeKilometres(12_345.0))
        assertEquals("8.4", Format.glideRatio(8.44))
        assertEquals("8.4", Format.neededGlide(8.44))
        assertEquals("99.9", Format.neededGlide(99.94))
        // Level with the place there is no glide that reaches it.
        assertEquals("99+", Format.neededGlide(100.0))
        assertEquals("99+", Format.neededGlide(1_396_016.2))
        assertEquals("17.28", Format.points(17.284))
        assertEquals("24%", Format.percent(0.239))
        assertEquals("1.4", Format.plain(1.4))
        assertEquals("1.2", Format.oneDecimal(1.24))
        assertEquals("3.0", Format.oneDecimal(3.0))
        // 2026-07-04 10:30 UTC.
        assertEquals("Sat, 4 Jul at 10:30", Format.dayAndTime(1_783_161_000, ZoneOffset.UTC))
        assertEquals("47.6642° N, 11.7967° E", Format.place(lat = 47.66421, lon = 11.79668))
        assertEquals("33.9000° S, 70.5000° W", Format.place(lat = -33.9, lon = -70.5))
        assertEquals("1 flight", Format.flights(1u))
        assertEquals("3 flights", Format.flights(3u))
    }

    @Test
    fun gainsAndLossesCarryTheirSignAndZeroCarriesNone() {
        assertEquals("+2.4", Format.climb(2.4))
        assertEquals("+244", Format.signedMetres(244.0))
        // Heights have no mark between the thousands, and a loss the true minus.
        assertEquals("+1140", Format.signedMetres(1140.0))
        assertEquals("−135", Format.signedMetres(-135.0))
        assertEquals("−135", Format.metres(-135.0))
        assertTrue(Format.climb(-1.5).endsWith("1.5"))
        assertFalse(Format.climb(-1.5).startsWith("+"))
        assertEquals("0.0", Format.climb(0.04))
        assertEquals("0.0", Format.climb(-0.04))
        assertEquals("0", Format.signedMetres(-0.3))
    }

    @Test
    fun compassPointsCoverTheCircle() {
        assertEquals(
            listOf("N", "NE", "E", "S", "W", "N"),
            listOf(0.0, 44.0, 90.0, 200.0, 270.0, 359.0).map(Format::compassPoint),
        )
    }

    @Test
    fun aReadingsAgeIsSaidInWords() {
        assertEquals("just now", Format.age(0))
        assertEquals("1 minute ago", Format.age(1))
        assertEquals("45 minutes ago", Format.age(45))
        assertEquals("89 minutes ago", Format.age(89))
        assertEquals("2 hours ago", Format.age(90))
        assertEquals("3 hours ago", Format.age(165))
        assertEquals("36 hours ago", Format.age(36 * 60 - 1))
        assertEquals("2 days ago", Format.age(36 * 60))
        assertEquals("7 days ago", Format.age(7 * 24 * 60))

        assertEquals(45, Format.minutes(from = 1_000, to = 1_000 + 45 * 60 + 59))
        // A reading from a clock that runs ahead is not from the future.
        assertEquals(0, Format.minutes(from = 2_000, to = 1_000))
    }

    @Test
    fun temperaturesWholeNumbersAndListsInASentence() {
        assertEquals("5", Format.temperature(4.5))
        assertEquals("0", Format.temperature(-0.4))
        assertEquals("0", Format.temperature(0.4))
        // A true minus sign, as wide as a plus.
        assertEquals("−7", Format.temperature(-7.1))
        assertEquals("24", Format.whole(24.4))
        assertEquals("1,013", Format.whole(1013.2))

        assertEquals("", Format.list(emptyList()))
        assertEquals("a", Format.list(listOf("a")))
        assertEquals("a and b", Format.list(listOf("a", "b")))
        assertEquals("a, b and c", Format.list(listOf("a", "b", "c")))
    }

    @Test
    fun datesAndTimesAreShownWhereThePhoneIs() {
        // 4 July 2026, 10:30 UTC.
        val moment = 1_783_161_000L
        val munich = ZoneId.of("Europe/Berlin")

        assertEquals("Sat, 4 Jul 2026", Format.day(moment, ZoneOffset.UTC))
        assertEquals("Sat, 4 Jul", Format.dayInYear(moment, ZoneOffset.UTC))
        assertEquals("10:30", Format.time(moment, ZoneOffset.UTC))
        assertEquals("12:30", Format.time(moment, munich))

        Format.uses24HourClock = false
        assertTrue(Format.time(moment + 3 * 3600, ZoneOffset.UTC).startsWith("1:30"))
        assertEquals("1:30", Format.timeOnAxis(moment + 3 * 3600, ZoneOffset.UTC))
        // Over a whole day a time needs its half of the day.
        assertTrue(Format.timeOnDayAxis(moment + 3 * 3600, ZoneOffset.UTC).startsWith("1 "))
        Format.uses24HourClock = true
        assertEquals("13:30", Format.timeOnDayAxis(moment + 3 * 3600, ZoneOffset.UTC))
    }

    @Test
    fun anAxisIsLabelledAtRoundValues() {
        assertEquals(listOf(1000.0, 1500.0, 2000.0), Axis.ticks(900.0, 2200.0, wanted = 3))
        assertEquals(listOf(0.0, 0.5, 1.0), Axis.ticks(0.0, 1.0, wanted = 2))
        assertTrue(Axis.ticks(5.0, 5.0, wanted = 3).isEmpty())

        assertEquals(1.0, Axis.ceiling(0.0), 0.0)
        assertEquals(1.0, Axis.ceiling(0.48), 0.0)
        assertEquals(4.0, Axis.ceiling(3.2), 0.0)
        assertEquals(60.0, Axis.ceiling(41.0), 0.0)
        assertEquals(150.0, Axis.ceiling(118.0), 0.0)
        // The axis ends on one of its own labels.
        assertEquals(150.0, Axis.ticks(0.0, Axis.ceiling(118.0), wanted = 3).last(), 0.0)
    }

    @Test
    fun aTimeAxisIsLabelledAtRoundClockTimes() {
        val start = 1_783_161_000L + 4 // 10:30:04 UTC
        val end = start + 1744

        val ticks = Axis.clockTicks(start, end, most = 5, zone = ZoneOffset.UTC)

        assertEquals(
            listOf("10:35", "10:40", "10:45", "10:50", "10:55"),
            ticks.map { Format.time(it, ZoneOffset.UTC) },
        )
        // A zone off the hour by half is still labelled on its own clock.
        val india = ZoneId.of("Asia/Kolkata")
        assertEquals("16:05", Format.time(Axis.clockTicks(start, end, most = 5, zone = india).first(), india))
        // A day's flight gets hours, not minutes.
        assertEquals(3600L, Axis.clockTicks(start, start + 5 * 3600, most = 5, zone = ZoneOffset.UTC).let { it[1] - it[0] })
        assertTrue(Axis.clockTicks(start, start, most = 5).isEmpty())
    }
}
