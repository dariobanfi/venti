package fly.venti.app.fly

import android.graphics.PointF
import android.os.SystemClock
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.latLng
import fly.venti.app.map.lineString
import fly.venti.app.map.point
import fly.venti.core.CockpitView
import fly.venti.core.Coordinate
import fly.venti.core.WayToTarget
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point

/**
 * The cockpit's map: the glider, the track behind it coloured by climb, the
 * take-off, and a line to the place the cockpit is navigating to. It keeps
 * the glider in the middle by itself, north up, and zooms in while the
 * glider circles.
 *
 * The glider and its track are handed to the map as one piece, so the map
 * can never draw one without the other: a glider ahead of its track, or a
 * track that ends short of it. And the map is given no more than a few
 * steps a second. A real flight has a new position a second; a replay at
 * twenty times that would hand a slow phone more than it can draw.
 */
class CockpitMap : MapContent {
    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var mapView: MapView? = null

    /** What the cockpit wants shown, and what the map was last given. */
    private var wanted: CockpitView? = null
    private var shown: CockpitView? = null
    private var shownAt = 0L
    private var isWaiting = false

    private var wasCircling: Boolean? = null
    private var drawnTakeoff: Coordinate? = null

    /** The place being navigated to, if any, and the line last drawn to it. */
    private var target: Coordinate? = null
    private var drawnCourse: Pair<Coordinate, Coordinate>? = null

    override fun prepare(map: MapLibreMap, view: MapView) {
        // The map follows the glider; dragging it away would only have to
        // be undone with a gloved hand. Zooming stays, and zooms about the
        // middle, where the glider is, wherever the fingers are.
        map.uiSettings.isScrollGesturesEnabled = false
        fun zoomAboutTheMiddle() {
            map.uiSettings.focalPoint = PointF(view.width / 2f, view.height / 2f)
        }
        zoomAboutTheMiddle()
        view.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> zoomAboutTheMiddle() }
        wanted?.position?.let { position ->
            map.cameraPosition = CameraPosition.Builder().target(position.latLng).zoom(CRUISE_ZOOM).build()
        }
    }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.map = map
        this.style = style
        this.mapView = view

        // The line to the place being flown to, added first so that it
        // lies under the track and the glider.
        style.addSource(GeoJsonSource(COURSE))
        val course = MapSetup.line("cockpit-course-line", COURSE, MapSetup.ink, 2f)
        course.setProperties(PropertyFactory.lineDasharray(arrayOf(2f, 2f)))
        style.addLayer(course)
        style.addSource(GeoJsonSource(TARGET))
        style.addLayer(MapSetup.dot("cockpit-target-dot", TARGET, 6f, MapSetup.ink, MapSetup.WHITE))

        style.addSource(GeoJsonSource(FLIGHT))
        MapSetup.addBandLayers(style, FLIGHT, edgeWidth = 6f)
        style.addSource(GeoJsonSource(TAKEOFF))
        style.addLayer(MapSetup.dot("cockpit-takeoff-dot", TAKEOFF, 5f, MapSetup.WHITE, MapSetup.ink))
        // The glider goes on top: the same mark as the one a finished
        // flight's map puts at the moment being inspected.
        val glider = MapSetup.dot("cockpit-glider-dot", FLIGHT, 8f, MapSetup.accent, MapSetup.WHITE)
        glider.setFilter(Expression.eq(Expression.geometryType(), Expression.literal("Point")))
        style.addLayer(glider)

        update()
    }

    /**
     * Called with every refresh of the cockpit, with the way to the place
     * being navigated to, if any; the map follows in its own steps.
     */
    fun show(view: CockpitView, way: WayToTarget?) {
        wanted = view
        if (way != null) {
            target = way.position
        }
        update()
    }

    /** Gives the map the newest state, now or as soon as its next step is due. */
    private fun update() {
        val map = map ?: return
        val style = style ?: return
        val mapView = mapView ?: return
        // A step that was waiting may come due after the cockpit has been
        // closed and its map taken down. A style that is gone must not be
        // touched: the map engine throws.
        if (mapView.isDestroyed || !style.isFullyLoaded) return
        val view = wanted ?: return
        val position = view.position ?: return
        if (view === shown) return

        val now = SystemClock.elapsedRealtime()
        val wait = shownAt + STEP_MILLISECONDS - now
        if (wait > 0) {
            if (!isWaiting) {
                isWaiting = true
                mapView.postDelayed(
                    {
                        isWaiting = false
                        update()
                    },
                    wait,
                )
            }
            return
        }
        val sinceLastStep = now - shownAt
        val previous = shown
        shown = view
        shownAt = now

        draw(view, position, style)
        if (previous != null && position == previous.position && view.circling == previous.circling) return

        // Zoom only when the kind of flying changes, so a zoom level the
        // pilot pinched to is kept until then.
        val move = if (wasCircling != view.circling) {
            wasCircling = view.circling
            CameraUpdateFactory.newLatLngZoom(position.latLng, if (view.circling) THERMAL_ZOOM else CRUISE_ZOOM)
        } else {
            CameraUpdateFactory.newLatLng(position.latLng)
        }
        if (MapSetup.movesAtOnce) {
            map.moveCamera(move)
        } else {
            // At an even pace, arriving as the next position is due.
            val glide = sinceLastStep.coerceIn(STEP_MILLISECONDS, LONGEST_GLIDE_MILLISECONDS).toInt()
            map.easeCamera(move, glide, false)
        }
    }

    private fun draw(view: CockpitView, position: Coordinate, style: Style) {
        val takeoff = view.takeoff
        if (takeoff != null && takeoff != drawnTakeoff) {
            style.getSourceAs<GeoJsonSource>(TAKEOFF)?.setGeoJson(takeoff.point)
            drawnTakeoff = takeoff
        }
        val trail = MapSetup.bandFeatures(
            places = view.trail.map { Point.fromLngLat(it.lon, it.lat) },
            climbs = view.trail.map { it.climbMs },
        )
        val glider = Feature.fromGeometry(position.point)
        style.getSourceAs<GeoJsonSource>(FLIGHT)?.setGeoJson(
            FeatureCollection.fromFeatures(trail.features().orEmpty() + glider),
        )

        val target = target ?: return
        val course = position to target
        if (course != drawnCourse) {
            if (target != drawnCourse?.second) {
                style.getSourceAs<GeoJsonSource>(TARGET)?.setGeoJson(target.point)
            }
            style.getSourceAs<GeoJsonSource>(COURSE)?.setGeoJson(listOf(position, target).lineString())
            drawnCourse = course
        }
    }

    private companion object {
        const val FLIGHT = "cockpit-flight"
        const val TAKEOFF = "cockpit-takeoff"
        const val COURSE = "cockpit-course"
        const val TARGET = "cockpit-target"
        const val CRUISE_ZOOM = 13.0
        const val THERMAL_ZOOM = 15.5

        /** The shortest time between two steps of the map. */
        const val STEP_MILLISECONDS = 250L

        /** The longest the map takes to slide to the glider's new position. */
        const val LONGEST_GLIDE_MILLISECONDS = 1000L
    }
}
