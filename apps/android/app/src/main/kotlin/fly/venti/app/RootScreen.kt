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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import fly.venti.app.flight.FlightScreen
import fly.venti.app.fly.CockpitScreen
import fly.venti.app.fly.FlyScreen
import fly.venti.app.logbook.FlightFiles
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.ImportNoticeDialog
import fly.venti.app.logbook.LogbookScreen
import fly.venti.app.logbook.StatisticsScreen
import fly.venti.app.map.MapScreen
import fly.venti.app.settings.CreditsScreen
import fly.venti.app.settings.SettingsScreen
import kotlinx.coroutines.CancellationException
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

    // A flight file opened from a file manager, mail or another app.
    LaunchedEffect(incoming) {
        if (incoming.isEmpty()) return@LaunchedEffect
        val report = logbook.importFiles(incoming.map { FlightFiles.source(context, it) })
        onImported()
        val flight = report.added.firstOrNull() ?: report.alreadyThere.firstOrNull()
        if (flight != null) {
            navigation.show(flight)
        }
        if (flight == null || report.failures.isNotEmpty()) {
            notice = report.notice
        }
    }

    if (fly.mode != null) {
        // A full-screen cover, not a tab: nothing can be navigated to by
        // accident in the air.
        CockpitScreen(
            fly = fly,
            varioName = session.vario.connectedName,
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
        Sections(session, navigation)
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
private fun Sections(session: Session.Ready, navigation: AppNavigation) {
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
        when (navigation.tab) {
            AppTab.Map -> kept.SaveableStateProvider("map") { MapScreen(logbook, bars) }
            AppTab.Fly -> kept.SaveableStateProvider("fly") {
                FlyScreen(session.fly, session.vario, session.files, bars)
            }
            AppTab.Logbook -> when (val route = navigation.logbookPath.lastOrNull()) {
                null -> kept.SaveableStateProvider("logbook") { LogbookScreen(logbook, navigation, bars) }
                is LogbookRoute.Flight -> kept.SaveableStateProvider("flight-${route.id}") {
                    FlightScreen(route.id, logbook, onBack = { navigation.back() })
                }
                LogbookRoute.Statistics -> kept.SaveableStateProvider("statistics") {
                    StatisticsScreen(
                        logbook,
                        onOpen = { navigation.open(LogbookRoute.Flight(it)) },
                        onBack = { navigation.back() },
                    )
                }
            }
            AppTab.Settings -> if (navigation.isShowingCredits) {
                CreditsScreen(onBack = { navigation.back() })
            } else {
                kept.SaveableStateProvider("settings") {
                    SettingsScreen(
                        session.preferences,
                        session.fly,
                        onCredits = { navigation.isShowingCredits = true },
                        bars = bars,
                    )
                }
            }
        }
    }
}
