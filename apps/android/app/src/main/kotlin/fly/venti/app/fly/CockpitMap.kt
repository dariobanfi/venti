package fly.venti.app.fly

import android.graphics.PointF
import android.os.SystemClock
import fly.venti.app.airspace.AirspaceLayers
import fly.venti.app.design.ClimbBand
import fly.venti.app.map.Covered
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.latLng
import fly.venti.app.map.lineString
import fly.venti.app.map.point
import fly.venti.core.AirspaceSet
import fly.venti.core.Bounds
import fly.venti.core.CockpitView
import fly.venti.app.wind.contains
import fly.venti.core.Coordinate
import fly.venti.core.FaiAreas
import fly.venti.core.LiftView
import fly.venti.core.TaskStatusView
import fly.venti.core.TaskTurnpointView
import fly.venti.core.TaskView
import fly.venti.core.ThermalMarkView
import fly.venti.core.WayToTarget
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
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

    /** The picture of the thermal being circled in that was last drawn, and whether any was. */
    private var drawnLift: LiftView? = null
    private var hasDrawnLift = false

    /** Where to fly for an FAI triangle, and the side the areas last drawn stand on. */
    private var fai: FaiAreas? = null
    private var drawnFaiSide: List<Coordinate>? = null

    /** Whether the whole flight is shown, and the box the map shows while it is. */
    private var showsWholeFlight = false
    private var wholeFlight: Bounds? = null

    /** The route the flight scores along so far, and the one last drawn. */
    private var route: List<Coordinate> = emptyList()
    private var drawnRoute: List<Coordinate>? = null

    /** The place being navigated to, if any, and the line last drawn to it. */
    private var target: Coordinate? = null
    private var drawnCourse: Pair<Coordinate, Coordinate>? = null

    /**
     * The task being flown, the number of the turnpoint to fly to next and
     * the shortest way still to fly; and what of them was last drawn.
     */
    private var task: TaskView? = null
    private var taskNext: UInt? = null
    private var taskWay: List<Coordinate> = emptyList()
    private var drawnTask: TaskView? = null
    private var drawnTaskNext: UInt? = null
    private var drawnTaskWay: List<Coordinate>? = null

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

        // Where to fly for an FAI triangle: two dashed outlines, under the
        // route they belong to.
        style.addSource(GeoJsonSource(FAI))
        val areas = MapSetup.line("cockpit-fai-line", FAI, MapSetup.accent, 1.5f)
        areas.setProperties(PropertyFactory.lineDasharray(arrayOf(3f, 2f)), PropertyFactory.lineOpacity(0.85f))
        style.addLayer(areas)
        drawnFaiSide = null

        // The task: its cylinders and goal in the map's ink, the one to fly
        // to next with a heavier line, and the shortest way still to fly
        // as a thin line through them.
        style.addSource(GeoJsonSource(TASK))
        val rings = MapSetup.line("cockpit-task-line", TASK, MapSetup.ink, 1.5f)
        rings.setFilter(Expression.eq(Expression.get(NEXT), Expression.literal(0)))
        style.addLayer(rings)
        val nextRing = MapSetup.line("cockpit-task-next", TASK, MapSetup.ink, 3.5f)
        nextRing.setFilter(Expression.eq(Expression.get(NEXT), Expression.literal(1)))
        style.addLayer(nextRing)
        style.addSource(GeoJsonSource(TASK_WAY))
        val ahead = MapSetup.line("cockpit-task-way-line", TASK_WAY, MapSetup.ink, 1.5f)
        ahead.setProperties(PropertyFactory.lineOpacity(0.6f))
        style.addLayer(ahead)
        drawnTask = null
        drawnTaskNext = null
        drawnTaskWay = null

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

        // The thermal being circled in. Around where the lift is, the
        // circle to fly as a dashed ring; over it the last circles as dots,
        // each where the air it was flown through is now, in the colour of
        // its climb and larger where the climb is stronger. The dots lie
        // over the ring: a circle that is centred covers it, and the dots
        // are what there is to read.
        style.addSource(GeoJsonSource(LIFT_RING))
        style.addLayer(MapSetup.line("cockpit-lift-ring-edge", LIFT_RING, MapSetup.WHITE, 4.5f))
        val circle = MapSetup.line("cockpit-lift-ring-line", LIFT_RING, MapSetup.ink, 2f)
        circle.setProperties(PropertyFactory.lineDasharray(arrayOf(2f, 2f)))
        style.addLayer(circle)
        style.addSource(GeoJsonSource(LIFT))
        for (band in ClimbBand.entries) {
            val dots = CircleLayer("cockpit-lift-${band.ordinal}", LIFT).withProperties(
                PropertyFactory.circleRadius(Expression.get(RADIUS)),
                PropertyFactory.circleColor(MapSetup.colour(band)),
                PropertyFactory.circleStrokeColor(MapSetup.WHITE),
                PropertyFactory.circleStrokeWidth(1f),
            )
            dots.setFilter(Expression.eq(Expression.get(BAND), Expression.literal(band.ordinal)))
            style.addLayer(dots)
        }
        style.addSource(GeoJsonSource(LIFT_CENTRE))
        style.addLayer(
            CircleLayer("cockpit-lift-centre-dot", LIFT_CENTRE).withProperties(
                PropertyFactory.circleRadius(4f),
                PropertyFactory.circleColor(MapSetup.ink),
                PropertyFactory.circleStrokeColor(MapSetup.WHITE),
                PropertyFactory.circleStrokeWidth(2f),
            ),
        )
        drawnLift = null
        hasDrawnLift = false

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
    fun show(
        view: CockpitView,
        way: WayToTarget?,
        route: List<Coordinate>,
        airspaceDistance: Double? = null,
        /** Where to fly for an FAI triangle, if the pilot wants it drawn. */
        fai: FaiAreas? = null,
        /** Whether to show the whole flight instead of following the glider closely. */
        showsWholeFlight: Boolean = false,
        /** The task being flown, for its cylinders, and where the flight stands in it. */
        task: TaskView? = null,
        taskStatus: TaskStatusView? = null,
    ) {
        wanted = view
        this.route = route
        this.airspaceDistance = airspaceDistance
        val nextTurnpoint = taskStatus?.next?.number
        val wayAhead = taskStatus?.way.orEmpty()
        if (fai?.side != this.fai?.side || showsWholeFlight != this.showsWholeFlight ||
            task != this.task || nextTurnpoint != this.taskNext || wayAhead != this.taskWay
        ) {
            // Something to draw or to frame anew, also while the glider stands still.
            shown = null
        }
        this.task = task
        this.taskNext = nextTurnpoint
        this.taskWay = wayAhead
        this.fai = fai
        this.showsWholeFlight = showsWholeFlight
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
        drawTask(style)

        // The whole flight: everything it has to show in one view, fitted
        // again only when something has grown out of it.
        if (showsWholeFlight) {
            val places = listOf(position) + route + fai?.outlines.orEmpty().flatten() +
                task?.turnpoints.orEmpty().flatMap { it.outline } +
                listOfNotNull(view.takeoff, target)
            val box = box(places)
            if (wholeFlight?.contains(box) != true) {
                wholeFlight = box
                MapSetup.show(box, map, mapView, Covered())
            }
            return
        }
        if (wholeFlight != null) {
            // Back to following the glider at the usual zoom.
            wholeFlight = null
            chosenZoom = null
        }
        val isStill = previous != null && position == previous.position && view.circling == previous.circling
        if (isStill && chosenZoom != null) return

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

    /** Draws the task's cylinders and the way still to fly, when either has changed. */
    private fun drawTask(style: Style) {
        if (task != drawnTask || taskNext != drawnTaskNext) {
            drawnTask = task
            drawnTaskNext = taskNext
            style.getSourceAs<GeoJsonSource>(TASK)
                ?.setGeoJson(FeatureCollection.fromFeatures(taskFeatures(task?.turnpoints.orEmpty(), taskNext)))
        }
        if (taskWay != drawnTaskWay) {
            drawnTaskWay = taskWay
            val line = if (taskWay.size >= 2) listOf(Feature.fromGeometry(taskWay.lineString())) else emptyList()
            style.getSourceAs<GeoJsonSource>(TASK_WAY)?.setGeoJson(FeatureCollection.fromFeatures(line))
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

        if (fai?.side != drawnFaiSide) {
            drawnFaiSide = fai?.side
            style.getSourceAs<GeoJsonSource>(FAI)?.setGeoJson(
                FeatureCollection.fromFeatures(
                    fai?.outlines.orEmpty().filter { it.size >= 2 }.map { Feature.fromGeometry(it.lineString()) },
                ),
            )
        }

        if (view.thermals != drawnThermals) {
            drawnThermals = view.thermals
            style.getSourceAs<GeoJsonSource>(THERMALS)?.setGeoJson(
                FeatureCollection.fromFeatures(view.thermals.map(::thermalFeature)),
            )
        }

        // The thermal's picture, and nothing of it once the thermal is left.
        val lift = view.lift
        if (!hasDrawnLift || lift != drawnLift) {
            hasDrawnLift = true
            drawnLift = lift
            style.getSourceAs<GeoJsonSource>(LIFT)?.setGeoJson(FeatureCollection.fromFeatures(liftFeatures(lift)))
            val centre = lift?.centre
            val outline = centre?.outline.orEmpty()
            val ring = if (outline.size >= 2) listOf(Feature.fromGeometry(outline.lineString())) else emptyList()
            style.getSourceAs<GeoJsonSource>(LIFT_RING)?.setGeoJson(FeatureCollection.fromFeatures(ring))
            val middle = listOfNotNull(centre?.position?.let { Feature.fromGeometry(it.point) })
            style.getSourceAs<GeoJsonSource>(LIFT_CENTRE)?.setGeoJson(FeatureCollection.fromFeatures(middle))
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
        private const val FAI = "cockpit-fai"
        private const val TASK = "cockpit-task"
        private const val TASK_WAY = "cockpit-task-way"
        private const val LIFT = "cockpit-lift"
        private const val LIFT_RING = "cockpit-lift-ring"
        private const val LIFT_CENTRE = "cockpit-lift-centre"
        const val LABEL = "label"
        const val NEXT = "next"
        const val BAND = "band"
        const val RADIUS = "radius"

        /**
         * The places of a thermal's picture, a dot each: its climb band for
         * the colour and its size in the screen's own units.
         */
        fun liftFeatures(lift: LiftView?): List<Feature> =
            lift?.points.orEmpty().map { point ->
                Feature.fromGeometry(Point.fromLngLat(point.lon, point.lat)).apply {
                    addNumberProperty(BAND, ClimbBand.of(point.climbMs).ordinal)
                    addNumberProperty(RADIUS, liftDotRadius(point.strength))
                }
            }

        /**
         * How large a dot is for the core's strength of its climb: 3.5 at
         * the mean of the picture, up to 5 where the climb is a metre a
         * second or more above it and down to 2 where it is as much below.
         * An even thermal is all 3.5, which at a point a second makes a
         * string of beads.
         */
        fun liftDotRadius(strength: Double): Double = 2 + 3 * strength.coerceIn(0.0, 1.0)

        /**
         * The outlines of a task's turnpoints, each marked with whether it
         * is the one to fly to next.
         */
        fun taskFeatures(turnpoints: List<TaskTurnpointView>, next: UInt?): List<Feature> =
            turnpoints.filter { it.outline.size >= 2 }.map { turnpoint ->
                Feature.fromGeometry(turnpoint.outline.lineString()).apply {
                    addNumberProperty(NEXT, if (turnpoint.number == next) 1 else 0)
                }
            }
        private const val CRUISE_ZOOM = 13.0
        private const val THERMAL_ZOOM = 15.5

        /** The map never zooms further out than this by itself. */
        private const val WIDEST_ZOOM = 10.0

        /** The shortest the map takes to slide to the glider's new position: the cockpit's own refresh. */
        private const val SHORTEST_GLIDE_MILLISECONDS = 200L

        /** The longest the map takes to slide to the glider's new position. */
        private const val LONGEST_GLIDE_MILLISECONDS = 1000L

        /** The smallest box that holds every one of these places. */
        fun box(places: List<Coordinate>): Bounds = Bounds(
            south = places.minOfOrNull { it.lat } ?: 0.0,
            west = places.minOfOrNull { it.lon } ?: 0.0,
            north = places.maxOfOrNull { it.lat } ?: 0.0,
            east = places.maxOfOrNull { it.lon } ?: 0.0,
        )

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
