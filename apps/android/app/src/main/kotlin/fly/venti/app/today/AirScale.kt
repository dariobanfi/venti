package fly.venti.app.today

import fly.venti.core.OutlookDay
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

/**
 * How heights are laid out in the picture of a day's air: from a floor
 * under the model's ground and the take-off to a ceiling over the thermals,
 * both on a round figure. The picture only places what the core gives it.
 */
data class AirScale(
    /** The lowest height drawn, metres above sea level. */
    val floorM: Double,
    /** The highest. */
    val ceilingM: Double,
) {
    /** How far down a height lies in a picture this tall. */
    fun y(metres: Double, height: Float): Float = (height * (1 - (metres - floorM) / (ceilingM - floorM))).toFloat()

    /** The heights that get a line: every 500 m, or every 1000 m where the picture spans more than 3000 m. */
    val lines: List<Double>
        get() {
            val step = if (ceilingM - floorM > 3000) 1000.0 else 500.0
            val lines = mutableListOf<Double>()
            var line = ceil(floorM / step) * step
            while (line <= ceilingM) {
                lines += line
                line += step
            }
            return lines
        }

    companion object {
        /** The scale for a day at a take-off, or none where the model's ground is not known. */
        fun of(day: OutlookDay, takeoffM: Double?): AirScale? {
            val ground = day.groundM ?: return null
            return of(
                groundM = ground,
                takeoffM = takeoffM,
                tops = day.hours.mapNotNull { it.thermalTopM },
                levels = day.hours.flatMap { hour -> hour.winds.map { it.altitudeM } },
            )
        }

        /**
         * The scale for a model's ground, a take-off where its height is
         * known, how high the thermals of the day's hours go, and the
         * heights the model gives a wind at.
         */
        fun of(groundM: Double, takeoffM: Double?, tops: List<Double>, levels: List<Double> = emptyList()): AirScale {
            val low = min(groundM, takeoffM ?: groundM)
            // Room over the thermals, and a kilometre over the take-off on a
            // day without any; and the model's first level above that
            // kilometre, so that there is always a wind from well above.
            val clear = (takeoffM ?: groundM) + 1000
            val above = levels.filter { it >= clear }.minOrNull() ?: 0.0
            val high = maxOf(tops.maxOrNull() ?: 0.0, clear, above)
            val floor = floor(low / 500) * 500
            return AirScale(floor, max(ceil(high / 500) * 500 + 500, floor + 1500))
        }
    }
}
