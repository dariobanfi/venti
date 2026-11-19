package fly.venti.app.design

import java.time.Instant
import java.time.ZoneId
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.log10
import kotlin.math.pow

/** Where the labels of a chart's axis go. Android has no chart of its own, so the app draws its two. */
object Axis {
    /**
     * Round values between `low` and `high` to label an axis with: about
     * `wanted` of them, at steps of 1, 2, 2.5 or 5 times a power of ten.
     */
    fun ticks(low: Double, high: Double, wanted: Int): List<Double> {
        if (high <= low || wanted < 1) return emptyList()
        val step = step((high - low) / wanted)
        val first = ceil(low / step - 1e-9).toLong()
        val last = floor(high / step + 1e-9).toLong()
        return (first..last).map { it * step }
    }

    /**
     * A round number at or above `value`, to end an axis on: three or four
     * round steps up from zero. Never less than `least`.
     */
    fun ceiling(value: Double, least: Double = 1.0): Double {
        if (value <= least) return least
        val step = step(value / 3)
        return ceil(value / step - 1e-9) * step
    }

    /** The smallest round step that is at least `rough`. */
    private fun step(rough: Double): Double {
        val magnitude = 10.0.pow(floor(log10(rough)))
        return listOf(1.0, 2.0, 2.5, 5.0, 10.0).map { it * magnitude }.first { it >= rough - 1e-12 }
    }

    /**
     * Round clock times between two moments, as seconds since 1970: at most
     * `most` of them, on whole minutes, quarters or hours of the clock in
     * `zone`.
     */
    fun clockTicks(startUnixS: Long, endUnixS: Long, most: Int, zone: ZoneId = ZoneId.systemDefault()): List<Long> {
        val span = endUnixS - startUnixS
        if (span <= 0 || most < 1) return emptyList()
        val step = CLOCK_STEPS.firstOrNull { span / it <= most } ?: CLOCK_STEPS.last()
        // A round time on the clock is a round number of seconds since
        // local midnight.
        val offset = zone.rules.getOffset(Instant.ofEpochSecond(startUnixS)).totalSeconds
        val first = Math.floorDiv(startUnixS + offset + step - 1, step) * step - offset
        return generateSequence(first) { it + step }.takeWhile { it <= endUnixS }.toList()
    }

    private val CLOCK_STEPS = listOf(60L, 120L, 300L, 600L, 900L, 1800L, 3600L, 7200L, 10_800L, 21_600L, 43_200L)
}
