package fly.venti.app.community

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.ValueRow
import fly.venti.app.sites.MonthBars
import fly.venti.core.FlownSiteLine
import fly.venti.core.PilotStatistics
import fly.venti.core.PilotTotalsView

/**
 * What a pilot's public flights add up to, over the list of them: the
 * choice between all years and each season, the numbers as rows, the
 * take-offs flown from most, the twelve months as the bars a site's flights
 * have, and under them what the numbers are made of and the terms the
 * flights are given under. Every word and every number is the core's; a
 * row that is one flight's opens that flight.
 */
@Composable
fun PilotTotalsSection(
    statistics: PilotStatistics,
    /** The season shown, by its year; 0 for all years. */
    year: Int,
    onYear: (Int) -> Unit,
    /** Opens a flight by its address on the server. */
    onOpenFlight: (String) -> Unit,
    /** Opens a take-off's page; `null` where there is none to open. */
    onOpenSite: ((FlownSiteLine) -> Unit)?,
    modifier: Modifier = Modifier,
) {
    val choices: List<PilotTotalsView> = listOf(statistics.overall) + statistics.seasons
    val shown = choices.firstOrNull { it.year == year } ?: statistics.overall
    Column(modifier.testTag("pilot-totals"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
        if (statistics.seasons.isNotEmpty()) {
            Row(
                modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).testTag("pilot-seasons"),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                for (choice in choices) {
                    FilterChip(
                        selected = choice.year == shown.year,
                        onClick = { onYear(choice.year) },
                        label = { Text(choice.name, maxLines = 1) },
                    )
                }
            }
        }
        Group {
            for (total in shown.rows) {
                row {
                    if (total.flightId.isEmpty()) {
                        ValueRow(total.label, total.value, Modifier.testTag("pilot-total"))
                    } else {
                        LinkRow(
                            total.label,
                            R.drawable.ic_chevron_right,
                            modifier = Modifier.testTag("pilot-total-flight"),
                            value = total.value,
                            onClick = { onOpenFlight(total.flightId) },
                        )
                    }
                }
            }
        }
        if (shown.sites.isNotEmpty()) {
            Group {
                for (site in shown.sites) {
                    row {
                        if (onOpenSite == null) {
                            ValueRow(site.words, "", Modifier.testTag("pilot-site"))
                        } else {
                            LinkRow(
                                site.words,
                                R.drawable.ic_chevron_right,
                                modifier = Modifier.testTag("pilot-site"),
                                leading = R.drawable.venti_takeoff,
                                onClick = { onOpenSite(site) },
                            )
                        }
                    }
                }
            }
        }
        if (shown.months.isNotEmpty()) {
            Group {
                row {
                    MonthBars(
                        shown.months,
                        Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3).testTag("pilot-months"),
                    )
                }
            }
        }
        Column {
            if (statistics.note.isNotEmpty()) {
                SectionFooter(statistics.note, Modifier.testTag("pilot-totals-note"))
            }
            if (statistics.terms.text.isNotEmpty()) {
                SectionFooter(statistics.terms.text, Modifier.testTag("pilot-totals-terms"))
            }
        }
    }
}
