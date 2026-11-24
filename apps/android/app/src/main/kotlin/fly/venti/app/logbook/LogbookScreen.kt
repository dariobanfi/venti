package fly.venti.app.logbook

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.AppNavigation
import fly.venti.app.LogbookRoute
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.design.text
import fly.venti.app.pilotMessage
import fly.venti.core.LogbookEntry
import fly.venti.core.LogbookYear
import fly.venti.core.RouteKind
import fly.venti.core.scoreKindWords
import fly.venti.core.Totals
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import fly.venti.app.design.Common

/** The pilot's flights, newest first, by year. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LogbookScreen(
    logbook: LogbookModel,
    navigation: AppNavigation,
    /** Where an import runs: it carries on when the pilot leaves this screen. */
    imports: CoroutineScope,
    /** Called with what there is to say about an import or a deletion. */
    onNotice: (ImportNotice?) -> Unit,
    /** The folder new flights come in from by themselves, if the pilot has chosen one. */
    watched: WatchedFolder? = null,
    bars: PaddingValues,
) {
    val context = LocalContext.current.applicationContext
    var flightToDelete by remember { mutableStateOf<LogbookEntry?>(null) }
    var isChoosingWhat by remember { mutableStateOf(false) }
    val progress = logbook.importProgress

    // After an import: open the flight if there was exactly one new one,
    // and say so if something needs saying.
    fun finish(report: ImportReport) {
        onNotice(report.notice)
        if (report.failures.isEmpty() && report.added.size == 1) {
            navigation.show(flight = report.added.first())
        }
    }

    // Flight files have no type the system knows, so any file can be
    // picked; the core says which ones are flights.
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { files ->
        if (files.isNotEmpty()) {
            imports.launch { finish(logbook.importFiles(files.map { FlightFiles.source(context, it) })) }
        }
    }
    // A folder brings in every flight file in it and in the folders inside
    // it: a whole history at once. Android has one picker for files and
    // another for a folder, so the pilot says which.
    val folderPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { folder ->
        if (folder != null) {
            imports.launch {
                finish(
                    logbook.importFolder(FlightFiles.folderName(context, folder)) {
                        FlightFiles.inFolder(context, folder)
                    },
                )
            }
        }
    }
    // A folder to look into again whenever the app opens: XCTrack's
    // tracklogs, as a rule. What is in it now comes in at once.
    val watchPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { folder ->
        if (folder != null && watched != null) {
            imports.launch {
                val report = try {
                    watched.watch(folder.toString())
                } catch (error: SecurityException) {
                    onNotice(ImportNotice("The folder cannot be kept", error.pilotMessage))
                    null
                }
                report?.let(::finish)
            }
        }
    }
    val chooseFiles = { filePicker.launch(arrayOf("*/*")) }
    val chooseFolder = { folderPicker.launch(null) }

    Scaffold(
        modifier = Modifier.padding(bottom = bars.calculateBottomPadding()),
        topBar = {
            LogbookBar(
                progress,
                isChoosingWhat,
                onChoosingWhat = { isChoosingWhat = it },
                chooseFiles,
                chooseFolder,
                watching = watched?.name,
                onWatch = if (watched != null) ({ watchPicker.launch(null) }) else null,
                onStopWatching = { watched?.stop() },
            )
        },
        // The tab bar below already keeps clear of the system's own bar.
        contentWindowInsets = WindowInsets(0),
    ) { padding ->
        Column(Modifier.padding(padding)) {
            // A watched folder that is lost: said over the flights, with
            // the two things to do about it.
            val lost = watched?.name?.takeIf { watched.isLost }
            if (lost != null) {
                LostFolderCard(
                    lost,
                    onChooseAgain = { watchPicker.launch(null) },
                    onStop = { watched.stop() },
                    modifier = Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s1, bottom = VentiSpace.s2),
                )
            }
            Box(Modifier.weight(1f)) {
                if (logbook.view.years.isEmpty()) {
                    EmptyLogbook(
                        isImporting = progress != null,
                        onImport = chooseFiles,
                        onImportFolder = chooseFolder,
                        onAddSample = { imports.launch { finish(logbook.addSample()) } },
                    )
                } else {
                    FlightList(
                        totals = logbook.view.totals,
                        years = logbook.view.years,
                        onOpen = { navigation.open(LogbookRoute.Flight(it.id)) },
                        onStatistics = { navigation.open(LogbookRoute.Statistics) },
                        onDelete = { flightToDelete = it },
                    )
                }
            }
            // One file is in before a bar could be read.
            if (progress != null && progress.total != 1) {
                ImportProgressBar(progress)
            }
        }
    }

    // Deleting asks first: the flight's file exists only on this phone.
    flightToDelete?.let { entry ->
        DeleteFlightDialog(
            warning = remember(entry.id) { logbook.deletionWarning },
            onConfirm = {
                flightToDelete = null
                try {
                    logbook.delete(entry.id)
                } catch (error: Exception) {
                    onNotice(ImportNotice("The flight could not be deleted", error.pilotMessage))
                }
            },
            onDismiss = { flightToDelete = null },
        )
    }
}

