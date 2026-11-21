package fly.venti.app.forecast

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.CollectionInfo
import androidx.compose.ui.semantics.CollectionItemInfo
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collectionInfo
import androidx.compose.ui.semantics.collectionItemInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.design.FloatElevation
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.StatusChip
import fly.venti.app.design.Venti
import fly.venti.app.design.status
import fly.venti.app.today.ThermalLine
import fly.venti.core.Bounds
import fly.venti.core.ThermalLegendEntry
import fly.venti.core.ThermalMap
import fly.venti.core.ThermalMapCell
import fly.venti.core.ThermalMapDay

/**
 * The card at the map's lower edge while the forecast map shows: the days
 * the map has, the key, the model with when its run began, and what to know
 * of a day that was not judged whole. While the map is asked for it says
 * so; when there is none, why, with "Try again".
 */
@Composable
fun ThermalMapCard(
    state: ThermalMapState,
    /** The part of the world in view, to say when the map has nothing there. */
    region: Bounds?,
    onChoose: (String) -> Unit,
    onTryAgain: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colours = Venti.colours
    // A long note at a large text size scrolls inside the card rather
    // than pushing it over the chips.
    val tallest = with(LocalDensity.current) { (LocalWindowInfo.current.containerSize.height * MAX_HEIGHT_SHARE).toDp() }
    Surface(
        modifier = modifier.testTag("forecast-map-card"),
        shape = MaterialTheme.shapes.medium,
        color = colours.surface,
        border = BorderStroke(1.dp, colours.border),
        shadowElevation = FloatElevation,
    ) {
        Column(
            modifier = Modifier
                .heightIn(max = tallest)
                .verticalScroll(rememberScrollState())
                .padding(VentiSpace.s3),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        ) {
            when (state) {
                ThermalMapState.Asking -> Row(
                    modifier = Modifier.testTag("forecast-map-asking"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                ) {
                    CircularProgressIndicator(Modifier.size(VentiSpace.s4), strokeWidth = 2.dp)
                    Text(ThermalMapWords.ASKING, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                }
                is ThermalMapState.Failed -> Problem(state.message, onTryAgain)
                is ThermalMapState.There -> {
                    val days = state.map.days
                    if (days.size > 1) {
                        SegmentedPills(
                            days.map { it.name },
                            days.indexOf(state.day).coerceAtLeast(0),
                            Modifier.testTag("forecast-days"),
                            onChoose = { index -> onChoose(days[index].date) },
                        )
                    }
                    if (region != null && !ThermalMapModel.hasSquaresIn(state.day, region)) {
                        Problem(ThermalMapWords.NONE_HERE, onTryAgain)
                    } else {
                        Key(state.map.legend)
                    }
                    Text(
                        ThermalMapWords.run(state.map),
                        style = MaterialTheme.typography.bodySmall,
                        color = colours.textSecondary,
                        modifier = Modifier.testTag("forecast-map-run"),
                    )
                    if (!state.day.whole && state.day.note.isNotEmpty()) {
                        Text(
                            state.day.note,
                            style = MaterialTheme.typography.bodySmall,
                            color = colours.textSecondary,
                            modifier = Modifier.testTag("forecast-map-day-note"),
                        )
                    }
                }
            }
        }
    }
}

/** At most this share of the screen's height is the card's. */
private const val MAX_HEIGHT_SHARE = 0.4f

/** Why there is no map to show, and the button that asks again. */
@Composable
private fun Problem(message: String, onTryAgain: () -> Unit) {
    Column(Modifier.testTag("forecast-map-problem")) {
        Text(message, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.text)
        TextButton(onClick = onTryAgain, modifier = Modifier.heightIn(min = VentiSize.touch)) {
            Text(ThermalMapWords.TRY_AGAIN)
        }
    }
}

/**
 * The key, the core's: a swatch in the colour the map fills a square with
 * and the words for it, from the best kind of day down, then "Not flyable"
 * with the veil's swatch, its words like the others'. It wraps at a narrow width or a large text size,
 * and TalkBack reads it as a list.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Key(legend: List<ThermalLegendEntry>) {
    FlowRow(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("forecast-key")
            .semantics { collectionInfo = CollectionInfo(rowCount = legend.size, columnCount = 1) },
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
    ) {
        legend.forEachIndexed { index, entry ->
            val fill = entry.scaleStep?.toInt() ?: ThermalMapLayer.NOT_FLYABLE
            Row(
                modifier = Modifier.clearAndSetSemantics {
                    contentDescription = entry.words
                    collectionItemInfo = CollectionItemInfo(rowIndex = index, rowSpan = 1, columnIndex = 0, columnSpan = 1)
                },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Swatch(fill)
                Text(
                    entry.words,
                    style = MaterialTheme.typography.labelMedium,
                    color = Venti.colours.textSecondary,
                )
            }
        }
    }
}

/** A square in the map's colour, with a hairline edge: the palest step is close to the surface. */
@Composable
private fun Swatch(fill: Int) {
    // Square, as what it stands for is.
    Box(
        Modifier
            .size(VentiSpace.s3)
            .background(ThermalMapLayer.colour(fill), RectangleShape)
            .border(1.dp, Venti.colours.borderStrong, RectangleShape),
    )
}

/**
 * A square of the forecast map, tapped: the pack's sheet with the day's
 * name, the status as icon, word and colour with what speaks against the
 * day beside it, the mark with the kind of day
 * and the thermals' line, the height the square was judged at, and what
 * the map is, with its credits. Every word but the height's is the core's.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThermalSquareSheet(map: ThermalMap, day: ThermalMapDay, square: ThermalMapCell, onDismiss: () -> Unit) {
    val colours = Venti.colours
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false),
        containerColor = MaterialTheme.colorScheme.surface,
        modifier = Modifier.semantics { paneTitle = "Forecast for this square" },
    ) {
        // Taller than half the screen, so that the sheet opens at half
        // height with the square still in view above it.
        Box(Modifier.fillMaxHeight()) {
            Column(Modifier.testTag("forecast-square")) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                ) {
                    Text(
                        day.name,
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                    TextButton(onClick = onDismiss) { Text("Done") }
                }
                // The status, and what speaks against the day beside it,
                // read by TalkBack as one: "Marginal, Gusts".
                Row(
                    modifier = Modifier
                        .padding(start = VentiSpace.s5, end = VentiSpace.s5)
                        .testTag("square-status")
                        .clearAndSetSemantics { contentDescription = ThermalMapWords.status(square) },
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                ) {
                    StatusChip(square.verdict.status, square.status)
                    if (square.why.isNotEmpty()) {
                        Text(
                            square.why,
                            style = MaterialTheme.typography.bodyMedium,
                            color = colours.textSecondary,
                            modifier = Modifier.testTag("square-why"),
                        )
                    }
                }
                Column(
                    modifier = Modifier
                        .verticalScroll(rememberScrollState())
                        .padding(start = VentiSpace.s5, end = VentiSpace.s5, top = VentiSpace.s2, bottom = VentiSpace.s6),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                ) {
                    if (square.thermalWords.isNotEmpty()) {
                        ThermalLine(square.thermalWords, square.thermalLine, ThermalMapLayer.colour(ThermalMapLayer.fill(square)))
                    }
                    square.groundM?.let { ground ->
                        Text(
                            ThermalMapWords.judgedAt(ground),
                            style = MaterialTheme.typography.bodyMedium,
                            color = colours.text,
                            modifier = Modifier.testTag("square-ground"),
                        )
                    }
                    if (!day.whole && day.note.isNotEmpty()) {
                        Text(day.note, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                    }
                    Text(
                        map.note,
                        style = MaterialTheme.typography.bodyMedium,
                        color = colours.textSecondary,
                        modifier = Modifier.testTag("square-note"),
                    )
                    Text(ThermalMapWords.run(map), style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                    for (credit in map.credits) {
                        Text(
                            "${credit.text}. ${credit.licence}.",
                            style = MaterialTheme.typography.bodySmall,
                            color = colours.textSecondary,
                        )
                    }
                }
            }
        }
    }
}
