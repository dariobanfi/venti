package fly.venti.app.map

import androidx.compose.ui.graphics.toArgb
import fly.venti.app.design.Palette
import fly.venti.app.groups.PlanPin
import android.annotation.SuppressLint
import android.graphics.PointF
import android.graphics.RectF
import android.view.HapticFeedbackConstants
import fly.venti.app.airspace.AirspaceLayers
import fly.venti.app.forecast.ThermalHoursLayers
import fly.venti.app.forecast.ThermalMapLayer
import fly.venti.app.live.GliderFeature
import fly.venti.app.live.LiveSymbols
import fly.venti.app.radar.RadarLayer
import fly.venti.app.radar.RadarShown
import fly.venti.app.sites.SiteAreas
import fly.venti.app.sites.SiteFeature
import fly.venti.app.sites.SiteSymbols
import fly.venti.app.wind.WindFeature
import fly.venti.app.wind.WindSymbols
import fly.venti.core.AirspaceShape
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.GlideReach
import fly.venti.core.LiveGlider
import fly.venti.core.SiteCandidateView
import fly.venti.core.SiteView
import fly.venti.core.ThermalMapCell
import fly.venti.core.ThermalMapKey
import fly.venti.core.ThermalMapShapes
import fly.venti.core.TrailPointView
import fly.venti.core.WindStation
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.location.LocationComponentActivationOptions
import org.maplibre.android.location.OnCameraTrackingChangedListener
import org.maplibre.android.location.modes.CameraMode
import org.maplibre.android.location.modes.RenderMode
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
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
import org.maplibre.geojson.MultiLineString
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon
import app.venti.design.VentiLight
import kotlin.math.hypot

/**
 * The map of the map tab: terrain, live wind, flying sites, other pilots in
 * the air, every flight in the logbook as a line, and the pilot's own
 * position when asked for.
 */
class OverviewMap : MapContent {
    /** Told when the map stops following the pilot by itself, because it was dragged. */
    var onFollowingEnded: () -> Unit = {}

    /** Told which part of the world is in view, whenever the map comes to rest. */
    var onRegion: (Bounds) -> Unit = {}

    /** Told a station's identifier when the pilot taps its arrow. */
    var onStation: (String) -> Unit = {}

    /** Told a site's identifier when the pilot taps its sign. */
    var onSite: (String) -> Unit = {}

    /** Told a live track's identifier when the pilot taps its glider. */
    var onGlider: (String) -> Unit = {}

    /** Told a place where pilots take off and no site stands, when the pilot taps its ring. */
    var onCandidate: (SiteCandidateView) -> Unit = {}

    /** Told the place under the finger after a long press on the map. */
    var onPlace: (Coordinate, Boolean) -> Unit = { _, _ -> }

    /** Called with the identifier of a plan whose mark was tapped. */
    var onPlan: (String) -> Unit = {}

    /** Told the square of the forecast map that was tapped where nothing else lies under the finger. */
    var onSquare: (ThermalMapCell) -> Unit = {}

    /**
     * A tap while a layer of hours is on, where nothing else of Venti's
     * lies: the place tapped, and the cell of the square drawn under the
     * finger if there is one (the core finds it from the place otherwise).
     */
    var onHourSquare: (place: Coordinate, cell: UInt?) -> Unit = { _, _ -> }

    /** The hour's shapes drawn, and the key they are drawn with; compared by identity. */
    private var hourShapes: ThermalMapShapes? = null
    private var hourLegend: List<ThermalMapKey> = emptyList()
    private var drawnHourShapes: ThermalMapShapes? = null
    private var hoursOn = false

    /** The rain radar's pictures to draw, and those drawn; compared by identity. */
    private var radar: RadarShown? = null
    private var drawnRadar: RadarShown? = null

    /** A tap where airspace is drawn and nothing else of Venti's lies: the place tapped. */
    var onAirspace: (Coordinate) -> Unit = {}

    private var squares: List<ThermalMapCell> = emptyList()
    private var drawnSquares: List<ThermalMapCell>? = null

    private var pins: List<PlanPin> = emptyList()
    private var drawnPins: List<PlanPin>? = null

    private var outlines: List<List<Coordinate>> = emptyList()
    private var drawn: List<List<Coordinate>>? = null
    private var stations: List<WindStation> = emptyList()
    private var drawnStations: List<WindStation>? = null
    private var airspace: List<AirspaceShape> = emptyList()
    private var drawnAirspace: List<AirspaceShape>? = null
    private var sites: List<SiteView> = emptyList()
    private var drawnSites: List<SiteView>? = null

