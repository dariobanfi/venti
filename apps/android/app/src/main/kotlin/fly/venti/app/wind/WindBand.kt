package fly.venti.app.wind

import androidx.compose.ui.graphics.Color
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
// One hue from light to dark: strength is shown as a magnitude, not as a
// verdict. Whether 25 km/h is too much depends on the pilot, the wing and
// the place, and the app does not decide that from one number. The steps
// were checked to be told apart by colour-blind pilots.

/** The band's colour as red, green and blue: `0xRRGGBB`. */
val WindBand.rgb: Int
    get() = when (this) {
        WindBand.LIGHT -> 0x86B6EF
        WindBand.MODERATE -> 0x3987E5
        WindBand.FRESH -> 0x1C5CAB
        WindBand.STRONG -> 0x0D366B
    }

val WindBand.color: Color get() = Color(0xFF000000 or rgb.toLong())

/**
 * The band's colour for a table, which follows the phone's appearance: on
 * a dark surface the scale runs the other way, dim for light wind and
 * bright for strong, so that every step still stands out. Both ramps were
 * checked as ordered ramps against their surface (`docs/design.md`). The
 * map keeps [color]: its arrows have a white edge and lie on a map that is
 * light in both appearances.
 */
fun WindBand.ink(dark: Boolean): Color =
    if (!dark) {
        color
    } else {
        when (this) {
            WindBand.LIGHT -> Color(0xFF256ABF)
            WindBand.MODERATE -> Color(0xFF5598E7)
            WindBand.FRESH -> Color(0xFF9CC4F3)
            WindBand.STRONG -> Color(0xFFE3EEFC)
        }
    }

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
