package fly.venti.app.wind

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiDark
import app.venti.design.VentiLight
import fly.venti.core.WindBand
import fly.venti.core.WindStation
import fly.venti.core.WindSymbol
import org.maplibre.geojson.Feature
import org.maplibre.geojson.Point

// How a wind station is marked is the core's to decide: which band a speed
// falls in, when wind is calm, when a reading is too old to draw, and how
// mean wind and gusts are written. Every station comes with its mark. What
// is here is how this app draws a mark: the colours and the sizes.
//
// The colours are the design pack's wind scale, one purple from light to
// dark: strength is shown as a magnitude, not as a verdict. The core sorts
// a speed into four bands (below 10, 10 to 20, 20 to 30, 30 km/h and more);
// each takes the pack's step for the middle of its span: the second, fourth,
// sixth and seventh of seven. The pack's own breaks every 5 km/h wait for
// the core to sort by them.

/** Which of the pack's seven steps of wind a band is drawn in, from 0. */
val WindBand.step: Int
    get() = when (this) {
        WindBand.LIGHT -> 1
        WindBand.MODERATE -> 3
        WindBand.FRESH -> 5
        WindBand.STRONG -> 6
    }

/** The band's colour on the map, which is light in both appearances. */
val WindBand.color: Color get() = VentiLight.wind[step]

/** The band's colour as red, green and blue: `0xRRGGBB`. */
val WindBand.rgb: Int get() = color.toArgb() and 0xFFFFFF

/**
 * The band's colour for a table, which follows the phone's appearance: on
 * a dark surface the pack's scale runs the other way, dim for light wind
 * and bright for strong. The map keeps [color].
 */
fun WindBand.ink(dark: Boolean): Color = if (dark) VentiDark.wind[step] else color

/** How large the band's arrow is drawn, relative to the smallest. */
val WindBand.arrowScale: Double
    get() = when (this) {
        WindBand.LIGHT -> 0.85
        WindBand.MODERATE -> 1.0
        WindBand.FRESH -> 1.15
        WindBand.STRONG -> 1.3
    }

/** The name the map knows a symbol's picture by. */
val WindSymbol.picture: String
    get() = when (this) {
        is WindSymbol.Arrow -> "wind-arrow-${band.name.lowercase()}"
        WindSymbol.Calm -> "wind-calm"
        WindSymbol.Old -> "wind-old"
    }

/** How large a symbol is drawn. */
val WindSymbol.scale: Double get() = (this as? WindSymbol.Arrow)?.band?.arrowScale ?: 1.0

/** One station as the map's symbol layer reads it: a place with what to draw there. */
object WindFeature {
    const val ID = "id"
    const val PICTURE = "picture"
    const val HEADING = "heading"
    const val SCALE = "scale"
    const val LABEL = "label"
    const val RANK = "rank"

    fun of(station: WindStation): Feature {
        val mark = station.mark
        return Feature.fromGeometry(Point.fromLngLat(station.position.lon, station.position.lat)).apply {
            addStringProperty(ID, station.id)
            addStringProperty(PICTURE, mark.symbol.picture)
            addNumberProperty(HEADING, mark.headingDeg)
            addNumberProperty(SCALE, mark.symbol.scale)
            addStringProperty(LABEL, mark.label)
            addNumberProperty(RANK, rank(station))
        }
    }

    /**
     * Where a station stands in the queue for room on the map. Lower ranks
     * are placed first: strong wind before weak, and stations with nothing
     * current to say last.
     */
    fun rank(station: WindStation): Double {
        val reading = station.latest
        if (station.mark.symbol == WindSymbol.Old || reading == null) return 1000.0
        return -(reading.gustMs ?: reading.speedMs ?: 0.0)
    }
}
