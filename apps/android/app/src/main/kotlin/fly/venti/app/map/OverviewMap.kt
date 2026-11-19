package fly.venti.app.map

import android.annotation.SuppressLint
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.location.LocationComponentActivationOptions
import org.maplibre.android.location.OnCameraTrackingChangedListener
import org.maplibre.android.location.modes.CameraMode
import org.maplibre.android.location.modes.RenderMode
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.MultiLineString

/**
 * The map of the map tab: terrain, every flight in the logbook as a line,
 * and the pilot's own position when asked for.
 */
class OverviewMap : MapContent {
    /** Told when the map stops following the pilot by itself, because it was dragged. */
    var onFollowingEnded: () -> Unit = {}

    private var outlines: List<List<Coordinate>> = emptyList()
    private var drawn: List<List<Coordinate>>? = null
    private var covered = Covered()
    private var isFollowing = false

    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var view: MapView? = null

    override fun prepare(map: MapLibreMap, view: MapView) {
        // The northern Alps between Lake Constance and Salzburg.
        map.cameraPosition = CameraPosition.Builder().target(LatLng(47.45, 11.6)).zoom(7.4).build()
    }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.map = map
        this.style = style
        this.view = view
        draw()
        if (isFollowing) {
            startFollowing()
        }
    }

    /** The flights to draw, and how much of the map the bars cover. */
    fun show(outlines: List<List<Coordinate>>, covered: Covered) {
        this.outlines = outlines
        this.covered = covered
        draw()
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
        val source = style.getSourceAs<GeoJsonSource>(TRACKS)
        if (source != null) {
            source.setGeoJson(lines)
        } else {
            style.addSource(GeoJsonSource(TRACKS, lines))
            style.addLayer(MapSetup.line("logbook-tracks-edge", TRACKS, MapSetup.WHITE, 5f))
            style.addLayer(MapSetup.line("logbook-tracks-line", TRACKS, MapSetup.accent, 2.5f))
        }

        box(outlines)?.let { box ->
            // The map may not have its size yet when the style arrives.
            view.post { MapSetup.show(box, map, view, covered) }
        }
    }

    companion object {
        private const val TRACKS = "logbook-tracks"
        private const val FOLLOWING_ZOOM = 13.0

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
