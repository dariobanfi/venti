package fly.venti.app.fly

import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.latLng
import fly.venti.app.map.point
import fly.venti.core.CockpitView
import fly.venti.core.Coordinate
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Point

/**
 * The cockpit's map: the glider, the track behind it coloured by climb, and
 * the take-off. It keeps the glider in the middle by itself, north up, and
 * zooms in while the glider circles.
 */
class CockpitMap : MapContent {
    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var shown: CockpitView? = null
    private var wasCircling: Boolean? = null

    override fun prepare(map: MapLibreMap, view: MapView) {
        // The map follows the glider; dragging it away would only have to
        // be undone with a gloved hand. Zooming stays.
        map.uiSettings.isScrollGesturesEnabled = false
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
        style.addSource(GeoJsonSource(GLIDER))
        style.addLayer(MapSetup.dot("cockpit-glider-dot", GLIDER, 9f, MapSetup.accent, MapSetup.WHITE))

        shown?.let { view ->
            shown = null
            show(view)
        }
    }

    /** Redraws when the glider has moved, about once a second. */
    fun show(view: CockpitView) {
        val previous = shown
        if (previous != null && view.position == previous.position && view.circling == previous.circling) return
        shown = view
        val map = map ?: return
        val style = style ?: return
        val position = view.position ?: return

        place(position, GLIDER, style)
        view.takeoff?.let { place(it, TAKEOFF, style) }
        style.getSourceAs<GeoJsonSource>(TRAIL)?.setGeoJson(
            MapSetup.bandFeatures(
                places = view.trail.map { Point.fromLngLat(it.lon, it.lat) },
                climbs = view.trail.map { it.climbMs },
            ),
        )

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

    private fun place(coordinate: Coordinate, source: String, style: Style) {
        style.getSourceAs<GeoJsonSource>(source)?.setGeoJson(coordinate.point)
    }

    private companion object {
        const val TRAIL = "cockpit-trail"
        const val TAKEOFF = "cockpit-takeoff"
        const val GLIDER = "cockpit-glider"
        const val CRUISE_ZOOM = 13.0
        const val THERMAL_ZOOM = 15.5

        /** How long the map takes to slide to the glider's new position. */
        const val GLIDE_MILLISECONDS = 600
    }
}