    /** The places where pilots take off and no site stands: a small ring each, while their layer is on. */
    private var candidates: List<SiteCandidateView> = emptyList()
    private var drawnCandidates: List<SiteCandidateView>? = null

    /** The sites shown apart from those that fit the pilot's filter: drawn dimmed. */
    private var unsaidSites: List<SiteView> = emptyList()
    private var drawnUnsaidSites: List<SiteView>? = null
    private var gliders: List<LiveGlider> = emptyList()
    private var drawnGliders: List<LiveGlider>? = null
    private var followed: List<TrailPointView> = emptyList()
    private var drawnFollowed: List<TrailPointView>? = null
    private var covered = Covered()
    private var isFollowing = false

    /** What a glide reaches from a place the pilot chose, and the one drawn; compared by identity. */
    private var reach: GlideReach? = null
    private var drawnReach: GlideReach? = null

    /** Where that glide starts, and the place whose area the map last moved to show, so that it moves once for each place. */
    private var reachFrom: Coordinate? = null
    private var reachShownAt: Coordinate? = null

    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var view: MapView? = null

    /** Whether the map shows the aerial view, photographs from above, in place of the drawn ground. */
    private var aerial = false

    /** Whether the thermal map shows. */
    private var thermals = false

    /** Shows or hides the thermal map; at once if the style is there, else when it comes. */
    fun showThermals(on: Boolean) {
        if (on == thermals) return
        thermals = on
        val style = style ?: return
        val view = view ?: return
        MapSetup.showThermals(on, style, view)
    }

    /** Switches between the drawn ground and the aerial view; at once if the style is there, else when it comes. */
    fun showAerial(aerial: Boolean) {
        if (aerial == this.aerial) return
        this.aerial = aerial
        val style = style ?: return
        val view = view ?: return
        MapSetup.showAerial(aerial, style, view)
        MapSetup.showThermals(thermals, style, view)
    }

    override fun prepare(map: MapLibreMap, view: MapView) {
        // The northern Alps between Lake Constance and Salzburg.
        map.cameraPosition = CameraPosition.Builder().target(LatLng(47.45, 11.6)).zoom(7.4).build()
        map.addOnCameraIdleListener { tellRegion(map, view) }
        map.addOnMapClickListener { place -> tapped(place, map, view) }
        // A long press asks about the place under the finger.
        map.addOnMapLongClickListener { place ->
            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
            onPlace(Coordinate(lat = place.latitude, lon = place.longitude), !hasSite(place, map, view))
            true
        }
    }

    /** Whether a flying site is drawn under a finger at a place: a tap there would open it. */
    private fun hasSite(place: LatLng, map: MapLibreMap, view: MapView): Boolean {
        val style = style ?: return false
        if (style.getLayer(SITES) == null) return false
        val point = map.projection.toScreenLocation(place)
        val reach = REACH_DP * view.resources.displayMetrics.density
        return map.queryRenderedFeatures(RectF(point.x - reach, point.y - reach, point.x + reach, point.y + reach), SITES).isNotEmpty()
    }

    /** Turns the map to a place, close enough to mark a site on it: where a site's place is chosen. */
    fun lookAt(place: Coordinate) {
        val map = map ?: return
        val zoom = maxOf(map.cameraPosition.zoom, PLACING_ZOOM)
        map.moveCamera(CameraUpdateFactory.newLatLngZoom(LatLng(place.lat, place.lon), zoom))
    }

