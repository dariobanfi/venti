package fly.venti.app.flight

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiLight
import fly.venti.app.community.CommunityModel
import fly.venti.app.community.PublicFlightRow
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.logbook.TrackThumbnail
import fly.venti.app.pilotMessage
import fly.venti.app.wind.grown
import fly.venti.core.ComparisonLead
import fly.venti.core.FlightAnalysis
import fly.venti.core.FlightComparison
import fly.venti.core.FlightView
import fly.venti.core.LogbookEntry
import fly.venti.core.PublicFlight
import fly.venti.core.TrackPoint
import fly.venti.core.compareFlights
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.abs
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.roundToLong
import kotlin.math.sin
import kotlin.math.sqrt
import fly.venti.app.design.Common

/**
 * A second flight laid beside the one on screen. Flights that were in the
 * air at the same time are laid over each other by the clock; others each
 * from their first moment. The core says which, and what there is to
 * compare in numbers.
 */
data class ComparedFlight(
    val flight: FlightView,
    /** Whose it is, or which of the pilot's own: "Mara Muster", "Your flight of Sat, 4 Jul". */
    val name: String,
    val comparison: FlightComparison,
) {
    /** Where the other glider was at a moment of the flight on screen; `null` when it was not in the air then. */
    fun point(atSeconds: UInt): TrackPoint? {
        val theirs = atSeconds.toDouble() - comparison.offsetS.toDouble()
        if (theirs < 0 || theirs > flight.durationS.toDouble() || flight.track.isEmpty()) return null
        return flight.track[TrackSearch.nearestIndex(flight.track, theirs)]
    }

    /**
     * What to say about the other glider at a moment of the flight on
     * screen: "1,840 m · 320 m away". `null` when it was not in the air then.
     */
    fun words(at: TrackPoint): String? {
        val other = point(at.seconds) ?: return null
        val apart = CompareWords.metresBetween(at.lat, at.lon, other.lat, other.lon)
        val away = if (apart < 1000) {
            "${Format.metres((apart / 10).roundToLong() * 10.0)} m away"
        } else {
            "${Format.kilometres(apart)} km away"
        }
        return "${Format.metres(other.altitudeM)} m · $away"
    }

    /** The position in the track on screen at which the two were nearest, if they flew together. */
    fun nearestIndex(track: List<TrackPoint>): Int? =
        comparison.nearest?.let { TrackSearch.nearestIndex(track, it.seconds.toDouble()) }

    companion object {
        /** Compares, off the main thread: both tracks go to the core whole. */
        suspend fun of(
            first: FlightView,
            firstAnalysis: FlightAnalysis?,
            other: FlightView,
            otherAnalysis: FlightAnalysis?,
            name: String,
        ): ComparedFlight = withContext(Dispatchers.Default) {
            ComparedFlight(other, name, compareFlights(first, other, firstAnalysis, otherAnalysis))
        }
    }
}

/** What the comparison says that is not the core's. */
object CompareWords {
    const val LAY_BESIDE = "Lay another flight beside it"
    const val SIDE_BY_SIDE_ROW = "The two flights side by side"
    const val CHOOSER_TITLE = "Lay a flight beside it"
    const val FLOWN_HERE = "Flown here that day"
    const val FLOWN_HERE_FOOTER =
        "Flights that other pilots show to everyone on Venti, begun within twelve hours of this one around " +
            "where it was flown."
    const val NOBODY_HERE = "Nobody shows a flight from around here on that day."
    const val YOUR_FLIGHTS = "Your flights"
    const val NO_OTHER_OWN = "Your logbook holds no other flight."
    const val NOT_LAID = "This flight could not be laid beside it"
    const val SHEET_TITLE = "Side by side"
    const val THIS_FLIGHT = "This flight"
    const val FOOTER =
        "The number that is ahead is set in bold. Worked out by Venti from the two recordings; where one of " +
            "them is cut short, so are its numbers."
    const val NEAREST = "Go to where they were nearest"
    const val END = "Take the other flight away"
    const val NOT_IN_THE_AIR = "Not in the air then"

    /** One of the pilot's own flights, named: "Your flight of Sat, 4 Jul 2026". */
    fun own(startUnixS: Long): String = "Your flight of ${Format.day(startUnixS)}"

