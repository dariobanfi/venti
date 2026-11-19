package fly.venti.app.map

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import fly.venti.app.R
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.sites.SiteSheet
import fly.venti.app.sites.SitesModel
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.wind.WindModel
import fly.venti.app.wind.WindServer
import fly.venti.app.wind.WindSheet
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.SiteView
import fly.venti.core.WindStation

/**
 * The map tab: terrain, live wind from the stations of the pilot's Venti
 * server, the flying sites it knows, the pilot's airspace and the pilot's
 * own flights.
 */
@Composable
fun MapScreen(
    logbook: LogbookModel,
    wind: WindModel,
    airspace: AirspaceModel,
    sites: SitesModel,
    waypoints: WaypointsModel,
    bars: PaddingValues,
) {
    val context = LocalContext.current
    var isFollowing by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var showsStations by remember { mutableStateOf(false) }
    var chosen by remember { mutableStateOf<WindStation?>(null) }
    var showsSites by remember { mutableStateOf(false) }
    var chosenSite by remember { mutableStateOf<SiteView?>(null) }
    val map = remember { OverviewMap() }
    map.onFollowingEnded = { isFollowing = false }
    var region by remember { mutableStateOf<Bounds?>(null) }
    map.onRegion = { shown ->
        region = shown
        wind.show(shown)
        sites.show(shown)
        // The airspace around what the pilot is looking at, unless it is
        // the whole continent. The model asks only when it is due.
        if (shown.north - shown.south < 4) {
            airspace.refreshSoon(
                Coordinate(lat = (shown.south + shown.north) / 2, lon = (shown.west + shown.east) / 2),
            )
        }
    }
    map.onStation = { id -> chosen = wind.stations.firstOrNull { it.id == id } }
    map.onSite = { id -> chosenSite = sites.sites.firstOrNull { it.id == id } }

    val askForPosition = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { answers ->
        if (answers.values.any { it }) {
            isFollowing = true
            problem = positionProblem(context)
        } else {
            problem = NOT_ALLOWED
        }
    }

    // Asks again every minute while the map is on screen, and at once when
    // the pilot changes the server. Not while the app is out of sight.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(wind.address, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { wind.keepCurrent() }
    }

    val statusBar = WindowInsets.statusBars.getTop(LocalDensity.current)
    val outlines = logbook.view.years.flatMap { year -> year.flights.map { it.outline } }
    val stations = wind.stations
    // The airspace in the part of the map in view. Zoomed far out, a
    // country's airspace is a tangle and is not drawn, so it is not asked
    // for either.
    val all = airspace.set
    val showsAirspace = region?.let { it.east - it.west < 6 } == true
    val airspaceInView = remember(region, all) {
        region?.takeIf { showsAirspace }?.let(airspace::shapes).orEmpty()
    }
    val flyingSites = sites.sites
    // Whose wind, airspace and sites are on the map. Their licences ask for
    // the credit next to the data; the full wording is on each station's
    // and each site's page and on the airspace screen.
    val credits = listOfNotNull(
        wind.creditLine,
        airspace.mapCredit.takeIf { showsAirspace },
        sites.creditLine.takeIf { region?.let(SitesModel::showsSites) == true },
    )

    Box(Modifier.fillMaxSize().padding(bottom = bars.calculateBottomPadding())) {
        MapSurface(
            content = map,
            modifier = Modifier.fillMaxSize().semantics { contentDescription = "Map" },
            onUpdate = {
                map.show(outlines, stations, airspaceInView, flyingSites, Covered(top = statusBar))
                map.follow(isFollowing)
            },
        )
        StatusBarScrim()

        wind.notice?.let { text ->
            Notice(text, Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(16.dp, 8.dp))
        }
        if (credits.isNotEmpty()) {
            // Clear of the map's own credits button on the left and of
            // the round buttons on the right.
            Credits(credits, Modifier.align(Alignment.BottomStart).padding(start = 44.dp, end = 88.dp, bottom = 8.dp))
        }

        Column(
            modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (wind.server is WindServer.Usable) {
                // Everything the arrows say, as a list: for reading the
                // numbers side by side, and for pilots who use TalkBack.
                MapButton(R.drawable.ic_air, "Wind stations") { showsStations = true }
            }
            if (flyingSites.isNotEmpty()) {
                // The signs on the map, as a list.
                MapButton(R.drawable.ic_pin_drop, "Flying sites") { showsSites = true }
            }
            // Centres the map on the pilot and follows until the map is dragged.
            MapButton(
                icon = if (isFollowing) R.drawable.ic_my_location else R.drawable.ic_location_searching,
                description = "Show my position",
                tint = if (isFollowing) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
            ) {
                when {
                    isFollowing -> isFollowing = false
                    mayUsePosition(context) -> {
                        isFollowing = true
                        problem = positionProblem(context)
                    }
                    else -> askForPosition.launch(POSITION_PERMISSIONS)
                }
            }
        }
    }

    if (showsStations || chosen != null) {
        WindSheet(
            wind = wind,
            showsList = showsStations,
            chosen = chosen,
            onChoose = { chosen = it },
            onDismiss = {
                chosen = null
                showsStations = false
            },
        )
    }

    if (showsSites || chosenSite != null) {
        SiteSheet(
            sites = sites,
            waypoints = waypoints,
            wind = wind,
            showsList = showsSites,
            chosen = chosenSite,
            onChoose = { chosenSite = it },
            onDismiss = {
                chosenSite = null
                showsSites = false
            },
        )
    }

    problem?.let { text ->
        AlertDialog(
            onDismissRequest = { problem = null },
            title = { Text("Position unavailable") },
            text = { Text(text) },
            confirmButton = { TextButton(onClick = { problem = null }) { Text("OK") } },
        )
    }
}

