package fly.venti.app

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
import fly.venti.app.wind.WindServer
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch

/**
 * The whole app: its four sections under a tab bar, the screens opened from
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

    if (fly.mode != null) {
        // A full-screen cover, not a tab: nothing can be navigated to by
        // accident in the air.
        CockpitScreen(
            fly = fly,
            waypoints = session.waypoints,
            airspace = session.airspace,
            tasks = session.tasks,
            account = session.account,
            preferences = session.preferences,
            varioName = session.vario.connectedName,
            showsFaiAreas = !session.preferences.hidesFaiAreas,
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
    BackHandler(enabled = !navigation.isAtTopLevel) { navigation.back() }

    Scaffold(
        bottomBar = {
            // The tab bar shows on the sections' first screens only: a
            // flight or the statistics have the whole screen.
            if (navigation.isAtTopLevel) {
                NavigationBar {
                    for (tab in AppTab.entries) {
                        NavigationBarItem(
                            selected = navigation.tab == tab,
                            onClick = { navigation.tab = tab },
                            icon = { Icon(painterResource(tab.icon), contentDescription = null) },
                            label = { Text(tab.title) },
                        )
                    }
                }
            }
        },
    ) { bars ->
        val logbook = session.logbook
        // One Venti server gives the wind, the airspace and the assistant's look-ups.
        val hasServer = session.wind.server is WindServer.Usable
        when (navigation.tab) {
            AppTab.Map -> kept.SaveableStateProvider("map") {
                MapScreen(
                    logbook,
                    session.wind,
                    session.airspace,
                    session.sites,
                    session.live,
                    session.waypoints,
                    bars,
                )
            }
            AppTab.Fly -> when (navigation.flyRoute) {
                FlyRoute.Airspace -> AirspaceScreen(session.airspace, hasServer, onBack = { navigation.back() })
                FlyRoute.Task -> TaskScreen(session.tasks, session.waypoints, onBack = { navigation.back() })
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
                        bars = bars,
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
                        community = session.community,
                        hasServer = hasServer,
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
                        onBack = { navigation.back() },
                    )
                }
                is LogbookRoute.PublicFlight -> {
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
                                server = (session.wind.server as? WindServer.Usable)?.url,
                                // Everything else this pilot shares with everyone.
                                onPilot = { navigation.open(LogbookRoute.Pilot(flight.handle, flight.pilot)) },
                                onBack = { navigation.back() },
                            )
                        }
                    }
                }
                is LogbookRoute.Pilot -> kept.SaveableStateProvider("pilot-${route.handle}") {
                    PilotFlightsScreen(
                        route.handle,
                        route.name,
                        session.community,
                        onOpen = { navigation.open(LogbookRoute.PublicFlight(it.id)) },
                        onBack = { navigation.back() },
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
            AppTab.Assistant -> kept.SaveableStateProvider("assistant") {
                AssistantScreen(session.chat, hasWind = hasServer, bars = bars)
            }
            AppTab.Settings -> when (navigation.settingsRoute) {
                SettingsRoute.Credits -> CreditsScreen(session.wind, onBack = { navigation.back() })
                SettingsRoute.Waypoints -> kept.SaveableStateProvider("waypoints") {
                    WaypointsScreen(session.waypoints, onBack = { navigation.back() })
                }
                SettingsRoute.Airspace -> AirspaceScreen(session.airspace, hasServer, onBack = { navigation.back() })
                SettingsRoute.Account -> kept.SaveableStateProvider("account") {
                    AccountScreen(session.account, session.wind, session.scope, onBack = { navigation.back() })
                }
                null -> kept.SaveableStateProvider("settings") {
                    SettingsScreen(
                        session.preferences,
                        session.fly,
                        session.wind,
                        session.account,
                        onOpen = { navigation.settingsRoute = it },
                        bars = bars,
                    )
                }
            }
        }
    }
}
