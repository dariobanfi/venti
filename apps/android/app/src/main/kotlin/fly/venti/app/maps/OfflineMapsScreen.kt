package fly.venti.app.maps

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.core.MapCreditView
import fly.venti.core.MapRegionRow
import fly.venti.core.MapRegionState
import fly.venti.core.MapRegionView
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/** What the page says that is not the core's. */
object OfflineMapsWords {
    const val TITLE = "Offline maps"
    const val ROW = "Offline map"
    /** Only a test is without Venti; the app always has it. */
    const val NO_SERVER = "Maps for flying without a network come from Venti, which could not be reached."
    const val ASKING = "Asking Venti which maps it has."
    const val NONE = "Venti has no maps to download."
    const val CHECKING = "Checking the download"
    const val FOOTER =
        "A downloaded region is drawn from this phone: in the cockpit wherever the glider is inside it, with a " +
            "network or without, and on the other maps when there is no network. The height of the ground comes " +
            "from it too. Outside the region the map comes from the network as before. A download runs while " +
            "Venti is open; if it stops, it goes on from where it was."
    const val CREDITS = "Whose data the downloaded maps hold"

    fun asking(region: MapRegionView): String =
        "${region.size} from Venti. On mobile data this counts against your plan."

    const val DELETING = "The map goes from this phone. It can be downloaded again while Venti has it."
}

/**
 * Maps for flying without a network: the regions on this phone and the
 * ones Venti hands out, with what a download does. Reached from the Fly
 * tab's line before take-off, and from the settings.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OfflineMapsScreen(maps: OfflineMapsModel, hasServer: Boolean, onBack: () -> Unit) {
    val scope = rememberCoroutineScope()
    val links = LocalUriHandler.current
    var asking by remember { mutableStateOf<MapRegionView?>(null) }
    var deleting by remember { mutableStateOf<MapRegionView?>(null) }

    LaunchedEffect(hasServer) { maps.refresh() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(OfflineMapsWords.TITLE) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
                actions = {
                    // Pulling a list down is not how a page of rows is asked again on Android: a button is.
                    IconButton(onClick = { scope.launch { maps.refresh() } }, enabled = !maps.isLoading) {
                        Icon(painterResource(R.drawable.ic_refresh), contentDescription = "Ask Venti again")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .testTag("offline-maps")
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 24.dp),
        ) {
            val rows = maps.rows
            if (rows.isEmpty()) {
                Text(
                    when {
                        !hasServer -> OfflineMapsWords.NO_SERVER
                        maps.problem != null -> checkNotNull(maps.problem)
                        maps.offered == null -> OfflineMapsWords.ASKING
                        else -> OfflineMapsWords.NONE
                    },
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 16.dp).testTag("maps-none"),
                )
            }
            for (row in rows) {
                Spacer(Modifier.height(16.dp))
                Group {
                    row { RegionLines(row, maps.progress[row.region.id]) }
                    for ((title, action) in actions(row, maps, onAsk = { asking = it }, onDelete = { deleting = it })) {
                        row {
                            ActionRow(
                                title,
                                if (title.startsWith("Delete")) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                                Modifier.testTag(tagOf(title)),
                                onClick = action,
                            )
                        }
                    }
                }
            }
            if (rows.isNotEmpty()) {
                maps.problem?.let { problem ->
                    Text(problem, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(4.dp, 12.dp))
                }
            }
            SectionFooter(OfflineMapsWords.FOOTER, Modifier.padding(top = 12.dp))

            val credits = maps.kept.flatMap { it.credits }.distinct()
            if (credits.isNotEmpty()) {
                SectionHeader(OfflineMapsWords.CREDITS)
                Group {
                    for (credit in credits) {
                        row { Credit(credit) { url -> links.openUri(url) } }
                    }
                }
            }
        }
    }

    asking?.let { region ->
        AlertDialog(
            onDismissRequest = { asking = null },
            title = { Text("Download ${region.name}?") },
            text = { Text(OfflineMapsWords.asking(region)) },
            confirmButton = {
                TextButton(
                    onClick = {
                        asking = null
                        maps.download(region)
                    },
                    modifier = Modifier.testTag("map-download-confirm"),
                ) { Text("Download ${region.size}") }
            },
            dismissButton = { TextButton(onClick = { asking = null }) { Text(Common.words.cancel) } },
        )
    }
    deleting?.let { region ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("Delete ${region.name}?") },
            text = { Text(OfflineMapsWords.DELETING) },
            confirmButton = {
                TextButton(onClick = {
                    deleting = null
                    maps.delete(region)
                }) { Text("Delete the map", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { deleting = null }) { Text(Common.words.cancel) } },
        )
    }
}

/** A test finds an action by what it does. */
private fun tagOf(title: String): String = when {
    title.startsWith("Delete") -> "map-delete"
    title.startsWith("Stop") -> "map-stop"
    else -> "map-download"
}

