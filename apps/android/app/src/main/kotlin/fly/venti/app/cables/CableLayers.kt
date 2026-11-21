package fly.venti.app.cables

import android.graphics.RectF
import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import fly.venti.app.map.MapSetup
import fly.venti.app.map.lineString
import fly.venti.core.CableLine
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection

/**
 * How cables are drawn on a map: a line in the pack's danger colour over
 * the dark line casing, so that it stands out on the aerial view as on the
 * drawn map; solid where the cable's height is mapped, dashed where it is
 * not. From zoom [FROM_ZOOM], where a line is a place and not a tangle.
 * The same on the map tab and in the cockpit.
 */
object CableLayers {
    const val SOURCE = "cables"
    private const val CASING = "cables-casing"
    const val SOLID = "cables-line"
    const val DASHED = "cables-dashed"

    /** The property a feature carries its cable's identifier in. */
    const val ID = "id"
    private const val MAPPED = "mapped"

    /** The zoom from which cables are drawn. */
    const val FROM_ZOOM = 12f

    /** The pack's danger colour, from the light theme: the map is light in both appearances. */
    private val COLOUR = VentiLight.danger.toArgb()
    private const val WIDTH = 2f
    private const val CASING_WIDTH = 4.5f

    /** Adds the source and its layers, empty, over whatever the map has drawn so far. */
    fun add(style: Style) {
        style.addSource(GeoJsonSource(SOURCE))
        style.addLayer(MapSetup.line(CASING, SOURCE, MapSetup.ink, CASING_WIDTH).apply { minZoom = FROM_ZOOM })
        val mapped = Expression.eq(Expression.get(MAPPED), Expression.literal(true))
        style.addLayer(
            MapSetup.line(SOLID, SOURCE, COLOUR, WIDTH).apply {
                setFilter(mapped)
                minZoom = FROM_ZOOM
            },
        )
        style.addLayer(
            MapSetup.line(DASHED, SOURCE, COLOUR, WIDTH).withProperties(
                PropertyFactory.lineDasharray(arrayOf(3f, 2f)),
                PropertyFactory.lineCap(Property.LINE_CAP_BUTT),
            ).apply {
                setFilter(Expression.not(mapped))
                minZoom = FROM_ZOOM
            },
        )
    }

    /** Draws these cables in place of those before. */
    fun show(lines: List<CableLine>, style: Style) {
        style.getSourceAs<GeoJsonSource>(SOURCE)?.setGeoJson(FeatureCollection.fromFeatures(features(lines)))
    }

    /** The cables as the map draws them: a line each, with its identifier and whether its height is mapped. */
    fun features(lines: List<CableLine>): List<Feature> =
        lines.filter { it.points.size >= 2 }.map { line ->
            Feature.fromGeometry(line.points.lineString()).apply {
                addStringProperty(ID, line.id)
                addBooleanProperty(MAPPED, line.heightMapped)
            }
        }

    /** The identifier of a cable drawn within a finger's width of the screen's place, if one is. */
    fun tapped(map: MapLibreMap, style: Style, finger: RectF): String? {
        if (style.getLayer(SOLID) == null) return null
        return map.queryRenderedFeatures(finger, SOLID, DASHED).firstOrNull()?.getStringProperty(ID)
    }
}
