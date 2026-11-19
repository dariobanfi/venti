package fly.venti.app.logbook

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
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.AppNavigation
import fly.venti.app.LogbookRoute
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Metric
import fly.venti.app.design.numbers
import fly.venti.app.pilotMessage
import fly.venti.core.LogbookEntry
import fly.venti.core.LogbookYear
import fly.venti.core.RouteKind
import fly.venti.core.Totals
import kotlinx.coroutines.launch

/** The pilot's flights, newest first, by year. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LogbookScreen(logbook: LogbookModel, navigation: AppNavigation, bars: PaddingValues) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var notice by remember { mutableStateOf<ImportNotice?>(null) }
    var flightToDelete by remember { mutableStateOf<LogbookEntry?>(null) }

    // After an import: open the flight if there was exactly one new one,
    // and say so if something needs saying.
    fun finish(report: ImportReport) {
        notice = report.notice
        if (report.failures.isEmpty() && report.added.size == 1) {
            navigation.show(flight = report.added.first())
        }
    }

    // Flight files have no type the system knows, so any file can be
    // picked; the core says which ones are flights.
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { files ->
        if (files.isNotEmpty()) {
            scope.launch { finish(logbook.importFiles(files.map { FlightFiles.source(context, it) })) }
        }
    }
    val chooseFiles = { picker.launch(arrayOf("*/*")) }

    Scaffold(
        modifier = Modifier.padding(bottom = bars.calculateBottomPadding()),
        topBar = {
            TopAppBar(
                title = { Text("Logbook") },
                actions = {
                    IconButton(onClick = chooseFiles) {
                        Icon(painterResource(R.drawable.ic_add), contentDescription = "Import flights")
                    }
                },
            )
        },
        // The tab bar below already keeps clear of the system's own bar.
        contentWindowInsets = WindowInsets(0),
    ) { padding ->
        if (logbook.view.years.isEmpty()) {
            EmptyLogbook(
                onImport = chooseFiles,
                onAddSample = { scope.launch { finish(logbook.addSample()) } },
                modifier = Modifier.padding(padding),
            )
        } else {
            FlightList(
                totals = logbook.view.totals,
                years = logbook.view.years,
                onOpen = { navigation.open(LogbookRoute.Flight(it.id)) },
                onStatistics = { navigation.open(LogbookRoute.Statistics) },
                onDelete = { flightToDelete = it },
                modifier = Modifier.padding(padding),
            )
        }
    }

    ImportNoticeDialog(notice) { notice = null }

    // Deleting asks first: the flight's file exists only on this phone.
    flightToDelete?.let { entry ->
        DeleteFlightDialog(
            onConfirm = {
                flightToDelete = null
                try {
                    logbook.delete(entry.id)
                } catch (error: Exception) {
                    notice = ImportNotice("The flight could not be deleted", error.pilotMessage)
                }
            },
            onDismiss = { flightToDelete = null },
        )
    }
}

/** Shows an import message as a dialog. */
@Composable
fun ImportNoticeDialog(notice: ImportNotice?, onDismiss: () -> Unit) {
    if (notice == null) return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(notice.title) },
        text = { Text(notice.text) },
        confirmButton = { TextButton(onClick = onDismiss) { Text("OK") } },
    )
}

