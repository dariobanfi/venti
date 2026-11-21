package fly.venti.app.sites

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.design.PillShape
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.Venti
import fly.venti.app.forecast.MapCard
import fly.venti.core.Coordinate
import fly.venti.core.SitesFilter
import fly.venti.core.SitesFiltered
import fly.venti.core.SitesShown
import fly.venti.core.SitesWindIs
import fly.venti.core.WindStation
import fly.venti.core.sitesFilterChoices
import fly.venti.core.stationsNear

/**
 * What the filter of the sites needs beside the core's choices and words:
 * the two names the screen gives its own controls, how a filter is kept
 * between runs, and which station "Measured now" is read from.
 */
object SitesFilterWords {
    /** The button that opens the filter, and the head of its sheet. */
    const val TITLE = "Filter sites"
    const val DONE = "Done"

    /** A filter that asks nothing: every site is shown. */
    val NOTHING = SitesFilter(
        shown = SitesShown.ALL,
        windFromDeg = null,
        windIs = SitesWindIs.CHOSEN,
        takeoffsFromM = null,
    )

    /** A filter as the settings keep it: its four parts in a line, an absent one empty. */
    fun kept(filter: SitesFilter): String = listOf(
        filter.shown.name,
        filter.windFromDeg?.toString().orEmpty(),
        filter.windIs.name,
        filter.takeoffsFromM?.toString().orEmpty(),
    ).joinToString(SEPARATOR)

    /** The filter a line of [kept] stands for; one that asks nothing for a line that is none. */
    fun read(kept: String?): SitesFilter {
        val parts = kept?.split(SEPARATOR) ?: return NOTHING
        if (parts.size != 4) return NOTHING
        val shown = SitesShown.entries.firstOrNull { it.name == parts[0] } ?: return NOTHING
        val windIs = SitesWindIs.entries.firstOrNull { it.name == parts[2] } ?: return NOTHING
        return SitesFilter(
            shown = shown,
            windFromDeg = parts[1].toDoubleOrNull(),
            windIs = windIs,
            takeoffsFromM = parts[3].toDoubleOrNull(),
        )
    }

    /**
     * Where the wind comes from for "Measured now": at the station nearest
     * the middle of the map that has a direction and is not old, within the
     * distance a site's page calls near. `null` when there is none. Which
     * is nearest is the core's.
     */
    fun measuredNow(stations: List<WindStation>, middle: Coordinate): Double? {
        val current = stations.filter { !it.isOld && it.latest?.directionDeg != null }
        return stationsNear(current, middle, null, emptyList(), SiteWords.STATIONS_WITHIN_M, 1u)
            .firstOrNull()?.station?.latest?.directionDeg
    }

    private const val SEPARATOR = "|"
}

/**
 * The line on the map while a filter is on, in the manner of the forecast
 * map's card: the filter in the core's words, what it leaves, and what the
 * take-offs shown apart are. A tap opens the filter.
 */
@Composable
fun SitesFilterCard(filtered: SitesFiltered, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    MapCard("sites-filter-card", isUnfolded = false, modifier = modifier) {
        Column(
            // One element for TalkBack and for a test: the three lines, and what a tap does.
            modifier = Modifier
                .fillMaxWidth()
                .testTag("sites-filter-lines")
                .clickable(role = Role.Button, onClickLabel = SitesFilterWords.TITLE, onClick = onOpen)
                .padding(bottom = VentiSpace.s3),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
        ) {
            Leaves(filtered)
        }
    }
}

/** The filter in a line, what it leaves, and the heading of what is shown apart: the core's three lines. */
@Composable
private fun Leaves(filtered: SitesFiltered) {
    val colours = Venti.colours
    Text(
        filtered.filterWords,
        style = MaterialTheme.typography.titleSmall,
        color = colours.text,
        modifier = Modifier.testTag("sites-filter-words"),
    )
    Text(
        filtered.words,
        style = MaterialTheme.typography.bodyMedium,
        color = colours.textSecondary,
        modifier = Modifier.testTag("sites-filter-leaves"),
    )
    if (filtered.unsaidTitle.isNotEmpty()) {
        Text(
            filtered.unsaidTitle,
            style = MaterialTheme.typography.bodyMedium,
            color = colours.textMuted,
            modifier = Modifier.testTag("sites-filter-unsaid"),
        )
    }
}

