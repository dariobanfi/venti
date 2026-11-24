package fly.venti.app.forecast

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import fly.venti.core.Flyability
import fly.venti.core.RegionLevel
import fly.venti.app.map.MapSetup
import fly.venti.app.wind.arrowScale
import fly.venti.core.WindBand
import fly.venti.core.ThermalMapCell
import fly.venti.core.ThermalMapKey
import fly.venti.core.ThermalMapScale
import fly.venti.core.ThermalMapShapes
import org.maplibre.android.style.layers.Property
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.LineLayer
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
    const val REGION_ZOOM = 8f
    const val VALLEY_ZOOM = 12f

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

/**
 * The forecast map by the hour: one source of squares, filled by each
 * square's `step` through the colour its line of the key names (the
 * core's `ThermalMapKey`: a scale of the pack and its step), and one of
 * arrows, the wind stations' dart turned where the wind blows, smaller
 * and without the stations' numbers' place, so that they are not taken
 * for stations. The squares lie where the days' squares lie; the arrows
 * under the stations, which stay above.
 */
object ThermalHoursLayers {
    const val SQUARES = "forecast-hours-squares"
    const val ARROWS = "forecast-hours-arrows"

    /** A square's place in the core's grid, and its step of the key. */
    const val CELL = "cell"
    const val STEP = "step"

    /**
     * How large the arrows are against a station's of their band: a little
     * smaller, so that the two are not taken for each other, and larger as
     * the wind is stronger, as the stations' are.
     */
    private const val ARROW_SIZE = 0.8f

    /** The zoom from which the arrows' labels have room beside the squares. */
    private const val LABELS_FROM_ZOOM = 9f

    /** The colour a line of the key names, from the light theme: the map is light in both appearances. */
    fun colour(key: ThermalMapKey): Color = colour(key.scale, key.scaleStep.toInt())

    /** A step of one of the pack's scales, from 1: what a line of a key names, the rain radar's too. */
    fun colour(scale: ThermalMapScale, scaleStep: Int): Color {
        val step = scaleStep - 1
        fun of(scale: List<Color>) = scale[step.coerceIn(0, scale.lastIndex)]
        return when (scale) {
            ThermalMapScale.THERMAL -> of(VentiLight.thermal)
            ThermalMapScale.WIND -> of(VentiLight.wind)
            ThermalMapScale.VARIO -> of(VentiLight.vario)
            ThermalMapScale.VEIL -> ThermalMapLayer.VEIL
        }
    }

    /** Adds the squares' source and layer, empty, where the days' squares lie. */
    fun addSquares(style: Style) {
        style.addSource(GeoJsonSource(SQUARES))
        val layer = FillLayer(SQUARES, SQUARES).withProperties(
            PropertyFactory.fillColor(Expression.color(Color.Transparent.toArgb())),
            PropertyFactory.fillOpacity(
                Expression.interpolate(
                    Expression.linear(),
                    Expression.zoom(),
                    Expression.stop(ThermalMapLayer.REGION_ZOOM, ThermalMapLayer.OPACITY_REGION),
                    Expression.stop(ThermalMapLayer.VALLEY_ZOOM, ThermalMapLayer.OPACITY_VALLEY),
                ),
            ),
            PropertyFactory.fillAntialias(false),
        )
        val days = style.getLayer(ThermalMapLayer.LAYER)
        if (days != null) style.addLayerAbove(layer, days.id) else style.addLayer(layer)
    }

    /**
     * Adds the arrows' source and layer, empty: call it before the wind
     * stations' layer, which then lies above. The pictures are the
     * stations' ("wind-arrow-…", "wind-calm"), added with them.
     */
    fun addArrows(style: Style) {
        style.addSource(GeoJsonSource(ARROWS))
        style.addLayer(
            SymbolLayer(ARROWS, ARROWS).withProperties(
                PropertyFactory.iconImage(
                    Expression.switchCase(
                        Expression.eq(Expression.get("symbol"), Expression.literal("calm")),
                        Expression.literal("wind-calm"),
                        Expression.concat(Expression.literal("wind-arrow-"), Expression.get("band")),
                    ),
                ),
                PropertyFactory.iconRotate(Expression.get("heading")),
                PropertyFactory.iconRotationAlignment(Property.ICON_ROTATION_ALIGNMENT_MAP),
                PropertyFactory.iconSize(
                    Expression.match(
                        Expression.get("band"),
                        Expression.literal(ARROW_SIZE),
                        *WindBand.entries.map { band -> Expression.stop(band.name.lowercase(), Expression.literal(ARROW_SIZE * band.arrowScale.toFloat())) }.toTypedArray(),
                    ),
                ),
                PropertyFactory.iconAllowOverlap(true),
                PropertyFactory.textField(
                    Expression.step(Expression.zoom(), Expression.literal(""), Expression.stop(LABELS_FROM_ZOOM, Expression.get("label"))),
                ),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(10f),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                PropertyFactory.textOffset(arrayOf(0f, 0.9f)),
                PropertyFactory.textColor(MapSetup.ink),
                PropertyFactory.textHaloColor(MapSetup.halo),
                PropertyFactory.textHaloWidth(1.4f),
                PropertyFactory.textOptional(true),
            ),
        )
    }

