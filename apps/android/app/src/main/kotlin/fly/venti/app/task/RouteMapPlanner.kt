package fly.venti.app.task

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.numbers
import fly.venti.app.flight.CompareWords
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSetup
import fly.venti.app.map.MapSurface
import fly.venti.app.map.lineString
import fly.venti.core.Coordinate
import fly.venti.core.RoutePlan
import fly.venti.core.planRoute
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.android.style.expressions.Expression
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point

/** What drawing a route says that is not the core's, and the rule for a tap. */
object RouteDrawing {
    const val TITLE = "Draw a route"
    const val ENTRY = "Draw a route on the map"
    const val UNDO = "Undo"
    const val BACK_TO_START = "Back to the start"
    const val CLEAR = "Clear"

    /** Two places of a route lie at least this far apart, metres: a finger that taps twice means one place. */
    const val APART_AT_LEAST_M = 150.0

    /** The places with a tapped place taken in, unless it lies on the place before. */
    fun add(places: List<Coordinate>, place: Coordinate): List<Coordinate> {
        val last = places.lastOrNull() ?: return listOf(place)
        val apart = CompareWords.metresBetween(last.lat, last.lon, place.lat, place.lon)
        return if (apart < APART_AT_LEAST_M) places else places + place
    }

    /** The legs in a line: "1  10.0 km E · 2  9.8 km NW". */
    fun legs(plan: RoutePlan): String =
        plan.legs.withIndex().joinToString(" · ") { (index, leg) -> "${index + 1}  ${leg.words}" }

    /** Whether "Back to the start" can be pressed: a route of three places or more that is not back there yet. */
    fun canGoBackToStart(places: List<Coordinate>): Boolean = places.size >= 3 && places.last() != places.first()
}

/**
 * Draws a route on the map: the pilot taps the places to fly by, in order,
 * and sees how long each leg is and what the route would score if it were
 * flown as drawn. Kept, it is the task the cockpit leads along. What the
 * route comes to is the core's.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RouteMapPlanner(tasks: TaskModel, onDismiss: () -> Unit) {
    var places by remember { mutableStateOf<List<Coordinate>>(emptyList()) }
    var problem by remember { mutableStateOf<String?>(null) }
    val plan = remember(places) { planRoute(places) }
    val map = remember { RouteDrawingMap { place ->
        problem = null
        places = RouteDrawing.add(places, place)
    } }
    BackHandler(onBack = onDismiss)

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(RouteDrawing.TITLE) },
                navigationIcon = {
                    IconButton(onClick = onDismiss) {
                        Icon(painterResource(R.drawable.ic_close), contentDescription = "Cancel")
                    }
                },
                actions = {
                    TextButton(
                        onClick = {
                            if (tasks.keepDrawnRoute(places)) onDismiss() else problem = tasks.problem
                        },
                        enabled = places.size >= 2,
                    ) { Text("Keep") }
                },
            )
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            MapSurface(
                content = map,
                modifier = Modifier
                    .fillMaxSize()
                    .testTag("route-map")
                    .semantics { contentDescription = "Map to draw the route on. Tap where the route goes next." },
                onUpdate = { map.show(places, plan) },
            )
            Surface(
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(8.dp).fillMaxWidth(),
                shape = MaterialTheme.shapes.large,
                color = MaterialTheme.colorScheme.surfaceContainerLow,
                shadowElevation = 6.dp,
            ) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        problem ?: plan.words,
                        style = MaterialTheme.typography.bodyMedium,
                        color = if (problem == null) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.error,
                        modifier = Modifier.testTag("route-words"),
                    )
                    if (plan.legs.isNotEmpty()) {
                        Text(
                            RouteDrawing.legs(plan),
                            style = MaterialTheme.typography.bodySmall.numbers,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 2,
                            modifier = Modifier.semantics {
                                contentDescription = "Legs"
                                stateDescription = RouteDrawing.legs(plan)
                            },
                        )
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        OutlinedButton(
                            onClick = {
                                problem = null
                                places = places.dropLast(1)
                            },
                            enabled = places.isNotEmpty(),
                        ) { Text(RouteDrawing.UNDO) }
                        Spacer(Modifier.width(8.dp))
                        // A triangle ends where it began: one tap instead of
                        // aiming a finger at the first place.
                        OutlinedButton(
                            onClick = {
                                problem = null
                                places = places + places.first()
                            },
                            enabled = RouteDrawing.canGoBackToStart(places),
                        ) { Text(RouteDrawing.BACK_TO_START) }
                        Spacer(Modifier.weight(1f))
                        IconButton(
                            onClick = {
                                problem = null
                                places = emptyList()
                            },
                            enabled = places.isNotEmpty(),
                        ) {
                            Icon(painterResource(R.drawable.ic_delete), contentDescription = RouteDrawing.CLEAR, modifier = Modifier.size(22.dp))
                        }
                    }
                }
            }
        }
    }
}

/**
 * The map a route is drawn on: the places tapped so far with their numbers,
 * the line through them, the route as it would be scored, and with one leg
 * the areas in which a third place makes an FAI triangle of it.
 */
