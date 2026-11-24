package fly.venti.app.community

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.InputChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiLight
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Common
import fly.venti.app.design.OnChip
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.map.MapContent
import fly.venti.app.map.MapSurface
import fly.venti.app.pilotMessage
import fly.venti.core.Coordinate
import fly.venti.core.FlightSearch
import fly.venti.core.FlightSearchBounds
import fly.venti.core.GliderClassKind
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.RouteKind
import fly.venti.core.flightSearchBounds
import fly.venti.core.flightSearchWords
import fly.venti.core.gliderClassName
import fly.venti.core.publicFlightsSearchPath
import fly.venti.core.routeKindWords
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.LineString
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * The words of the flight search that are not the core's: the web's
 * (`SearchFilter.svelte`, `AreaSheet.svelte`, `words.ts`) and the
 * iPhone's (`FlightSearchViews.swift`), the same on all three.
 */
object FlightSearchWords {
    const val FILTER = "Filter"
    const val GLIDER = "Glider"
    const val ANY_CLASS = "Any class"
    const val KIND = "Kind"
    const val ANY_KIND = "Any kind"
    const val AT_LEAST = "At least"
    const val ANY_DISTANCE = "Any distance"
    const val COUNTRY = "Take-off in"
    const val WORLDWIDE = "Worldwide"
    const val AREA = "Area"
    const val ANYWHERE = "Anywhere"
    const val RADIUS = "Radius"
    const val DRAW = "Draw an area"
    const val USE = "Use this area"
    const val NO_AREA = "Remove the area"
    const val UNDO = "Undo"
    const val START_AGAIN = "Start again"
    const val CLEAR = "Clear"
    const val MAP_RADIUS = "Map: the place and the radius"
    const val MAP_DRAW = "Map: tap the corners of the area"
    const val TAP_CORNERS = "Tap the corners on the map."
    const val NONE_FOUND = "No flights match"
    const val NONE_FOUND_LINE = "Nobody shares a flight that matches these choices. Remove one, or widen the area."

    /** Over a ranking a search leaves empty. */
    const val NOBODY_RANKED = "Nobody is ranked"

    /** The ranking's choice of whose pilots: everyone, or one country's alone. */
    const val PILOTS_FROM = "Pilots from"
    const val GLOBAL = "Global"
    const val EVERY_COUNTRY = "Every country"

    /** "Remove EN C", for the × of a chip. */
    fun remove(words: String) = "Remove $words"

    /** "Filter, 2 on", for TalkBack. */
    fun count(on: Int) = if (on == 0) FILTER else "$FILTER, $on on"

    /** How many corners are drawn, against the most an area has. */
    fun corners(count: Int, most: Int): String = when {
        count == 0 -> TAP_CORNERS
        count < most -> "$count of at most $most corners"
        else -> "$most corners, the most an area has"
    }
}

/** Whether a search chooses anything: a search of everything is the list without one. */
val FlightSearch.isSearching: Boolean
    get() = (near != null && radiusKm != null) || area.isNotEmpty() || gliderClass != null || kind != null || minKm != null || !country.isNullOrBlank()

/** The least distances offered, kilometres: a few steps, as XContest's filter has them. */
internal val LEAST_KM = listOf(10.0, 25.0, 50.0, 100.0, 200.0)

/** One choice of a search, with the core's words for it alone, and the search without it. */
data class SearchChip(val words: String, val tag: String, val without: FlightSearch)

/** The choices of a search one by one, in the core's order and words: each chip says one and drops it. */
fun searchChips(search: FlightSearch): List<SearchChip> = buildList {
    fun words(alone: FlightSearch) = flightSearchWords(alone).firstOrNull().orEmpty()
    if (search.near != null && search.radiusKm != null) {
        add(SearchChip(words(FlightSearch(near = search.near, radiusKm = search.radiusKm)), "search-chip-near", search.copy(near = null, radiusKm = null)))
    }
    if (search.area.isNotEmpty()) add(SearchChip(words(FlightSearch(area = search.area)), "search-chip-area", search.copy(area = emptyList())))
    search.gliderClass?.let { add(SearchChip(words(FlightSearch(gliderClass = it)), "search-chip-class", search.copy(gliderClass = null))) }
    search.kind?.let { add(SearchChip(words(FlightSearch(kind = it)), "search-chip-kind", search.copy(kind = null))) }
    search.minKm?.let { add(SearchChip(words(FlightSearch(minKm = it)), "search-chip-least", search.copy(minKm = null))) }
    search.country?.takeIf { it.isNotBlank() }?.let { add(SearchChip(words(FlightSearch(country = it)), "search-chip-country", search.copy(country = null))) }
}

