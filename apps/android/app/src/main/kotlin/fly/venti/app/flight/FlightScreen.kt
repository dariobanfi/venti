package fly.venti.app.flight

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.SharedFiles
import fly.venti.app.fly.FlySession
import fly.venti.app.logbook.DeleteFlightDialog
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.map.Covered
import fly.venti.app.map.MapSurface
import fly.venti.app.map.StatusBarScrim
import fly.venti.app.pilotMessage
import fly.venti.core.FlightAnalysis
import fly.venti.core.FlightView
import fly.venti.app.account.AccountModel
import fly.venti.app.account.ShareFlightSheet
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.wind.grown
import fly.venti.core.AirspaceShape
import fly.venti.core.FlightAirspace
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.math.min

private sealed interface Phase {
    data object Loading : Phase

    data class Loaded(val flight: FlightView) : Phase

    data class Failed(val message: String) : Phase
}

/**
 * One flight: its track on the map, coloured by climb, under a panel with
 * its numbers and altitude profile.
 */
@Composable
fun FlightScreen(
    id: String,
    logbook: LogbookModel,
    airspace: AirspaceModel,
    /** The pilot's account on a Venti server, to share the flight with a link. */
    account: AccountModel,
    /** Where requests to that server run, so that one carries on when the pilot leaves the flight. */
    requests: CoroutineScope,
    /** The cockpit, to play the flight through it once more. */
    fly: FlySession,
    /** Sending the flight to a league. */
    leagues: Leagues,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var isSharing by remember { mutableStateOf(false) }
    var isSendingToALeague by remember { mutableStateOf(false) }
    var phase by remember(id) { mutableStateOf<Phase>(Phase.Loading) }
    // Arrives a moment after the flight itself; the screen does not wait
    // for it.
    var analysis by remember(id) { mutableStateOf<FlightAnalysis?>(null) }
    // The flight checked against the airspace the app has. Arrives last,
    // and again when that airspace changes.
    var airspaceCheck by remember(id) { mutableStateOf<FlightAirspace?>(null) }
    var isConfirmingDelete by remember { mutableStateOf(false) }

    val all = airspace.set
    LaunchedEffect(id, all) {
        airspaceCheck = try {
            logbook.airspace(id)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // Without the check the flight is still shown in full.
            null
        }
    }

    LaunchedEffect(id) {
        try {
            phase = Phase.Loaded(logbook.flight(id))
            // Without the analysis the flight is still shown in full.
            analysis = logbook.analysis(id)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (phase !is Phase.Loaded) {
                phase = Phase.Failed(error.pilotMessage)
            }
        }
    }

    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        when (val current = phase) {
            Phase.Loading -> CircularProgressIndicator(Modifier.align(Alignment.Center))
            is Phase.Failed -> Failure(current.message, Modifier.align(Alignment.Center))
            is Phase.Loaded -> FlightDetail(
                current.flight,
                analysis,
                airspaceCheck,
                // The airspace around the flight, for its map.
                airspace = remember(current.flight, all) { airspace.shapes(current.flight.bounds.grown(0.2)) },
            )
        }
        TopButtons(
            onBack = onBack,
            onShareWithALink = { isSharing = true },
            onLeague = { isSendingToALeague = true },
            onShare = {
                scope.launch {
                    try {
                        SharedFiles.share(context, logbook.exportFile(id, SharedFiles.folder(context)), "Share flight file")
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Exception) {
                        phase = Phase.Failed(error.pilotMessage)
                    }
                }
            },
            // The flight once more, through the cockpit a pilot flies
            // with, from its own file. Nothing is recorded and nothing is
            // shared. The cockpit covers the app while it plays, and gives
            // this page back when it closes.
            onReplay = { speed ->
                scope.launch {
                    try {
                        fly.startReplay(logbook.fileText(id), speed)
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Exception) {
                        fly.problem = error.pilotMessage
                    }
                }
            },
            onDelete = { isConfirmingDelete = true },
        )
    }

    if (isSharing) {
        ShareFlightSheet(id, account, requests, onDismiss = { isSharing = false })
    }

    if (isSendingToALeague) {
        LeagueSheet(id, leagues, onDismiss = { isSendingToALeague = false })
    }

    if (isConfirmingDelete) {
        DeleteFlightDialog(
            warning = remember(id) { logbook.deletionWarning },
            onConfirm = {
                isConfirmingDelete = false
                try {
                    logbook.delete(id)
                    onBack()
                } catch (error: Exception) {
                    phase = Phase.Failed(error.pilotMessage)
                }
            },
            onDismiss = { isConfirmingDelete = false },
        )
    }
}

/** How fast a flight of the logbook can be played through the cockpit, and what the menu calls each. */
val REPLAY_SPEEDS: List<Pair<String, Double>> = listOf(
    "In real time" to 1.0,
    "10 times faster" to 10.0,
    "20 times faster" to 20.0,
)