/** Asks before a flight is removed for good. */
@Composable
fun DeleteFlightDialog(onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Delete this flight?") },
        text = { Text("Its file is removed from this phone. This cannot be undone.") },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text("Delete flight", color = MaterialTheme.colorScheme.error)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun EmptyLogbook(onImport: () -> Unit, onAddSample: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxSize().padding(horizontal = 32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(
            painterResource(R.drawable.ic_book),
            contentDescription = null,
            modifier = Modifier.size(56.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.size(16.dp))
        Text("No flights yet", style = MaterialTheme.typography.headlineSmall)
        Spacer(Modifier.size(8.dp))
        Text(
            "Import the IGC files your vario or flight app recorded. They stay on this phone.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.size(24.dp))
        Button(onClick = onImport, modifier = Modifier.heightIn(min = 52.dp)) {
            Text("Import flights", style = MaterialTheme.typography.titleMedium)
        }
        Spacer(Modifier.size(4.dp))
        TextButton(onClick = onAddSample) {
            Text("Add a sample flight")
        }
    }
}

@Composable
private fun FlightList(
    totals: Totals,
    years: List<LogbookYear>,
    onOpen: (LogbookEntry) -> Unit,
    onStatistics: () -> Unit,
    onDelete: (LogbookEntry) -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(
        modifier = modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
    ) {
        item(key = "totals") {
            TotalsCard(totals, onClick = onStatistics)
        }
        for (year in years) {
            item(key = "year-${year.year}") {
                YearHeader(year)
            }
            itemsIndexed(year.flights, key = { _, entry -> entry.id }) { index, entry ->
                LogbookRow(
                    entry = entry,
                    shape = groupedShape(index, year.flights.size),
                    onOpen = { onOpen(entry) },
                    onDelete = { onDelete(entry) },
                )
                if (index < year.flights.lastIndex) {
                    Spacer(Modifier.size(2.dp))
                }
            }
        }
    }
}

/**
 * The shape of one row in a group of rows: the group has round corners, and
 * the rows inside it meet with small ones.
 */
private fun groupedShape(index: Int, count: Int): Shape {
    val outer = 20.dp
    val inner = 4.dp
    val top = if (index == 0) outer else inner
    val bottom = if (index == count - 1) outer else inner
    return RoundedCornerShape(topStart = top, topEnd = top, bottomStart = bottom, bottomEnd = bottom)
}

/** All flights counted together, at the top of the list. It leads to the statistics. */
@Composable
private fun TotalsCard(totals: Totals, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth().testTag("logbook-totals"),
        shape = RoundedCornerShape(20.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLow,
    ) {
        Row(
            modifier = Modifier.padding(start = 16.dp, top = 14.dp, bottom = 14.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Metric(
                caption = if (totals.flights == 1u) "Flight" else "Flights",
                value = Format.count(totals.flights.toLong()),
                modifier = Modifier.weight(1f),
            )
            Metric(
                caption = "Airtime",
                value = Format.hoursMinutes(totals.airtimeS),
                unit = "h",
                modifier = Modifier.weight(1f),
            )
            Metric(
                caption = "Distance",
                value = Format.wholeKilometres(totals.distanceM),
                unit = "km",
                modifier = Modifier.weight(1f),
            )
            Icon(
                painterResource(R.drawable.ic_chevron_right),
                contentDescription = "Statistics",
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun YearHeader(year: LogbookYear) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 4.dp, end = 4.dp, top = 20.dp, bottom = 8.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        Text(
            year.year.toString(),
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.weight(1f),
        )
        Text(
            "${Format.flights(year.totals.flights)} · ${Format.hoursMinutes(year.totals.airtimeS)} h",
            style = MaterialTheme.typography.bodyMedium.numbers,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * One flight in the list: its shape, when it was, and how long and far.
 * Pressing and holding offers to delete it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LogbookRow(entry: LogbookEntry, shape: Shape, onOpen: () -> Unit, onDelete: () -> Unit) {
    var isShowingMenu by remember { mutableStateOf(false) }
    val score = entry.score

    Box {
        Surface(shape = shape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("logbook-row")
                    .combinedClickable(
                        onClick = onOpen,
                        onLongClick = { isShowingMenu = true },
                        onLongClickLabel = "Delete",
                    )
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TrackThumbnail(entry.outline)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(Format.dayInYear(entry.startUnixS), style = MaterialTheme.typography.titleMedium)
                    Text(
                        subtitle(entry),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Spacer(Modifier.width(8.dp))
                Column(horizontalAlignment = Alignment.End) {
                    Text(
                        "${Format.hoursMinutes(entry.durationS)} h",
                        style = MaterialTheme.typography.titleMedium.numbers,
                    )
                    // The scored cross-country distance; a triangle is marked.
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (score != null && score.kind != RouteKind.FREE_FLIGHT) {
                            Icon(
                                painterResource(R.drawable.ic_triangle),
                                contentDescription = if (score.kind == RouteKind.FAI_TRIANGLE) {
                                    "FAI triangle"
                                } else {
                                    "Triangle"
                                },
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Spacer(Modifier.width(3.dp))
                        }
                        Text(
                            "${Format.kilometres(score?.distanceM ?: entry.trackLengthM)} km",
                            style = MaterialTheme.typography.bodyMedium.numbers,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
            DropdownMenuItem(
                text = { Text("Delete") },
                leadingIcon = { Icon(painterResource(R.drawable.ic_delete), contentDescription = null) },
                onClick = {
                    isShowingMenu = false
                    onDelete()
                },
            )
        }
    }
}

private fun subtitle(entry: LogbookEntry): String {
    val time = Format.time(entry.startUnixS)
    val glider = entry.glider
    return if (glider.isNullOrEmpty()) time else "$time · $glider"
}