/** Why the core would not ask for a search, in its words; `null` when it would. */
fun searchRefused(search: FlightSearch): String? =
    runCatching { publicFlightsSearchPath(PublicFlightsOrder.NEWEST, null, null, null, search, null) }.exceptionOrNull()?.pilotMessage

/**
 * "Filter" over a list of public flights, with the count of what is on,
 * and a chip for each choice in the core's words, whose × drops it.
 * `before` stands first in the row: the ranking's "Pilots from".
 */
@Composable
fun FlightSearchBar(
    search: FlightSearch,
    onOpen: () -> Unit,
    onChange: (FlightSearch) -> Unit,
    modifier: Modifier = Modifier,
    before: @Composable () -> Unit = {},
) {
    val chips = searchChips(search)
    Row(
        modifier = modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        before()
        OutlinedButton(
            onClick = onOpen,
            modifier = Modifier
                .heightIn(min = VentiSize.touch)
                .testTag("flights-filter")
                .semantics { contentDescription = FlightSearchWords.count(chips.size) },
        ) {
            Icon(painterResource(R.drawable.ic_filter), contentDescription = null, modifier = Modifier.size(VentiSpace.s5))
            Spacer(Modifier.size(VentiSpace.s1))
            Text(if (chips.isEmpty()) FlightSearchWords.FILTER else "${FlightSearchWords.FILTER} · ${chips.size}", style = MaterialTheme.typography.labelLarge.numbers)
        }
        for (chip in chips) {
            InputChip(
                selected = true,
                onClick = { onChange(chip.without) },
                label = { Text(chip.words, maxLines = 1) },
                trailingIcon = { Icon(painterResource(R.drawable.ic_close), contentDescription = FlightSearchWords.remove(chip.words), modifier = Modifier.size(VentiSpace.s4)) },
                modifier = Modifier.heightIn(min = VentiSize.touch).testTag(chip.tag),
            )
        }
    }
}

/**
 * The choices of the search in a sheet: the glider's class, the kind of
 * route, the least distance, the take-off's country, and the area, a mark
 * with a radius or corners drawn on a map. "Done" hands the search back,
 * as the iPhone's does; the core says why one cannot be asked.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun FlightSearchSheet(
    search: FlightSearch,
    /** Where a new circle starts: where the map tab rests. */
    middle: Coordinate,
    onShow: (FlightSearch) -> Unit,
    onDismiss: () -> Unit,
) {
    val colours = Venti.colours
    val bounds = remember { flightSearchBounds() }
    var chosen by remember { mutableStateOf(search) }
    var picksArea by remember { mutableStateOf(false) }
    val refused = remember(chosen) { searchRefused(chosen) }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
        modifier = Modifier.semantics { paneTitle = FlightSearchWords.FILTER },
    ) {
        Column(Modifier.fillMaxHeight().testTag("flight-search")) {
            Row(Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    if (picksArea) FlightSearchWords.AREA else FlightSearchWords.FILTER,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                if (picksArea) {
                    TextButton(onClick = { picksArea = false }) { Text(Common.words.back) }
                } else {
                    TextButton(onClick = { chosen = FlightSearch() }, enabled = chosen.isSearching, modifier = Modifier.testTag("flight-search-clear")) { Text(FlightSearchWords.CLEAR) }
                }
            }
            if (picksArea) {
                AreaPicker(
                    chosen,
                    middle,
                    bounds,
                    onUse = { area ->
                        chosen = area
                        picksArea = false
                    },
                )
                return@Column
            }
            Column(
                modifier = Modifier
                    .weight(1f)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = VentiSpace.s5),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                Part(FlightSearchWords.AREA)
                val areaWords = searchChips(chosen.copy(gliderClass = null, kind = null, minKm = null, country = null)).firstOrNull()?.words
                OutlinedButton(
                    onClick = { picksArea = true },
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("flight-search-area"),
                ) {
                    Icon(painterResource(R.drawable.venti_map), contentDescription = null, modifier = Modifier.size(VentiSpace.s5))
                    Spacer(Modifier.size(VentiSpace.s2))
                    Text(areaWords ?: FlightSearchWords.ANYWHERE)
                }
                if (areaWords != null) {
                    TextButton(onClick = { chosen = chosen.copy(near = null, radiusKm = null, area = emptyList()) }, modifier = Modifier.testTag("flight-search-no-area")) {
                        Text(FlightSearchWords.NO_AREA)
                    }
                }

                Part(FlightSearchWords.GLIDER)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    OnChip(chosen.gliderClass == null, { chosen = chosen.copy(gliderClass = null) }, FlightSearchWords.ANY_CLASS, Modifier.testTag("search-class-any"))
                    for (kind in GliderClassKind.entries) {
                        OnChip(chosen.gliderClass == kind, { chosen = chosen.copy(gliderClass = kind) }, gliderClassName(kind), Modifier.testTag("search-class-${kind.name.lowercase()}"))
                    }
                }

                Part(FlightSearchWords.KIND)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    OnChip(chosen.kind == null, { chosen = chosen.copy(kind = null) }, FlightSearchWords.ANY_KIND, Modifier.testTag("search-kind-any"))
                    for (kind in RouteKind.entries) {
                        OnChip(chosen.kind == kind, { chosen = chosen.copy(kind = kind) }, routeKindWords(kind), Modifier.testTag("search-kind-${kind.name.lowercase()}"))
                    }
                }

                Part(FlightSearchWords.AT_LEAST)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    OnChip(chosen.minKm == null, { chosen = chosen.copy(minKm = null) }, FlightSearchWords.ANY_DISTANCE, Modifier.testTag("search-least-any"))
                    for (km in LEAST_KM.filter { it <= bounds.maxMinKm }) {
                        OnChip(chosen.minKm == km, { chosen = chosen.copy(minKm = km) }, "${km.roundToInt()} km", Modifier.testTag("search-least-${km.roundToInt()}"))
                    }
                }

                Part(FlightSearchWords.COUNTRY)
                CountryMenu(chosen.country, onChoose = { chosen = chosen.copy(country = it) })

                refused?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = colours.danger, modifier = Modifier.testTag("flight-search-refused")) }
                Spacer(Modifier.height(VentiSpace.s2))
            }
            Button(
                onClick = { onShow(chosen) },
                enabled = refused == null,
                modifier = Modifier
                    .fillMaxWidth()
                    .navigationBarsPadding()
                    .padding(horizontal = VentiSpace.s5, vertical = VentiSpace.s3)
                    .heightIn(min = VentiSize.touch)
                    .testTag("flight-search-show"),
            ) { Text(Common.words.done) }
        }
    }
}

