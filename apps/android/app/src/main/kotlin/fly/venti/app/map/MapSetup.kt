package fly.venti.app.map

import fly.venti.app.maps.KeptMap
import android.content.Context
import android.graphics.Color
import android.view.Gravity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import fly.venti.app.AssetFiles
import fly.venti.app.AppFiles
import fly.venti.app.BuildConfig
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Palette
import fly.venti.app.terrain.TerrainModel
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapLibreMapOptions
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.expressions.Expression
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.LineString
import org.maplibre.geojson.MultiLineString
import org.maplibre.geojson.Point
import java.io.File
import kotlin.math.roundToInt

/** What one screen draws on its map. The map calls it as it comes to life. */
interface MapContent {
    /** The map exists: set its gestures and where it first looks. */
    fun prepare(map: MapLibreMap, view: MapView) = Unit

    /** The style has loaded: add this screen's sources and layers. */
    fun styleLoaded(map: MapLibreMap, style: Style, view: MapView)
}

/**
 * A map with the app's style, filling the space it is given.
 *
 * The map engine is MapLibre Native, wrapped directly: its view lives inside
 * the Compose screen and follows the screen's life, so it stops drawing when
 * the app is not on screen and frees its memory when the screen goes.
 */
@Composable
fun MapSurface(
    content: MapContent,
    modifier: Modifier = Modifier,
    /**
     * A region kept on the phone to draw from, where this map has one; by
     * default the one maps for looking around use without a network.
     */
    kept: KeptMap? = MapSetup.browsing(),
    onUpdate: () -> Unit = {},
) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val view = remember(content) { MapSetup.makeMapView(context, content, kept) }

    DisposableEffect(lifecycle, view) {
        // The view is told of each step of the screen's life in order, and
        // taken back down the same steps when the screen goes away.
        var started = false
        var resumed = false
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> { view.onStart(); started = true }
                Lifecycle.Event.ON_RESUME -> { view.onResume(); resumed = true }
                Lifecycle.Event.ON_PAUSE -> { view.onPause(); resumed = false }
                Lifecycle.Event.ON_STOP -> { view.onStop(); started = false }
                else -> Unit
            }
        }
        lifecycle.addObserver(observer)
        onDispose {
            lifecycle.removeObserver(observer)
            if (resumed) view.onPause()
            if (started) view.onStop()
            view.onDestroy()
        }
    }

    // A screen that draws something else gets a view of its own.
    key(view) {
        AndroidView(factory = { view }, modifier = modifier, update = { onUpdate() })
    }
}

/** What every map in the app has in common. */
object MapSetup {
    /** Which tiles the maps draw. The UI tests switch to the bundled region. */
    @Volatile
    var source: MapStyle.Source = MapStyle.Source.Online

    /** Whether the camera moves without animation, so a test's screenshot shows where it ended. */
    @Volatile
    var movesAtOnce: Boolean = false

    private var prepared: Pair<MapStyle.Source, String>? = null

    /**
     * The region kept on the phone that maps for looking around are drawn
     * from, where there is one to use: without a network. The app sets this.
     */
    @Volatile
    var browsing: () -> KeptMap? = { null }

    /**
     * The style for the current source, as JSON. Prepared once: it copies
     * the label fonts out of the app the first time.
     */
    @Synchronized
    fun style(context: Context): String {
        prepared?.let { (source, json) -> if (source == this.source) return json }
        val files = AssetFiles(context)
        return MapStyle.json(source, files.text(AppFiles.MAP_STYLE), region(context)).also { prepared = source to it }
    }

    /**
     * The folder with the label fonts and the region that came with the
     * app, copied out of the app the first time it is asked for. The
     * cockpit reads the ground from the region's elevation tiles when the
     * map is drawn from it.
     */
    @Synchronized
    fun region(context: Context): File = MapRegion.install(
        AssetFiles(context),
        folder = context.noBackupFilesDir.resolve("map"),
        stamp = "${BuildConfig.VERSION_NAME}-${installedAt(context)}",
    )

    /**
     * Where the ground's elevation tiles come from: the place the map's
     * own come from, so that the heights are of the relief the pilot sees.
     */
    fun terrain(context: Context): TerrainModel.Source = when (source) {
        MapStyle.Source.Online -> TerrainModel.Source.Server(MapStyle.Online.TERRAIN_TILES)
        MapStyle.Source.Bundled ->
            runCatching { TerrainModel.Source.Archive(region(context).resolve(MapStyle.BUNDLED_TERRAIN)) }
                .getOrDefault(TerrainModel.Source.Nowhere)
    }

    /** When this copy of the app was installed or last updated. */
    private fun installedAt(context: Context): Long =
        context.packageManager.getPackageInfo(context.packageName, 0).lastUpdateTime

