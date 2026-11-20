package fly.venti.app.flight

import androidx.compose.ui.graphics.toArgb
import org.maplibre.geojson.LineString
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Feature
import fly.venti.core.Bounds
import fly.venti.app.airspace.AirspaceLayers
import fly.venti.app.map.Covered
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.RouteShape
import fly.venti.app.map.lineString
import fly.venti.core.AirspaceShape
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
 * ended, the scored route, a marker at the moment the pilot is inspecting,
 * and under it all the airspace the app has around it today.
 */
class FlightMap(private val flight: FlightView) : MapContent {
    private var cursor: TrackPoint? = null
    private var score: FlightScore? = null
    private var covered = Covered()

    /** The airspace around the flight that the app has, to draw under it. */
    private var airspace: List<AirspaceShape> = emptyList()

    /** A second flight laid beside this one, and where its glider was at the marked moment. */
    private var other: FlightView? = null
    private var otherCursor: TrackPoint? = null

    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var view: MapView? = null

    /**
     * What the screen wants shown now: the moment to mark, if any; how much
     * of the map the bars and the panel cover; and the scored route, once
     * the flight has been analysed.
     */
    fun update(
        cursor: TrackPoint?,
        covered: Covered,
        score: FlightScore?,
        airspace: List<AirspaceShape> = emptyList(),
        other: FlightView? = null,
        otherCursor: TrackPoint? = null,
    ) {
        if (other != this.other) {
            this.other = other
            drawOther()
            frame()
        }
        if (otherCursor != this.otherCursor) {
            this.otherCursor = otherCursor
            markOtherCursor()
        }
        if (airspace != this.airspace) {
            this.airspace = airspace
            drawAirspace()
        }
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
        // Airspace lowest of all, under the route and the track.
        AirspaceLayers.add(style)
        drawAirspace()
        addRoute(style)
        addOther(style)
        addTrack(style)
        addEnds(style)
        addOtherCursor(style)
        addCursor(style)
        // The map may not have its size yet when the style arrives.
        view.post { if (!view.isDestroyed) frame() }
        drawRoute()
        drawOther()
        markOtherCursor()
        markCursor()
    }

    /** Draws the airspace around the flight that the app has today. */
    private fun drawAirspace() {
        val style = style ?: return
        val view = view ?: return
        if (view.isDestroyed || !style.isFullyLoaded) return
        AirspaceLayers.show(airspace, style)
    }

    /** Fits the whole track into the part of the map that shows. */
    private fun frame() {
        val map = map ?: return
        val view = view ?: return
        val density = view.resources.displayMetrics.density
        // The map's credits button moves up with the panel.
        map.uiSettings.setAttributionMargins((8 * density).roundToInt(), 0, 0, covered.bottom)
        // Room for both tracks when a second flight lies beside this one.
        val bounds = other?.bounds?.let { theirs ->
            val mine = flight.bounds
            Bounds(
                south = minOf(mine.south, theirs.south),
                west = minOf(mine.west, theirs.west),
                north = maxOf(mine.north, theirs.north),
                east = maxOf(mine.east, theirs.east),
            )
        } ?: flight.bounds
        MapSetup.show(bounds, map, view, covered)
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

    /** The second flight: a line of one colour under the first, and its glider at the marked moment. */
    private fun addOther(style: Style) {
        style.addSource(GeoJsonSource(OTHER))
        style.addLayer(MapSetup.line("flight-other-edge", OTHER, MapSetup.WHITE, 5f))
        style.addLayer(MapSetup.line("flight-other-line", OTHER, CompareColours.onMap.toArgb(), 3f))
    }

    private fun drawOther() {
        val style = style ?: return
        val source = style.getSourceAs<GeoJsonSource>(OTHER) ?: return
        val track = other?.track.orEmpty()
        val line = if (track.size >= 2) listOf(Feature.fromGeometry(LineString.fromLngLats(track.map { Point.fromLngLat(it.lon, it.lat) }))) else emptyList()
        source.setGeoJson(FeatureCollection.fromFeatures(line))
    }

    private fun addOtherCursor(style: Style) {
        style.addSource(GeoJsonSource(OTHER_CURSOR))
        style.addLayer(MapSetup.dot("flight-other-cursor-dot", OTHER_CURSOR, 7f, CompareColours.onMap.toArgb(), MapSetup.WHITE))
    }

    private fun markOtherCursor() {
        val source = style?.getSourceAs<GeoJsonSource>(OTHER_CURSOR) ?: return
        val point = otherCursor
        source.setGeoJson(
            FeatureCollection.fromFeatures(
                if (point != null) listOf(Feature.fromGeometry(Point.fromLngLat(point.lon, point.lat))) else emptyList(),
            ),
        )
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
        const val OTHER = "flight-other"
        const val OTHER_CURSOR = "flight-other-cursor"
    }
}
