package fly.venti.app.wind

import androidx.compose.ui.graphics.Color
import fly.venti.app.design.Format
import fly.venti.core.WindStation
import org.maplibre.geojson.Feature
import org.maplibre.geojson.Point

/**
 * How strong the wind is, in four bands of its mean speed.
 *
 * One hue from light to dark: strength is shown as a magnitude, not as a
 * verdict. Whether 25 km/h is too much depends on the pilot, the wing and
 * the place, and the app does not decide that from one number. The steps
 * were checked to be told apart by colour-blind pilots.
 */
enum class WindBand(
    /** The colour as red, green and blue: `0xRRGGBB`. */
    val rgb: Int,
    /** The band in words, for a legend. */
    val label: String,
    /** How large the band's arrow is drawn, relative to the smallest. */
    val arrowScale: Double,
) {
    Light(0x86B6EF, "Below 10 km/h", 0.85),
    Moderate(0x3987E5, "10 to 20 km/h", 1.0),
    Fresh(0x1C5CAB, "20 to 30 km/h", 1.15),
    Strong(0x0D366B, "30 km/h and more", 1.3);

    val color: Color get() = Color(0xFF000000 or rgb.toLong())

    companion object {
        fun of(metresPerSecond: Double): WindBand {
            val kilometresPerHour = metresPerSecond * 3.6
            return when {
                kilometresPerHour < 10 -> Light
                kilometresPerHour < 20 -> Moderate
                kilometresPerHour < 30 -> Fresh
                else -> Strong
            }
        }
    }
}

/** How one station is drawn on the map and read in a list. */
data class WindMark(
    val symbol: Symbol,
    /**
     * Where the arrow points, in degrees clockwise from north: the
     * direction the wind blows towards.
     */
    val heading: Double,
    /**
     * Mean wind and gusts in km/h, "12/25", or just "12" without gusts.
     * Empty for a station with nothing current to say.
     */
    val label: String,
) {
    sealed interface Symbol {
        /** An arrow in the band's colour, pointing where the wind blows. */
        data class Arrow(val band: WindBand) : Symbol

        /** Wind too weak to have a direction worth drawing. */
        data object Calm : Symbol

        /** A reading too old to stand for the wind now, or none at all. */
        data object Old : Symbol
    }

    /** The name the map knows the symbol's picture by. */
    val picture: String
        get() = when (symbol) {
            is Symbol.Arrow -> "wind-arrow-${symbol.band.ordinal}"
            Symbol.Calm -> "wind-calm"
            Symbol.Old -> "wind-old"
        }

    /** How large the symbol is drawn. */
    val scale: Double get() = (symbol as? Symbol.Arrow)?.band?.arrowScale ?: 1.0

    companion object {
        /** Below this the wind has no direction worth an arrow. */
        const val CALM_BELOW_MS = 0.5

        fun of(station: WindStation): WindMark {
            val reading = station.latest
            val speed = reading?.speedMs
            if (reading == null || station.isOld || speed == null) {
                return WindMark(Symbol.Old, heading = 0.0, label = "")
            }
            val label = speeds(speed, reading.gustMs)
            val from = reading.directionDeg
            return if (from != null && speed >= CALM_BELOW_MS) {
                WindMark(Symbol.Arrow(WindBand.of(speed)), heading = (from + 180) % 360, label = label)
            } else {
                WindMark(Symbol.Calm, heading = 0.0, label = label)
            }
        }

        /**
         * "12/25": mean wind and gusts in whole km/h. Gusts are left out
         * when they are not known or round to the same number.
         */
        fun speeds(mean: Double, gust: Double?): String {
            val wind = Format.kilometresPerHour(mean)
            val gusts = gust?.let(Format::kilometresPerHour) ?: return wind
            return if (gusts == wind) wind else "$wind/$gusts"
        }

        /**
         * Where a station stands in the queue for room on the map. Lower
         * ranks are placed first: strong wind before weak, and stations
         * with nothing current to say last.
         */
        fun rank(station: WindStation): Double {
            val reading = station.latest
            if (reading == null || station.isOld || reading.speedMs == null) return 1000.0
            return -(reading.gustMs ?: reading.speedMs ?: 0.0)
        }
    }
}

/** One station as the map's symbol layer reads it: a place with what to draw there. */
object WindFeature {
    const val ID = "id"
    const val PICTURE = "picture"
    const val HEADING = "heading"
    const val SCALE = "scale"
    const val LABEL = "label"
    const val RANK = "rank"

    fun of(station: WindStation): Feature {
        val mark = WindMark.of(station)
        return Feature.fromGeometry(Point.fromLngLat(station.position.lon, station.position.lat)).apply {
            addStringProperty(ID, station.id)
            addStringProperty(PICTURE, mark.picture)
            addNumberProperty(HEADING, mark.heading)
            addNumberProperty(SCALE, mark.scale)
            addStringProperty(LABEL, mark.label)
            addNumberProperty(RANK, WindMark.rank(station))
        }
    }
}