private class RouteDrawingMap(private val onTap: (Coordinate) -> Unit) : MapContent {
    private var style: Style? = null
    private var view: MapView? = null
    private var places: List<Coordinate> = emptyList()
    private var plan: RoutePlan? = null
    private var drawn: Pair<List<Coordinate>, RoutePlan?>? = null

    override fun prepare(map: MapLibreMap, view: MapView) {
        // The northern Alps between Lake Constance and Salzburg, as the map tab begins.
        map.cameraPosition = CameraPosition.Builder().target(LatLng(47.45, 11.6)).zoom(7.4).build()
        map.addOnMapClickListener { at ->
            onTap(Coordinate(at.latitude, at.longitude))
            true
        }
    }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.style = style
        this.view = view
        style.addSource(GeoJsonSource(FAI))
        style.addLayer(
            MapSetup.line("route-fai-line", FAI, MapSetup.accent, 1.5f).withProperties(
                PropertyFactory.lineDasharray(arrayOf(3f, 2f)),
                PropertyFactory.lineOpacity(0.85f),
            ),
        )
        style.addSource(GeoJsonSource(DRAWN))
        style.addLayer(MapSetup.line("route-drawn-edge", DRAWN, MapSetup.WHITE, 6f))
        style.addLayer(MapSetup.line("route-drawn-line", DRAWN, MapSetup.ink, 3f))
        style.addSource(GeoJsonSource(SCORED))
        style.addLayer(
            MapSetup.line("route-scored-line", SCORED, MapSetup.accent, 2f)
                .withProperties(PropertyFactory.lineDasharray(arrayOf(3f, 2f))),
        )
        style.addSource(GeoJsonSource(PLACES))
        style.addLayer(MapSetup.dot("route-places-dot", PLACES, 11f, MapSetup.WHITE, MapSetup.ink))
        style.addLayer(
            SymbolLayer("route-places-number", PLACES).withProperties(
                PropertyFactory.textField(Expression.get(LABEL)),
                PropertyFactory.textFont(arrayOf("noto-sans-regular")),
                PropertyFactory.textSize(13f),
                PropertyFactory.textColor(MapSetup.ink),
                PropertyFactory.textAllowOverlap(true),
                PropertyFactory.textIgnorePlacement(true),
                PropertyFactory.textAnchor(Property.TEXT_ANCHOR_CENTER),
            ),
        )
        drawn = null
        draw()
    }

    fun show(places: List<Coordinate>, plan: RoutePlan) {
        this.places = places
        this.plan = plan
        draw()
    }

    private fun draw() {
        val style = style ?: return
        val view = view ?: return
        if (view.isDestroyed || !style.isFullyLoaded) return
        val plan = plan
        if (drawn == places to plan) return
        drawn = places to plan

        fun line(points: List<Coordinate>): List<Feature> =
            if (points.size >= 2) listOf(Feature.fromGeometry(points.lineString())) else emptyList()
        style.getSourceAs<GeoJsonSource>(FAI)
            ?.setGeoJson(FeatureCollection.fromFeatures(plan?.faiAreas.orEmpty().flatMap(::line)))
        style.getSourceAs<GeoJsonSource>(DRAWN)?.setGeoJson(FeatureCollection.fromFeatures(line(places)))
        style.getSourceAs<GeoJsonSource>(SCORED)?.setGeoJson(FeatureCollection.fromFeatures(line(plan?.score?.route.orEmpty())))
        // A place the route comes back to keeps its first number.
        val numbered = mutableListOf<Pair<Coordinate, Int>>()
        places.forEachIndexed { index, place ->
            if (numbered.none { it.first == place }) numbered += place to index + 1
        }
        style.getSourceAs<GeoJsonSource>(PLACES)?.setGeoJson(
            FeatureCollection.fromFeatures(
                numbered.map { (place, number) ->
                    Feature.fromGeometry(Point.fromLngLat(place.lon, place.lat)).apply { addStringProperty(LABEL, "$number") }
                },
            ),
        )
    }

    companion object {
        const val FAI = "route-fai"
        const val DRAWN = "route-drawn"
        const val SCORED = "route-scored"
        const val PLACES = "route-places"
        const val LABEL = "label"
    }
}