/**
 * The watched folder can no longer be found, so nothing comes in from it:
 * the caution's sign and words on a card of the pack, and under them
 * choosing the folder again or giving it up. It stays until the pilot does
 * one of the two, or the folder is found again.
 */
@Composable
private fun LostFolderCard(name: String, onChooseAgain: () -> Unit, onStop: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) {
        Column(Modifier.padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
            Row(
                modifier = Modifier.testTag("watched-folder-lost"),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
            ) {
                Icon(
                    painterResource(R.drawable.venti_caution),
                    contentDescription = null,
                    modifier = Modifier.size(VentiSize.icon),
                    tint = Venti.colours.caution,
                )
                Text(LogbookWords.lost(name), style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
            }
            Button(
                onClick = onChooseAgain,
                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("watched-folder-again"),
            ) {
                Text(LogbookWords.CHOOSE_AGAIN)
            }
            TextButton(
                onClick = onStop,
                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("watched-folder-stop"),
            ) {
                Text(LogbookWords.stopWatching(name), textAlign = TextAlign.Center)
            }
        }
    }
}

/** The title bar, with the button that brings flights in. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LogbookBar(
    progress: ImportProgress?,
    isChoosingWhat: Boolean,
    onChoosingWhat: (Boolean) -> Unit,
    chooseFiles: () -> Unit,
    chooseFolder: () -> Unit,
    /** The name of the folder new flights come in from by themselves, if any. */
    watching: String? = null,
    /** Chooses that folder; `null` where there is none to choose. */
    onWatch: (() -> Unit)? = null,
    onStopWatching: () -> Unit = {},
) {
    TopAppBar(
        title = { Text("Logbook") },
        actions = {
            Box {
                IconButton(onClick = { onChoosingWhat(true) }, enabled = progress == null) {
                    Icon(painterResource(R.drawable.ic_add), contentDescription = "Import flights")
                }
                DropdownMenu(expanded = isChoosingWhat, onDismissRequest = { onChoosingWhat(false) }) {
                    DropdownMenuItem(
                        text = { Text("Flight files") },
                        leadingIcon = { Icon(painterResource(R.drawable.ic_file_open), contentDescription = null) },
                        onClick = {
                            onChoosingWhat(false)
                            chooseFiles()
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("A folder of flights") },
                        leadingIcon = { Icon(painterResource(R.drawable.ic_folder_open), contentDescription = null) },
                        onClick = {
                            onChoosingWhat(false)
                            chooseFolder()
                        },
                    )
                    if (onWatch != null) {
                        DropdownMenuItem(
                            text = { Text(if (watching == null) LogbookWords.WATCH else LogbookWords.watchAnother(watching)) },
                            leadingIcon = { Icon(painterResource(R.drawable.ic_refresh), contentDescription = null) },
                            onClick = {
                                onChoosingWhat(false)
                                onWatch()
                            },
                        )
                    }
                    if (watching != null) {
                        DropdownMenuItem(
                            text = { Text(LogbookWords.stopWatching(watching)) },
                            leadingIcon = { Icon(painterResource(R.drawable.ic_close), contentDescription = null) },
                            onClick = {
                                onChoosingWhat(false)
                                onStopWatching()
                            },
                        )
                    }
                }
            }
        },
    )
}

/**
 * How far an import of many files has come, under the list. It takes its
 * room from the list and does not float over it.
 */
