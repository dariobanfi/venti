package fly.venti.app.airspace

import fly.venti.app.map.MapSetup
import fly.venti.app.map.MapText
import fly.venti.core.AirspacePolicy
import fly.venti.core.AirspaceShape
import fly.venti.core.Coordinate
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
 * How airspace is drawn on a map: an outline in the colour of what it
 * means for a paraglider, a faint wash inside what is closed or needs care,
 * and its name with its limits. The same on the map tab and in the cockpit.
 */
object AirspaceLayers {
    const val SOURCE = "airspace"

    /**
     * The names have a source of their own. Should one of them ever hold a
     * character the map cannot draw, that costs the names and not the
     * outlines, which are what keeps a pilot out.
     */
    private const val NAMES = "airspace-names"

    /** Below this zoom a country's airspace is a tangle, and is not drawn. */
    private const val MINIMUM_ZOOM = 7f

    /** From this zoom on there is room for names. */
    private const val NAMES_FROM_ZOOM = 9f

    private const val ID = "id"
    const val POLICY = "policy"
    const val LABEL = "label"

    /** The colour as red, green and blue: `0xRRGGBB`. */
    fun rgb(policy: AirspacePolicy): Int = when (policy) {
        AirspacePolicy.AVOID -> 0xD55181
        AirspacePolicy.CAUTION -> 0xC98500
        AirspacePolicy.FREE -> 0x85847F
    }

    private fun colour(policy: AirspacePolicy): Int = (0xFF000000 or rgb(policy).toLong()).toInt()

    /** Where a policy stands in the order of drawing: what matters most lies on top. */
    fun rank(policy: AirspacePolicy): Int = when (policy) {
        AirspacePolicy.FREE -> 0
        AirspacePolicy.CAUTION -> 1
        AirspacePolicy.AVOID -> 2
    }

    /** Adds the source and its layers, empty. Called before a map's own layers, so airspace lies lowest. */
    fun add(style: Style) {
        style.addSource(GeoJsonSource(SOURCE))
        // The least restrictive first, so that what matters most lies on top.
        for (policy in listOf(AirspacePolicy.FREE, AirspacePolicy.CAUTION, AirspacePolicy.AVOID)) {
            val only = Expression.eq(Expression.get(POLICY), Expression.literal(rank(policy)))
            if (policy != AirspacePolicy.FREE) {
                val wash = FillLayer("airspace-wash-${rank(policy)}", SOURCE).withProperties(
                    PropertyFactory.fillColor(colour(policy)),
                    PropertyFactory.fillOpacity(if (policy == AirspacePolicy.AVOID) 0.1f else 0.06f),
                )
                wash.setFilter(only)
                wash.minZoom = MINIMUM_ZOOM
                style.addLayer(wash)
            }
            val edge = MapSetup.line(
                "airspace-edge-${rank(policy)}",
                SOURCE,
                colour(policy),
                if (policy == AirspacePolicy.AVOID) 2f else 1.2f,
            )
            edge.setFilter(only)
            edge.minZoom = MINIMUM_ZOOM
            style.addLayer(edge)
        }
        style.addSource(GeoJsonSource(NAMES))
        val names = SymbolLayer(NAMES, NAMES).withProperties(
            PropertyFactory.textField(Expression.get(LABEL)),
            PropertyFactory.textFont(arrayOf("noto-sans-regular")),
            PropertyFactory.textSize(11f),
            PropertyFactory.textColor(0xFF7A1F48.toInt()),
            PropertyFactory.textHaloColor(MapSetup.WHITE),
            PropertyFactory.textHaloWidth(1.4f),
        )
        // Airspace open to a paraglider is drawn, but not named.
        names.setFilter(Expression.gt(Expression.get(POLICY), Expression.literal(0)))
        names.minZoom = NAMES_FROM_ZOOM
        style.addLayer(names)
    }

    /** Draws these airspaces in place of what was drawn before. */
    fun show(shapes: List<AirspaceShape>, style: Style) {
        val features = FeatureCollection.fromFeatures(shapes.map(::feature))
        style.getSourceAs<GeoJsonSource>(SOURCE)?.setGeoJson(features)
        style.getSourceAs<GeoJsonSource>(NAMES)?.setGeoJson(features)
    }

    /** One airspace as the layers read it: its outline with any holes, and what to draw it as. */
    fun feature(shape: AirspaceShape): Feature =
        Feature.fromGeometry(Polygon.fromLngLats(listOf(ring(shape.outline)) + shape.holes.map(::ring))).apply {
            addStringProperty(ID, shape.id)
            addNumberProperty(POLICY, rank(shape.policy))
            addStringProperty(LABEL, label(shape))
        }

    /**
     * The name over the limits, or the limits alone for an airspace without
     * a name, in characters the map's fonts hold: a name comes from the
     * pilot's file and can hold anything.
     */
    fun label(shape: AirspaceShape): String {
        val limits = "${shape.lower} - ${shape.upper}"
        return MapText.shipped(if (shape.name.isEmpty()) limits else "${shape.name}\n$limits")
    }

    /** A ring as GeoJSON wants it: closed, its last point the same as its first. */
    private fun ring(outline: List<Coordinate>): List<Point> {
        val points = outline.map { Point.fromLngLat(it.lon, it.lat) }
        return if (points.size >= 2 && points.first() != points.last()) points + points.first() else points
    }
}