    /** How far apart two places are over the ground, metres: on a sphere, which is near enough for a line in a panel. */
    fun metresBetween(lat1: Double, lon1: Double, lat2: Double, lon2: Double): Double {
        val radians = Math.PI / 180
        val dLat = (lat2 - lat1) * radians
        val dLon = (lon2 - lon1) * radians
        val a = sin(dLat / 2) * sin(dLat / 2) + cos(lat1 * radians) * cos(lat2 * radians) * sin(dLon / 2) * sin(dLon / 2)
        return 2 * 6_371_000.0 * asin(sqrt(a.coerceIn(0.0, 1.0)))
    }

    /**
     * The pilot's other flights, those nearest in time first: flights of the
     * same day lead the list. No more than fifty.
     */
    fun ownCandidates(flights: List<LogbookEntry>, flight: FlightView): List<LogbookEntry> =
        flights
            .filter { it.id != flight.id && it.startUnixS != flight.startUnixS }
            .sortedBy { abs(it.startUnixS - flight.startUnixS) }
            .take(50)
}

/**
 * The colour of a second flight laid beside the one on screen: the pack's
 * first colour for comparing flights, which no climb band has. On the map,
 * which is light in both appearances, the light theme's.
 */
object CompareColours {
    val onMap: Color = VentiLight.series[0]

    val onScreen: Color
        @Composable get() = Venti.colours.series[0]
}

/** Where the flights to lay beside one come from: the pilot's logbook, and everyone's flights on Venti. */
class CompareSources(val logbook: LogbookModel, val community: CommunityModel)