/**
 * The filter of the sites, as a sheet over the map: which sites, the wind
 * a take-off is to work in and the height it is to lie at, each choice in
 * the core's words; under them what the filter leaves of the sites at
 * hand, and the core's note on what a fit says and what it does not. A
 * choice counts at once: the map behind the sheet and the list follow it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SitesFilterSheet(
    sites: SitesModel,
    /** Told the filter after every choice, to keep it for the next run. */
    onChange: (SitesFilter) -> Unit,
    onDismiss: () -> Unit,
) {
    val colours = Venti.colours
    val choices = remember { sitesFilterChoices() }
    val filter = sites.filter
    val asked = sites.asked
    val filtered = sites.filtered
    val choose = { chosen: SitesFilter ->
        sites.filter = chosen
        onChange(chosen)
    }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                .testTag("sites-filter")
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    SitesFilterWords.TITLE,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("sites-filter-done")) {
                    Text(SitesFilterWords.DONE)
                }
            }

            Title(choices.shownTitle)
            SegmentedPills(
                choices.shown.map { it.words },
                choices.shown.indexOfFirst { it.shown == filter.shown }.coerceAtLeast(0),
                Modifier.testTag("sites-filter-shown"),
                onChoose = { index -> choose(filter.copy(shown = choices.shown[index].shown)) },
            )

            // The wind and the height are asked of take-offs only: under
            // "Landings" the core asks neither, and neither is offered.
            if (choices.shown.firstOrNull { it.shown == filter.shown }?.asksTakeoffs != false) {
                Title(choices.windTitle)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState())
                        .selectableGroup()
                        .testTag("sites-filter-winds"),
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Choice(choices.anyWind, isOn = asked.windFromDeg == null, tag = "sites-wind-any") {
                        choose(filter.copy(windFromDeg = null, windIs = SitesWindIs.CHOSEN))
                    }
                    // The wind now, where there is a direction to give: a
                    // station's. The forecast's is not at hand on the map.
                    val measured = sites.measuredNowDeg
                    if (measured != null) {
                        choices.windsNow.firstOrNull { it.windIs == SitesWindIs.MEASURED_NOW }?.let { now ->
                            Choice(
                                now.words,
                                isOn = filter.windIs == SitesWindIs.MEASURED_NOW,
                                tag = "sites-wind-measured-now",
                            ) {
                                choose(filter.copy(windFromDeg = measured, windIs = SitesWindIs.MEASURED_NOW))
                            }
                        }
                    }
                    for (wind in choices.winds) {
                        Choice(
                            wind.words,
                            isOn = filter.windIs == SitesWindIs.CHOSEN && filter.windFromDeg == wind.fromDeg,
                            tag = "sites-wind-${wind.words}",
                        ) {
                            choose(filter.copy(windFromDeg = wind.fromDeg, windIs = SitesWindIs.CHOSEN))
                        }
                    }
                }

                Title(choices.heightTitle)
                SegmentedPills(
                    choices.heights.map { it.words },
                    choices.heights.indexOfFirst { it.fromM == filter.takeoffsFromM }.coerceAtLeast(0),
                    Modifier.testTag("sites-filter-heights"),
                    onChoose = { index -> choose(filter.copy(takeoffsFromM = choices.heights[index].fromM)) },
                )
            }

            Column(
                modifier = Modifier.padding(top = VentiSpace.s3),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Leaves(filtered)
            }
            Text(
                choices.note,
                style = MaterialTheme.typography.bodySmall,
                color = colours.textSecondary,
                modifier = Modifier.testTag("sites-filter-note"),
            )
        }
    }
}

/** What stands over a group of choices. */
@Composable
private fun Title(words: String) {
    Text(
        words,
        style = MaterialTheme.typography.labelLarge,
        color = Venti.colours.textSecondary,
        modifier = Modifier.padding(top = VentiSpace.s2).semantics { heading() },
    )
}

/** One choice of a row, as the pack's chip of which one is on: the forecast card's layers are drawn the same. */
@Composable
private fun Choice(words: String, isOn: Boolean, tag: String, onChoose: () -> Unit) {
    val colours = Venti.colours
    Box(
        modifier = Modifier
            .heightIn(min = VentiSize.touch)
            .testTag(tag)
            .selectable(selected = isOn, role = Role.Tab, onClick = onChoose),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            shape = PillShape,
            color = if (isOn) colours.primary else colours.surfaceRaised,
            contentColor = if (isOn) colours.onPrimary else colours.text,
        ) {
            Text(
                words,
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                modifier = Modifier.padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s2),
            )
        }
    }
}
