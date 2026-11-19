package fly.venti.app.flight

import fly.venti.app.map.Covered
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.RouteShape
import fly.venti.app.map.lineString
import fly.venti.core.FlightScore
import fly.venti.core.FlightView
import fly.venti.core.TrackPoint
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.MultiLineString
import org.maplibre.geojson.Point
import kotlin.math.roundToInt

/**
 * The map of one flight: its track coloured by climb, where it started and
 * ended, the scored route, and a marker at the moment the pilot is
 * inspecting.
 */
class FlightMap(private val flight: FlightView) : MapContent {
    private var cursor: TrackPoint? = null
    private var score: FlightScore? = null
    private var covered = Covered()

    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var view: MapView? = null

    /**
     * What the screen wants shown now: the moment to mark, if any; how much
     * of the map the bars and the panel cover; and the scored route, once
     * the flight has been analysed.
     */
    fun update(cursor: TrackPoint?, covered: Covered, score: FlightScore?) {
        if (score != this.score) {
            this.score = score
            drawRoute()
        }
        if (covered != this.covered) {
            this.covered = covered
            frame()
        }
        if (cursor != this.cursor) {
            this.cursor = cursor
            markCursor()
        }
    }

    override fun prepare(map: MapLibreMap, view: MapView) {
        flight.track.firstOrNull()?.let { first ->
            map.cameraPosition = CameraPosition.Builder().target(LatLng(first.lat, first.lon)).zoom(11.0).build()
        }
    }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.map = map
        this.style = style
        this.view = view
        addRoute(style)
        addTrack(style)
        addEnds(style)
        addCursor(style)
        // The map may not have its size yet when the style arrives.
        view.post { frame() }
        drawRoute()
        markCursor()
    }

    /** Fits the whole track into the part of the map that shows. */
    private fun frame() {
        val map = map ?: return
        val view = view ?: return
        val density = view.resources.displayMetrics.density
        // The map's credits button moves up with the panel.
        map.uiSettings.setAttributionMargins((8 * density).roundToInt(), 0, 0, covered.bottom)
        MapSetup.show(flight.bounds, map, view, covered)
    }

    private fun addTrack(style: Style) {
        val places = flight.track.map { Point.fromLngLat(it.lon, it.lat) }
        val climbs = flight.track.map { it.climbMs }
        style.addSource(GeoJsonSource(TRACK, MapSetup.bandFeatures(places, climbs)))
        MapSetup.addBandLayers(style, TRACK, edgeWidth = 6.5f)
    }

    /** The scored route goes under the track: a thin dashed line. */
    private fun addRoute(style: Style) {
        style.addSource(GeoJsonSource(ROUTE))
        val line = MapSetup.line("flight-route-line", ROUTE, MapSetup.ink, 1.5f)
        line.setProperties(
            PropertyFactory.lineOpacity(0.7f),
            PropertyFactory.lineDasharray(arrayOf(4f, 3f)),
        )
        style.addLayer(line)
    }

    private fun drawRoute() {
        val source = style?.getSourceAs<GeoJsonSource>(ROUTE) ?: return
        val pieces = score?.let(RouteShape::pieces).orEmpty().filter { it.size >= 2 }
        source.setGeoJson(MultiLineString.fromLineStrings(pieces.map { it.lineString() }))
    }

    /** A hollow dot where the recording starts and a solid one where it ends. */
    private fun addEnds(style: Style) {
        val first = flight.track.firstOrNull() ?: return
        val last = flight.track.last()
        style.addSource(GeoJsonSource(START, Point.fromLngLat(first.lon, first.lat)))
        style.addLayer(MapSetup.dot("flight-start-dot", START, 5f, MapSetup.WHITE, MapSetup.ink))
        style.addSource(GeoJsonSource(END, Point.fromLngLat(last.lon, last.lat)))
        style.addLayer(MapSetup.dot("flight-end-dot", END, 5f, MapSetup.ink, MapSetup.WHITE))
    }

    private fun addCursor(style: Style) {
        style.addSource(GeoJsonSource(CURSOR))
        val dot = MapSetup.dot(CURSOR_DOT, CURSOR, 8f, MapSetup.accent, MapSetup.WHITE)
        dot.setProperties(PropertyFactory.visibility(Property.NONE))
        style.addLayer(dot)
    }

    private fun markCursor() {
        val style = style ?: return
        val source = style.getSourceAs<GeoJsonSource>(CURSOR) ?: return
        val dot = style.getLayer(CURSOR_DOT) ?: return
        cursor?.let { source.setGeoJson(Point.fromLngLat(it.lon, it.lat)) }
        dot.setProperties(PropertyFactory.visibility(if (cursor != null) Property.VISIBLE else Property.NONE))
    }

    private companion object {
        const val TRACK = "flight-track"
        const val ROUTE = "flight-route"
        const val START = "flight-start"
        const val END = "flight-end"
        const val CURSOR = "flight-cursor"
        const val CURSOR_DOT = "flight-cursor-dot"
    }
}
