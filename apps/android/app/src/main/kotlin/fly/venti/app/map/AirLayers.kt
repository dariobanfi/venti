package fly.venti.app.map

import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import fly.venti.app.design.ClimbBand
import fly.venti.core.ClimbView
import fly.venti.core.FlownThermals
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon

/**
 * What a map draws of the air others found: a mark for each place where
 * pilots climb now, with the core's label ("+2.4") under it, and a
 * rectangle for each cell where the public flights' thermals have been,
 * the stronger the more it weighs. The climbs take the colour of their
 * rate from the pack's scale "Climb and sink", the cells one colour of the
 * scale "Thermals": no colour here is the app's own.
 */
object AirLayers {
    /** The climbs' marks: also the name a test asks the map about. */
    const val CLIMBS = "live-climbs"

    /** The cells of where thermals have been. */
    const val FLOWN = "flown-thermals"

    /** A feature's place in the list it was drawn from. */
    const val INDEX = "index"
    private const val LABEL = "label"
    private const val COLOUR = "colour"
    private const val WEIGHT = "weight"

    /** The pack's scale "Thermals" at its fourth step: strong enough over the relief, not the strongest. */
    private val flownColour = VentiLight.thermal[3].toArgb()

    /** How see-through the lightest and the heaviest cell are: the forecast squares' range. */
    private const val FLOWN_LEAST = 0.2f
    private const val FLOWN_MOST = 0.75f

    private const val CLIMB_RADIUS = 7f

    /** The layer of the cells, empty at first. */
    fun addFlown(style: Style) {
        style.addSource(GeoJsonSource(FLOWN))
        style.addLayer(
            FillLayer(FLOWN, FLOWN).withProperties(
                PropertyFactory.fillColor(flownColour),
                PropertyFactory.fillOpacity(
                    Expression.interpolate(
                        Expression.linear(),
                        Expression.get(WEIGHT),
                        Expression.stop(0, FLOWN_LEAST),
                        Expression.stop(1, FLOWN_MOST),
                    ),
                ),
            ),
        )
    }

    /** The layers of the climbs, empty at first: a dot in the rate's colour, the label under it. */
    fun addClimbs(style: Style) {
        style.addSource(GeoJsonSource(CLIMBS))
        style.addLayer(
            CircleLayer(CLIMBS, CLIMBS).withProperties(
                PropertyFactory.circleRadius(CLIMB_RADIUS),
                PropertyFactory.circleColor(Expression.toColor(Expression.get(COLOUR))),
                PropertyFactory.circleStrokeColor(MapSetup.halo),
                PropertyFactory.circleStrokeWidth(2.5f),
            ),
        )
        style.addLayer(
            SymbolLayer("$CLIMBS-labels", CLIMBS).withProperties(
                PropertyFactory.textField(Expression.get(LABEL)),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(12f),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                PropertyFactory.textOffset(arrayOf(0f, 0.9f)),
                PropertyFactory.textColor(MapSetup.ink),
                PropertyFactory.textHaloColor(MapSetup.halo),
                PropertyFactory.textHaloWidth(1.6f),
                PropertyFactory.textAllowOverlap(true),
            ),
        )
    }

    /** Draws the climbs given; none empties the layer. */
    fun showClimbs(style: Style, climbs: List<ClimbView>) {
        style.getSourceAs<GeoJsonSource>(CLIMBS)?.setGeoJson(FeatureCollection.fromFeatures(climbFeatures(climbs)))
    }

    /** Draws the cells given; `null` empties the layer. */
    fun showFlown(style: Style, flown: FlownThermals?) {
        style.getSourceAs<GeoJsonSource>(FLOWN)?.setGeoJson(FeatureCollection.fromFeatures(flownFeatures(flown)))
    }

    /** A climb as the map's feature: its place, the core's label, the colour of its rate, its place in the list. */
    fun climbFeatures(climbs: List<ClimbView>): List<Feature> = climbs.mapIndexed { index, climb ->
        Feature.fromGeometry(Point.fromLngLat(climb.position.lon, climb.position.lat)).apply {
            addStringProperty(INDEX, index.toString())
            addStringProperty(LABEL, climb.label)
            addStringProperty(COLOUR, colour(climb.climbMs))
        }
    }

    /** A cell as the map's feature: the rectangle of the answer's side around its middle, with its weight. */
    fun flownFeatures(flown: FlownThermals?): List<Feature> {
        val half = (flown?.cellDeg ?: return emptyList()) / 2
        return flown.cells.mapIndexed { index, cell ->
            val (lat, lon) = cell.position.lat to cell.position.lon
            val ring = listOf(
                Point.fromLngLat(lon - half, lat - half),
                Point.fromLngLat(lon + half, lat - half),
                Point.fromLngLat(lon + half, lat + half),
                Point.fromLngLat(lon - half, lat + half),
                Point.fromLngLat(lon - half, lat - half),
            )
            Feature.fromGeometry(Polygon.fromLngLats(listOf(ring))).apply {
                addStringProperty(INDEX, index.toString())
                addNumberProperty(WEIGHT, cell.weight.coerceIn(0.0, 1.0))
            }
        }
    }

    /** The colour of a rate of climb on the map, from the pack's vario scale: "#RRGGBB". */
    fun colour(climbMs: Double): String = "#%06X".format(java.util.Locale.ROOT, ClimbBand.of(climbMs).rgb)
}