@Composable
private fun Part(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        color = Venti.colours.textSecondary,
        modifier = Modifier.padding(top = VentiSpace.s3).semantics { heading() },
    )
}

/**
 * Whose pilots the ranking ranks, as on the web: "Global", or one
 * country's alone (a national ranking, by the country the pilots set on
 * their account), the reader's own first, then every country.
 */
@Composable
fun PilotsFromChip(country: String?, own: String?, onChoose: (String?) -> Unit, modifier: Modifier = Modifier) {
    var isOpen by remember { mutableStateOf(false) }
    val shown = country?.let(Countries::named) ?: FlightSearchWords.GLOBAL
    Box(modifier) {
        FilterChip(
            selected = country != null,
            onClick = { isOpen = true },
            label = { Text(shown, maxLines = 1, overflow = TextOverflow.Ellipsis) },
            trailingIcon = { Icon(painterResource(R.drawable.ic_arrow_drop_down), contentDescription = null, modifier = Modifier.size(VentiSpace.s5)) },
            modifier = Modifier
                .heightIn(min = VentiSize.touch)
                .testTag("ranking-pilots-from")
                .semantics {
                    contentDescription = FlightSearchWords.PILOTS_FROM
                    stateDescription = country?.let(Countries::name) ?: FlightSearchWords.GLOBAL
                },
        )
        // Each opening starts at its head, Global and the reader's own country: the menu would keep its scroll.
        val scroll = remember(isOpen) { ScrollState(0) }
        DropdownMenu(expanded = isOpen, onDismissRequest = { isOpen = false }, scrollState = scroll) {
            fun choose(code: String?) {
                isOpen = false
                onChoose(code)
            }
            DropdownMenuItem(text = { Text(FlightSearchWords.GLOBAL) }, onClick = { choose(null) }, modifier = Modifier.testTag("pilots-from-global"))
            own?.takeIf { Countries.flag(it) != null }?.let { code ->
                DropdownMenuItem(text = { Text(Countries.named(code)) }, onClick = { choose(code.uppercase()) }, modifier = Modifier.testTag("pilots-from-own"))
            }
            HorizontalDivider(color = Venti.colours.border)
            Text(
                FlightSearchWords.EVERY_COUNTRY,
                style = MaterialTheme.typography.labelMedium,
                color = Venti.colours.textSecondary,
                modifier = Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2).semantics { heading() },
            )
            for ((code, _) in Countries.all) {
                DropdownMenuItem(text = { Text(Countries.named(code)) }, onClick = { choose(code) }, modifier = Modifier.testTag("pilots-from-$code"))
            }
        }
    }
}

