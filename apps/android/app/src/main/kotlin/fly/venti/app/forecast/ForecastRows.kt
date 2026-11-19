package fly.venti.app.forecast

import fly.venti.app.design.Format
import fly.venti.core.ForecastDay
import fly.venti.core.ForecastHourView
import fly.venti.core.ForecastWind
import fly.venti.core.ModelForecast
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import kotlin.math.roundToInt
import kotlin.math.roundToLong

/** What one hour of one row of the forecast's table holds. */
sealed interface ForecastCell {
    /** A wind: where the air moves, and how fast. */
    data class Wind(val wind: ForecastWind) : ForecastCell

    /** A number, written out. */
    data class Number(val text: String) : ForecastCell

    /** How much of the sky is covered, in percent. */
    data class Cloud(val percent: Double) : ForecastCell

    /** The model has nothing for the hour. */
    data object Empty : ForecastCell
}

/** One row of the table: its name, a line under the name, and what each hour holds. */
data class ForecastRow(val id: String, val name: String, val detail: String, val cells: List<ForecastCell>)

/**
 * How the forecast of one day is laid out as a table, and the words around
 * it. What a model says, which models there are and what a wind's band and
 * label are is the core's; this arranges it for the screen, as the iPhone
 * app does.
 */
object ForecastRows {
    /** What an hour without rain shows: a dot, so that the hours with rain stand out. */
    const val DRY = "·"

    /** The days worth choosing between: those with the sun up in some hour the model has. */
    fun days(model: ModelForecast): List<ForecastDay> = model.days.filter { day -> day.hours.any { it.isDaylight } }

    /** The day that holds this moment, or the first one. */
    fun today(days: List<ForecastDay>, now: Long): ForecastDay? =
        days.firstOrNull { now >= it.startUnixS && now < it.startUnixS + 86_400 } ?: days.firstOrNull()

    /**
     * "Today", or the weekday and the day of the month in short, so that a
     * forecast kept from last week is not taken for this week's.
     */
    fun name(day: ForecastDay, now: Long, zone: ZoneId = ZoneId.systemDefault()): String {
        if (now >= day.startUnixS && now < day.startUnixS + 86_400) return "Today"
        return DateTimeFormatter.ofPattern("EEE d").format(Instant.ofEpochSecond(day.startUnixS + 12 * 3600).atZone(zone))
    }

    /**
     * The hours worth a column: those with the sun up. A day without any,
     * far in the north in winter, shows all it has.
     */
    fun hours(day: ForecastDay): List<ForecastHourView> = day.hours.filter { it.isDaylight }.ifEmpty { day.hours }

    /** The hour of the day on this phone's clock: "9", "14". */
    fun clock(unixSeconds: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        Instant.ofEpochSecond(unixSeconds).atZone(zone).hour.toString()

    /**
     * The rows of the table for these hours: the wind from the highest
     * level down to the ground, then cloud base, cloud, rain, temperature
     * and the height of the 0 °C level, each only if the model has it for
     * some hour.
     */
    fun of(model: ModelForecast, hours: List<ForecastHourView>): List<ForecastRow> {
        val rows = mutableListOf<ForecastRow>()
        for ((index, level) in model.levels.withIndex().reversed()) {
            // An hour without a level is a gap in its row, not a shift.
            val cells = hours.map { hour -> hour.aloft.getOrNull(index)?.let(ForecastCell::Wind) ?: ForecastCell.Empty }
            if (cells.all { it == ForecastCell.Empty }) continue
            val altitude = level.aboutAltitudeM
            rows += if (altitude != null) {
                // A pressure surface is named by about how high it lies.
                ForecastRow(level.name, "${Format.metres(altitude)} m", level.name, cells)
            } else {
                ForecastRow(level.name, level.name.removeSuffix(" above ground"), "above ground", cells)
            }
        }
        rows += ForecastRow("ground", "10 m", "with gusts", hours.map { cell(it.wind, ForecastCell::Wind) })
        val sky = listOf(
            ForecastRow("base", "Cloud base", "m", hours.map { cell(it.cloudBaseM) { base -> number(hundreds(base)) } }),
            ForecastRow("cloud", "Cloud", "% of sky", hours.map { cell(it.cloudPct, ForecastCell::Cloud) }),
            ForecastRow(
                "rain",
                "Rain",
                "mm",
                hours.map { cell(it.precipitationMm) { rain -> number(if (rain < 0.05) DRY else Format.oneDecimal(rain)) } },
            ),
            ForecastRow(
                "temperature",
                "Temperature",
                "°C at 2 m",
                hours.map { cell(it.temperatureC) { temperature -> number(Format.temperature(temperature)) } },
            ),
            ForecastRow(
                "freezing",
                "0 °C level",
                "m",
                hours.map { cell(it.freezingLevelM) { level -> number(hundreds(level)) } },
            ),
        )
        rows += sky.filter { row -> row.cells.any { it != ForecastCell.Empty } }
        return rows
    }

    private fun <T : Any> cell(value: T?, make: (T) -> ForecastCell): ForecastCell = value?.let(make) ?: ForecastCell.Empty

    private fun number(text: String): ForecastCell = ForecastCell.Number(text)

    /**
     * A height to the nearest hundred metres, which is more than a model
     * knows of a cloud base: "2,400".
     */
    private fun hundreds(metres: Double): String = Format.metres((metres / 100).roundToLong() * 100.0)

    /** What a row is called aloud: its name, and the line under it. */
    fun spokenName(row: ForecastRow): String = if (row.detail.isEmpty()) row.name else "${row.name}, ${row.detail}"

    /** A row read out: every third hour, so that it can be listened to. */
    fun spoken(row: ForecastRow, hours: List<ForecastHourView>, zone: ZoneId = ZoneId.systemDefault()): String =
        hours.zip(row.cells)
            .filterIndexed { index, _ -> index % 3 == 0 }
            .mapNotNull { (hour, cell) ->
                val at = "at ${clock(hour.timeUnixS, zone)}"
                when (cell) {
                    is ForecastCell.Wind -> {
                        val from = cell.wind.fromDeg?.let { " from ${Format.compassPoint(it)}" }.orEmpty()
                        "$at ${cell.wind.label.replace("/", " gusting ")}$from"
                    }
                    is ForecastCell.Number -> if (cell.text == DRY) null else "$at ${cell.text}"
                    is ForecastCell.Cloud -> "$at ${cell.percent.roundToInt()} percent"
                    ForecastCell.Empty -> null
                }
            }
            .joinToString(", ")

    /** What the numbers are, and what they are not. */
    fun caveat(model: ModelForecast): String = buildString {
        append("What the weather model ${model.name} computes for the cell of its grid that holds this place")
        if (model.grid.isNotEmpty()) append(" (${model.grid})")
        append(".")
        model.cellElevationM?.let { ground ->
            append(
                " The model's ground there is at ${Format.metres(ground)} m: its wind at 10 m is the wind 10 m " +
                    "above that, which in the mountains is not the wind at a take-off or in the valley.",
            )
        }
        append(
            " The cloud base is that of the model's convective cloud; an hour without one shows none. " +
                "Nothing here says whether the day can be flown.",
        )
    }
}
