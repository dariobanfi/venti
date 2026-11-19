package fly.venti.app.fly

import android.graphics.PointF
import android.os.SystemClock
import fly.venti.app.airspace.AirspaceLayers
import fly.venti.app.design.ClimbBand
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.latLng
import fly.venti.app.map.lineString
import fly.venti.app.map.point
import fly.venti.core.AirspaceSet
import fly.venti.core.Bounds
import fly.venti.core.CockpitView
import fly.venti.core.Coordinate
import fly.venti.core.ThermalMarkView
import fly.venti.core.WayToTarget
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.log2
import kotlin.math.max
import kotlin.math.min

/**
 * The cockpit's map: the glider, the track behind it coloured by climb, the
 * take-off, and a line to the place the cockpit is navigating to. It keeps
 * the glider in the middle by itself, north up, zooms in while the glider
 * circles, and steps back as far as it takes to show the airspace a warning
 * is about.
 *
 * The glider and its track are handed to the map as one piece, so the map
 * can never draw one without the other: a glider ahead of its track, or a
 * track that ends short of it.
 */
class CockpitMap : MapContent {
    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var mapView: MapView? = null

    /** What the cockpit wants shown, and what the map was last given. */
    private var wanted: CockpitView? = null
    private var shown: CockpitView? = null
    private var shownAt = 0L

    /** The zoom the map was last set to by itself. */
    private var chosenZoom: Double? = null
    private var drawnTakeoff: Coordinate? = null

    /** How far the airspace of the loudest warning is from the glider, in metres. */
    private var airspaceDistance: Double? = null

    /**
     * The pilot's airspace, and where it was last drawn around. Files
     * brought in or removed during a flight are drawn at the next step.
     */
    var airspace: AirspaceSet? = null
        set(value) {
            if (value !== field) {
                field = value
                airspaceDrawnAt = null
                airspaceIsNew = true
            }
        }
    private var airspaceDrawnAt: Coordinate? = null
    private var airspaceIsNew = false

    /** The thermals last drawn. */
    private var drawnThermals: List<ThermalMarkView>? = null

    /** The route the flight scores along so far, and the one last drawn. */
    private var route: List<Coordinate> = emptyList()
    private var drawnRoute: List<Coordinate>? = null

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

        // Airspace lowest of all.
        AirspaceLayers.add(style)

        // The route the flight scores along: thin and dashed, as on a
        // finished flight's map.
        style.addSource(GeoJsonSource(ROUTE))
        val scored = MapSetup.line("cockpit-route-line", ROUTE, MapSetup.ink, 1.5f)
        scored.setProperties(PropertyFactory.lineOpacity(0.7f), PropertyFactory.lineDasharray(arrayOf(4f, 3f)))
        style.addLayer(scored)

        // The line to the place being flown to, under the track and the
        // glider.
        style.addSource(GeoJsonSource(COURSE))
        val course = MapSetup.line("cockpit-course-line", COURSE, MapSetup.ink, 2f)
        course.setProperties(PropertyFactory.lineDasharray(arrayOf(2f, 2f)))
        style.addLayer(course)
        style.addSource(GeoJsonSource(TARGET))
        style.addLayer(MapSetup.dot("cockpit-target-dot", TARGET, 6f, MapSetup.ink, MapSetup.WHITE))

        style.addSource(GeoJsonSource(FLIGHT))
        MapSetup.addBandLayers(style, FLIGHT, edgeWidth = 6f)

