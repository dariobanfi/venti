package fly.venti.app

import fly.venti.app.maps.OfflineMapsScreen
import fly.venti.app.flight.CompareSources
import androidx.compose.foundation.layout.Column
import fly.venti.app.design.VentiTheme
import fly.venti.app.design.Venti
import fly.venti.app.design.Appearance
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.NavigationBarItemDefaults
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import fly.venti.app.account.AccountScreen
import fly.venti.app.airspace.AirspaceScreen
import fly.venti.app.assistant.AssistantScreen
import fly.venti.app.community.LeagueScreen
import fly.venti.app.community.PilotFlightsScreen
import fly.venti.app.flight.FlightScreen
import fly.venti.app.flight.PublicFlightScreen
import fly.venti.app.fly.CockpitScreen
import fly.venti.app.fly.FlyScreen
import fly.venti.app.fly.LiveScreen
import fly.venti.app.logbook.FlightFiles
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.ImportNoticeDialog
import fly.venti.app.logbook.LogbookScreen
import fly.venti.app.logbook.StatisticsScreen
import fly.venti.app.map.MapScreen
import fly.venti.app.settings.CreditsScreen
import fly.venti.app.settings.SettingsScreen
import fly.venti.app.task.TaskModel
import fly.venti.app.task.TaskScreen
import fly.venti.app.waypoints.WaypointsScreen
import fly.venti.app.sites.SiteActions
import fly.venti.app.sites.SiteSheet
import fly.venti.app.task.RouteMapPlanner
import fly.venti.core.Coordinate
import fly.venti.app.today.TodayScreen
import fly.venti.app.wind.WindServer
import fly.venti.core.Bounds
import fly.venti.core.SiteView
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch

/**
 * The whole app: its sections under a tab bar, the screens opened from
 * them, and the cockpit, which covers everything while a flight runs.
 */
@Composable
fun RootScreen(
    session: Session.Ready,
    /** Flight files another app handed over. */
    incoming: List<Uri>,
    /** Called once they have been imported. */
    onImported: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val navigation = rememberSaveable(saver = AppNavigation.Saver) { AppNavigation() }
    var notice by remember { mutableStateOf<ImportNotice?>(null) }
    val logbook = session.logbook
    val fly = session.fly

    // Flight files opened from a file manager, mail or another app.
    LaunchedEffect(incoming) {
        if (incoming.isEmpty()) return@LaunchedEffect
        // A task file, by its name: it is kept, and found on the Fly tab.
        val opened = incoming.map { FlightFiles.source(context, it, what = "a task") }
        if (opened.size == 1 && opened[0].name.endsWith(TaskModel.FILE_ENDING, ignoreCase = true)) {
            val isKept = session.tasks.keep(opened[0])
            onImported()
            if (isKept) {
                navigation.tab = AppTab.Fly
                navigation.flyRoute = null
                notice = ImportNotice(
                    "Task kept",
                    "It replaces the task you had. The cockpit flies it; look at it on the Fly tab, under Task.",
                )
            } else {
                notice = ImportNotice("The task could not be read", session.tasks.problem.orEmpty())
            }
            return@LaunchedEffect
        }
        val several = incoming.size > 1
        if (several) {
            // The logbook counts them as they come in.
            navigation.showLogbook()
        }
        val report = logbook.importFiles(incoming.map { FlightFiles.source(context, it) })
        onImported()
        if (several) {
            // A batch says what it did, as one picked in the logbook does.
            notice = report.notice
            if (report.failures.isEmpty() && report.added.size == 1) {
                navigation.show(report.added.first())
            }
        } else {
            // One file opens its flight, also when the logbook had it already.
            val flight = report.added.firstOrNull() ?: report.alreadyThere.firstOrNull()
            if (flight != null) {
                navigation.show(flight)
            } else {
                notice = report.notice
            }
        }
    }

    // With an account, the logbook is brought in step with the server when
    // the app comes to the front and whenever the logbook has changed:
    // after an import, a deletion or a flight. The sync runs in the app's
    // own scope, so that leaving a screen does not cut it short.
    val account = session.account
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(lifecycle, account) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            session.scope.launch { account.sync() }
        }
    }
    LaunchedEffect(logbook, account) {
        var seen = logbook.syncs
        snapshotFlow { logbook.view.totals.flights to logbook.syncs }
            .drop(1)
            .collect { (_, syncs) ->
                // What a sync itself brought in needs no sync of its own.
                if (syncs == seen) {
                    session.scope.launch { account.sync(force = true) }
                }
                seen = syncs
            }
    }

    // A flight that ended by itself and was saved: shown as one the pilot ended.
    LaunchedEffect(fly.savedFlight) {
        fly.savedFlight?.let { id ->
            navigation.show(id)
            fly.savedFlight = null
        }
    }

    if (fly.mode != null) {
        // A full-screen cover, not a tab: nothing can be navigated to by
        // accident in the air. In the design pack's flight theme, whatever
        // the phone is set to.
        VentiTheme(Appearance.Flight) {
        CockpitScreen(
            fly = fly,
            waypoints = session.waypoints,
            airspace = session.airspace,
            tasks = session.tasks,
            account = session.account,
            preferences = session.preferences,
            varioName = session.vario.connectedName,
            showsFaiAreas = !session.preferences.hidesFaiAreas,
            keptMap = session.maps.flyingMap,
            onFinish = {
                // Ends the session, and for a real flight saves it and shows it.
                val recording = fly.finish() ?: return@CockpitScreen
                scope.launch {
                    try {
                        navigation.show(logbook.adopt(recording))
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Exception) {
                        fly.problem = "The flight was recorded but could not be added to the logbook: " +
                            "${error.pilotMessage} It will be tried again the next time Venti starts."
                    }
                }
            },
        )
        }
    } else {
        Sections(session, navigation, onNotice = { notice = it })
    }

    ImportNoticeDialog(notice) { notice = null }

    fly.problem?.let { problem ->
        AlertDialog(
            onDismissRequest = { fly.problem = null },
            title = { Text("Flying") },
            text = { Text(problem) },
            confirmButton = { TextButton(onClick = { fly.problem = null }) { Text("OK") } },
        )
    }
}

