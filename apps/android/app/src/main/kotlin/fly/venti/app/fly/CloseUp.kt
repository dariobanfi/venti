package fly.venti.app.fly

import fly.venti.core.Coordinate
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max

/**
 * The geometry of the thermal view beside the cockpit's map: where a place
 * lies in metres from the glider, how much of the air the view spans, and
 * how long its scale bar is. Which places are shown, the circle and its
 * words are the core's; this only places them.
 */
object CloseUp {
    /** Metres of a degree of latitude, and of longitude at the equator, on a round earth. */
    private const val METRES_PER_DEGREE = 111_195.0

    /** The view is never closer than this across while circling, metres. */
    const val SMALLEST_CIRCLING_SPAN_M = 150.0

    /** How much the view spans on the glide, the glider in its middle, metres. */
    const val GLIDE_SPAN_M = 600.0

    /** Room left round what the view must hold, as a share of its size. */
    private const val MARGIN = 1.15

    /** The lengths the scale bar takes, metres. */
    private val BARS = listOf(10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0)

    /** Where a place lies from the glider: metres east and metres north. Near enough flat for a few kilometres. */
    data class Offset(val eastM: Double, val northM: Double)

    fun offset(from: Coordinate, lat: Double, lon: Double): Offset = Offset(
        eastM = (lon - from.lon) * METRES_PER_DEGREE * cos(Math.toRadians(from.lat)),
        northM = (lat - from.lat) * METRES_PER_DEGREE,
    )

    fun offset(from: Coordinate, to: Coordinate): Offset = offset(from, to.lat, to.lon)

    /**
     * How many metres across the view shows. While circling, as wide as
     * the places it holds need, the glider in the middle, and never closer
     * than [SMALLEST_CIRCLING_SPAN_M]; on the glide, [GLIDE_SPAN_M].
     */
    fun span(circling: Boolean, places: List<Offset>): Double {
        if (!circling) return GLIDE_SPAN_M
        val farthest = places.maxOfOrNull { max(abs(it.eastM), abs(it.northM)) } ?: 0.0
        return max(SMALLEST_CIRCLING_SPAN_M, 2 * farthest * MARGIN)
    }

    /** The scale bar's length for a view this wide: the longest round length up to a quarter of it. */
    fun scaleBar(spanM: Double): Double = BARS.lastOrNull { it <= spanM / 4 } ?: BARS.first()

    /** The scale bar in words: "100 m", "1 km". */
    fun scaleWords(lengthM: Double): String =
        if (lengthM >= 1000) "${(lengthM / 1000).toInt()} km" else "${lengthM.toInt()} m"
}