/** The take-off's country: worldwide, or one of every country there is, by name with its flag. */
@Composable
private fun CountryMenu(country: String?, onChoose: (String?) -> Unit) {
    var isOpen by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(
            onClick = { isOpen = true },
            modifier = Modifier.heightIn(min = VentiSize.touch).testTag("search-country"),
        ) {
            Text(country?.let(Countries::named) ?: FlightSearchWords.WORLDWIDE, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Icon(painterResource(R.drawable.ic_arrow_drop_down), contentDescription = null)
        }
        DropdownMenu(expanded = isOpen, onDismissRequest = { isOpen = false }) {
            DropdownMenuItem(text = { Text(FlightSearchWords.WORLDWIDE) }, onClick = {
                isOpen = false
                onChoose(null)
            })
            for ((code, _) in Countries.all) {
                DropdownMenuItem(
                    text = { Text(Countries.named(code)) },
                    onClick = {
                        isOpen = false
                        onChoose(code)
                    },
                    modifier = Modifier.testTag("search-country-$code"),
                )
            }
        }
    }
}

/**
 * Where the flights took off: a mark and a radius around it, or an area
 * drawn on the map, corner by corner. "Use this area" hands it back; the
 * bounds are the server's, as the core says them.
 */
@Composable
private fun AreaPicker(search: FlightSearch, middle: Coordinate, bounds: FlightSearchBounds, onUse: (FlightSearch) -> Unit) {
    val colours = Venti.colours
    var draws by remember { mutableStateOf(search.area.isNotEmpty()) }
    var centre by remember { mutableStateOf(search.near ?: middle) }
    var radiusKm by remember { mutableStateOf(search.radiusKm ?: DEFAULT_RADIUS_KM.coerceIn(bounds.minRadiusKm, bounds.maxRadiusKm)) }
    var corners by remember { mutableStateOf(search.area) }
    val most = bounds.maxAreaPoints.toInt()
    val area = if (draws) search.copy(near = null, radiusKm = null, area = corners) else search.copy(near = centre, radiusKm = radiusKm, area = emptyList())
    val refused = if (draws && corners.size < 3) null else searchRefused(area)
    val content = remember { AreaMap(centre) }
    content.onTap = { at ->
        if (draws) {
            if (corners.size < most) corners = corners + at
        } else {
            centre = at
        }
    }
    content.show(if (draws) null else centre, radiusKm, if (draws) corners else emptyList())

    Column(Modifier.fillMaxHeight().padding(horizontal = VentiSpace.s5), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        SegmentedPills(listOf(FlightSearchWords.RADIUS, FlightSearchWords.DRAW), if (draws) 1 else 0, Modifier.fillMaxWidth().testTag("search-area-mode")) { index ->
            draws = index == 1
        }
        MapSurface(
            content = content,
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .testTag("search-area-map")
                .semantics { contentDescription = if (draws) FlightSearchWords.MAP_DRAW else FlightSearchWords.MAP_RADIUS },
            onUpdate = { content.draw() },
        )
        if (draws) {
            Text(
                FlightSearchWords.corners(corners.size, most),
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = colours.textSecondary,
                modifier = Modifier.testTag("search-area-corners"),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                TextButton(onClick = { corners = corners.dropLast(1) }, enabled = corners.isNotEmpty()) { Text(FlightSearchWords.UNDO) }
                TextButton(onClick = { corners = emptyList() }, enabled = corners.isNotEmpty()) { Text(FlightSearchWords.START_AGAIN) }
            }
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                Text(FlightSearchWords.RADIUS, style = MaterialTheme.typography.bodyMedium)
                Slider(
                    value = radiusKm.toFloat(),
                    onValueChange = { radiusKm = it.roundToInt().toDouble() },
                    valueRange = bounds.minRadiusKm.toFloat()..bounds.maxRadiusKm.toFloat(),
                    modifier = Modifier
                        .weight(1f)
                        .testTag("search-area-radius")
                        .semantics { stateDescription = "${radiusKm.roundToInt()} km" },
                )
                Text("${radiusKm.roundToInt()} km", style = MaterialTheme.typography.bodyMedium.numbers)
            }
        }
        refused?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = colours.danger, modifier = Modifier.testTag("search-area-refused")) }
        Button(
            onClick = { onUse(area) },
            enabled = refused == null && (!draws || corners.size >= 3),
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(bottom = VentiSpace.s3)
                .heightIn(min = VentiSize.touch)
                .testTag("search-area-use"),
        ) { Text(FlightSearchWords.USE) }
    }
}

