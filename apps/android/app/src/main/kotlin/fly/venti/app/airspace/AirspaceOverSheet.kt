package fly.venti.app.airspace

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiSpace
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.StatusChip
import fly.venti.app.design.StatusKind
import fly.venti.app.design.Venti
import fly.venti.core.AirspaceOverPlace
import fly.venti.core.AirspaceOverhead
import fly.venti.core.AirspacePolicy
import fly.venti.core.airspaceHeightFilter

/** How Venti treats an airspace, as the pack's status: never colour alone. */
fun AirspacePolicy.status(): StatusKind = when (this) {
    AirspacePolicy.AVOID -> StatusKind.Danger
    AirspacePolicy.CAUTION -> StatusKind.Caution
    AirspacePolicy.FREE -> StatusKind.Good
}

/**
 * What lies over a place the pilot tapped on the map where airspace is
 * drawn: the core's list from the lowest floor up, what is not known, and
 * at the foot the filter by height for the maps, with the core's words.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AirspaceOverSheet(
    over: AirspaceOverPlace,
    /** The ceiling the maps draw airspace up to now; `null` for all. */
    ceilingM: Double?,
    onCeiling: (Double?) -> Unit,
    onDismiss: () -> Unit,
) {
    val filter = remember { airspaceHeightFilter() }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false),
        containerColor = MaterialTheme.colorScheme.surface,
        modifier = Modifier.semantics { paneTitle = "Airspace over this place" },
    ) {
        // Taller than half the screen, so that the sheet opens at half
        // height with the place still in view above it.
        Box(Modifier.fillMaxHeight()) {
            Column(Modifier.testTag("airspace-over")) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        Format.place(over.position.lat, over.position.lon),
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                    TextButton(onClick = onDismiss) { Text("Done") }
                }
                Column(
                    modifier = Modifier
                        .verticalScroll(rememberScrollState())
                        .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s6),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                ) {
                    Text(
                        over.words,
                        style = MaterialTheme.typography.titleMedium,
                        modifier = Modifier.padding(horizontal = VentiSpace.s1).testTag("airspace-over-words"),
                    )
                    over.coverNote?.let { SectionFooter(it) }
                    for (airspace in over.airspaces) {
                        Airspace(airspace)
                    }
                    over.hoursNote?.let { SectionFooter(it) }
                    SectionFooter(over.caveat, Modifier.testTag("airspace-over-caveat"))
                    // The filter by height for the maps, under the core's title.
                    Column {
                        SectionHeader(filter.title)
                        SegmentedPills(
                            filter.steps.map { it.words },
                            filter.steps.indexOfFirst { it.ceilingM == ceilingM }.coerceAtLeast(0),
                            Modifier.testTag("airspace-height-filter"),
                            onChoose = { index -> onCeiling(filter.steps[index].ceilingM) },
                        )
                        SectionFooter(filter.note)
                    }
                }
            }
        }
    }
}

/** One airspace over the place: its name and how Venti treats it, what it is, its limits, why, its use and hours. */
@Composable
private fun Airspace(airspace: AirspaceOverhead) {
    val quiet = Venti.colours.textSecondary
    Group(Modifier.testTag("airspace-over-row")) {
        row {
            Column(
                Modifier.fillMaxWidth().padding(VentiSpace.s4),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    Text(
                        airspace.name,
                        style = MaterialTheme.typography.titleMedium,
                        modifier = Modifier.weight(1f),
                    )
                    StatusChip(airspace.policy.status(), airspace.policyWords)
                }
                Text(airspace.kind, style = MaterialTheme.typography.bodyMedium, color = quiet)
                Text("${airspace.lower} to ${airspace.upper}", style = MaterialTheme.typography.bodyLarge)
                Text(airspace.why, style = MaterialTheme.typography.bodyMedium, color = quiet)
                airspace.publishedUse?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                for (line in airspace.hours) {
                    Text(line, style = MaterialTheme.typography.bodyMedium, color = quiet)
                }
                airspace.hoursRemarks?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = quiet) }
                airspace.remarks?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = quiet) }
                Text(
                    AirspaceWords.origin(airspace.origin, airspace.source, airspace.dataUnixS),
                    style = MaterialTheme.typography.bodySmall,
                    color = quiet,
                )
            }
        }
    }
}