        // The thermals already used: a dot in the colour of climb with the
        // climb beside it, over the trail, so that the lift can be found
        // again. A number that has no room gives way; its dot stays.
        style.addSource(GeoJsonSource(THERMALS))
        style.addLayer(
            MapSetup.dot("cockpit-thermal-dot", THERMALS, 5f, MapSetup.colour(ClimbBand.Climb), MapSetup.WHITE),
        )
        style.addLayer(
            SymbolLayer("cockpit-thermal-climb", THERMALS).withProperties(
                PropertyFactory.textField(Expression.get(LABEL)),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(13f),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_LEFT),
                PropertyFactory.textOffset(arrayOf(0.7f, 0f)),
                PropertyFactory.textColor(0xFF0B0B0B.toInt()),
                PropertyFactory.textHaloColor(MapSetup.WHITE),
                PropertyFactory.textHaloWidth(1.6f),
            ),
        )
        drawnThermals = null

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
     * being navigated to, if any, the route the flight scores along so
     * far, and how far the airspace of the loudest warning is, if one
     * warns.
     */
    fun show(view: CockpitView, way: WayToTarget?, route: List<Coordinate>, airspaceDistance: Double? = null) {
        wanted = view
        this.route = route
        this.airspaceDistance = airspaceDistance
        if (way != null) {
            target = way.position
        }
        update()
    }

    /** Gives the map the newest state, once its style is there. */
    private fun update() {
        val map = map ?: return
        val style = style ?: return
        val mapView = mapView ?: return
        // A map that has been taken down must not be touched: the map
        // engine throws when a style that is gone is asked for a source.
        if (mapView.isDestroyed || !style.isFullyLoaded) return
        val view = wanted ?: return
        val position = view.position ?: return
        if (view === shown) return

        val now = SystemClock.elapsedRealtime()
        val sinceLastStep = now - shownAt
        val previous = shown
        shown = view
        shownAt = now

        draw(view, position, style)
        drawAirspace(position, style)
        if (previous != null && position == previous.position && view.circling == previous.circling) return

        // Zoom only when the kind of flying changes or the airspace a
        // warning is about comes into or goes out of reach, so a zoom level
        // the pilot pinched to is kept until then.
        val density = mapView.resources.displayMetrics.density
        val zoom = zoom(view.circling, airspaceDistance, position.lat, mapView.width / density, mapView.height / density)
        val move = if (zoom != chosenZoom) {
            chosenZoom = zoom
            CameraUpdateFactory.newLatLngZoom(position.latLng, zoom)
        } else {
            CameraUpdateFactory.newLatLng(position.latLng)
        }
        if (MapSetup.movesAtOnce) {
            map.moveCamera(move)
        } else {
            // At an even pace, arriving as the next position is due.
            val glide = sinceLastStep.coerceIn(SHORTEST_GLIDE_MILLISECONDS, LONGEST_GLIDE_MILLISECONDS).toInt()
            map.easeCamera(move, glide, false)
        }
    }

    /**
     * Draws the airspace within about 40 km of the glider, again once the
     * glider has flown some 10 km from where it was last drawn.
     */
    private fun drawAirspace(position: Coordinate, style: Style) {
        val airspace = airspace
        if (airspace == null) {
            // The files are gone: so is what was drawn from them.
            if (airspaceIsNew) {
                AirspaceLayers.show(emptyList(), style)
                airspaceIsNew = false
            }
            return
        }
        airspaceIsNew = false
        val drawn = airspaceDrawnAt
        if (drawn != null && abs(drawn.lat - position.lat) < 0.09 && abs(drawn.lon - position.lon) < 0.13) return
        airspaceDrawnAt = position
        val area = Bounds(
            south = position.lat - 0.36,
            west = position.lon - 0.54,
            north = position.lat + 0.36,
            east = position.lon + 0.54,
        )
        AirspaceLayers.show(airspace.inArea(area), style)
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

        if (view.thermals != drawnThermals) {
            drawnThermals = view.thermals
            style.getSourceAs<GeoJsonSource>(THERMALS)?.setGeoJson(
                FeatureCollection.fromFeatures(view.thermals.map(::thermalFeature)),
            )
        }

        if (route != drawnRoute) {
            drawnRoute = route
            style.getSourceAs<GeoJsonSource>(ROUTE)?.setGeoJson(
                FeatureCollection.fromFeatures(
                    if (route.size >= 2) listOf(Feature.fromGeometry(route.lineString())) else emptyList(),
                ),
            )
        }

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

    companion object {
        private const val FLIGHT = "cockpit-flight"
        private const val TAKEOFF = "cockpit-takeoff"
        private const val COURSE = "cockpit-course"
        private const val ROUTE = "cockpit-route"
        private const val TARGET = "cockpit-target"
        private const val THERMALS = "cockpit-thermals"
        const val LABEL = "label"
        private const val CRUISE_ZOOM = 13.0
        private const val THERMAL_ZOOM = 15.5

        /** The map never zooms further out than this by itself. */
        private const val WIDEST_ZOOM = 10.0

        /** The shortest the map takes to slide to the glider's new position: the cockpit's own refresh. */
        private const val SHORTEST_GLIDE_MILLISECONDS = 200L

        /** The longest the map takes to slide to the glider's new position. */
        private const val LONGEST_GLIDE_MILLISECONDS = 1000L

        /** A thermal the glider has left, as the map draws it: a point with its climb to write beside it. */
        fun thermalFeature(mark: ThermalMarkView): Feature =
            Feature.fromGeometry(mark.position.point).apply { addStringProperty(LABEL, mark.label) }

        /**
         * The zoom for the kind of flying, or less if it takes less to
         * show something `showing` metres from the glider: the airspace a
         * warning is about. In half steps, so that the map does not breathe
         * with every metre flown. `width` and `height` are the map's size
         * in density-independent pixels.
         */
        fun zoom(circling: Boolean, showing: Double?, latitude: Double, width: Float, height: Float): Double {
            val usual = if (circling) THERMAL_ZOOM else CRUISE_ZOOM
            // The glider is in the middle; four fifths of the way to the
            // nearer edge is as far out as something is still seen well.
            val reach = min(width, height) / 2.0 * 0.8
            if (showing == null || showing <= 0 || reach <= 0) return usual
            // The map's zoom 0 shows the world 512 of those pixels wide.
            val metresPerPixel = 40_075_016.686 * cos(Math.toRadians(latitude)) / 512
            val fitting = log2(metresPerPixel * reach / showing)
            return min(usual, max(WIDEST_ZOOM, floor(fitting * 2) / 2))
        }
    }
}