/** Where a new circle starts its radius, kilometres. */
private const val DEFAULT_RADIUS_KM = 10.0

/**
 * The map of the area: the circle round its mark, or the corners drawn and
 * the shape they make, in the app's colour; a tap is told where.
 */
private class AreaMap(private val opensAt: Coordinate) : MapContent {
    var onTap: (Coordinate) -> Unit = {}
    private var style: Style? = null
    private var centre: Coordinate? = null
    private var radiusKm = 0.0
    private var corners: List<Coordinate> = emptyList()

    override fun prepare(map: MapLibreMap, view: MapView) {
        map.uiSettings.isRotateGesturesEnabled = false
        map.uiSettings.isTiltGesturesEnabled = false
        map.cameraPosition = CameraPosition.Builder().target(LatLng(opensAt.lat, opensAt.lon)).zoom(OPENS_ZOOM).build()
        map.addOnMapClickListener { at ->
            onTap(Coordinate(lat = at.latitude, lon = at.longitude))
            true
        }
    }

    override fun styleLoaded(map: MapLibreMap, style: Style, view: MapView) {
        this.style = style
        style.addSource(GeoJsonSource(SHAPE))
        style.addSource(GeoJsonSource(MARKS))
        val ink = VentiLight.primary.toArgb()
        style.addLayer(FillLayer(SHAPE_FILL, SHAPE).withProperties(PropertyFactory.fillColor(ink), PropertyFactory.fillOpacity(0.15f)))
        style.addLayer(LineLayer(SHAPE_LINE, SHAPE).withProperties(PropertyFactory.lineColor(ink), PropertyFactory.lineWidth(2f)))
        style.addLayer(
            CircleLayer(MARKS, MARKS).withProperties(
                PropertyFactory.circleColor(ink),
                PropertyFactory.circleRadius(5f),
                PropertyFactory.circleStrokeColor(VentiLight.surface.toArgb()),
                PropertyFactory.circleStrokeWidth(2f),
            ),
        )
        draw()
    }

    fun show(centre: Coordinate?, radiusKm: Double, corners: List<Coordinate>) {
        this.centre = centre
        this.radiusKm = radiusKm
        this.corners = corners
    }

    fun draw() {
        val style = style ?: return
        val shape: List<Feature>
        val marks: List<Point>
        val centre = centre
        if (centre != null) {
            val ring = (0..CIRCLE_STEPS).map { step ->
                val angle = 2 * Math.PI * step / CIRCLE_STEPS
                val dLat = radiusKm / KM_PER_DEGREE * cos(angle)
                val dLon = radiusKm / (KM_PER_DEGREE * cos(Math.toRadians(centre.lat))) * sin(angle)
                Point.fromLngLat(centre.lon + dLon, centre.lat + dLat)
            }
            shape = listOf(Feature.fromGeometry(Polygon.fromLngLats(listOf(ring))))
            marks = listOf(Point.fromLngLat(centre.lon, centre.lat))
        } else {
            marks = corners.map { Point.fromLngLat(it.lon, it.lat) }
            shape = when {
                marks.size >= 3 -> listOf(Feature.fromGeometry(Polygon.fromLngLats(listOf(marks + marks.first()))))
                marks.size == 2 -> listOf(Feature.fromGeometry(LineString.fromLngLats(marks)))
                else -> emptyList()
            }
        }
        style.getSourceAs<GeoJsonSource>(SHAPE)?.setGeoJson(FeatureCollection.fromFeatures(shape))
        style.getSourceAs<GeoJsonSource>(MARKS)?.setGeoJson(FeatureCollection.fromFeatures(marks.map(Feature::fromGeometry)))
    }

    private companion object {
        const val SHAPE = "search-area"
        const val SHAPE_FILL = "search-area-fill"
        const val SHAPE_LINE = "search-area-line"
        const val MARKS = "search-area-marks"
        const val CIRCLE_STEPS = 64
        const val KM_PER_DEGREE = 111.32
        const val OPENS_ZOOM = 8.0
    }
}