/** What can be done with a region, each a row of its own. */
private fun actions(
    row: MapRegionRow,
    maps: OfflineMapsModel,
    onAsk: (MapRegionView) -> Unit,
    onDelete: (MapRegionView) -> Unit,
): List<Pair<String, () -> Unit>> {
    val id = row.region.id
    val delete = "Delete from this phone" to { onDelete(row.region) }
    return when (maps.progress[id]) {
        is OfflineMapsModel.Progress.Downloading, OfflineMapsModel.Progress.Checking ->
            listOf("Stop the download" to { maps.stop(id) })
        is OfflineMapsModel.Progress.Stopped ->
            listOf("Go on with the download" to { maps.download(row.newer ?: row.region) }) +
                if (row.state != MapRegionState.OFFERED) listOf(delete) else emptyList()
        null -> when (row.state) {
            MapRegionState.OFFERED -> listOf("Download" to { onAsk(row.region) })
            MapRegionState.KEPT -> listOf(delete)
            MapRegionState.KEPT_WITH_NEWER ->
                listOfNotNull(row.newer?.let { newer -> "Download the newer map" to { onAsk(newer) } }) + delete
        }
    }
}

/** A region's name, what there is to say about it, and how far its download is. */
@Composable
private fun RegionLines(row: MapRegionRow, progress: OfflineMapsModel.Progress?) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    val status = when (progress) {
        is OfflineMapsModel.Progress.Downloading -> progress.words
        OfflineMapsModel.Progress.Checking -> OfflineMapsWords.CHECKING
        is OfflineMapsModel.Progress.Stopped -> progress.words
        null -> null
    }
    Column(
        Modifier
            .fillMaxWidth()
            .testTag("map-region")
            .clearAndSetSemantics {
                contentDescription = row.region.name
                stateDescription = listOfNotNull(row.words, status).joinToString(". ")
            }
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(row.region.name, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
            if (row.state != MapRegionState.OFFERED) {
                // Kept: a sign and the words beside it say so.
                Icon(painterResource(R.drawable.venti_good), contentDescription = null, tint = Venti.colours.good, modifier = Modifier.size(20.dp))
            }
        }
        Text(row.words, style = MaterialTheme.typography.bodyMedium.numbers, color = quiet)
        when (progress) {
            is OfflineMapsModel.Progress.Downloading -> {
                Spacer(Modifier.height(8.dp))
                LinearProgressIndicator(progress = { progress.share }, modifier = Modifier.fillMaxWidth())
                Spacer(Modifier.height(4.dp))
                Text(progress.words, style = MaterialTheme.typography.bodySmall.numbers, color = quiet)
            }
            OfflineMapsModel.Progress.Checking -> {
                Spacer(Modifier.height(8.dp))
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                Spacer(Modifier.height(4.dp))
                Text(OfflineMapsWords.CHECKING, style = MaterialTheme.typography.bodySmall, color = quiet)
            }
            is OfflineMapsModel.Progress.Stopped -> {
                Spacer(Modifier.height(6.dp))
                Row(verticalAlignment = Alignment.Top) {
                    Icon(painterResource(R.drawable.venti_caution), contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(progress.words, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
            }
            null -> Unit
        }
    }
}

@Composable
private fun Credit(credit: MapCreditView, open: (String) -> Unit) {
    val url = credit.url
    if (url != null) {
        ActionRow(credit.text, MaterialTheme.colorScheme.primary) { open(url) }
    } else {
        Text(credit.text, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(16.dp, 12.dp))
    }
}
