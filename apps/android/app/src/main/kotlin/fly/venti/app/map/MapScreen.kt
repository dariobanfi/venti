package fly.venti.app.map

import fly.venti.core.PublicFlight
import fly.venti.app.community.CommunityModel
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
import androidx.compose.runtime.mutableIntStateOf
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
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import app.venti.design.VentiSpace
import fly.venti.app.design.AerialButton
import fly.venti.app.design.LayerChip
import fly.venti.app.design.RoundButton
import fly.venti.app.design.Venti
import fly.venti.app.settings.Preferences
import fly.venti.app.sites.SiteActions
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.airspace.AirspaceOverSheet
import fly.venti.app.forecast.ForecastSheet
import fly.venti.app.forecast.ThermalMapCard
import fly.venti.app.forecast.ThermalMapModel
import fly.venti.app.forecast.ThermalHourSheet
import fly.venti.app.forecast.ThermalSquareSheet
import fly.venti.core.ThermalHoursLayer
import fly.venti.core.ThermalMapCell
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.foundation.layout.height
import app.venti.design.VentiSize
import kotlinx.coroutines.launch
import fly.venti.app.groups.GroupsModel
import fly.venti.app.groups.PlansSheet
import fly.venti.app.live.GliderListSheet
import fly.venti.app.live.GliderSheet
import fly.venti.app.live.LiveModel
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.radar.RadarCard
import fly.venti.app.radar.RadarModel
import fly.venti.app.radar.RadarWords
import kotlinx.coroutines.Job
import fly.venti.app.sites.SiteSheet
import fly.venti.app.sites.SitesFilterCard
import fly.venti.app.sites.SitesFilterSheet
import fly.venti.app.sites.SitesFilterWords
import androidx.compose.runtime.SideEffect
import fly.venti.app.sites.KeptSitesModel
import fly.venti.app.sites.SitesModel
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.wind.WindModel
import fly.venti.app.wind.WindServer
import fly.venti.app.wind.WindSheet
import fly.venti.core.Bounds
import fly.venti.core.AirspaceOverPlace
import fly.venti.core.Coordinate
import fly.venti.core.SiteView
import fly.venti.core.WindStation
import kotlinx.coroutines.CoroutineScope

/**
 * The map tab: terrain, live wind from the stations of the pilot's Venti
 * server, the flying sites it knows, the pilots who show their flight to
 * everyone, the pilot's airspace and the pilot's own flights. A long press
 * shows the forecast for the place pressed.
 */
