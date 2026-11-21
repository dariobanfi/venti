package fly.venti.app.forecast

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import fly.venti.core.Flyability
import fly.venti.core.ThermalMapCell
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon

/**
 * How the forecast map's squares are drawn: one source of squares and one
 * layer that fills each in a step of the design pack's thermal scale, as
 * the core says, or with a quiet veil where the day is not flyable: the
 * danger colour would look like the scale's darkest step, a big day.
 * See-through, over the ground and its relief and under everything else
 * on the map: the cable cars, the borders and the names of the base map,
 * then airspace, tracks, sites, wind and pilots. The map is light in both
 * appearances, so the light theme's colours are used.
 */
object ThermalMapLayer {
    const val SOURCE = "forecast-squares"
    const val LAYER = "forecast-squares"

    /** What a square is filled with: the core's step of the thermal scale, 1 to 6, or [NOT_FLYABLE]. */
    const val FILL = "fill"

    /** The square's place in the list it was drawn from. */
    const val CELL = "cell"

    /** The fill of a square whose day is not flyable: the veil. */
    const val NOT_FLYABLE = 0

    /**
     * How see-through the squares are, like the airspace's washes: by the
     * zoom. Over a region they are what is looked at; close in, the relief
     * and the names matter more, so they fade.
     */
    const val OPACITY_REGION = 0.5f
    const val OPACITY_VALLEY = 0.3f
    private const val REGION_ZOOM = 8f
    private const val VALLEY_ZOOM = 12f

    /**
     * The first layers of the base map that lie over the squares: the
     * cable cars, which a pilot must not miss, the borders and every name.
     */
    private val OVER = setOf("aerial-aerialway-casing", "aerialway", "boundary")

    /** The fill of a square, or `null` for one that is not drawn: one without an estimate. */
    fun fill(cell: ThermalMapCell): Int? = when {
        cell.verdict == Flyability.NOT_FLYABLE -> NOT_FLYABLE
        else -> cell.scaleStep?.toInt()
    }

    /**
     * The colour of a fill on the map, or of its swatch in the key, from the
     * light theme's tokens: a step of the thermal scale, or for a day that
     * is not flyable the veil, the muted text colour.
     */
    fun colour(fill: Int?): Color = when (fill) {
        null, NOT_FLYABLE -> VEIL
        else -> VentiLight.thermal[(fill - 1).coerceIn(0, VentiLight.thermal.lastIndex)]
    }

    /** The quiet veil over a square whose day is not flyable. */
    val VEIL: Color = VentiLight.textMuted

    /** The squares of a day as the source takes them: those that are drawn, each with its fill. */
    fun features(cells: List<ThermalMapCell>): FeatureCollection =
        FeatureCollection.fromFeatures(
            cells.mapIndexedNotNull { index, cell ->
                fill(cell)?.let { fill ->
                    val corners = listOf(
                        Point.fromLngLat(cell.west, cell.south),
                        Point.fromLngLat(cell.east, cell.south),
                        Point.fromLngLat(cell.east, cell.north),
                        Point.fromLngLat(cell.west, cell.north),
                        Point.fromLngLat(cell.west, cell.south),
                    )
                    Feature.fromGeometry(Polygon.fromLngLats(listOf(corners))).apply {
                        addNumberProperty(FILL, fill)
                        addNumberProperty(CELL, index)
                    }
                }
            },
        )

    /**
     * Adds the source and its layer, empty, under the base map's cable
     * cars, borders and names. Called before the map's own layers, which
     * all lie over it.
     */
    fun add(style: Style) {
        style.addSource(GeoJsonSource(SOURCE))
        val colours = (1..VentiLight.thermal.size).flatMap { step ->
            listOf(Expression.stop(step, Expression.color(colour(step).toArgb())))
        }
        val layer = FillLayer(LAYER, SOURCE).withProperties(
            PropertyFactory.fillColor(
                Expression.match(
                    Expression.get(FILL),
                    Expression.color(Color.Transparent.toArgb()),
                    Expression.stop(NOT_FLYABLE, Expression.color(colour(NOT_FLYABLE).toArgb())),
                    *colours.toTypedArray(),
                ),
            ),
            PropertyFactory.fillOpacity(
                Expression.interpolate(
                    Expression.linear(),
                    Expression.zoom(),
                    Expression.stop(REGION_ZOOM, OPACITY_REGION),
                    Expression.stop(VALLEY_ZOOM, OPACITY_VALLEY),
                ),
            ),
            // Neighbouring squares meet without a seam.
            PropertyFactory.fillAntialias(false),
        )
        val over = style.layers.firstOrNull { it is SymbolLayer || it.id in OVER }
        if (over != null) style.addLayerBelow(layer, over.id) else style.addLayer(layer)
    }

    /** Draws these squares in place of what was drawn before. */
    fun show(cells: List<ThermalMapCell>, style: Style) {
        style.getSourceAs<GeoJsonSource>(SOURCE)?.setGeoJson(features(cells))
    }
}