@Composable
private fun Sections(
    session: Session.Ready,
    navigation: AppNavigation,
    /** Called with what there is to say about an import, whichever section is in front by then. */
    onNotice: (ImportNotice?) -> Unit,
) {
    // Each section keeps what it remembers, such as how far its list was
    // scrolled, while the pilot is in another.
    val kept = rememberSaveableStateHolder()
    BackHandler(enabled = navigation.canGoBack) { navigation.back() }
    // A take-off opened from the Today tab: its page, in a sheet over it.
    var todaySite by remember { mutableStateOf<SiteView?>(null) }
    // A route being drawn from a site's page, from that site.
    var routeFrom by remember { mutableStateOf<Coordinate?>(null) }
    // What a site's page leads to: its forecast, a route from it, a flight.
    val siteActions = SiteActions(
        outlook = { site -> session.today.outlook(site) },
        canStartFlight = { session.fly.mode == null },
        onPlanRoute = { site -> routeFrom = site.position },
        onStartFlight = { navigation.startFlight() },
    )

    Scaffold(
        bottomBar = {
            // The tab bar shows on the sections' first screens and the
            // Today tab's pages: a flight or the statistics have the whole
            // screen.
            if (navigation.showsTabBar) {
                // The pack's tab bar: on the surface under a line, the
                // section in front in the text's colour with its icon in
                // coral, the others muted. No pill behind the icon.
                val colours = Venti.colours
                Column {
                HorizontalDivider(color = colours.border)
                NavigationBar(containerColor = colours.surface) {
                    for (tab in AppTab.entries) {
                        val selected = navigation.tab == tab
                        NavigationBarItem(
                            selected = selected,
                            onClick = { navigation.choose(tab) },
                            icon = { Icon(painterResource(tab.icon), contentDescription = null) },
                            label = {
                                Text(
                                    tab.title,
                                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
                                )
                            },
                            colors = NavigationBarItemDefaults.colors(
                                selectedIconColor = colours.accentText,
                                selectedTextColor = colours.text,
                                indicatorColor = Color.Transparent,
                                unselectedIconColor = colours.textMuted,
                                unselectedTextColor = colours.textMuted,
                            ),
                        )
                    }
                }
                }
            }
        },
    ) { bars ->
        val logbook = session.logbook
        // One Venti server gives the wind, the airspace and the assistant's look-ups.
        val hasServer = session.wind.server is WindServer.Usable
        when (navigation.tab) {
            AppTab.Today -> when (navigation.todayRoute) {
                TodayRoute.Assistant -> kept.SaveableStateProvider("assistant") {
                    AssistantScreen(session.chat, hasWind = hasServer, bars = bars, onBack = { navigation.back() })
                }
                TodayRoute.Settings -> when (navigation.settingsRoute) {
                    SettingsRoute.Credits -> CreditsScreen(session.wind, onBack = { navigation.back() })
                    SettingsRoute.Waypoints -> kept.SaveableStateProvider("waypoints") {
                        WaypointsScreen(session.waypoints, onBack = { navigation.back() })
                    }
                    SettingsRoute.Airspace -> AirspaceScreen(session.airspace, hasServer, onBack = { navigation.back() })
                    SettingsRoute.OfflineMaps -> OfflineMapsScreen(session.maps, hasServer, onBack = { navigation.back() })
                    SettingsRoute.Account -> kept.SaveableStateProvider("account") {
                        AccountScreen(session.account, session.wind, session.scope, onBack = { navigation.back() })
                    }
                    null -> kept.SaveableStateProvider("settings") {
                        SettingsScreen(
                            session.preferences,
                            session.fly,
                            session.wind,
                            session.account,
                            offlineMaps = session.maps.inALine,
                            onOpen = { navigation.settingsRoute = it },
                            bars = bars,
                            onBack = { navigation.back() },
                        )
                    }
                }
                null -> kept.SaveableStateProvider("today") {
                    TodayScreen(
                        session.today,
                        session.wind,
                        session.preferences,
                        onOpen = { site ->
                            // The site's page names its source's credit, from the sites around it.
                            session.sites.show(around(site))
                            todaySite = site
                        },
                        onOpenSettings = { navigation.showSettings() },
                        onOpenAssistant = { navigation.todayRoute = TodayRoute.Assistant },
                        bars = bars,
                    )
                    todaySite?.let { site ->
                        SiteSheet(
                            sites = session.sites,
                            waypoints = session.waypoints,
                            wind = session.wind,
                            showsList = false,
                            chosen = site,
                            onChoose = { todaySite = it },
                            onDismiss = { todaySite = null },
                            community = session.community,
                            onOpenFlight = { flight ->
                                todaySite = null
                                navigation.showPublic(flight.id)
                            },
                            actions = SiteActions(
                                outlook = siteActions.outlook,
                                canStartFlight = siteActions.canStartFlight,
                                onPlanRoute = { site ->
                                    todaySite = null
                                    navigation.tab = AppTab.Map
                                    routeFrom = site.position
                                },
                                onStartFlight = {
                                    todaySite = null
                                    navigation.startFlight()
                                },
                            ),
                        )
                    }
                }
            }
            AppTab.Map -> routeFrom?.let { from ->
                RouteMapPlanner(session.tasks, onDismiss = { routeFrom = null }, from = from, preferences = session.preferences)
            } ?: kept.SaveableStateProvider("map") {
                MapScreen(
                    logbook,
                    session.wind,
                    session.airspace,
                    session.sites,
                    session.live,
                    session.waypoints,
                    session.account,
                    session.groups,
                    requests = session.scope,
                    bars,
                    community = session.community,
                    // A flight flown from a site opens like one of everyone's, on the League tab.
                    onOpenFlight = { flight -> navigation.showPublic(flight.id) },
                    preferences = session.preferences,
                    siteActions = siteActions,
                )
            }
            AppTab.Fly -> when (navigation.flyRoute) {
                FlyRoute.Airspace -> AirspaceScreen(session.airspace, hasServer, onBack = { navigation.back() })
                FlyRoute.Task -> TaskScreen(session.tasks, session.waypoints, onBack = { navigation.back() }, preferences = session.preferences)
                FlyRoute.OfflineMaps -> OfflineMapsScreen(session.maps, hasServer, onBack = { navigation.back() })
                FlyRoute.Live -> LiveScreen(
                    session.fly,
                    session.account,
                    session.preferences,
                    onBack = { navigation.back() },
                )
                null -> kept.SaveableStateProvider("fly") {
                    FlyScreen(
                        session.fly,
                        session.vario,
                        session.airspace,
                        session.tasks,
                        session.account,
                        session.preferences,
                        session.files,
                        onOpenAirspace = { navigation.flyRoute = FlyRoute.Airspace },
                        onOpenTask = { navigation.flyRoute = FlyRoute.Task },
                        onOpenLive = { navigation.flyRoute = FlyRoute.Live },
                        offlineMaps = session.maps.inALine,
                        onOpenMaps = { navigation.flyRoute = FlyRoute.OfflineMaps },
                        bars = bars,
                        wantsCockpit = navigation.wantsCockpit,
                        onCockpitTaken = { navigation.wantsCockpit = false },
                    )
                }
            }
            AppTab.Logbook -> when (val route = navigation.logbookPath.lastOrNull()) {
                null -> kept.SaveableStateProvider("logbook") {
                    LogbookScreen(
                        logbook,
                        navigation,
                        imports = session.scope,
                        onNotice = onNotice,
                        bars = bars,
                    )
                }
                is LogbookRoute.Flight -> kept.SaveableStateProvider("flight-${route.id}") {
                    FlightScreen(
                        route.id,
                        logbook,
                        session.airspace,
                        session.account,
                        requests = session.scope,
                        fly = session.fly,
                        leagues = session.leagues,
                        compareSources = CompareSources(logbook, session.community),
                        onBack = { navigation.back() },
                        preferences = session.preferences,
                    )
                }
                LogbookRoute.Statistics -> kept.SaveableStateProvider("statistics") {
                    StatisticsScreen(
                        logbook,
                        onOpen = { navigation.open(LogbookRoute.Flight(it)) },
                        onBack = { navigation.back() },
                    )
                }
            }
            AppTab.League -> when (val route = navigation.leaguePath.lastOrNull()) {
                null -> kept.SaveableStateProvider("league") {
                    LeagueScreen(
                        session.community,
                        hasServer = hasServer,
                        onOpenFlight = { navigation.open(LeagueRoute.PublicFlight(it.id)) },
                        onOpenPilot = { handle, name -> navigation.open(LeagueRoute.Pilot(handle, name)) },
                        bars = bars,
                    )
                }
                is LeagueRoute.PublicFlight -> {
                    val flight = session.community.flight(route.id)
                    if (flight == null) {
                        // The list it was opened from is gone: the app was
                        // started anew in between. Back to the list.
                        LaunchedEffect(route) { navigation.back() }
                    } else {
                        kept.SaveableStateProvider("public-flight-${route.id}") {
                            PublicFlightScreen(
                                flight,
                                session.community,
                                session.airspace,
                                session.account,
                                server = (session.wind.server as? WindServer.Usable)?.url,
                                requests = session.scope,
                                compareSources = CompareSources(logbook, session.community),
                                // Everything else this pilot shares with everyone.
                                onPilot = { navigation.open(LeagueRoute.Pilot(flight.handle, flight.pilot)) },
                                onBack = { navigation.back() },
                                preferences = session.preferences,
                            )
                        }
                    }
                }
                is LeagueRoute.Pilot -> kept.SaveableStateProvider("pilot-${route.handle}") {
                    PilotFlightsScreen(
                        route.handle,
                        route.name,
                        session.community,
                        session.account,
                        server = (session.wind.server as? WindServer.Usable)?.url,
                        requests = session.scope,
                        onOpen = { navigation.open(LeagueRoute.PublicFlight(it.id)) },
                        onBack = { navigation.back() },
                    )
                }
            }
        }
    }
}

/** A small box around a site, for the sites around it. */
private fun around(site: SiteView): Bounds = Bounds(
    south = site.position.lat - 0.05,
    west = site.position.lon - 0.07,
    north = site.position.lat + 0.05,
    east = site.position.lon + 0.07,
)