    /** A map view with the app's style and no camera yet. */
    fun makeMapView(context: Context, content: MapContent, kept: KeptMap? = null): MapView {
        MapLibre.getInstance(context.applicationContext)
        val density = context.resources.displayMetrics.density
        val options = MapLibreMapOptions.createFromAttributes(context)
            .logoEnabled(false)
            // The map is flat and north is up: tilting shows nothing more,
            // and a map turned by accident is hard to read.
            .tiltGesturesEnabled(false)
            .rotateGesturesEnabled(false)
            .compassEnabled(false)
            // The credits button is the map's, not one of the app's controls.
            .attributionGravity(Gravity.BOTTOM or Gravity.START)
            .attributionTintColor(Color.argb(190, 64, 64, 64))
            .attributionMargins(intArrayOf((8 * density).roundToInt(), 0, 0, (8 * density).roundToInt()))
        val view = MapView(context, options)
        view.onCreate(null)
        view.getMapAsync { map ->
            content.prepare(map, view)
            // If the style cannot be prepared the map stays empty; the unit
            // tests check that it can.
            val json = runCatching {
                if (kept != null) {
                    MapStyle.json(source, AssetFiles(context).text(AppFiles.MAP_STYLE), region(context), kept)
                } else {
                    style(context)
                }
            }.getOrNull() ?: return@getMapAsync
            map.setStyle(Style.Builder().fromJson(json)) { style -> content.styleLoaded(map, style, view) }
        }
        return view
    }

    /**
     * Moves the camera so the box is in view, clear of what covers the map.
     * `covered` is how much of the map the bars and panels hide from each
     * edge, in pixels.
     */
    fun show(box: Bounds, on: MapLibreMap, view: MapView, covered: Covered) {
        // A map that has not been laid out has no size to fit anything into.
        if (view.width == 0 || view.height == 0) return
        val density = view.resources.displayMetrics.density
        // A box with no extent, such as a single position, would zoom in
        // without limit.
        val margin = 0.002
        val bounds = LatLngBounds.Builder()
            .include(LatLng(box.south - margin, box.west - margin))
            .include(LatLng(box.north + margin, box.east + margin))
            .build()
        val side = (36 * density).roundToInt()
        val end = (28 * density).roundToInt()
        // What is left must be a real area, or the engine cannot fit a box in it.
        if (covered.top + covered.bottom + 2 * end >= view.height) return
        on.moveCamera(
            CameraUpdateFactory.newLatLngBounds(bounds, side, covered.top + end, side, covered.bottom + end),
        )
    }

    /** A line layer with round ends and corners. */
    fun line(id: String, source: String, colour: Int, width: Float): LineLayer =
        LineLayer(id, source).withProperties(
            PropertyFactory.lineColor(colour),
            PropertyFactory.lineWidth(width),
            PropertyFactory.lineJoin(Property.LINE_JOIN_ROUND),
            PropertyFactory.lineCap(Property.LINE_CAP_ROUND),
        )

    /** A round dot with a ring around it. */
    fun dot(id: String, source: String, radius: Float, colour: Int, ring: Int): CircleLayer =
        CircleLayer(id, source).withProperties(
            PropertyFactory.circleRadius(radius),
            PropertyFactory.circleColor(colour),
            PropertyFactory.circleStrokeColor(ring),
            PropertyFactory.circleStrokeWidth(2.5f),
        )

    /**
     * A line as one feature per climb band, each holding every stretch of
     * the line flown in that band. `climbs` has the climb rate at each of
     * the `places`.
     */
    fun bandFeatures(places: List<Point>, climbs: List<Double>): FeatureCollection =
        FeatureCollection.fromFeatures(
            TrackBands.runsOfClimbs(climbs)
                .groupBy({ it.band }, { LineString.fromLngLats(places.slice(it.points)) })
                .map { (band, stretches) ->
                    Feature.fromGeometry(MultiLineString.fromLineStrings(stretches)).apply {
                        addNumberProperty(BAND, band.ordinal)
                    }
                },
        )

    /**
     * Adds the layers that draw such a line: a white edge, which keeps it
     * readable over any ground colour, then the bands, sink first and
     * climbs last, so that where the line crosses itself the thermals stay
     * on top.
     */
    fun addBandLayers(style: Style, source: String, edgeWidth: Float) {
        style.addLayer(line("$source-edge", source, halo, edgeWidth))
        for (band in ClimbBand.entries) {
            val layer = line("$source-${band.ordinal}", source, colour(band), 3.5f)
            layer.setFilter(Expression.eq(Expression.get(BAND), Expression.literal(band.ordinal)))
            style.addLayer(layer)
        }
    }

    private const val BAND = "band"

    // The app's colours as the map engine takes them.
    val accent: Int = Palette.accentOnMap.toArgb()
    val ink: Int = Palette.inkOnMap.toArgb()
    /** The map's cream halo, around words and under lines and signs: the map is light in both appearances. */
    val halo: Int = Palette.haloOnMap.toArgb()

    fun colour(band: ClimbBand): Int = band.color.toArgb()
}

/** How much of a map is hidden from its top and bottom edges, in pixels. */
data class Covered(val top: Int = 0, val bottom: Int = 0)

val Coordinate.point: Point get() = Point.fromLngLat(lon, lat)

val Coordinate.latLng: LatLng get() = LatLng(lat, lon)

/** A line through these places, in the map engine's own form. */
fun List<Coordinate>.lineString(): LineString = LineString.fromLngLats(map { it.point })