@Composable
private fun ImportProgressBar(progress: ImportProgress) {
    // A folder's files are counted before the first is read.
    val isLooking = progress.total == 0
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("import-progress")
            .clearAndSetSemantics {
                contentDescription = "Importing flights"
                stateDescription = if (isLooking) "Looking for flight files" else "${progress.done} of ${progress.total}"
            },
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
    ) {
        Column(Modifier.padding(horizontal = 20.dp, vertical = 14.dp)) {
            Text(
                if (isLooking) {
                    "Looking for flight files"
                } else {
                    "Importing ${Format.count(progress.done.toLong())} of ${Format.count(progress.total.toLong())}"
                },
                style = MaterialTheme.typography.titleSmall.numbers,
            )
            Spacer(Modifier.size(10.dp))
            if (isLooking) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
            } else {
                LinearProgressIndicator(
                    progress = { progress.done.toFloat() / progress.total },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
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
        confirmButton = { TextButton(onClick = onDismiss) { Text(Common.words.ok) } },
    )
}

/** Asks before a flight is removed for good. */
@Composable
fun DeleteFlightDialog(
    /** What deleting means: [LogbookModel.deletionWarning]. */
    warning: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Delete this flight?") },
        text = { Text(warning) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text("Delete flight", color = MaterialTheme.colorScheme.error)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(Common.words.cancel) } },
    )
}