/**
 * Chooses the flight to lay beside the one on screen: one that other pilots
 * show to everyone and that was flown around the same place on the same
 * day, or one of the pilot's own.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CompareChooser(
    flight: FlightView,
    analysis: FlightAnalysis?,
    sources: CompareSources,
    onChosen: (ComparedFlight) -> Unit,
    onDismiss: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val community = sources.community
    val hasServer = community.server() != null
    var others by remember { mutableStateOf<List<PublicFlight>?>(null) }
    var listProblem by remember { mutableStateOf<String?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf<String?>(null) }
    val own = remember(flight) {
        CompareWords.ownCandidates(sources.logbook.view.years.flatMap { it.flights }, flight)
    }

    LaunchedEffect(flight.id) {
        if (!hasServer) return@LaunchedEffect
        try {
            val found = community.flightsAround(
                flight.bounds.grown(0.1),
                fromUnixS = flight.startUnixS - 12 * 3600,
                toUnixS = flight.startUnixS + 12 * 3600,
            )
            // The flight on screen may be one of them.
            others = found.filter { it.startUnixS != flight.startUnixS }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            listProblem = community.words(error)
        }
    }

    fun finish(other: FlightView, otherAnalysis: FlightAnalysis?, name: String) {
        scope.launch {
            onChosen(ComparedFlight.of(flight, analysis, other, otherAnalysis, name))
            onDismiss()
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(CompareWords.CHOOSER_TITLE, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text(Common.words.cancel) }
        }
        Box(Modifier.fillMaxHeight()) {
            LazyColumn(
                modifier = Modifier.testTag("compare-chooser"),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp),
            ) {
                if (hasServer) {
                    item { SectionHeader(CompareWords.FLOWN_HERE) }
                    val found = others
                    when {
                        found == null && listProblem != null -> item { Quiet(checkNotNull(listProblem)) }
                        found == null -> item { CircularProgressIndicator(Modifier.padding(16.dp)) }
                        found.isEmpty() -> item { Quiet(CompareWords.NOBODY_HERE) }
                        else -> items(found.size) { index ->
                            val other = found[index]
                            PublicFlightRow(other, MaterialTheme.shapes.medium, tag = "compare-public-row") {
                                if (loading != null) return@PublicFlightRow
                                loading = other.id
                                scope.launch {
                                    try {
                                        val opened = community.open(other)
                                        finish(opened.flight, opened.analysis, other.pilot)
                                    } catch (error: CancellationException) {
                                        throw error
                                    } catch (error: Exception) {
                                        problem = community.words(error)
                                    } finally {
                                        loading = null
                                    }
                                }
                            }
                        }
                    }
                    item { SectionFooter(CompareWords.FLOWN_HERE_FOOTER) }
                }
                item { SectionHeader(CompareWords.YOUR_FLIGHTS) }
                if (own.isEmpty()) {
                    item { Quiet(CompareWords.NO_OTHER_OWN) }
                } else {
                    item {
                        Group {
                            for (entry in own) {
                                row {
                                    OwnFlightLine(entry, isLoading = loading == entry.id) {
                                        if (loading != null) return@OwnFlightLine
                                        loading = entry.id
                                        scope.launch {
                                            try {
                                                val other = sources.logbook.flight(entry.id)
                                                val otherAnalysis = runCatching { sources.logbook.analysis(entry.id) }.getOrNull()
                                                finish(other, otherAnalysis, CompareWords.own(entry.startUnixS))
                                            } catch (error: CancellationException) {
                                                throw error
                                            } catch (error: Exception) {
                                                problem = error.pilotMessage
                                            } finally {
                                                loading = null
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    problem?.let { words ->
        AlertDialog(
            onDismissRequest = { problem = null },
            title = { Text(CompareWords.NOT_LAID) },
            text = { Text(words) },
            confirmButton = { TextButton(onClick = { problem = null }) { Text(Common.words.ok) } },
        )
    }
}

@Composable
private fun Quiet(text: String) {
    Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 4.dp, vertical = 12.dp))
}

/** One of the pilot's own flights, in a line of the chooser. */
@Composable
private fun OwnFlightLine(entry: LogbookEntry, isLoading: Boolean, onChoose: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("compare-own-row")
            .clickable(role = Role.Button, onClick = onChoose)
            .padding(horizontal = 12.dp, vertical = 10.dp)
            .clearAndSetSemantics {
                contentDescription = Format.day(entry.startUnixS)
                stateDescription = "${Format.kilometres(entry.score?.distanceM ?: entry.trackLengthM)} kilometres, " +
                    "${Format.hoursMinutes(entry.durationS)} hours"
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TrackThumbnail(entry.outline)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(Format.day(entry.startUnixS), style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                entry.takeoffName ?: Format.time(entry.startUnixS),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (isLoading) {
            CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
            Spacer(Modifier.width(8.dp))
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(
                "${Format.kilometres(entry.score?.distanceM ?: entry.trackLengthM)} km",
                style = MaterialTheme.typography.titleMedium.numbers,
            )
            Text(
                "${Format.hoursMinutes(entry.durationS)} h",
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * The two flights in numbers, side by side, with what the pilot can do from
 * here: go to where the two were nearest, and end the comparison.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ComparisonSheet(compared: ComparedFlight, onNearest: () -> Unit, onEnd: () -> Unit, onDismiss: () -> Unit) {
    val other = CompareColours.onScreen
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(CompareWords.SHEET_TITLE, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text(Common.words.done) }
        }
        LazyColumn(contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp)) {
            item {
                Text(
                    compared.comparison.words,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 12.dp),
                )
            }
            item {
                Group(Modifier.testTag("comparison-rows")) {
                    row {
                        ComparisonLine(
                            "",
                            CompareWords.THIS_FLIGHT,
                            compared.name,
                            ComparisonLead.NEITHER,
                            other,
                            heading = true,
                        )
                    }
                    for (line in compared.comparison.rows) {
                        row { ComparisonLine(line.label, line.first, line.other, line.lead, other) }
                    }
                }
            }
            item { SectionFooter(CompareWords.FOOTER) }
            item {
                Spacer(Modifier.size(16.dp))
                Group {
                    if (compared.comparison.nearest != null) {
                        row {
                            ActionRow(CompareWords.NEAREST, MaterialTheme.colorScheme.primary) {
                                onNearest()
                                onDismiss()
                            }
                        }
                    }
                    row {
                        ActionRow(CompareWords.END, MaterialTheme.colorScheme.error, Modifier.testTag("compare-end")) {
                            onEnd()
                            onDismiss()
                        }
                    }
                }
            }
        }
    }
}

/** One number of both flights: the label, this flight's, the other's in its colour; the one ahead in bold. */
@Composable
private fun ComparisonLine(
    label: String,
    first: String,
    second: String,
    lead: ComparisonLead,
    otherColour: Color,
    heading: Boolean = false,
) {
    val style = if (heading) MaterialTheme.typography.labelLarge else MaterialTheme.typography.bodyMedium.numbers
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .clearAndSetSemantics {
                contentDescription = label.ifEmpty { "Flights" }
                stateDescription = "$first, $second"
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = style, color = quiet, modifier = Modifier.weight(1.2f), maxLines = 2)
        Text(
            first,
            style = style,
            color = if (heading) quiet else MaterialTheme.colorScheme.onSurface,
            fontWeight = if (lead == ComparisonLead.FIRST) FontWeight.Bold else null,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            second,
            style = style,
            color = if (heading) quiet else otherColour,
            fontWeight = if (lead == ComparisonLead.OTHER) FontWeight.Bold else null,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
