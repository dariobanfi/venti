package fly.venti.app

import fly.venti.app.maps.OfflineMapsScreen
import fly.venti.app.flight.CompareSources
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.window.Popup
import app.venti.design.VentiSpace
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
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
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableIntStateOf
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
import fly.venti.app.alerts.AlertsEntry
import fly.venti.app.alerts.AlertsScreen
import fly.venti.app.alerts.LocalAlerts
import fly.venti.app.live.LocalAir
import fly.venti.app.airspace.AirspaceScreen
import fly.venti.app.assistant.AssistantScreen
import fly.venti.app.community.FeedScreen
import fly.venti.app.community.LeagueScreen
import fly.venti.app.community.NotificationsScreen
import fly.venti.app.community.PilotFlightsScreen
import fly.venti.app.flight.FlightScreen
import fly.venti.app.flight.PublicFlightScreen
import fly.venti.app.fly.CockpitScreen
import fly.venti.app.fly.FlyScreen
import fly.venti.app.logbook.FlightFiles
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.ImportNoticeDialog
import fly.venti.app.logbook.LogbookScreen
import fly.venti.app.logbook.StatisticsScreen
import fly.venti.app.logbook.TendenciesScreen
import fly.venti.app.map.MapScreen
import fly.venti.app.settings.CreditsScreen
import fly.venti.app.settings.CockpitSettingsScreen
import fly.venti.app.settings.SettingsScreen
import fly.venti.app.task.TaskModel
import fly.venti.app.task.TaskScreen
import fly.venti.app.waypoints.WaypointsScreen
import fly.venti.app.sites.KeptSitesScreen
import fly.venti.app.sites.SiteActions
import fly.venti.app.sites.SiteEditing
import fly.venti.app.sites.SiteFormDraft
import fly.venti.app.sites.TodayCandidateOffer
import fly.venti.app.sites.SiteFormSheet
import fly.venti.app.sites.SiteSources
import fly.venti.app.sites.SiteSheet
import fly.venti.app.task.RouteMapPlanner
import fly.venti.core.Coordinate
import fly.venti.core.FlightPhase
import fly.venti.core.PhoneTarget
import fly.venti.app.fly.FlySession
import fly.venti.app.nearby.HelpOpening
import fly.venti.app.nearby.HelpSheet
import fly.venti.app.nearby.NearbyTiming
import fly.venti.app.today.PilotPosition
import fly.venti.app.today.TodayScreen
import fly.venti.app.wind.WindServer
import fly.venti.core.Bounds
import fly.venti.core.SiteView
import fly.venti.core.SiteWritten
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

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
    /** Whether a notification of what an alarm fired opened the app: "Alerts" comes to the front. */
    opensAlerts: Boolean = false,
    /** Called once it has. */
    onAlertsOpened: () -> Unit = {},
    /** Whether a tap on the home-screen widget opened the app: Today comes to the front. */
    opensToday: Boolean = false,
    /** Called once it has. */
    onTodayOpened: () -> Unit = {},
    /** What a tapped push is about: help, an open plan, or what an alarm fired. */
    opensTarget: PhoneTarget? = null,
    /** Called once it is shown. */
    onTargetOpened: () -> Unit = {},
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val navigation = rememberSaveable(saver = AppNavigation.Saver) { AppNavigation() }
    var notice by remember { mutableStateOf<ImportNotice?>(null) }
    val logbook = session.logbook
    val fly = session.fly

    LaunchedEffect(opensAlerts) {
        if (opensAlerts) {
            navigation.showAlerts()
            onAlertsOpened()
        }
    }

    LaunchedEffect(opensTarget) {
        when (opensTarget) {
            null -> return@LaunchedEffect
            is PhoneTarget.Alert -> navigation.showAlerts()
            is PhoneTarget.OpenPlan -> navigation.showOpenPlan(opensTarget.id)
            is PhoneTarget.Help -> navigation.help = HelpOpening.One(opensTarget.id)
        }
        onTargetOpened()
    }

    LaunchedEffect(opensToday) {
        if (opensToday) {
            navigation.tab = AppTab.Today
            navigation.todayRoute = null
            onTodayOpened()
        }
    }

    // Flight files opened from a file manager, mail or another app.
    LaunchedEffect(incoming) {
        if (incoming.isEmpty()) return@LaunchedEffect
        // A task file, by its name: it is kept, and found on the Fly tab.
        val opened = incoming.map { FlightFiles.source(context, it, what = "a task") }
        if (opened.size == 1 && opened[0].name.endsWith(TaskModel.FILE_ENDING, ignoreCase = true)) {
            val isKept = session.tasks.keep(opened[0])
            onImported()
            if (isKept) {
                // The task's page; its back arrow leads to the cockpit, whose settings list it under Task.
                navigation.tab = AppTab.Fly
                navigation.flyRoute = FlyRoute.Task
                notice = ImportNotice(
                    "Task kept",
                    "It replaces the task you had. The cockpit flies it; it is under Task in the cockpit's settings.",
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
    // What the server says of the account can change while the phone stays
    // signed in, whether it is one of Venti's partners for flying sites
    // above all: asked when the app starts.
    LaunchedEffect(account) {
        session.scope.launch { account.refresh() }
    }
    // Signed in, the phone's push token is registered on the account
    // (where the build has push) and the settings for pilots nearby are
    // read; signed out, what was read is forgotten.
    val nearby = session.nearby
    LaunchedEffect(account.isSignedIn) {
        if (account.isSignedIn) {
            session.scope.launch { session.push?.register() }
            nearby.loadSettings()
        } else {
            nearby.signedOut()
        }
    }
    // While the pilot may be asked for help and the app is in front, Venti
    // is told roughly where the phone is, now and every ten minutes, and
    // never from the back; nothing is asked of the pilot from here.
    LaunchedEffect(lifecycle, nearby) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            val position = PilotPosition(context.applicationContext)
            snapshotFlow { nearby.mayBeAsked }.collectLatest { mayBeAsked ->
                while (mayBeAsked) {
                    position.readIfAllowed()
                    withTimeoutOrNull(NearbyTiming.POSITION_WAIT_MILLISECONDS) { snapshotFlow { position.place }.filterNotNull().first() }
                    position.place?.let { nearby.tellPosition(it) }
                    delay(NearbyTiming.POSITION_EVERY_MILLISECONDS)
                }
            }
        }
    }
    // And where the glider landed, once the cockpit sees it on the ground.
    val landedAt = fly.view?.takeIf { it.phase == FlightPhase.LANDED && fly.mode !is FlySession.Mode.Replay }?.position
    LaunchedEffect(landedAt != null) {
        landedAt?.let { session.scope.launch { nearby.tellPosition(it) } }
    }

    // The watched folder, XCTrack's as a rule: what is new there comes in
    // whenever the app comes to the front, without a word.
    LaunchedEffect(lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            session.scope.launch { session.watched.bringInNew() }
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

    // While the app is in front, on any tab, the take-off is watched for
    // (where the position is allowed: nothing is asked from here), and
    // again whenever a session has ended. A take-off seen brings the
    // cockpit to the front, recording.
    LaunchedEffect(lifecycle, fly) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            try {
                snapshotFlow { fly.mode }.collect { if (it == null) fly.watchForTakeoff() }
            } finally {
                fly.stopWatching()
            }
        }
    }
    // A take-off taken back closes the cockpit it opened: back to the tab
    // the pilot was on, where the core's words say why, quietly.
    var falseStartsShown by rememberSaveable { mutableIntStateOf(fly.closedByFalseStart) }
    LaunchedEffect(fly.closedByFalseStart) {
        if (fly.closedByFalseStart > falseStartsShown) {
            falseStartsShown = fly.closedByFalseStart
            navigation.leaveFly()
        }
    }
    var takeoffsShown by rememberSaveable { mutableIntStateOf(fly.takeoffsSeenUnseen) }
    LaunchedEffect(fly.takeoffsSeenUnseen) {
        if (fly.takeoffsSeenUnseen > takeoffsShown) {
            takeoffsShown = fly.takeoffsSeenUnseen
            navigation.showCockpit()
        }
    }

    // The cockpit covers everything while it is open, but for a list its
    // settings opened, whose back arrow leads back to it.
    if (fly.isOpen && !(navigation.tab == AppTab.Fly && navigation.flyRoute != null)) {
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
            vario = session.vario,
            showsFaiAreas = session.preferences.showsFaiAreas,
            keptMap = session.maps.flyingMap,
            offlineMaps = session.maps.inALine,
            cables = session.cables,
            onOpenAirspace = { navigation.flyRoute = FlyRoute.Airspace },
            onOpenMaps = { navigation.flyRoute = FlyRoute.OfflineMaps },
            onOpenTask = { navigation.flyRoute = FlyRoute.Task },
            onAskForHelp = { at -> navigation.help = HelpOpening.Home(landedAt = at) },
            onReplay = { speed ->
                val text = try {
                    session.files.text(AppFiles.SAMPLE_FLIGHT)
                } catch (_: java.io.IOException) {
                    fly.problem = "The sample flight is missing from the app."
                    null
                }
                text?.let { fly.startReplay(it, speed) }
            },
            onFinish = {
                // Ends the session, and for a real flight saves it and
                // shows it. Without one the pilot is back where they were
                // before the cockpit.
                navigation.leaveFly()
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
        // "Alert me" on a station's page and a take-off's page, wherever
        // they open; signed out it leads to the account.
        val alerts = remember(session) { AlertsEntry(session.alerts, onSignIn = { navigation.showAccount() }) }
        CompositionLocalProvider(LocalAlerts provides alerts, LocalAir provides session.air) {
            Sections(session, navigation, onNotice = { notice = it })
        }
    }

    // Help after landing, over the cockpit or any tab.
    navigation.help?.let { opening ->
        androidx.compose.runtime.key(opening) {
            HelpSheet(
                nearby,
                account,
                session.scope,
                opening,
                onDismiss = { navigation.help = null },
                onSignIn = {
                    navigation.leaveFly()
                    navigation.showAccount()
                },
                onOpenPilot = { handle, _ ->
                    navigation.leaveFly()
                    navigation.showPilot(handle)
                },
            )
        }
    }

    ImportNoticeDialog(notice) { notice = null }

    // Said quietly, over the cockpit or a tab, and gone by itself.
    fly.quietNotice?.let { words ->
        LaunchedEffect(words) {
            delay(QUIET_NOTICE_MILLISECONDS)
            fly.putQuietNoticeAway()
        }
        QuietNotice(words, onPutAway = fly::putQuietNoticeAway)
    }

    fly.problem?.let { problem ->
        AlertDialog(
            onDismissRequest = { fly.problem = null },
            title = { Text("Flying") },
            text = { Text(problem) },
            confirmButton = { TextButton(onClick = { fly.problem = null }) { Text(Common.words.ok) } },
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
    // The day chosen on Today when the site's page was opened, by its first moment.
    var todaySiteDay by remember { mutableStateOf<Long?>(null) }
    // A take-off opened from a pilot's page: its page, in a sheet over it.
    var pilotsSite by remember { mutableStateOf<SiteView?>(null) }
    // The core's line about what was just done to that site, and the form that corrects it.
    var todaySiteSaid by remember { mutableStateOf<String?>(null) }
    var todaySiteForm by remember { mutableStateOf<SiteFormDraft?>(null) }
    // The site was corrected, put back or confirmed: its page as it stands, with the core's line.
    val showTodaySiteWritten = { written: SiteWritten ->
        todaySiteForm = null
        todaySiteSaid = written.words
        todaySite = written.site
        session.scope.launch { session.sites.again() }
        Unit
    }
    // A route being drawn from a site's page, from that site.
    var routeFrom by remember { mutableStateOf<Coordinate?>(null) }
    // What a site's page leads to: its forecast, a route from it, a flight.
    val siteActions = SiteActions(
        outlook = { site -> session.today.outlook(site) },
        canStartFlight = { !session.fly.isOpen },
        onPlanRoute = { site -> routeFrom = site.position },
        onStartFlight = { navigation.startFlight() },
    )
    // A public flight a notification is about, by its address on the
    // server: it opens on the League tab, unless it is no longer public.
    val openPublicFlightById: (String) -> Unit = { id ->
        session.scope.launch {
            try {
                session.community.publicFlightById(id)
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                // No longer public: nothing opens.
                return@launch
            }
            navigation.showPublic(id)
        }
    }

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
                    SettingsRoute.KeptSites -> KeptSitesScreen(
                        session.keptSites,
                        session.preferences,
                        onBack = { navigation.back() },
                    )
                    SettingsRoute.Airspace -> AirspaceScreen(session.airspace, hasServer, onBack = { navigation.back() })
                    SettingsRoute.GliderAndCockpit -> CockpitSettingsScreen(
                        session.preferences,
                        onBack = { navigation.back() },
                        // The account takes the class too, so that uploads carry it.
                        onGliderClass = { kind -> session.scope.launch { session.account.gliderClassChanged(kind) } },
                    )
                    SettingsRoute.OfflineMaps -> OfflineMapsScreen(session.maps, hasServer, onBack = { navigation.back() })
                    SettingsRoute.Account -> kept.SaveableStateProvider("account") {
                        AccountScreen(
                            session.account,
                            session.wind,
                            session.scope,
                            onAlerts = { navigation.settingsRoute = SettingsRoute.Alerts },
                            onBack = { navigation.back() },
                            social = session.social,
                            // What a notification is about opens on the League tab.
                            onOpenFlight = openPublicFlightById,
                            onOpenPilot = { handle -> navigation.showPilot(handle) },
                            nearby = session.nearby,
                        )
                    }
                    SettingsRoute.Alerts -> AlertsScreen(
                        session.alerts,
                        session.scope,
                        onSignIn = { navigation.settingsRoute = SettingsRoute.Account },
                        onBack = { navigation.back() },
                    )
                    null -> kept.SaveableStateProvider("settings") {
                        SettingsScreen(
                            session.preferences,
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
                        session.keptSites,
                        session.wind,
                        session.preferences,
                        onOpen = { site, day ->
                            // The site's page names its source's credit, from the sites around it.
                            session.sites.show(around(site))
                            todaySiteDay = day
                            todaySite = site
                        },
                        onOpenSettings = { navigation.showSettings() },
                        onOpenAssistant = { navigation.todayRoute = TodayRoute.Assistant },
                        bars = bars,
                        underTakeoffs = {
                            TodayCandidateOffer(
                                takeoffs = session.today.places.map { it.site.position },
                                edit = session.siteEdit,
                                account = session.account,
                                requests = session.scope,
                                onAdd = { draft -> todaySiteForm = draft },
                            )
                        },
                    )
                    todaySite?.let { site ->
                        SiteSheet(
                            sites = session.sites,
                            waypoints = session.waypoints,
                            own = session.keptSites,
                            wind = session.wind,
                            showsList = false,
                            chosen = site,
                            // The page speaks of the day chosen on Today.
                            dayStartUnixS = todaySiteDay,
                            // A take-off of the pilot's own may lie where the map
                            // has no answer for: Today has the credit it was kept with.
                            known = SiteSources(session.today.siteCredits, session.today.sitesAsOf),
                            onChoose = { todaySite = it },
                            onDismiss = {
                                todaySite = null
                                todaySiteSaid = null
                            },
                            // What the core offers for a site of Venti's own
                            // layers is on its page here as on the map's.
                            editing = SiteEditing(
                                model = session.siteEdit,
                                account = session.account,
                                requests = session.scope,
                                said = todaySiteSaid,
                                onCorrect = { stands ->
                                    // The core refuses a site of another source: nothing opens then.
                                    todaySiteForm = runCatching { SiteFormDraft.of(stands) }.getOrNull()
                                },
                                onSignIn = {
                                    todaySite = null
                                    navigation.showAccount()
                                },
                                onWritten = showTodaySiteWritten,
                                onSaid = { words ->
                                    todaySiteSaid = words
                                    session.scope.launch { session.sites.again() }
                                },
                            ),
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
                    // A correction from Today's page: the form without the
                    // map, which the place is moved on from the map tab.
                    todaySiteForm?.let { formed ->
                        SiteFormSheet(
                            entry = formed,
                            edit = session.siteEdit,
                            requests = session.scope,
                            onMove = null,
                            onWritten = showTodaySiteWritten,
                            onOpen = { todaySiteForm = null },
                            onStands = { now -> if (todaySite?.id == now.id) todaySite = now },
                            onDismiss = { todaySiteForm = null },
                        )
                    }
                }
            }
            AppTab.Map -> routeFrom?.let { from ->
                RouteMapPlanner(
                    session.tasks,
                    onDismiss = { routeFrom = null },
                    from = from,
                    preferences = session.preferences,
                    thermalMap = session.thermalMap,
                    terrain = session.terrain,
                )
            } ?: kept.SaveableStateProvider("map") {
                MapScreen(
                    logbook,
                    session.wind,
                    session.airspace,
                    session.sites,
                    session.live,
                    session.waypoints,
                    session.keptSites,
                    session.account,
                    session.groups,
                    requests = session.scope,
                    bars,
                    community = session.community,
                    // A flight flown from a site opens like one of everyone's, on the League tab.
                    onOpenFlight = { flight -> navigation.showPublic(flight.id) },
                    preferences = session.preferences,
                    siteActions = siteActions,
                    thermalMap = session.thermalMap,
                    radar = session.radar,
                    siteEdit = session.siteEdit,
                    onSignIn = { navigation.showAccount() },
                    terrain = session.terrain,
                    cables = session.cables,
                    air = session.air,
                )
            }
            AppTab.Fly -> when (navigation.flyRoute) {
                FlyRoute.Airspace -> AirspaceScreen(session.airspace, hasServer, onBack = { navigation.back() })
                FlyRoute.Task -> TaskScreen(
                    session.tasks,
                    session.waypoints,
                    onBack = { navigation.back() },
                    preferences = session.preferences,
                    thermalMap = session.thermalMap,
                    terrain = session.terrain,
                )
                FlyRoute.OfflineMaps -> OfflineMapsScreen(session.maps, hasServer, onBack = { navigation.back() })
                null -> FlyScreen(
                    session.fly,
                    session.vario,
                    bars = bars,
                    wantsCockpit = navigation.wantsCockpit,
                    onCockpitTaken = { navigation.wantsCockpit = false },
                )
            }
            AppTab.Logbook -> when (val route = navigation.logbookPath.lastOrNull()) {
                null -> kept.SaveableStateProvider("logbook") {
                    LogbookScreen(
                        logbook,
                        navigation,
                        imports = session.scope,
                        onNotice = onNotice,
                        watched = session.watched,
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
                        terrain = session.terrain,
                        social = session.social,
                    )
                }
                LogbookRoute.Statistics -> kept.SaveableStateProvider("statistics") {
                    StatisticsScreen(
                        logbook,
                        onOpen = { navigation.open(LogbookRoute.Flight(it)) },
                        onBack = { navigation.back() },
                        onTendencies = { navigation.open(LogbookRoute.Tendencies) },
                    )
                }
                LogbookRoute.Tendencies -> kept.SaveableStateProvider("tendencies") {
                    TendenciesScreen(
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
                        session.social,
                        hasServer = hasServer,
                        onOpenFlight = { navigation.open(LeagueRoute.PublicFlight(it.id)) },
                        onOpenPilot = { handle, name -> navigation.open(LeagueRoute.Pilot(handle, name)) },
                        bars = bars,
                        groups = session.groups,
                        openPlans = session.openPlans,
                        account = session.account,
                        requests = session.scope,
                        waypoints = session.waypoints,
                        wind = session.wind,
                        onOpenFeed = { navigation.open(LeagueRoute.Feed) },
                        onOpenNotifications = { navigation.open(LeagueRoute.Notifications) },
                        onOpenFlightById = openPublicFlightById,
                        onSignIn = { navigation.showAccount() },
                        nearby = session.nearby,
                        onOpenHelp = { navigation.help = it },
                        opensOpenPlan = navigation.openPlanAsked,
                        onOpenPlanOpened = { navigation.openPlanAsked = null },
                    )
                }
                LeagueRoute.Feed -> kept.SaveableStateProvider("feed") {
                    FeedScreen(
                        session.social,
                        session.community,
                        onOpen = { navigation.open(LeagueRoute.PublicFlight(it.id)) },
                        onBack = { navigation.back() },
                    )
                }
                LeagueRoute.Notifications -> NotificationsScreen(
                    session.social,
                    onOpenFlight = openPublicFlightById,
                    onOpenPilot = { handle -> navigation.open(LeagueRoute.Pilot(handle, "@$handle")) },
                    onBack = { navigation.back() },
                )
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
                                session.social,
                                session.account,
                                server = (session.wind.server as? WindServer.Usable)?.url,
                                requests = session.scope,
                                compareSources = CompareSources(logbook, session.community),
                                // Everything else this pilot shares with everyone.
                                onPilot = { navigation.open(LeagueRoute.Pilot(flight.handle, flight.pilot)) },
                                onBack = { navigation.back() },
                                preferences = session.preferences,
                                terrain = session.terrain,
                            )
                        }
                    }
                }
                is LeagueRoute.Pilot -> kept.SaveableStateProvider("pilot-${route.handle}") {
                    PilotFlightsScreen(
                        route.handle,
                        route.name,
                        session.community,
                        session.social,
                        session.account,
                        server = (session.wind.server as? WindServer.Usable)?.url,
                        requests = session.scope,
                        onOpen = { navigation.open(LeagueRoute.PublicFlight(it.id)) },
                        onBack = { navigation.back() },
                        onOpenSite = { flown ->
                            session.scope.launch {
                                // The site as the server has it now. One it
                                // no longer has opens nothing.
                                val site = try {
                                    session.siteEdit.page(flown.id).site
                                } catch (error: CancellationException) {
                                    throw error
                                } catch (_: Exception) {
                                    return@launch
                                }
                                // The site's page names its source's credit, from the sites around it.
                                session.sites.show(around(site))
                                pilotsSite = site
                            }
                        },
                    )
                    pilotsSite?.let { site ->
                        SiteSheet(
                            sites = session.sites,
                            waypoints = session.waypoints,
                            own = session.keptSites,
                            wind = session.wind,
                            showsList = false,
                            chosen = site,
                            onChoose = { pilotsSite = it },
                            onDismiss = { pilotsSite = null },
                            community = session.community,
                            onOpenFlight = { flight ->
                                pilotsSite = null
                                navigation.open(LeagueRoute.PublicFlight(flight.id))
                            },
                        )
                    }
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

/** How long words said quietly stay on screen. */
private const val QUIET_NOTICE_MILLISECONDS = 10_000L

/**
 * Words said quietly: a pill at the foot of the screen, over the tab bar or
 * the cockpit's map, which a tap puts away. No dialog, no sound.
 */
@Composable
private fun QuietNotice(words: String, onPutAway: () -> Unit) {
    Popup(alignment = Alignment.BottomCenter, onDismissRequest = onPutAway) {
        Surface(
            modifier = Modifier
                .navigationBarsPadding()
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s8)
                .clickable(onClick = onPutAway)
                .testTag("quiet-notice"),
            shape = MaterialTheme.shapes.medium,
            color = MaterialTheme.colorScheme.inverseSurface,
            contentColor = MaterialTheme.colorScheme.inverseOnSurface,
        ) {
            Text(
                words,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
            )
        }
    }
}