@Composable
private fun EmptyLogbook(
    /** While files are being imported nothing more can be started. */
    isImporting: Boolean,
    onImport: () -> Unit,
    onImportFolder: () -> Unit,
    onAddSample: () -> Unit,
    modifier: Modifier = Modifier,
) {
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
            "Import the IGC files your vario or flight app recorded, one by one or a whole folder of " +
                "them. They stay on this phone.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.size(24.dp))
        Button(onClick = onImport, enabled = !isImporting, modifier = Modifier.heightIn(min = 52.dp)) {
            Text("Import flights", style = MaterialTheme.typography.titleMedium)
        }
        Spacer(Modifier.size(4.dp))
        TextButton(onClick = onImportFolder, enabled = !isImporting) {
            Text("Import a folder of flights")
        }
        TextButton(onClick = onAddSample, enabled = !isImporting) {
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
        modifier = modifier.fillMaxSize().testTag("logbook-list"),
        contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s1, bottom = VentiSpace.s5),
    ) {
        item(key = "totals") {
            TotalsCard(totals, onClick = onStatistics)
        }
        // The flights by day, newest first: each day a card of its own
        // under its heading, the first day of a year with the year's
        // count above it.
        for (year in years) {
            val days = LogbookDay.days(year.flights)
            days.forEachIndexed { index, day ->
                item(key = "day-${day.date}") {
                    DayHeader(day, year = year.takeIf { index == 0 })
                    Group {
                        for (entry in day.flights) {
                            row {
                                LogbookRow(
                                    entry = entry,
                                    onOpen = { onOpen(entry) },
                                    onDelete = { onDelete(entry) },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * The shape of one row in a group of rows: the group has round corners, and
 * the rows inside it meet with small ones.
 */
internal fun groupedShape(index: Int, count: Int): Shape {
    val outer = 20.dp
    val inner = 4.dp
    val top = if (index == 0) outer else inner
    val bottom = if (index == count - 1) outer else inner
    return RoundedCornerShape(topStart = top, topEnd = top, bottomStart = bottom, bottomEnd = bottom)
}

/** The flights of one day, by the clock of the place each was flown, newest first. */
data class LogbookDay(
    /** The day, as the calendar where the flights took off has it. */
    val date: LocalDate,
    val flights: List<LogbookEntry>,
) {
    /** The day's time in the air. */
    val airtimeS: Long get() = flights.sumOf { it.durationS }

    companion object {
        /**
         * The flights, already newest first, in runs of one calendar day:
         * each flight's day by the zone it was flown in, the phone's own
         * where the core knows none (or `zone`, for a test).
         */
        fun days(flights: List<LogbookEntry>, zone: ZoneId? = null): List<LogbookDay> {
            val days = mutableListOf<LogbookDay>()
            for (entry in flights) {
                val date = Instant.ofEpochSecond(entry.startUnixS).atZone(zone ?: Format.zone(entry.timeZone)).toLocalDate()
                val last = days.lastOrNull()
                if (last != null && last.date == date) {
                    days[days.lastIndex] = last.copy(flights = last.flights + entry)
                } else {
                    days += LogbookDay(date, listOf(entry))
                }
            }
            return days
        }
    }
}

/**
 * All flights counted together, at the top of the list, quieter than the
 * flights under it. It leads to the statistics.
 */
@Composable
private fun TotalsCard(totals: Totals, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth().testTag("logbook-totals"),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) {
        Row(
            modifier = Modifier.padding(start = VentiSpace.s4, top = VentiSpace.s3, bottom = VentiSpace.s3, end = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Figure(
                caption = if (totals.flights == 1u) "Flight" else "Flights",
                value = Format.count(totals.flights.toLong()),
                modifier = Modifier.weight(1f),
            )
            Figure(
                caption = "Airtime",
                value = Format.hoursMinutes(totals.airtimeS),
                unit = "h",
                modifier = Modifier.weight(1f),
            )
            Figure(
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

/** One of the totals: its number with the unit, its name under it. */
@Composable
private fun Figure(caption: String, value: String, modifier: Modifier = Modifier, unit: String = "") {
    val small = MaterialTheme.typography.bodySmall
    Column(
        modifier.clearAndSetSemantics {
            // One element for a screen reader: "Airtime, 0:29 h".
            contentDescription = caption
            stateDescription = if (unit.isEmpty()) value else "$value $unit"
        },
    ) {
        Text(
            buildAnnotatedString {
                append(value)
                if (unit.isNotEmpty()) {
                    withStyle(SpanStyle(fontSize = small.fontSize, color = Venti.colours.textSecondary)) { append(" $unit") }
                }
            },
            style = MaterialTheme.typography.titleMedium.numbers,
            maxLines = 1,
            softWrap = false,
        )
        Text(caption, style = small, color = Venti.colours.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * A day's heading: the day, and with several flights their count and
 * airtime. The first day of a year has the year above it, with the year's
 * count and airtime, small.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun DayHeader(day: LogbookDay, year: LogbookYear?) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s1, end = VentiSpace.s1, top = VentiSpace.s5, bottom = VentiSpace.s2),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        if (year != null) {
            Row(
                modifier = Modifier.fillMaxWidth().testTag("logbook-year-${year.year}").semantics(mergeDescendants = true) {},
                verticalAlignment = Alignment.Bottom,
            ) {
                Text(
                    year.year.toString(),
                    style = MaterialTheme.typography.labelLarge,
                    color = Venti.colours.textSecondary,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    "${Format.flights(year.totals.flights)} · ${Format.hoursMinutes(year.totals.airtimeS)} h",
                    style = MaterialTheme.typography.bodySmall.numbers,
                    color = Venti.colours.textSecondary,
                )
            }
        }
        FlowRow(
            modifier = Modifier.fillMaxWidth().testTag("logbook-day").semantics(mergeDescendants = true) { heading() },
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                day.flights.first().let { Format.day(it.startUnixS, Format.zone(it.timeZone)) },
                style = MaterialTheme.typography.titleMedium,
                color = Venti.colours.text,
                modifier = Modifier.padding(end = VentiSpace.s2),
            )
            if (day.flights.size > 1) {
                Text(
                    "${Format.flights(day.flights.size.toUInt())} · ${Format.hoursMinutes(day.airtimeS)} h",
                    style = MaterialTheme.typography.bodyMedium.numbers,
                    color = Venti.colours.textSecondary,
                )
            }
        }
    }
}

/**
 * One flight in the list, made to be told apart at a glance: the track's
 * shape on a little map of its own; the take-off's time first and large,
 * and the airtime large across from it; where it took off; the glider,
 * with the distance across from it. Pressing and holding offers to delete
 * it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LogbookRow(entry: LogbookEntry, onOpen: () -> Unit, onDelete: () -> Unit) {
    var isShowingMenu by remember { mutableStateOf(false) }
    // With the largest text sizes the shape stands above the words, which
    // then have the row's whole width.
    val stacked = LocalDensity.current.fontScale >= LogbookWords.STACKED_FROM_FONT_SCALE

    Box {
        val modifier = Modifier
            .fillMaxWidth()
            .testTag("logbook-row")
            .combinedClickable(
                onClick = onOpen,
                onLongClick = { isShowingMenu = true },
                onLongClickLabel = "Delete",
            )
            .clearAndSetSemantics {
                contentDescription = LogbookWords.place(entry)
                stateDescription = LogbookWords.spoken(entry)
            }
            .padding(VentiSpace.s3)
        if (stacked) {
            Column(modifier, verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                TrackThumbnail(entry.outline, large = true)
                RowWords(entry)
            }
        } else {
            Row(modifier, verticalAlignment = Alignment.CenterVertically) {
                TrackThumbnail(entry.outline, large = true)
                Spacer(Modifier.width(VentiSpace.s3))
                RowWords(entry, Modifier.weight(1f))
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

/** The words of a flight's row: the time and the airtime, the place, the glider and the distance. */
@Composable
private fun RowWords(entry: LogbookEntry, modifier: Modifier = Modifier) {
    val score = entry.score
    val quiet = Venti.colours.textSecondary
    val small = MaterialTheme.typography.bodyMedium.numbers
    Column(modifier, verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                Format.time(entry.startUnixS, Format.zone(entry.timeZone)),
                style = VentiType.instrumentM.text,
                color = Venti.colours.text,
                maxLines = 1,
                modifier = Modifier.weight(1f).alignByBaseline(),
            )
            Text(
                buildAnnotatedString {
                    append(Format.hoursMinutes(entry.durationS))
                    withStyle(SpanStyle(fontSize = VentiType.label.text.fontSize, fontWeight = FontWeight.Medium, color = quiet)) {
                        append(" h")
                    }
                },
                style = VentiType.instrumentM.text,
                color = Venti.colours.text,
                maxLines = 1,
                softWrap = false,
                modifier = Modifier.alignByBaseline(),
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
            Sign(R.drawable.venti_takeoff)
            Text(
                LogbookWords.place(entry),
                style = MaterialTheme.typography.titleMedium,
                color = Venti.colours.text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            val glider = entry.glider?.takeIf { it.isNotEmpty() }
            Row(
                Modifier.weight(1f),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                if (glider != null) {
                    Sign(R.drawable.venti_wing)
                    Text(glider, style = small, color = quiet, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            Spacer(Modifier.width(VentiSpace.s2))
            // The scored cross-country distance; a triangle is marked.
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                if (score != null && score.kind != RouteKind.FREE_FLIGHT) {
                    Sign(R.drawable.venti_triangle)
                }
                Text(LogbookWords.distance(entry), style = small, color = quiet, maxLines = 1, softWrap = false)
            }
        }
    }
}

/** One of the pack's icons beside the smaller lines, in the quiet ink, growing with the text. */
@Composable
private fun Sign(icon: Int) {
    Icon(
        painterResource(icon),
        contentDescription = null,
        modifier = Modifier.size(VentiSpace.s4 * LocalDensity.current.fontScale),
        tint = Venti.colours.textMuted,
    )
}

/** What the logbook says about the folder it brings flights in from by itself. */
object LogbookWords {
    const val WATCH = "Bring in new flights from a folder"

    fun watchAnother(name: String): String = "Bring in from another folder than $name"

    fun stopWatching(name: String): String = "Stop bringing in from $name"

    /** Said over the flights while the watched folder is lost; the iPhone's words. */
    fun lost(name: String): String =
        "Venti can no longer find the folder $name, so no new flights come in from it. " +
            "Choose it again, or stop bringing in from it."

    const val CHOOSE_AGAIN = "Choose the folder again"

    /** From this text size on a row's drawing stands above its words. */
    const val STACKED_FROM_FONT_SCALE = 1.5f

    /**
     * Where it took off: the name the core gives (the pilot's waypoint
     * there, or the server's site once synced), else where it is.
     */
    fun place(entry: LogbookEntry): String {
        entry.takeoffName?.takeIf { it.isNotEmpty() }?.let { return it }
        val first = entry.outline.firstOrNull() ?: return Format.day(entry.startUnixS, Format.zone(entry.timeZone))
        return Format.place(first.lat, first.lon)
    }

    /** The scored cross-country distance, else the track's length: "12.3 km". */
    fun distance(entry: LogbookEntry): String = "${Format.kilometres(entry.score?.distanceM ?: entry.trackLengthM)} km"

    /** What a screen reader says of a row after the place. */
    fun spoken(entry: LogbookEntry): String {
        // A triangle is named, as the core names it; a free flight is not.
        val kind = entry.score?.takeIf { it.kind != RouteKind.FREE_FLIGHT }?.let { scoreKindWords(it.kind, it.closed) }
        return listOfNotNull(
            "Took off at ${Format.time(entry.startUnixS, Format.zone(entry.timeZone))}",
            "airtime ${Format.hoursMinutes(entry.durationS)} h",
            entry.glider?.takeIf { it.isNotEmpty() },
            listOfNotNull(kind, distance(entry)).joinToString(" "),
        ).joinToString(", ")
    }
}
