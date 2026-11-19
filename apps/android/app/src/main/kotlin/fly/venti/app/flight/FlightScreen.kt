package fly.venti.app.flight

import android.content.ClipData
import android.content.Context
import android.content.Intent
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
import androidx.core.content.FileProvider
import fly.venti.app.R
import fly.venti.app.logbook.DeleteFlightDialog
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.map.Covered
import fly.venti.app.map.MapSurface
import fly.venti.app.map.StatusBarScrim
import fly.venti.app.pilotMessage
import fly.venti.core.FlightAnalysis
import fly.venti.core.FlightView
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.io.File
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
fun FlightScreen(id: String, logbook: LogbookModel, onBack: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var phase by remember(id) { mutableStateOf<Phase>(Phase.Loading) }
    // Arrives a moment after the flight itself; the screen does not wait
    // for it.
    var analysis by remember(id) { mutableStateOf<FlightAnalysis?>(null) }
    var isConfirmingDelete by remember { mutableStateOf(false) }

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
            is Phase.Loaded -> FlightDetail(current.flight, analysis)
        }
        TopButtons(
            onBack = onBack,
            onShare = {
                scope.launch {
                    try {
                        share(context, logbook.exportFile(id, exportFolder(context)))
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Exception) {
                        phase = Phase.Failed(error.pilotMessage)
                    }
                }
            },
            onDelete = { isConfirmingDelete = true },
        )
    }

    if (isConfirmingDelete) {
        DeleteFlightDialog(
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

/** Back and "more", as round buttons floating over the map. */
@Composable
private fun TopButtons(onBack: () -> Unit, onShare: () -> Unit, onDelete: () -> Unit) {
    var isShowingMenu by remember { mutableStateOf(false) }
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
            FilledTonalIconButton(onClick = { isShowingMenu = true }, colors = colours) {
                Icon(painterResource(R.drawable.ic_more_vert), contentDescription = "More")
            }
            DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
                DropdownMenuItem(
                    text = { Text("Share flight file") },
                    leadingIcon = { Icon(painterResource(R.drawable.ic_share), contentDescription = null) },
                    onClick = {
                        isShowingMenu = false
                        onShare()
                    },
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
private fun Failure(message: String, modifier: Modifier = Modifier) {
    Column(modifier.padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Icon(
            painterResource(R.drawable.ic_warning),
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
private fun FlightDetail(flight: FlightView, analysis: FlightAnalysis?) {
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
            onUpdate = { map.update(cursor?.let { flight.track.getOrNull(it) }, covered, analysis?.score) },
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
            onSelect = { index ->
                // Show a thermal or glide the pilot picked from the list.
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

private fun exportFolder(context: Context): File = context.cacheDir.resolve("venti-export")

/** Offers the flight's file to other apps: mail, a file manager, a league's uploader. */
private fun share(context: Context, file: File) {
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    val send = Intent(Intent.ACTION_SEND).apply {
        // Flight files have no type of their own that Android knows.
        type = "application/octet-stream"
        putExtra(Intent.EXTRA_STREAM, uri)
        putExtra(Intent.EXTRA_TITLE, file.name)
        clipData = ClipData.newRawUri(file.name, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(Intent.createChooser(send, "Share flight file"))
}
