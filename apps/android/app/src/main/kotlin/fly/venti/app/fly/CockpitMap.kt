package fly.venti.app.fly

import android.graphics.PointF
import android.os.SystemClock
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.latLng
import fly.venti.app.map.point
import fly.venti.core.CockpitView
import fly.venti.core.Coordinate
import fly.venti.core.TrailPointView
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Point

/**
 * The cockpit's map: the track behind the glider coloured by climb, and the
 * take-off. It keeps the glider in the middle by itself, north up, and
 * zooms in while the glider circles.
 *
 * The glider itself is not on the map. It is always in the middle, so the
 * screen draws its mark there, over the map: a mark that is part of the map
 * arrives a moment after the map has moved, and at twenty times the speed
 * of flight that moment is half a screen.
 */
class CockpitMap : MapContent {
    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var shown: CockpitView? = null
    private var wasCircling: Boolean? = null
    private var drawnTakeoff: Coordinate? = null
    private var drawnTrail: List<TrailPointView> = emptyList()
    private var trailDrawnAt = 0L

    override fun prepare(map: MapLibreMap, view: MapView) {
        // The map follows the glider; dragging it away would only have to
        // be undone with a gloved hand. Zooming stays, and zooms about the
        // glider, wherever the fingers are.
        map.uiSettings.isScrollGesturesEnabled = false
        fun zoomAboutTheMiddle() {
            map.uiSettings.focalPoint = PointF(view.width / 2f, view.height / 2f)
        }
        zoomAboutTheMiddle()
        view.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> zoomAboutTheMiddle() }
        shown?.position?.let { position ->
            map.cameraPosition = CameraPosition.Builder().target(position.latLng).zoom(CRUISE_ZOOM).build()
        }
    }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.map = map
        this.style = style

        style.addSource(GeoJsonSource(TRAIL))
        MapSetup.addBandLayers(style, TRAIL, edgeWidth = 6f)
        style.addSource(GeoJsonSource(TAKEOFF))
        style.addLayer(MapSetup.dot("cockpit-takeoff-dot", TAKEOFF, 5f, MapSetup.WHITE, MapSetup.ink))

        shown?.let { view ->
            shown = null
            show(view)
        }
    }

    /** Called with every refresh of the cockpit; moves the map when the glider has moved. */
    fun show(view: CockpitView) {
        val map = map
        val style = style
        val position = view.position
        if (map == null || style == null || position == null) {
            shown = view
            return
        }
        draw(view, style)

        val previous = shown
        shown = view
        if (previous != null && position == previous.position && view.circling == previous.circling) return

        // Zoom only when the kind of flying changes, so a zoom level the
        // pilot pinched to is kept until then.
        val move = if (wasCircling != view.circling) {
            wasCircling = view.circling
            CameraUpdateFactory.newLatLngZoom(position.latLng, if (view.circling) THERMAL_ZOOM else CRUISE_ZOOM)
        } else {
            CameraUpdateFactory.newLatLng(position.latLng)
        }
        if (MapSetup.movesAtOnce) map.moveCamera(move) else map.animateCamera(move, GLIDE_MILLISECONDS)
    }

    /**
     * Redraws what is on the map if it has changed: the take-off once, and
     * the trail no more than once a second. A real flight has a new
     * position a second; a replay at twenty times that would otherwise
     * hand the map more than it can draw.
     */
    private fun draw(view: CockpitView, style: Style) {
        val takeoff = view.takeoff
        if (takeoff != null && takeoff != drawnTakeoff) {
            style.getSourceAs<GeoJsonSource>(TAKEOFF)?.setGeoJson(takeoff.point)
            drawnTakeoff = takeoff
        }
        val now = SystemClock.elapsedRealtime()
        if (view.trail != drawnTrail && now - trailDrawnAt >= TRAIL_MILLISECONDS) {
            style.getSourceAs<GeoJsonSource>(TRAIL)?.setGeoJson(
                MapSetup.bandFeatures(
                    places = view.trail.map { Point.fromLngLat(it.lon, it.lat) },
                    climbs = view.trail.map { it.climbMs },
                ),
            )
            drawnTrail = view.trail
            trailDrawnAt = now
        }
    }

    private companion object {
        const val TRAIL = "cockpit-trail"
        const val TAKEOFF = "cockpit-takeoff"
        const val CRUISE_ZOOM = 13.0
        const val THERMAL_ZOOM = 15.5

        /** How long the map takes to slide to the glider's new position. */
        const val GLIDE_MILLISECONDS = 600

        /** The shortest time between two drawings of the trail. */
        const val TRAIL_MILLISECONDS = 1000L
    }
}