    /**
     * Hands the two sources an hour's texts, and the squares' layer the
     * colours of the chosen layer's key; empty to take them away. Without
     * `squares` the arrows alone are drawn: the wind and its gusts.
     */
    fun show(shapes: ThermalMapShapes?, legend: List<ThermalMapKey>, style: Style, squares: Boolean = true) {
        // On the wind's layers the arrows stand in place of the squares.
        style.getSourceAs<GeoJsonSource>(SQUARES)?.setGeoJson(shapes?.squares?.takeIf { squares } ?: EMPTY)
        style.getSourceAs<GeoJsonSource>(ARROWS)?.setGeoJson(shapes?.arrows ?: EMPTY)
        val layer = style.getLayerAs<FillLayer>(SQUARES) ?: return
        val stops = legend.map { key -> Expression.stop(key.step.toInt(), Expression.color(colour(key).toArgb())) }
        layer.setProperties(
            PropertyFactory.fillColor(
                if (stops.isEmpty()) {
                    Expression.color(Color.Transparent.toArgb())
                } else {
                    Expression.match(Expression.get(STEP), Expression.color(Color.Transparent.toArgb()), *stops.toTypedArray())
                },
            ),
        )
    }

    private const val EMPTY = """{"type":"FeatureCollection","features":[]}"""
}

/**
 * How the thermal forecast by region is drawn (owner decision 27, the
 * web's `regions.ts`, the iPhone's `RegionLayers`): each region that has a
 * level filled with its level's colour at the level's own opacity
 * (design/DESIGN.md, "Day levels"), the light theme's, the map being light
 * in both appearances; a thin edge in the labels' paper round each; over
 * the grid's squares and under the base map's cable cars, borders and
 * names. The grid's squares under a region with a level are not drawn.
 */
object RegionLayers {
    const val SOURCE = "forecast-regions"
    const val FILL = "forecast-regions"
    const val EDGE = "forecast-regions-edge"

    /** A region's identifier, in the core's outlines. */
    const val ID = "id"

    /** Adds the two layers, empty, over the hours' squares: call it after `ThermalHoursLayers.addSquares`. */
    fun add(style: Style) {
        style.addSource(GeoJsonSource(SOURCE))
        val fill = FillLayer(FILL, SOURCE).withProperties(
            PropertyFactory.fillColor(Expression.color(Color.Transparent.toArgb())),
            PropertyFactory.fillOpacity(0f),
            PropertyFactory.fillAntialias(false),
        )
        val edge = LineLayer(EDGE, SOURCE).withProperties(
            PropertyFactory.lineColor(MapSetup.halo),
            PropertyFactory.lineWidth(
                Expression.interpolate(Expression.linear(), Expression.zoom(), Expression.stop(7f, 0.5f), Expression.stop(11f, 1.5f)),
            ),
            PropertyFactory.lineOpacity(0f),
        )
        val hours = style.getLayer(ThermalHoursLayers.SQUARES)
        if (hours != null) style.addLayerAbove(fill, hours.id) else style.addLayer(fill)
        style.addLayerAbove(edge, FILL)
    }

    /** Sets the regions' outlines, the core's GeoJSON whose features carry their `id`; `null` for none. */
    fun showOutlines(geoJson: String?, style: Style) {
        style.getSourceAs<GeoJsonSource>(SOURCE)?.setGeoJson(geoJson ?: EMPTY)
    }

    /** Fills each region by its level, and leaves out the grid's squares under them; nothing filled and the whole grid for no levels. */
    fun show(levels: Map<String, RegionLevel>, under: Set<Int>, style: Style) {
        style.getLayerAs<FillLayer>(FILL)?.setProperties(
            PropertyFactory.fillColor(byRegion(levels, { Expression.color(it.colourOnMap.toArgb()) }, Expression.color(Color.Transparent.toArgb()))),
            PropertyFactory.fillOpacity(byRegion(levels, { Expression.literal(it.fillOpacityOnMap) }, Expression.literal(0f))),
        )
        style.getLayerAs<LineLayer>(EDGE)?.setProperties(
            PropertyFactory.lineOpacity(byRegion(levels, { Expression.literal(EDGE_OPACITY) }, Expression.literal(0f))),
        )
        val outside = outside(under, ThermalMapLayer.CELL)
        style.getLayerAs<FillLayer>(ThermalMapLayer.LAYER)?.setFilter(outside)
        style.getLayerAs<FillLayer>(ThermalHoursLayers.SQUARES)?.setFilter(outside)
    }

    /** The squares whose place is not among `cells`; every square for none. */
    fun outside(cells: Set<Int>, property: String): Expression =
        if (cells.isEmpty()) {
            Expression.literal(true)
        } else {
            Expression.match(
                Expression.get(property),
                Expression.literal(true),
                *cells.sorted().map { Expression.stop(it, Expression.literal(false)) }.toTypedArray(),
            )
        }

    private fun byRegion(levels: Map<String, RegionLevel>, value: (RegionLevel) -> Expression, otherwise: Expression): Expression {
        if (levels.isEmpty()) return otherwise
        val stops = levels.map { (id, level) -> Expression.stop(id, value(level)) }
        return Expression.match(Expression.get(ID), otherwise, *stops.toTypedArray())
    }

    private const val EDGE_OPACITY = 0.9f
    private const val EMPTY = """{"type":"FeatureCollection","features":[]}"""
}

/**
 * What the map draws of the thermal forecast by region: the regions'
 * outlines, the levels of the day or the hour shown by region, and the
 * squares of the grid under a region with a level, which are not drawn.
 * `id` names the set of regions, the day and the hour: the map draws again
 * only when it changes.
 */
class RegionsDrawing(
    val id: String,
    val regions: fly.venti.core.ForecastRegions,
    val levels: Map<String, RegionLevel>,
    val under: Set<Int>,
)
