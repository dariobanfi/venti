package fly.venti.app.map

import android.annotation.SuppressLint
import android.graphics.PointF
import android.graphics.RectF
import android.view.HapticFeedbackConstants
import fly.venti.app.airspace.AirspaceLayers
import fly.venti.app.sites.SiteFeature
import fly.venti.app.sites.SiteSymbols
import fly.venti.app.wind.WindFeature
import fly.venti.app.wind.WindSymbols
import fly.venti.core.AirspaceShape
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.SiteView
import fly.venti.core.WindStation
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.location.LocationComponentActivationOptions
import org.maplibre.android.location.OnCameraTrackingChangedListener
import org.maplibre.android.location.modes.CameraMode
import org.maplibre.android.location.modes.RenderMode
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
import org.maplibre.geojson.MultiLineString
import org.maplibre.geojson.Point
import kotlin.math.hypot

/**
 * The map of the map tab: terrain, live wind, flying sites, every flight in
 * the logbook as a line, and the pilot's own position when asked for.
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

    /** Told the place under the finger after a long press on the map. */
    var onPlace: (Coordinate) -> Unit = {}

    private var outlines: List<List<Coordinate>> = emptyList()
    private var drawn: List<List<Coordinate>>? = null
    private var stations: List<WindStation> = emptyList()
    private var drawnStations: List<WindStation>? = null
    private var airspace: List<AirspaceShape> = emptyList()
    private var drawnAirspace: List<AirspaceShape>? = null
    private var sites: List<SiteView> = emptyList()
    private var drawnSites: List<SiteView>? = null
    private var covered = Covered()
    private var isFollowing = false

    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var view: MapView? = null

    override fun prepare(map: MapLibreMap, view: MapView) {
        // The northern Alps between Lake Constance and Salzburg.
        map.cameraPosition = CameraPosition.Builder().target(LatLng(47.45, 11.6)).zoom(7.4).build()
        map.addOnCameraIdleListener { tellRegion(map, view) }
        map.addOnMapClickListener { place -> tapped(place, map, view) }
        // A long press asks about the place under the finger.
        map.addOnMapLongClickListener { place ->
            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
            onPlace(Coordinate(lat = place.latitude, lon = place.longitude))
            true
        }
    }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.map = map
        this.style = style
        this.view = view
        // What this map draws on top of the base map: airspace lowest,
        // the flights over it, then the flying sites, the wind above them.
        AirspaceLayers.add(style)
        addTracks(style)
        addSites(style, view)
        addWind(style, view)
        draw()
        drawStations()
        drawSites()
        drawAirspace()
        if (isFollowing) {
            startFollowing()
        }
        // The map may not have its size yet when the style arrives, and
        // may be gone by the time it has.
        view.post { if (!view.isDestroyed) tellRegion(map, view) }
    }

    /**
     * The flights, the wind stations, the airspace and the flying sites to
     * draw, and how much of the map the bars cover.
     */
    fun show(
        outlines: List<List<Coordinate>>,
        stations: List<WindStation>,
        airspace: List<AirspaceShape>,
        sites: List<SiteView>,
        covered: Covered,
    ) {
        this.outlines = outlines
        this.stations = stations
        this.airspace = airspace
        this.sites = sites
        this.covered = covered
        draw()
        drawStations()
        drawSites()
        drawAirspace()
    }

    /** Draws the airspace, once the style is there and only when it has changed. */
    private fun drawAirspace() {
        val style = style ?: return
        if (airspace == drawnAirspace) return
        drawnAirspace = airspace
        AirspaceLayers.show(airspace, style)
    }

    /** The layers that draw the flights, empty at first. */
    private fun addTracks(style: Style) {
        style.addSource(GeoJsonSource(TRACKS))
        style.addLayer(MapSetup.line("logbook-tracks-edge", TRACKS, MapSetup.WHITE, 5f))
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
                PropertyFactory.textHaloColor(MapSetup.WHITE),
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
        style.addSource(GeoJsonSource(SITES))
        style.addLayer(
            SymbolLayer(SITES, SITES).withProperties(
                PropertyFactory.iconImage(Expression.get(SiteFeature.PICTURE)),
                PropertyFactory.iconAllowOverlap(true),
                PropertyFactory.textField(Expression.get(SiteFeature.LABEL)),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(11f),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                PropertyFactory.textOffset(arrayOf(0f, 1.1f)),
                PropertyFactory.textColor(MapSetup.ink),
                PropertyFactory.textHaloColor(MapSetup.WHITE),
                PropertyFactory.textHaloWidth(1.4f),
                PropertyFactory.textOptional(true),
            ).apply { minZoom = SITES_FROM_ZOOM },
        )
    }

    /** Draws the flying sites, once the style is there and only when they have changed. */
    private fun drawSites() {
        val source = style?.getSourceAs<GeoJsonSource>(SITES) ?: return
        if (sites == drawnSites) return
        drawnSites = sites
        source.setGeoJson(FeatureCollection.fromFeatures(sites.map(SiteFeature::of)))
    }

    /**
     * A tap on a station's arrow opens the station, one on a site's sign
     * the site. A finger is wider than either, so the nearest within its
     * width counts. Wind lies over the sites, and wins where both are under
     * the finger.
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
        nearest(WIND, WindFeature.ID)?.let { id ->
            onStation(id)
            return true
        }
        nearest(SITES, SiteFeature.ID)?.let { id ->
            onSite(id)
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

        /** The zoom from which the sites' signs are drawn, as on the iPhone and the web. */
        private const val SITES_FROM_ZOOM = 9f
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