@Composable
fun MapScreen(
    logbook: LogbookModel,
    wind: WindModel,
    airspace: AirspaceModel,
    sites: SitesModel,
    live: LiveModel,
    waypoints: WaypointsModel,
    /** The pilot's own take-offs, which a site's page marks. */
    keptSites: KeptSitesModel,
    /** The pilot's own account, which a report of a glider's track is sent from. */
    account: AccountModel,
    /** The pilot's groups and their plans for a flying day. */
    groups: GroupsModel,
    /** Where a report is sent from, so that it goes on when its form is closed. */
    requests: CoroutineScope,
    bars: PaddingValues,
    /** Everyone's flights on Venti, for what was flown from a site. */
    community: CommunityModel? = null,
    /** Opens a flight another pilot shares, on its own page. */
    onOpenFlight: (PublicFlight) -> Unit = {},
    /** Where the layers the pilot switched off are remembered. */
    preferences: Preferences? = null,
    /** The pages of a site that lead elsewhere: planning a route from it, and starting a flight there. */
    siteActions: SiteActions? = null,
    /** The kind of day, square by square, for the "Forecast" layer. */
    thermalMap: ThermalMapModel? = null,
    /** The rain radar, for the "Radar" layer. */
    radar: RadarModel? = null,
) {
    val context = LocalContext.current
    // The layers the chips switch, as the pilot left them.
    var hidesWind by remember { mutableStateOf(preferences?.mapHidesWind == true) }
    var hidesAirspace by remember { mutableStateOf(preferences?.mapHidesAirspace == true) }
    var hidesSites by remember { mutableStateOf(preferences?.mapHidesSites == true) }
    var hidesPilots by remember { mutableStateOf(preferences?.mapHidesPilots == true) }
    // The ground: the drawn map, or photographs from above. One choice for every map.
    var aerial by remember { mutableStateOf(preferences?.mapShowsAerial == true) }
    // Who made the photographs in view, as the map last said.
    var photos by remember { mutableStateOf<String?>(null) }
    // The thermal map, where the style has it: only with the app's domain set.
    var thermals by remember { mutableStateOf(preferences?.mapShowsThermals == true) }
    val offersThermals = remember { MapSetup.offersThermals(context) }
    // The forecast map, the kind of day square by square. Off until the pilot switches it on.
    var forecast by remember { mutableStateOf(preferences?.mapShowsForecast == true) }
    // The rain radar. Off until the pilot switches it on.
    var rain by remember { mutableStateOf(preferences?.mapShowsRadar == true) }
    // A square of it the pilot tapped: its sheet shows over the map.
    var square by remember { mutableStateOf<ThermalMapCell?>(null) }
    // A square of the forecast map by the hour, tapped.
    var hourSquare by remember { mutableStateOf<Pair<Coordinate, UInt?>?>(null) }
    val asking = rememberCoroutineScope()
    var isFollowing by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var showsStations by remember { mutableStateOf(false) }
    var chosen by remember { mutableStateOf<WindStation?>(null) }
    var showsSites by remember { mutableStateOf(false) }
    var showsPlans by remember { mutableStateOf(false) }
    // A plan whose mark on the map was tapped: the board opens at it.
    var planOnMap by remember { mutableStateOf<String?>(null) }
    var chosenSite by remember { mutableStateOf<SiteView?>(null) }
    // The filter of the sites, open over the map or over the list.
    var showsSitesFilter by remember { mutableStateOf(false) }
    var showsGliders by remember { mutableStateOf(false) }
    // The glider whose page is open, by its track's identifier: the page
    // shows it as the newest answer has it.
    var chosenGlider by remember { mutableStateOf<String?>(null) }
    // A place the pilot pressed on, to show its forecast.
    var pressed by remember { mutableStateOf<Coordinate?>(null) }
    // What lies over a place tapped where airspace is drawn.
    var airspaceOver by remember { mutableStateOf<AirspaceOverPlace?>(null) }
    // The height the maps draw airspace up to: one choice for the maps.
    var airspaceCeiling by remember { mutableStateOf(preferences?.mapAirspaceCeilingM) }
    val map = remember { OverviewMap() }
    map.onFollowingEnded = { isFollowing = false }
    var region by remember { mutableStateOf<Bounds?>(null) }
    map.onRegion = { shown ->
        region = shown
        wind.show(shown)
        sites.show(shown)
        live.show(shown)
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
    map.onGlider = { id -> chosenGlider = id }
    map.onSquare = { cell -> square = cell }
    map.onHourSquare = { place, cell ->
        // Only where the map's grid reaches: the core finds the square.
        if (thermalMap?.hours?.square(cell, place.lat, place.lon) != null) hourSquare = place to cell
    }
    map.onAirspace = { place -> airspaceOver = airspace.over(place) }
    map.onPlan = { id ->
        planOnMap = id
        showsPlans = true
    }
    // The board is asked for when the map shows, signed in: the plans of
    // today and tomorrow that say where are marked on it.
    LaunchedEffect(account.isSignedIn) {
        if (account.isSignedIn) groups.load()
    }
    // A long press: the forecast for the place under the finger, where
    // there is a server to ask.
    map.onPlace = { place ->
        if (wind.server is WindServer.Usable) {
            pressed = place
        }
    }

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
    // The forecast map is asked for when its layer is switched on, and
    // again once it is half an hour old, while the map is on screen.
    val hasServer = wind.server is WindServer.Usable
    val showsForecast = forecast && hasServer && thermalMap != null
    LaunchedEffect(showsForecast, wind.address, lifecycle) {
        if (showsForecast && thermalMap != null) {
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { thermalMap.keepCurrent() }
        }
    }
    // The rain radar's frames are asked for when its layer is switched on
    // and when the map comes back to the front, and again every five
    // minutes while it is there.
    val showsRadar = rain && hasServer && radar != null
    LaunchedEffect(showsRadar, wind.address, lifecycle) {
        if (showsRadar) {
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { radar.keepCurrent() }
        }
    }
    // The picture of the frame looked at: fetched when the frame changes,
    // and again when the pilot asks.
    var pictureAttempt by remember { mutableIntStateOf(0) }
    LaunchedEffect(showsRadar, radar?.frame?.path, pictureAttempt) {
        if (showsRadar) radar.showFrame()
    }
    // The frames played through, until they end or the pilot stops them.
    var playing by remember { mutableStateOf<Job?>(null) }
    // Other pilots are asked for again every quarter of a minute while the
    // map is on screen, and not when it is not.
    LaunchedEffect(wind.address, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { live.keepCurrent() }
    }

    val statusBar = WindowInsets.statusBars.getTop(LocalDensity.current)
    val outlines = logbook.view.years.flatMap { year -> year.flights.map { it.outline } }
    val stations = if (hidesWind) emptyList() else wind.stations
    // The airspace in the part of the map in view. Zoomed far out, a
    // country's airspace is a tangle and is not drawn, so it is not asked
    // for either.
    val all = airspace.set
    val showsAirspace = region?.let { it.east - it.west < 6 } == true
    val airspaceInView = remember(region, all, airspaceCeiling) {
        region?.takeIf { showsAirspace && !hidesAirspace }?.let { airspace.shapes(it, airspaceCeiling) }.orEmpty()
    }
    // What the pilot asks of the sites, as they left it. The wind of
    // "Measured now" is the station's nearest the middle of the map.
    LaunchedEffect(sites, preferences) {
        if (preferences != null) sites.filter = SitesFilterWords.read(preferences.mapSitesFilter)
    }
    val middle = region?.let { Coordinate(lat = (it.south + it.north) / 2, lon = (it.west + it.east) / 2) }
    val windStations = wind.stations
    val measuredNow = remember(windStations, middle) { middle?.let { SitesFilterWords.measuredNow(windStations, it) } }
    SideEffect { sites.measuredNowDeg = measuredNow }
    // The sites the filter leaves: those that fit as ever, those whose
    // source does not say dimmed, the rest not drawn. Which is the core's.
    val filtered = sites.filtered
    val hasSites = !hidesSites && sites.sites.isNotEmpty()
    val flyingSites = if (hidesSites) emptyList() else filtered.fitting
    val unsaidSites = if (hidesSites) emptyList() else filtered.unsaid
    val gliders = if (hidesPilots) emptyList() else live.gliders
    // Where the pilot's groups plan to fly today and tomorrow.
    val pins = groups.pins()
    // The flight of the glider whose page is open, as far as the server
    // has handed it out.
    val followed = live.followed?.track.orEmpty()
    // Whose wind, airspace and sites are on the map. Their licences ask for
    // the credit next to the data; the full wording is on each station's
    // and each site's page and on the airspace screen.
    val credits = listOfNotNull(
        wind.creditLine.takeIf { !hidesWind },
        airspace.mapCredit.takeIf { showsAirspace && !hidesAirspace },
        sites.creditLine.takeIf { region?.let(SitesModel::showsSites) == true && !hidesSites },
        // Who made the photographs, by name, while they show.
        photos.takeIf { aerial },
        MapSetup.THERMALS_CREDIT.takeIf { thermals && offersThermals },
        thermalMap?.creditLine?.takeIf { showsForecast },
        // The radar's source asks for its credit directly at the data.
        radar?.creditLine?.takeIf { showsRadar },
    )
    // The forecast map by the hour: the layer and the level as the pilot
    // left them, the hour not.
    val hours = thermalMap?.hours
    LaunchedEffect(hours) {
        if (hours != null && preferences != null) {
            hours.layer = preferences.mapForecastLayer
                ?.let { name -> ThermalHoursLayer.entries.firstOrNull { it.name == name } }
                ?: ThermalHoursLayer.THERMALS
            hours.heightM = preferences.mapForecastHeightM
        }
    }
    var hoursAttempt by remember { mutableIntStateOf(0) }
    LaunchedEffect(showsForecast, thermalMap?.map, thermalMap?.day?.date, hours?.layer, hours?.heightM, hoursAttempt) {
        if (showsForecast) thermalMap?.showHours()
    }
    val byTheHour = showsForecast && hours?.isOn == true
    val hourShapes = if (byTheHour) hours?.shapes else null
    val hourLegend = thermalMap?.map?.layers?.firstOrNull { it.layer == hours?.layer }?.legend.orEmpty()
    // The squares of the day the pilot looks at; a day's squares are one
    // list from the core, handed over whole. Not while an hour is shown.
    val squares = if (showsForecast && !byTheHour) thermalMap?.day?.cells.orEmpty() else emptyList()

    Box(Modifier.fillMaxSize().padding(bottom = bars.calculateBottomPadding())) {
        MapSurface(
            content = map,
            modifier = Modifier.fillMaxSize().semantics { contentDescription = "Map" },
            onPhotosCredit = { photos = it },
            onUpdate = {
                map.showAerial(aerial)
                map.showThermals(thermals && offersThermals)
                map.showForecast(squares)
                map.showHours(hourShapes, hourLegend, byTheHour)
                map.showRadar(if (showsRadar) radar.shown else null)
                map.show(outlines, stations, airspaceInView, flyingSites, gliders, followed, Covered(top = statusBar), pins, unsaidSites)
                map.follow(isFollowing)
            },
        )
        StatusBarScrim()

        Column(Modifier.align(Alignment.TopCenter).statusBarsPadding()) {
            // The chips that switch the map's layers: what Venti gives the
            // map, each only where there is something to switch.
            Row(
                modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = VentiSpace.s4),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                // The kind of day first, as on the iPhone and the web.
                if (hasServer && thermalMap != null) {
                    LayerChip("Forecast", R.drawable.venti_forecast, forecast, onChange = { on ->
                        forecast = on
                        preferences?.mapShowsForecast = on
                    }, Modifier.testTag("layer-forecast"))
                }
                // What fell in the last hours, beside what is forecast.
                if (hasServer && radar != null) {
                    LayerChip(RadarWords.LAYER, R.drawable.ic_rainy, rain, onChange = { on ->
                        rain = on
                        preferences?.mapShowsRadar = on
                        if (!on) playing?.cancel()
                    }, Modifier.testTag("layer-radar"))
                }
                if (offersThermals) {
                    LayerChip("Thermals", R.drawable.venti_thermal, thermals, onChange = { on ->
                        thermals = on
                        preferences?.mapShowsThermals = on
                    }, Modifier.testTag("layer-thermals"))
                }
                if (hasServer) {
                    LayerChip("Wind", R.drawable.venti_wind, !hidesWind, onChange = { on ->
                        hidesWind = !on
                        preferences?.mapHidesWind = !on
                    }, Modifier.testTag("layer-wind"))
                }
                LayerChip("Airspace", R.drawable.venti_airspace, !hidesAirspace, onChange = { on ->
                    hidesAirspace = !on
                    preferences?.mapHidesAirspace = !on
                }, Modifier.testTag("layer-airspace"))
                if (hasServer) {
                    LayerChip("Sites", R.drawable.venti_site, !hidesSites, onChange = { on ->
                        hidesSites = !on
                        preferences?.mapHidesSites = !on
                    }, Modifier.testTag("layer-sites"))
                    LayerChip("Pilots", R.drawable.venti_live, !hidesPilots, onChange = { on ->
                        hidesPilots = !on
                        preferences?.mapHidesPilots = !on
                    }, Modifier.testTag("layer-pilots"))
                }
            }
            wind.notice?.takeIf { !hidesWind }?.let { text ->
                Notice(text, Modifier.align(Alignment.CenterHorizontally).padding(horizontal = VentiSpace.s4))
            }
        }
        Column(
            modifier = Modifier.align(Alignment.BottomStart),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        ) {
            if (hasSites && filtered.isOn) {
                // What the pilot asks of the sites and what that leaves,
                // while a filter is on: a tap opens the filter.
                SitesFilterCard(
                    filtered,
                    onOpen = { showsSitesFilter = true },
                    modifier = Modifier.padding(start = VentiSpace.s4, end = VentiSize.touch + VentiSpace.s4 * 2),
                )
            }
            if (showsRadar) {
                // The frames and the key of the rain radar, over the
                // forecast map's card when both layers show.
                RadarCard(
                    radar = radar,
                    onChoose = { place ->
                        playing?.cancel()
                        radar.choose(place)
                    },
                    onPlay = {
                        val job = playing
                        if (job?.isActive == true) {
                            job.cancel()
                        } else {
                            playing = asking.launch { radar.play() }
                        }
                    },
                    onTryAgain = { asking.launch { radar.load() } },
                    onTryAgainPicture = { pictureAttempt += 1 },
                    modifier = Modifier.padding(start = VentiSpace.s4, end = VentiSize.touch + VentiSpace.s4 * 2),
                )
            }
            if (showsForecast && thermalMap != null) {
                // The days and the key of the forecast map, beside the
                // round buttons and over the credits.
                ThermalMapCard(
                    state = thermalMap.state,
                    region = region,
                    onChoose = thermalMap::choose,
                    onTryAgain = { asking.launch { thermalMap.load() } },
                    modifier = Modifier.padding(start = VentiSpace.s4, end = VentiSize.touch + VentiSpace.s4 * 2),
                    hours = thermalMap.hours,
                    onLayer = { layer ->
                        thermalMap.hours.layer = layer
                        preferences?.mapForecastLayer = layer.name
                    },
                    onLevel = { height ->
                        thermalMap.hours.heightM = height
                        preferences?.mapForecastHeightM = height
                    },
                    onTryAgainHours = { hoursAttempt += 1 },
                )
            }
            if (credits.isNotEmpty()) {
                // Clear of the map's own credits button on the left and of
                // the round buttons on the right.
                Credits(credits, Modifier.padding(start = 44.dp, end = 88.dp, bottom = 8.dp))
            } else if (showsForecast || showsRadar || (hasSites && filtered.isOn)) {
                // The card stands clear of the map's own credits button.
                Spacer(Modifier.height(VentiSpace.s6))
            }
        }

        Column(
            modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (hasServer && !hidesWind) {
                // Everything the arrows say, as a list: for reading the
                // numbers side by side, and for pilots who use TalkBack.
                MapButton(R.drawable.venti_wind, "Wind stations") { showsStations = true }
            }
            if (hasSites) {
                // The signs on the map, as a list; and beside it what the
                // pilot asks of them, drawn as a selected chip while a
                // filter is on. Both stay when a filter leaves no site.
                MapButton(R.drawable.venti_site, "Flying sites") { showsSites = true }
                RoundButton(
                    R.drawable.ic_filter,
                    SitesFilterWords.TITLE,
                    Modifier.testTag("sites-filter-open"),
                    isOn = filtered.isOn,
                    fillsWhenOn = true,
                    onClick = { showsSitesFilter = true },
                )
            }
            if (gliders.isNotEmpty()) {
                // Who is in the air on this part of the map, as a list.
                MapButton(R.drawable.venti_live, "Pilots in the air") { showsGliders = true }
            }
            if (account.isSignedIn || wind.server is WindServer.Usable) {
                // Who flies where today and on the days ahead: the plans
                // of the pilot's groups.
                MapButton(R.drawable.ic_calendar_month, "Flying plans") { showsPlans = true }
            }
            // The ground under everything: the drawn map, or photographs from above.
            AerialButton(aerial, onChange = { on ->
                aerial = on
                preferences?.mapShowsAerial = on
            })
            // Centres the map on the pilot and follows until the map is dragged.
            MapButton(
                icon = if (isFollowing) R.drawable.ic_my_location else R.drawable.ic_location_searching,
                description = "Show my position",
                tint = if (isFollowing) Venti.colours.accentText else Venti.colours.text,
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
            own = keptSites,
            wind = wind,
            showsList = showsSites,
            chosen = chosenSite,
            onChoose = { chosenSite = it },
            onDismiss = {
                chosenSite = null
                showsSites = false
            },
            onFilter = { showsSitesFilter = true },
            community = community,
            onOpenFlight = { flight ->
                chosenSite = null
                showsSites = false
                onOpenFlight(flight)
            },
            // The sheet closes behind what its buttons open.
            actions = siteActions?.let { given ->
                SiteActions(
                    outlook = given.outlook,
                    canStartFlight = given.canStartFlight,
                    onPlanRoute = { site ->
                        chosenSite = null
                        showsSites = false
                        given.onPlanRoute(site)
                    },
                    onStartFlight = { site ->
                        chosenSite = null
                        showsSites = false
                        given.onStartFlight(site)
                    },
                )
            },
        )
    }

    // Over the map, or over the list of sites when it was opened from there.
    if (showsSitesFilter) {
        SitesFilterSheet(
            sites,
            onChange = { filter -> preferences?.mapSitesFilter = SitesFilterWords.kept(filter) },
            onDismiss = { showsSitesFilter = false },
        )
    }

    if (showsGliders) {
        GliderListSheet(
            live,
            // One sheet at a time: the list closes, and the glider's page
            // opens at half height, with its flight on the map above it.
            onChoose = { id ->
                showsGliders = false
                chosenGlider = id
            },
            onDismiss = { showsGliders = false },
        )
    }

    chosenGlider?.let { id ->
        GliderSheet(
            id,
            live,
            account,
            server = (wind.server as? WindServer.Usable)?.url,
            requests = requests,
            onDismiss = { chosenGlider = null },
        )
    }

    airspaceOver?.let { over ->
        AirspaceOverSheet(
            over,
            ceilingM = airspaceCeiling,
            onCeiling = { ceiling ->
                airspaceCeiling = ceiling
                preferences?.mapAirspaceCeilingM = ceiling
            },
            onDismiss = { airspaceOver = null },
        )
    }

    pressed?.let { place ->
        ForecastSheet(place, wind, onDismiss = { pressed = null })
    }

    val shownSquare = square
    val forecastMap = thermalMap?.map
    val forecastDay = thermalMap?.day
    if (shownSquare != null && forecastMap != null && forecastDay != null && showsForecast) {
        ThermalSquareSheet(forecastMap, forecastDay, shownSquare, wind, onDismiss = { square = null })
    }
    val shownHourSquare = hourSquare
    if (shownHourSquare != null && thermalMap != null && forecastMap != null && forecastDay != null && showsForecast) {
        ThermalHourSheet(thermalMap, shownHourSquare.first, shownHourSquare.second, wind, onDismiss = { hourSquare = null })
    }

    if (showsPlans) {
        PlansSheet(
            groups,
            account,
            requests,
            onDismiss = {
                showsPlans = false
                planOnMap = null
            },
            waypoints = waypoints,
            wind = wind,
            startAt = planOnMap,
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
    tint: Color = Venti.colours.text,
    onClick: () -> Unit,
) {
    RoundButton(icon, description, tint = tint, onClick = onClick)
}

/** A line over the top of the map about wind that is missing or not current. */
@Composable
private fun Notice(text: String, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.testTag("wind-notice"),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        shadowElevation = 2.dp,
    ) {
        Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(
                painterResource(R.drawable.venti_caution),
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
        shape = MaterialTheme.shapes.small,
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.85f),
    ) {
        Column(Modifier.padding(horizontal = 8.dp, vertical = 4.dp)) {
            for (line in lines) {
                Text(
                    line,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    // Never cut: the licences ask that each credit can be
                    // read, and four providers' names take three lines of
                    // a narrow phone.
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