/** Back and "more", as round buttons floating over the map. */
@Composable
private fun TopButtons(
    onBack: () -> Unit,
    onShareWithALink: () -> Unit,
    onLeague: () -> Unit,
    onShare: () -> Unit,
    /** Called with how many times faster than it was flown the flight is to be replayed. */
    onReplay: (Double) -> Unit,
    onDelete: () -> Unit,
) {
    var isShowingMenu by remember { mutableStateOf(false) }
    // Material's menus do not open one out of another: "Replay in the
    // cockpit" changes this menu into the choice of speeds.
    var isChoosingSpeed by remember { mutableStateOf(false) }
    val colours = IconButtonDefaults.filledTonalIconButtonColors(
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        contentColor = MaterialTheme.colorScheme.onSurface,
    )
    Row(
        modifier = Modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        FilledTonalIconButton(onClick = onBack, colors = colours) {
            Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
        }
        Box {
            FilledTonalIconButton(
                onClick = {
                    isChoosingSpeed = false
                    isShowingMenu = true
                },
                colors = colours,
            ) {
                Icon(painterResource(R.drawable.ic_more_vert), contentDescription = "More")
            }
            DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
                if (isChoosingSpeed) {
                    for ((title, speed) in REPLAY_SPEEDS) {
                        DropdownMenuItem(
                            text = { Text(title) },
                            onClick = {
                                isShowingMenu = false
                                onReplay(speed)
                            },
                        )
                    }
                    return@DropdownMenu
                }
                DropdownMenuItem(
                    text = { Text("Share with a link") },
                    leadingIcon = { Icon(painterResource(R.drawable.ic_link), contentDescription = null) },
                    onClick = {
                        isShowingMenu = false
                        onShareWithALink()
                    },
                )
                DropdownMenuItem(
                    text = { Text(LeagueWords.TITLE) },
                    leadingIcon = { Icon(painterResource(R.drawable.venti_league), contentDescription = null) },
                    onClick = {
                        isShowingMenu = false
                        onLeague()
                    },
                )
                DropdownMenuItem(
                    text = { Text("Share flight file") },
                    leadingIcon = { Icon(painterResource(R.drawable.ic_share), contentDescription = null) },
                    onClick = {
                        isShowingMenu = false
                        onShare()
                    },
                )
                DropdownMenuItem(
                    text = { Text("Replay in the cockpit") },
                    leadingIcon = { Icon(painterResource(R.drawable.ic_play_arrow), contentDescription = null) },
                    trailingIcon = { Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null) },
                    onClick = { isChoosingSpeed = true },
                )
                DropdownMenuItem(
                    text = { Text("Delete flight") },
                    leadingIcon = { Icon(painterResource(R.drawable.ic_delete), contentDescription = null) },
                    onClick = {
                        isShowingMenu = false
                        onDelete()
                    },
                )
            }
        }
    }
}

@Composable
internal fun Failure(message: String, modifier: Modifier = Modifier) {
    Column(modifier.padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Icon(
            painterResource(R.drawable.venti_caution),
            contentDescription = null,
            modifier = Modifier.size(48.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.size(16.dp))
        Text(
            "This flight could not be opened",
            style = MaterialTheme.typography.titleLarge,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.size(8.dp))
        Text(
            message,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * The map fills the screen and the panel floats over its lower part, as a
 * control layer above the content. The map is told how much of it the panel
 * covers so the track is fitted into the part that shows.
 */
@Composable
internal fun FlightDetail(
    flight: FlightView,
    analysis: FlightAnalysis?,
    airspaceCheck: FlightAirspace?,
    airspace: List<AirspaceShape>,
) {
    // The position in the track the pilot is inspecting, if any.
    var cursor by rememberSaveable(flight.id) { mutableStateOf<Int?>(null) }
    // Whether the marker is running along the track by itself.
    var isPlaying by remember(flight.id) { mutableStateOf(false) }
    var isShowingDetails by remember { mutableStateOf(false) }
    var panelHeight by remember { mutableIntStateOf(0) }
    val map = remember(flight) { FlightMap(flight) }

    val density = LocalDensity.current
    val covered = with(density) {
        Covered(
            top = WindowInsets.statusBars.getTop(this) + TOP_BUTTONS.roundToPx(),
            bottom = panelHeight + 2 * PANEL_MARGIN.roundToPx() + WindowInsets.navigationBars.getBottom(this),
        )
    }

    Box(Modifier.fillMaxSize()) {
        MapSurface(
            content = map,
            modifier = Modifier.fillMaxSize().semantics { contentDescription = "Map of the flight" },
            onUpdate = { map.update(cursor?.let { flight.track.getOrNull(it) }, covered, analysis?.score, airspace) },
        )
        StatusBarScrim()
        FlightPanel(
            flight = flight,
            score = analysis?.score,
            cursor = cursor,
            onCursor = { cursor = it },
            isPlaying = isPlaying,
            onPlaying = { isPlaying = it },
            onDetails = { isShowingDetails = true },
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(PANEL_MARGIN)
                // On a wide screen the panel stays the width of a phone.
                .widthIn(max = 560.dp)
                .onSizeChanged { panelHeight = it.height },
        )
    }

    // Playing moves the marker from where it is to the end of the flight in
    // about half a minute, whatever the flight's length.
    LaunchedEffect(isPlaying) {
        val last = flight.track.lastIndex
        if (!isPlaying || last < 1) return@LaunchedEffect
        val step = max(1, flight.track.size / 450)
        var index = cursor ?: 0
        if (index >= last) {
            index = 0
        }
        while (index < last) {
            index = min(index + step, last)
            cursor = index
            delay(66)
        }
        isPlaying = false
    }

    if (isShowingDetails) {
        FlightDetailsSheet(
            flight = flight,
            analysis = analysis,
            airspace = airspaceCheck,
            onSelect = { index ->
                // Show a thermal, a glide or an airspace the pilot picked from the list.
                isPlaying = false
                cursor = min(index, flight.track.lastIndex)
                isShowingDetails = false
            },
            onDismiss = { isShowingDetails = false },
        )
    }
}

private val PANEL_MARGIN = 8.dp

/** The height the back and "more" buttons take below the status bar. */
private val TOP_BUTTONS = 56.dp