/** One of the round buttons over the map. */
@Composable
private fun MapButton(
    @DrawableRes icon: Int,
    description: String,
    tint: Color = MaterialTheme.colorScheme.onSurface,
    onClick: () -> Unit,
) {
    FloatingActionButton(
        onClick = onClick,
        shape = CircleShape,
        containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        contentColor = tint,
    ) {
        Icon(painterResource(icon), contentDescription = description)
    }
}

/** A line over the top of the map about wind that is missing or not current. */
@Composable
private fun Notice(text: String, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.testTag("wind-notice"),
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        shadowElevation = 2.dp,
    ) {
        Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(
                painterResource(R.drawable.ic_warning),
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.width(8.dp))
            Text(text, style = MaterialTheme.typography.bodySmall)
        }
    }
}

/**
 * Whose wind, airspace and sites are on the map, in small type at its lower
 * edge: a line for each, so that none is cut short by another.
 */
@Composable
private fun Credits(lines: List<String>, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.testTag("wind-credits"),
        shape = RoundedCornerShape(10.dp),
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.85f),
    ) {
        Column(Modifier.padding(horizontal = 8.dp, vertical = 4.dp)) {
            for (line in lines) {
                Text(
                    line,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    // Two providers' names do not fit one line of a phone.
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

private val POSITION_PERMISSIONS =
    arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)

private const val NOT_ALLOWED =
    "Venti is not allowed to use your position. You can allow it in the phone's Settings, " +
        "under Apps, Venti, Permissions."

/** For the map an approximate position will do. */
private fun mayUsePosition(context: Context): Boolean =
    POSITION_PERMISSIONS.any { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }

/** Words for the pilot if no position can come, or `null`. */
private fun positionProblem(context: Context): String? {
    val manager = context.getSystemService(LocationManager::class.java)
    val anyProvider = manager?.getProviders(true).orEmpty().any { it != LocationManager.PASSIVE_PROVIDER }
    return if (anyProvider) null else "Your position could not be found. Location is switched off on this phone."
}
