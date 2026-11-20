package fly.venti.app.design

import java.text.NumberFormat
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToLong

/**
 * Turns the core's plain numbers into text for the screen.
 *
 * Metric units only for now. Values and units are separate so a screen can
 * set them in different sizes. The names of days and months follow the
 * phone's language; numbers do not.
 */
object Format {
    /**
     * How numbers are written: the English way, with a point before the
     * decimals and a comma between thousands, whatever language the phone
     * is set to. The core words its own numbers that way ("10.0 km NE"),
     * and a screen must not write one number with a comma and the next with
     * a point: on a phone set to German it did. This changes with the app's
     * language, when the app is translated.
     */
    private val numbers: Locale = Locale.ENGLISH

    /**
     * Whether times of day use the 24-hour clock. The app sets it at launch
     * from the phone's own setting, which Java's formatters do not know.
     */
    @Volatile
    var uses24HourClock: Boolean = true

    private fun at(unixSeconds: Long, pattern: String, zone: ZoneId): String =
        DateTimeFormatter.ofPattern(pattern).format(Instant.ofEpochSecond(unixSeconds).atZone(zone))

    /** "Sat, 4 Jul 2026" */
    fun day(unixSeconds: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        at(unixSeconds, "EEE, d MMM yyyy", zone)

    /** "Sat, 4 Jul", for lists already grouped by year. */
    fun dayInYear(unixSeconds: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        at(unixSeconds, "EEE, d MMM", zone)

    /** "10:30" */
    fun time(unixSeconds: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        at(unixSeconds, if (uses24HourClock) "HH:mm" else "h:mm a", zone)

    /** "Sat, 4 Jul at 10:30" */
    fun dayAndTime(unixSeconds: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        "${dayInYear(unixSeconds, zone)} at ${time(unixSeconds, zone)}"

    /** "10:30" on either clock: for chart axes, where the heading already says which half of the day. */
    fun timeOnAxis(unixSeconds: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        at(unixSeconds, if (uses24HourClock) "HH:mm" else "h:mm", zone)

    /**
     * A time on an axis that spans a day, where a time needs its half of
     * the day: "18:00", or "6 PM".
     */
    fun timeOnDayAxis(unixSeconds: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        at(unixSeconds, if (uses24HourClock) "HH:mm" else "h a", zone)

    /** Hours and minutes the way pilots log airtime: "0:29", "5:42". */
    fun hoursMinutes(seconds: Long): String {
        val minutes = (seconds / 60.0).roundToLong()
        return "%d:%02d".format(minutes / 60, minutes % 60)
    }

    /** Minutes and seconds of a short stretch: "4:12". */
    fun minutesSeconds(seconds: Long): String = "%d:%02d".format(seconds / 60, seconds % 60)

    /** Hours, minutes and seconds of a running flight: "0:12:34". */
    fun stopwatch(seconds: Long): String =
        "%d:%02d:%02d".format(seconds / 3600, seconds / 60 % 60, seconds % 60)

    /** Kilometres with one decimal. */
    fun kilometres(metres: Double): String = number(metres / 1000, decimals = 1)

    /** Whole kilometres, for totals. */
    fun wholeKilometres(metres: Double): String = number(metres / 1000, decimals = 0)

    /** Whole metres. */
    fun metres(metres: Double): String = number(metres, decimals = 0)

    /** Whole metres with a sign, for a gain or a loss. */
    fun signedMetres(metres: Double): String = signed(metres, decimals = 0)

    /** Metres per second with one decimal and always a sign, except for zero. */
    fun climb(metresPerSecond: Double): String = signed(metresPerSecond, decimals = 1)

    /** Kilometres per hour, whole, from metres per second. */
    fun kilometresPerHour(metresPerSecond: Double): String = number(metresPerSecond * 3.6, decimals = 0)

    /** A speed as a screen reader says it: "1 kilometre per hour", "12 kilometres per hour". */
    fun spokenKilometresPerHour(metresPerSecond: Double): String {
        val whole = kilometresPerHour(metresPerSecond)
        return if (whole == "1") "1 kilometre per hour" else "$whole kilometres per hour"
    }

    /** A glide ratio with one decimal. */
    fun glideRatio(ratio: Double): String = number(ratio, decimals = 1)

    /**
     * The glide ratio needed to reach a place. Level with the place the
     * ratio has no upper end, and from a hundred on the number says no more
     * than "out of reach": it is shown as "99+".
     */
    fun neededGlide(ratio: Double): String = if (ratio >= 99.95) "99+" else glideRatio(ratio)

    /** Cross-country points, with two decimals. */
    fun points(points: Double): String = number(points, decimals = 2)

    /** A share of a whole as a percentage: "24%". */
    fun percent(share: Double): String = "${number(share * 100, decimals = 0)}%"

    /** A number as it is, with no more decimals than it has: "1.4", "20". */
    fun plain(value: Double): String = NumberFormat.getNumberInstance(numbers).format(value)

    /** A number rounded to a whole one: "24". */
    fun whole(value: Double): String = number(value, decimals = 0)

    /** A number with one decimal: "1.2". */
    fun oneDecimal(value: Double): String = number(value, decimals = 1)

    /** A count: "1,745". */
    fun count(count: Long): String = NumberFormat.getIntegerInstance(numbers).format(count)

    /** A compass direction in one or two letters: "N", "SW". */
    fun compassPoint(degrees: Double): String {
        val points = listOf("N", "NE", "E", "SE", "S", "SW", "W", "NW")
        val index = ((degrees % 360 + 360 + 22.5) / 45).toInt() % 8
        return points[index]
    }

    /**
     * A place in degrees with four decimals, which is to about ten metres:
     * "47.6642° N, 11.7967° E".
     */
    fun place(lat: Double, lon: Double): String =
        "${number(abs(lat), decimals = 4)}° ${if (lat < 0) "S" else "N"}, " +
            "${number(abs(lon), decimals = 4)}° ${if (lon < 0) "W" else "E"}"

    /** Whole degrees Celsius. */
    fun temperature(celsius: Double): String {
        // Minus zero would read "-0".
        val rounded = celsius.roundToLong()
        return if (rounded < 0) "\u2212${number(-rounded.toDouble(), decimals = 0)}" else number(rounded.toDouble(), 0)
    }

    /** Whole minutes between two moments, never negative. */
    fun minutes(from: Long, to: Long): Long = maxOf(0, (to - from) / 60)

    /**
     * How long ago something was measured: "just now", "25 minutes ago",
     * "3 hours ago", "2 days ago".
     */
    fun age(minutes: Long): String = when {
        minutes < 1 -> "just now"
        minutes == 1L -> "1 minute ago"
        minutes < 90 -> "$minutes minutes ago"
        minutes < 36 * 60 -> "${(minutes + 30) / 60} hours ago"
        else -> "${(minutes + 720) / 1440} days ago"
    }

    /** "a", "a and b", "a, b and c": a list in a sentence, in English like the rest of the app's words. */
    fun list(items: List<String>): String = when (items.size) {
        0 -> ""
        1 -> items[0]
        else -> items.dropLast(1).joinToString(", ") + " and " + items.last()
    }

    fun flights(count: UInt): String = if (count == 1u) "1 flight" else "${count(count.toLong())} flights"

    private fun number(value: Double, decimals: Int): String =
        NumberFormat.getNumberInstance(numbers).apply {
            minimumFractionDigits = decimals
            maximumFractionDigits = decimals
        }.format(value)

    /** A plus before a gain and a minus before a loss; nothing before what rounds to zero. */
    private fun signed(value: Double, decimals: Int): String {
        val text = number(abs(value), decimals)
        val zero = number(0.0, decimals)
        return when {
            text == zero -> text
            value > 0 -> "+$text"
            else -> "−$text"
        }
    }
}
