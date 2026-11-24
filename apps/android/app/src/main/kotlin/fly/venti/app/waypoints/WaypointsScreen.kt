package fly.venti.app.waypoints

import fly.venti.core.nothingFoundWords
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.SharedFiles
import fly.venti.app.design.numbers
import fly.venti.app.fly.ReachTone
import fly.venti.app.fly.ReachWords
import fly.venti.app.fly.type
import fly.venti.app.logbook.FlightFiles
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.ImportNoticeDialog
import fly.venti.app.pilotMessage
import fly.venti.core.ReachView
import fly.venti.core.WaypointView
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/** What the app says about the files it reads waypoints from. */
const val WAYPOINT_FILES = "Venti reads SeeYou files (.cup) and GEO files (.wpt)."

/**
 * The pilot's waypoints: bring them in from a file, look one up, remove
 * one, or take them all out again as a file.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WaypointsScreen(waypoints: WaypointsModel, onBack: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var search by rememberSaveable { mutableStateOf("") }
    var isShowingMenu by remember { mutableStateOf(false) }
    var isConfirmingDeleteAll by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf<ImportNotice?>(null) }

    // Waypoint files have no type Android knows, so any file can be picked;
    // the core says which ones hold waypoints.
    val chooseFiles = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) {
            scope.launch {
                notice = waypoints.importFiles(uris.map { FlightFiles.source(context, it, what = "a waypoint file") })
            }
        }
    }
    val all = waypoints.all
    val shown = remember(all, search) { all.matching(search) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Waypoints") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
                actions = {
                    Box {
                        IconButton(onClick = { isShowingMenu = true }) {
                            Icon(painterResource(R.drawable.ic_more_vert), contentDescription = Common.words.more)
                        }
                        DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
                            DropdownMenuItem(
                                text = { Text("Add from a file") },
                                leadingIcon = { Icon(painterResource(R.drawable.ic_file_open), contentDescription = null) },
                                onClick = {
                                    isShowingMenu = false
                                    chooseFiles.launch(arrayOf("*/*"))
                                },
                            )
                            if (all.isNotEmpty()) {
                                DropdownMenuItem(
                                    text = { Text("Share as a file") },
                                    leadingIcon = { Icon(painterResource(R.drawable.ic_share), contentDescription = null) },
                                    onClick = {
                                        isShowingMenu = false
                                        scope.launch {
                                            try {
                                                val file = waypoints.exportFile(SharedFiles.folder(context))
                                                SharedFiles.share(context, file, "Share waypoints")
                                            } catch (error: CancellationException) {
                                                throw error
                                            } catch (error: Exception) {
                                                notice = ImportNotice("The waypoints could not be shared", error.pilotMessage)
                                            }
                                        }
                                    },
                                )
                                DropdownMenuItem(
                                    text = { Text("Delete all", color = MaterialTheme.colorScheme.error) },
                                    leadingIcon = {
                                        Icon(
                                            painterResource(R.drawable.ic_delete),
                                            contentDescription = null,
                                            tint = MaterialTheme.colorScheme.error,
                                        )
                                    },
                                    onClick = {
                                        isShowingMenu = false
                                        isConfirmingDeleteAll = true
                                    },
                                )
                            }
                        }
                    }
                },
            )
        },
    ) { padding ->
        if (all.isEmpty()) {
            Column(
                modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
            ) {
                Icon(
                    painterResource(R.drawable.ic_pin_drop),
                    contentDescription = null,
                    modifier = Modifier.size(56.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text("No waypoints yet", style = MaterialTheme.typography.headlineSmall)
                Text(
                    "Bring in a waypoint file to fly to its places in the cockpit. $WAYPOINT_FILES",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
                Spacer(Modifier.size(8.dp))
                Button(onClick = { chooseFiles.launch(arrayOf("*/*")) }) { Text("Choose a file") }
            }
        } else {
            Column(Modifier.fillMaxSize().padding(padding)) {
                WaypointSearch(search, onChange = { search = it })
                if (shown.isEmpty()) {
                    val nothing = nothingFoundWords(search)
                    Column(Modifier.padding(horizontal = 20.dp, vertical = 16.dp)) {
                        Text(nothing.title, style = MaterialTheme.typography.titleMedium)
                        Text(nothing.line, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                LazyColumn(contentPadding = PaddingValues(bottom = 24.dp)) {
                    items(shown, key = { it.id }) { waypoint ->
                        DeletableRow(waypoint, onDelete = { waypoints.delete(waypoint.id) })
                        RowDivider()
                    }
                }
            }
        }
    }

    ImportNoticeDialog(notice) { notice = null }

    if (isConfirmingDeleteAll) {
        AlertDialog(
            onDismissRequest = { isConfirmingDeleteAll = false },
            title = { Text("Delete all waypoints?") },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingDeleteAll = false
                        waypoints.deleteAll()
                    },
                ) {
                    Text("Delete all", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { isConfirmingDeleteAll = false }) { Text(Common.words.cancel) } },
        )
    }
}

/** The waypoints whose name or short name contains what the pilot typed. */
fun List<WaypointView>.matching(search: String): List<WaypointView> {
    val wanted = search.trim()
    if (wanted.isEmpty()) return this
    return filter { it.name.contains(wanted, ignoreCase = true) || it.code.contains(wanted, ignoreCase = true) }
}

/** The field a waypoint is looked up in. */
@Composable
fun WaypointSearch(search: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = search,
        onValueChange = onChange,
        placeholder = { Text("Waypoint") },
        leadingIcon = { Icon(painterResource(R.drawable.ic_search), contentDescription = null) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false),
        shape = MaterialTheme.shapes.extraLarge,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
    )
}

/** The hairline between two rows, clear of their icons. */
@Composable
fun RowDivider() {
    HorizontalDivider(
        modifier = Modifier.padding(start = 64.dp),
        color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f),
    )
}

/** A waypoint in the pilot's list: pressing and holding it offers to delete it. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DeletableRow(waypoint: WaypointView, onDelete: () -> Unit) {
    var isShowingMenu by remember { mutableStateOf(false) }
    Box {
        WaypointRow(
            waypoint,
            Modifier.combinedClickable(
                onClick = {},
                onLongClick = { isShowingMenu = true },
                onLongClickLabel = "Delete",
            ),
        )
        DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
            DropdownMenuItem(
                text = { Text("Delete", color = MaterialTheme.colorScheme.error) },
                onClick = {
                    isShowingMenu = false
                    onDelete()
                },
            )
        }
    }
}

/**
 * One waypoint in a line: what kind of place it is, its name, and what is
 * known about it.
 */
@Composable
fun WaypointRow(
    waypoint: WaypointView,
    modifier: Modifier = Modifier,
    isChosen: Boolean = false,
    /** How it lies within the glider's reach, in the cockpit's list. */
    reach: ReachView? = null,
) {
    val detail = waypoint.detail
    val spoken = listOfNotNull("Chosen.".takeIf { isChosen }, detail, ReachWords.spoken(reach)).joinToString(" ")
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 60.dp)
            .padding(horizontal = 20.dp, vertical = 8.dp)
            // One element for a screen reader: the name, then the rest.
            .clearAndSetSemantics {
                contentDescription = waypoint.name
                stateDescription = spoken
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(waypoint.icon),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(20.dp))
        Column(Modifier.weight(1f)) {
            Text(waypoint.name, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (detail.isNotEmpty()) {
                Text(
                    detail,
                    style = MaterialTheme.typography.bodyMedium.numbers,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        // A straight glide would get there, and a ridge is in its way: a
        // mountain in red, in place of a height that says nothing.
        if (reach?.groundInTheWayM != null) {
            Spacer(Modifier.width(8.dp))
            Icon(
                painterResource(R.drawable.ic_landscape),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.error,
            )
        }
        // The height the glider would arrive with: plain with height to
        // spare, amber with little, red when it comes down before.
        ReachWords.beside(reach)?.let { height ->
            Spacer(Modifier.width(8.dp))
            Text(
                height,
                style = MaterialTheme.typography.bodyLarge.numbers,
                fontWeight = FontWeight.Medium,
                color = ReachTone.of(checkNotNull(reach).reach).type,
                maxLines = 1,
            )
        }
        if (isChosen) {
            Spacer(Modifier.width(8.dp))
            Icon(
                painterResource(R.drawable.ic_check),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}