    /** The place in the middle of the map, where the mark for a site's place stands. */
    fun middle(): Coordinate? = map?.cameraPosition?.target?.let { Coordinate(lat = it.latitude, lon = it.longitude) }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.map = map
        this.style = style
        this.view = view
        MapSetup.showAerial(aerial, style, view)
        // The forecast's squares lie under the base map's names, and under
        // everything this map draws.
        ThermalMapLayer.add(style)
        ThermalHoursLayers.addSquares(style)
        // What this map draws on top of the base map: airspace lowest,
        // the flights over it, then the flying sites, the wind above them,
        // and other pilots above everything.
        AirspaceLayers.add(style)
        addReach(style)
        addTracks(style)
        addFollowed(style)
        addSites(style, view)
        addCandidates(style)
        addPlans(style)
        // The forecast's arrows under the stations' arrows.
        ThermalHoursLayers.addArrows(style)
        addWind(style, view)
        addGliders(style, view)
        draw()
        drawStations()
        drawSites()
        drawnCandidates = null
        drawCandidates()
        drawPins()
        drawGliders()
        drawFollowed()
        drawAirspace()
        drawnReach = null
        drawReach()
        drawSquares()
        drawnHourShapes = null
        drawHours()
        // The rain radar's pictures over the forecast's squares, under all the rest.
        drawnRadar = null
        drawRadar()
        if (isFollowing) {
            startFollowing()
        }
        // The map may not have its size yet when the style arrives, and
        // may be gone by the time it has.
        view.post { if (!view.isDestroyed) tellRegion(map, view) }
    }

    /**
     * The flights, the wind stations, the airspace, the flying sites and
     * the gliders to draw, the track of the pilot whose flight is being
     * followed, and how much of the map the bars cover.
     */
    fun show(
        outlines: List<List<Coordinate>>,
        stations: List<WindStation>,
        airspace: List<AirspaceShape>,
        sites: List<SiteView>,
        gliders: List<LiveGlider>,
        followed: List<TrailPointView>,
        covered: Covered,
        /** Where the pilot's groups plan to fly today and tomorrow. */
        pins: List<PlanPin> = emptyList(),
        /**
         * The sites of which their source does not say what the pilot's
         * filter asks: drawn dimmed, apart from `sites`, which fit.
         */
        unsaidSites: List<SiteView> = emptyList(),
    ) {
        this.unsaidSites = unsaidSites
        this.pins = pins
        this.outlines = outlines
        this.stations = stations
        this.airspace = airspace
        this.sites = sites
        this.gliders = gliders
        this.followed = followed
        this.covered = covered
        draw()
        drawStations()
        drawSites()
        drawPins()
        drawGliders()
        drawFollowed()
        drawAirspace()
    }

    /**
     * The squares of the forecast map for the day the pilot looks at; none
     * while the layer is off. A day's squares are one list from the core,
     * swapped whole when the day changes: nothing is drawn square by square.
     */
    /**
     * The forecast map by the hour: the hour's squares and arrows with the
     * chosen layer's key, or `null` to take them away. `on` says whether a
     * layer of hours is the one looked at, so that a tap anywhere opens
     * its square.
     */
    fun showHours(shapes: ThermalMapShapes?, legend: List<ThermalMapKey>, on: Boolean) {
        hourShapes = shapes
        hourLegend = legend
        hoursOn = on
        drawHours()
    }

    private fun drawHours() {
        val style = style ?: return
        if (hourShapes === drawnHourShapes) return
        drawnHourShapes = hourShapes
        ThermalHoursLayers.show(hourShapes, hourLegend, style)
    }

    /**
     * The rain radar: a moment's pictures in the app's colours, one for
     * each of the radar's layers that has one, each pinned by its four
     * corners, or `null` to take them away. A change of moment hands other
     * pictures, which take the place of those before.
     */
    fun showRadar(shown: RadarShown?) {
        radar = shown
        drawRadar()
    }

    private fun drawRadar() {
        val style = style ?: return
        if (radar === drawnRadar) return
        val before = drawnRadar
        drawnRadar = radar
        RadarLayer.show(radar, before, style)
    }

    /** The places where pilots take off and no site stands; none while their layer is off. */
    fun showCandidates(places: List<SiteCandidateView>) {
        candidates = places
        drawCandidates()
    }

    /**
     * The layer of those places, empty at first, over the sites' signs: a
     * small ring in the pack's muted ink, as small as the map's smallest
     * mark (a calm station's dot), so that it is not taken for a site. No
     * pin and no picture of the pack: the owner's to draw.
     */
    private fun addCandidates(style: Style) {
        style.addSource(GeoJsonSource(CANDIDATES))
        style.addLayer(
            CircleLayer(CANDIDATES, CANDIDATES).withProperties(
                PropertyFactory.circleRadius(CANDIDATE_RADIUS),
                PropertyFactory.circleOpacity(0f),
                PropertyFactory.circleStrokeColor(SiteAreas.dimmedColour),
                PropertyFactory.circleStrokeWidth(CANDIDATE_RING),
            ).apply { minZoom = SITES_FROM_ZOOM },
        )
    }

    private fun drawCandidates() {
        val source = style?.getSourceAs<GeoJsonSource>(CANDIDATES) ?: return
        if (candidates == drawnCandidates) return
        drawnCandidates = candidates
        source.setGeoJson(
            FeatureCollection.fromFeatures(
                candidates.mapIndexed { index, place ->
                    Feature.fromGeometry(Point.fromLngLat(place.position.lon, place.position.lat)).apply {
                        addStringProperty(CANDIDATE_INDEX, index.toString())
                    }
                },
            ),
        )
    }

    fun showForecast(cells: List<ThermalMapCell>) {
        squares = cells
        drawSquares()
    }

    /** Draws the squares, once the style is there and only when they are another day's or another map's. */
    private fun drawSquares() {
        val style = style ?: return
        // The same list as before is the same day of the same map: a
        // comparison of each square would cost more than it saves.
        if (squares === drawnSquares) return
        drawnSquares = squares
        ThermalMapLayer.show(squares, style)
    }

    /**
     * Where the pilot's groups plan to fly today and tomorrow: a dot in the
     * app's colour with the place and the time under it. Over the sites,
     * since a plan is often for one of them.
     */
    private fun addPlans(style: Style) {
        style.addSource(GeoJsonSource(PLANS))
        style.addLayer(MapSetup.dot(PLANS, PLANS, 7f, MapSetup.accent, MapSetup.halo))
        style.addLayer(
            SymbolLayer("$PLANS-names", PLANS).withProperties(
                PropertyFactory.textField(Expression.get(PLAN_LABEL)),
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

    private fun drawPins() {
        val source = style?.getSourceAs<GeoJsonSource>(PLANS) ?: return
        if (pins == drawnPins) return
        drawnPins = pins
        source.setGeoJson(
            FeatureCollection.fromFeatures(
                pins.map { pin ->
                    Feature.fromGeometry(Point.fromLngLat(pin.position.lon, pin.position.lat)).apply {
                        addStringProperty(PLAN_ID, pin.id)
                        addStringProperty(PLAN_LABEL, pin.label)
                    }
                },
            ),
        )
    }

    /** Draws the airspace, once the style is there and only when it has changed. */
    private fun drawAirspace() {
        val style = style ?: return
        if (airspace == drawnAirspace) return
        drawnAirspace = airspace
        AirspaceLayers.show(airspace, style)
    }

    /**
     * What a glide reaches from the place the pilot chose, or `null` to
     * take it away. The map moves once for each place to show all of the
     * area above the card that lies over its lower half.
     */
    fun showReach(reach: GlideReach?, from: Coordinate?) {
        this.reach = reach
        reachFrom = from.takeIf { reach != null }
        if (reach == null) reachShownAt = null
        drawReach()
    }

    /**
     * The layers of what a glide reaches, under the flights: the area in
     * the pack's colour for "good" at an airspace's low opacity, its edge
     * in that colour with the track's dark casing, dashed where the ground
     * loaded ends, and the start as the map's smallest dot.
     */
    private fun addReach(style: Style) {
        style.addSource(GeoJsonSource(REACH_AREA))
        style.addSource(GeoJsonSource(REACH_EDGE))
        style.addSource(GeoJsonSource(REACH_DASHED))
        style.addSource(GeoJsonSource(REACH_START))
        style.addLayer(
            FillLayer(REACH_AREA, REACH_AREA).withProperties(
                PropertyFactory.fillColor(REACH_COLOUR),
                PropertyFactory.fillOpacity(REACH_FILL_OPACITY),
            ),
        )
        style.addLayer(MapSetup.line("$REACH_EDGE-casing", REACH_EDGE, MapSetup.ink, REACH_LINE + REACH_CASING))
        style.addLayer(MapSetup.line(REACH_EDGE, REACH_EDGE, REACH_COLOUR, REACH_LINE))
        style.addLayer(
            MapSetup.line(REACH_DASHED, REACH_DASHED, REACH_COLOUR, REACH_LINE)
                .withProperties(PropertyFactory.lineDasharray(arrayOf(3f, 2f)), PropertyFactory.lineCap(Property.LINE_CAP_BUTT)),
        )
        style.addLayer(
            CircleLayer(REACH_START, REACH_START).withProperties(
                PropertyFactory.circleRadius(CANDIDATE_RADIUS),
                PropertyFactory.circleColor(MapSetup.ink),
                PropertyFactory.circleStrokeColor(MapSetup.halo),
                PropertyFactory.circleStrokeWidth(CANDIDATE_RING),
            ),
        )
    }

    private fun drawReach() {
        val style = style ?: return
        if (reach === drawnReach) return
        drawnReach = reach
        val shown = reach
        val outline = shown?.outline.orEmpty()
        val edges = GlideReachWords.edges(shown?.rays.orEmpty())
        fun set(source: String, features: List<Feature>) =
            style.getSourceAs<GeoJsonSource>(source)?.setGeoJson(FeatureCollection.fromFeatures(features))
        set(
            REACH_AREA,
            if (outline.size >= 4) listOf(Feature.fromGeometry(Polygon.fromLngLats(listOf(outline.map { it.point })))) else emptyList(),
        )
        set(REACH_EDGE, edges.solid.filter { it.size >= 2 }.map { Feature.fromGeometry(it.lineString()) })
        set(REACH_DASHED, edges.dashed.filter { it.size >= 2 }.map { Feature.fromGeometry(it.lineString()) })
        set(REACH_START, listOfNotNull(reachFrom?.let { Feature.fromGeometry(it.point) }))
        // Once for each place, when its area first comes.
        val from = reachFrom ?: return
        val box = box(listOf(outline)) ?: return
        if (from == reachShownAt) return
        reachShownAt = from
        val map = map ?: return
        val view = view ?: return
        view.post {
            if (!view.isDestroyed) MapSetup.show(box, map, view, Covered(top = covered.top, bottom = view.height / 2))
        }
    }

    /** The layers that draw the flights, empty at first. */
    private fun addTracks(style: Style) {
        style.addSource(GeoJsonSource(TRACKS))
        style.addLayer(MapSetup.line("logbook-tracks-edge", TRACKS, MapSetup.halo, 5f))
        style.addLayer(MapSetup.line("logbook-tracks-line", TRACKS, MapSetup.accent, 2.5f))
    }

    /**
     * Keeps the pilot's position in the middle of the map, or stops doing
     * so. The screen asks for the permission before it asks for this.
     */
    fun follow(following: Boolean) {
        if (following == isFollowing) return
        isFollowing = following
        if (following) {
            startFollowing()
        } else {
            val component = map?.locationComponent ?: return
            if (component.isLocationComponentActivated) {
                component.cameraMode = CameraMode.NONE
            }
        }
    }

    // The screen holds the permission before `follow(true)` is called, and
    // a refusal by Android is caught.
    @SuppressLint("MissingPermission")
    private fun startFollowing() {
        val map = map ?: return
        val style = style ?: return
        val view = view ?: return
        try {
            val component = map.locationComponent
            if (!component.isLocationComponentActivated) {
                // The map's own location engine reads Android's location
                // manager; Google Play Services are not involved.
                component.activateLocationComponent(
                    LocationComponentActivationOptions.builder(view.context, style)
                        .useDefaultLocationEngine(true)
                        .build(),
                )
                component.addOnCameraTrackingChangedListener(object : OnCameraTrackingChangedListener {
                    override fun onCameraTrackingDismissed() {
                        isFollowing = false
                        onFollowingEnded()
                    }

                    override fun onCameraTrackingChanged(currentMode: Int) = Unit
                })
            }
            component.isLocationComponentEnabled = true
            component.renderMode = RenderMode.COMPASS
            component.cameraMode = CameraMode.TRACKING
            // Close enough to see the slope the pilot stands on.
            component.zoomWhileTracking(FOLLOWING_ZOOM)
        } catch (_: SecurityException) {
            isFollowing = false
            onFollowingEnded()
        }
    }

    /**
     * Draws the flights, once the style is there and only when they have
     * changed, then moves the camera to show them all.
     */
    private fun draw() {
        val map = map ?: return
        val style = style ?: return
        val view = view ?: return
        if (outlines == drawn) return
        drawn = outlines

        val lines = MultiLineString.fromLineStrings(outlines.filter { it.size >= 2 }.map { it.lineString() })
        style.getSourceAs<GeoJsonSource>(TRACKS)?.setGeoJson(lines)

        box(outlines)?.let { box ->
            // The map may not have its size yet when the style arrives.
            view.post { if (!view.isDestroyed) MapSetup.show(box, map, view, covered) }
        }
    }

    private fun tellRegion(map: MapLibreMap, view: MapView) {
        // A map that has not been laid out shows no part of the world.
        if (view.width == 0 || view.height == 0) return
        val visible = map.projection.visibleRegion.latLngBounds
        onRegion(
            Bounds(
                south = visible.latitudeSouth,
                west = visible.longitudeWest,
                north = visible.latitudeNorth,
                east = visible.longitudeEast,
            ),
        )
    }

    /**
     * The layer that draws the stations, empty at first, above the flights:
     * an arrow or a dot each, with the wind in numbers below it.
     */
    private fun addWind(style: Style, view: MapView) {
        val metrics = view.resources.displayMetrics
        for ((name, picture) in WindSymbols.all(metrics.density, metrics.densityDpi)) {
            style.addImage(name, picture)
        }
        style.addSource(GeoJsonSource(WIND))
        style.addLayer(
            SymbolLayer(WIND, WIND).withProperties(
                PropertyFactory.iconImage(Expression.get(WindFeature.PICTURE)),
                PropertyFactory.iconRotate(Expression.get(WindFeature.HEADING)),
                // Turned against north, not against the screen: the arrow
                // would keep pointing where the wind blows on a turned map.
                PropertyFactory.iconRotationAlignment(Property.ICON_ROTATION_ALIGNMENT_MAP),
                PropertyFactory.iconSize(Expression.get(WindFeature.SCALE)),
                PropertyFactory.textField(Expression.get(WindFeature.LABEL)),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(12f),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                PropertyFactory.textOffset(arrayOf(0f, 1.15f)),
                PropertyFactory.textColor(MapSetup.ink),
                PropertyFactory.textHaloColor(MapSetup.halo),
                PropertyFactory.textHaloWidth(1.6f),
                // Where stations crowd, the windiest are placed first and
                // the others give way; a station keeps its arrow when only
                // its numbers have no room.
                PropertyFactory.symbolSortKey(Expression.get(WindFeature.RANK)),
                PropertyFactory.textOptional(true),
            ),
        )
    }

    /** Draws the stations, once the style is there and only when they have changed. */
    private fun drawStations() {
        val source = style?.getSourceAs<GeoJsonSource>(WIND) ?: return
        if (stations == drawnStations) return
        drawnStations = stations
        source.setGeoJson(FeatureCollection.fromFeatures(stations.map(WindFeature::of)))
    }

    /**
     * The layer that draws the flying sites, empty at first, between the
     * flights and the wind: a sign each, from the zoom at which they can be
     * told apart, and a name where there is room for one.
     */
    private fun addSites(style: Style, view: MapView) {
        for ((name, picture) in SiteSymbols.all(view.context)) {
            style.addImage(name, picture)
        }
        // A site mapped as an area, a landing field mostly: its outline
        // under the signs, from the zoom at which it is more than a dot.
        val dimmed = Expression.eq(Expression.get(SiteFeature.DIMMED), Expression.literal(true))
        style.addSource(GeoJsonSource(SiteAreas.SOURCE))
        style.addLayer(
            FillLayer(SiteAreas.FILL, SiteAreas.SOURCE).withProperties(
                PropertyFactory.fillColor(
                    Expression.switchCase(dimmed, Expression.color(SiteAreas.dimmedColour), Expression.color(SiteAreas.colour)),
                ),
                PropertyFactory.fillOpacity(SiteAreas.FILL_OPACITY),
            ).apply { minZoom = SiteAreas.FROM_ZOOM },
        )
        style.addLayer(
            MapSetup.line(SiteAreas.EDGE, SiteAreas.SOURCE, SiteAreas.colour, SiteAreas.EDGE_WIDTH).withProperties(
                PropertyFactory.lineColor(
                    Expression.switchCase(dimmed, Expression.color(SiteAreas.dimmedColour), Expression.color(SiteAreas.colour)),
                ),
            ).apply { minZoom = SiteAreas.FROM_ZOOM },
        )
        style.addSource(GeoJsonSource(SITES))
        style.addLayer(
            SymbolLayer(SITES, SITES).withProperties(
                PropertyFactory.iconImage(Expression.get(SiteFeature.PICTURE)),
                PropertyFactory.iconAllowOverlap(true),
                // A pin stands on the site with its tip; the name is under it.
                PropertyFactory.iconAnchor(Property.ICON_ANCHOR_BOTTOM),
                PropertyFactory.textField(Expression.get(SiteFeature.LABEL)),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(12f),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                PropertyFactory.textOffset(arrayOf(0f, 0.2f)),
                // The name of a site shown apart is dimmed with its sign.
                PropertyFactory.textColor(
                    Expression.switchCase(dimmed, Expression.color(SiteAreas.dimmedColour), Expression.color(MapSetup.ink)),
                ),
                PropertyFactory.textHaloColor(Palette.haloOnMap.toArgb()),
                PropertyFactory.textHaloWidth(1.4f),
                PropertyFactory.textOptional(true),
            ).apply { minZoom = SITES_FROM_ZOOM },
        )
    }

    /** Draws the flying sites, once the style is there and only when they have changed. */
    private fun drawSites() {
        val source = style?.getSourceAs<GeoJsonSource>(SITES) ?: return
        if (sites == drawnSites && unsaidSites == drawnUnsaidSites) return
        drawnSites = sites
        drawnUnsaidSites = unsaidSites
        source.setGeoJson(FeatureCollection.fromFeatures(SiteFeature.all(sites, unsaidSites)))
        style?.getSourceAs<GeoJsonSource>(SiteAreas.SOURCE)
            ?.setGeoJson(FeatureCollection.fromFeatures(SiteAreas.all(sites, unsaidSites)))
    }

    /**
     * The layers that draw the flight of a pilot who is being followed,
     * empty at first: a line in the colours of climb, like a flight of the
     * logbook when it is opened, over the logbook's own lines.
     */
    private fun addFollowed(style: Style) {
        style.addSource(GeoJsonSource(LIVE_TRACK))
        MapSetup.addBandLayers(style, LIVE_TRACK, edgeWidth = 5.5f)
    }

    /**
     * Draws the followed flight, once the style is there and only when it
     * has changed. When it first appears the map moves once to show all of
     * it above the page that lies over the lower half.
     */
    private fun drawFollowed() {
        val map = map ?: return
        val view = view ?: return
        val source = style?.getSourceAs<GeoJsonSource>(LIVE_TRACK) ?: return
        if (followed == drawnFollowed) return
        val isNew = drawnFollowed.isNullOrEmpty() && followed.isNotEmpty()
        drawnFollowed = followed
        source.setGeoJson(
            MapSetup.bandFeatures(
                places = followed.map { Point.fromLngLat(it.lon, it.lat) },
                climbs = followed.map { it.climbMs },
            ),
        )
        if (isNew) {
            box(listOf(followed.map { Coordinate(lat = it.lat, lon = it.lon) }))?.let { box ->
                view.post {
                    if (!view.isDestroyed) {
                        MapSetup.show(box, map, view, Covered(top = covered.top, bottom = view.height / 2))
                    }
                }
            }
        }
    }

    /**
     * The layer that draws other pilots in the air, empty at first, above
     * everything: a wing turned the way it flies, with the pilot's name and
     * altitude where there is room. Gliders are never left out for want of
     * room.
     */
    private fun addGliders(style: Style, view: MapView) {
        for ((name, picture) in LiveSymbols.all(view.context)) {
            style.addImage(name, picture)
        }
        style.addSource(GeoJsonSource(GLIDERS))
        style.addLayer(
            SymbolLayer(GLIDERS, GLIDERS).withProperties(
                PropertyFactory.iconImage(Expression.get(GliderFeature.PICTURE)),
                PropertyFactory.iconRotate(Expression.get(GliderFeature.HEADING)),
                // Turned against north, as the wind's arrows are.
                PropertyFactory.iconRotationAlignment(Property.ICON_ROTATION_ALIGNMENT_MAP),
                PropertyFactory.iconAllowOverlap(true),
                PropertyFactory.textField(Expression.get(GliderFeature.LABEL)),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(12f),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                PropertyFactory.textOffset(arrayOf(0f, 1.15f)),
                PropertyFactory.textColor(MapSetup.ink),
                PropertyFactory.textHaloColor(MapSetup.halo),
                PropertyFactory.textHaloWidth(1.6f),
                PropertyFactory.textOptional(true),
            ),
        )
    }

    /** Draws the gliders, once the style is there and only when they have changed. */
    private fun drawGliders() {
        val source = style?.getSourceAs<GeoJsonSource>(GLIDERS) ?: return
        if (gliders == drawnGliders) return
        drawnGliders = gliders
        source.setGeoJson(FeatureCollection.fromFeatures(gliders.map(GliderFeature::of)))
    }

    /**
     * A tap on a glider opens its page, one on a station's arrow the
     * station, one on a site's sign the site, and one on none of them the
     * forecast's square, while the forecast map shows. A finger is wider than any of
     * them, so the nearest within its width counts. Gliders lie over the
     * wind and the wind over the sites: what is on top wins where several
     * are under the finger.
     */
    private fun tapped(place: LatLng, map: MapLibreMap, view: MapView): Boolean {
        val style = style ?: return false
        val point = map.projection.toScreenLocation(place)
        val reach = REACH_DP * view.resources.displayMetrics.density
        val finger = RectF(point.x - reach, point.y - reach, point.x + reach, point.y + reach)
        fun nearest(layer: String, key: String): String? {
            if (style.getLayer(layer) == null) return null
            return map.queryRenderedFeatures(finger, layer)
                .minByOrNull { distance(point, it, map) }
                ?.getStringProperty(key)
        }
        nearest(GLIDERS, GliderFeature.ID)?.let { id ->
            onGlider(id)
            return true
        }
        nearest(PLANS, PLAN_ID)?.let { id ->
            onPlan(id)
            return true
        }
        nearest(WIND, WindFeature.ID)?.let { id ->
            onStation(id)
            return true
        }
        nearest(SITES, SiteFeature.ID)?.let { id ->
            onSite(id)
            return true
        }
        nearest(CANDIDATES, CANDIDATE_INDEX)?.toIntOrNull()?.let(drawnCandidates.orEmpty()::getOrNull)?.let { place ->
            onCandidate(place)
            return true
        }
        // While a layer of hours is on: its square under the finger, or the
        // place, whose square the core finds.
        if (hoursOn) {
            val cell = map.queryRenderedFeatures(point, ThermalHoursLayers.SQUARES).firstOrNull()
                ?.getNumberProperty(ThermalHoursLayers.CELL)?.toLong()?.toUInt()
            onHourSquare(Coordinate(lat = place.latitude, lon = place.longitude), cell)
            return true
        }
        // Nothing else of Venti's under the finger: the square under its middle.
        if (style.getLayer(ThermalMapLayer.LAYER) != null) {
            val drawn = drawnSquares.orEmpty()
            map.queryRenderedFeatures(point, ThermalMapLayer.LAYER).firstOrNull()
                ?.getNumberProperty(ThermalMapLayer.CELL)?.toInt()
                ?.let(drawn::getOrNull)
                ?.let { square ->
                    onSquare(square)
                    return true
                }
        }
        // Then airspace drawn under the finger: what lies over the place.
        if (style.getLayer(AirspaceLayers.HIT) != null && map.queryRenderedFeatures(point, AirspaceLayers.HIT).isNotEmpty()) {
            onAirspace(Coordinate(lat = place.latitude, lon = place.longitude))
            return true
        }
        return false
    }

    private fun distance(from: PointF, feature: Feature, map: MapLibreMap): Float {
        val place = feature.geometry() as? Point ?: return Float.MAX_VALUE
        val at = map.projection.toScreenLocation(LatLng(place.latitude(), place.longitude()))
        return hypot(at.x - from.x, at.y - from.y)
    }

    companion object {
        private const val TRACKS = "logbook-tracks"
        private const val WIND = "wind-stations"
        private const val SITES = "flying-sites"
        private const val GLIDERS = "live-gliders"
        private const val PLANS = "plan-pins"
        private const val PLAN_ID = "id"
        private const val PLAN_LABEL = "label"
        private const val LIVE_TRACK = "live-track"
        private const val CANDIDATES = "site-candidates"
        private const val CANDIDATE_INDEX = "index"
        private const val REACH_AREA = "glide-reach-area"
        private const val REACH_EDGE = "glide-reach-edge"
        private const val REACH_DASHED = "glide-reach-dashed"
        private const val REACH_START = "glide-reach-start"

        /** The area a glide reaches: the pack's "good", as the cockpit's own reach, under an airspace's fill opacity. */
        private val REACH_COLOUR = VentiLight.good.toArgb()
        private const val REACH_FILL_OPACITY = 0.14f

        /** The edge's width and its casing, about 1.5 px either side, as a track has. */
        private const val REACH_LINE = 2f
        private const val REACH_CASING = 3f

        /** A calm station's dot, the map's smallest mark, and the width of the ring round it. */
        private const val CANDIDATE_RADIUS = 4.5f
        private const val CANDIDATE_RING = 2.5f

        /** The zoom from which the sites' signs are drawn, as on the iPhone and the web. */
        private const val SITES_FROM_ZOOM = 9f

        /** How close the map shows a site's place while it is chosen: a path and a clearing can be told apart. */
        private const val PLACING_ZOOM = 15.0
        private const val FOLLOWING_ZOOM = 13.0

        /** Half the width of a finger, in the screen's own units. */
        private const val REACH_DP = 22f

        /** The rectangle that contains every outline, or `null` if there are none. */
        fun box(outlines: List<List<Coordinate>>): Bounds? {
            val points = outlines.flatten()
            if (points.isEmpty()) return null
            return Bounds(
                south = points.minOf { it.lat },
                west = points.minOf { it.lon },
                north = points.maxOf { it.lat },
                east = points.maxOf { it.lon },
            )
        }
    }
}
