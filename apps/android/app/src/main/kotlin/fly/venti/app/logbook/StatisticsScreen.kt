package fly.venti.app.logbook

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.platform.testTag
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.design.Venti
import fly.venti.core.RouteKind
import fly.venti.core.RouteStatistics
import fly.venti.core.SeasonStatistics
import fly.venti.core.YearStatistics
import fly.venti.app.design.Axis
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.GroupRows
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.core.BestFlight
import fly.venti.core.MonthStatistics
import java.time.Month
import java.time.format.TextStyle
import java.util.Locale

/** What the bar chart of the months measures. */
private enum class Measure(val title: String, val unit: String) {
    Airtime("Airtime", "hours"),
    Distance("Distance", "kilometres"),
    Flights("Flights", "flights"),
    ;

    fun of(month: MonthStatistics): Double = of(month.airtimeS, month.distanceM, month.flights)

    fun of(season: SeasonStatistics): Double = of(season.airtimeS, season.distanceM, season.flights)

    private fun of(airtimeS: Long, distanceM: Double, flights: UInt): Double = when (this) {
        Airtime -> airtimeS / 3600.0
        Distance -> distanceM / 1000
        Flights -> flights.toDouble()
    }
}

/** What the statistics say in words of their own. */
object StatisticsWords {
    const val TYPICAL = "The middle one of all your flights for each figure, so that one long day does not speak for the rest."

    /** "12 flights · 320.0 km · longest 3:34 h · highest 2626 m" */
    fun of(year: YearStatistics): String = listOfNotNull(
        Format.flights(year.flights),
        year.distanceM.takeIf { it > 0 }?.let { "${Format.kilometres(it)} km" },
        "longest ${Format.hoursMinutes(year.longestS)} h",
        "highest ${Format.metres(year.highestM.toDouble())} m",
    ).joinToString(" · ")

    /** "12 flights · 3 closed · farthest 44.3 km on Sun, 31 Aug 2025" */
    fun of(route: RouteStatistics, farthest: BestFlight): String = listOfNotNull(
        Format.flights(route.flights),
        route.closed.takeIf { it > 0u }?.let { "$it closed" },
        "farthest ${Format.kilometres(farthest.value)} km on ${Format.day(farthest.startUnixS)}",
    ).joinToString(" · ")

    fun name(kind: RouteKind): String = when (kind) {
        RouteKind.FREE_FLIGHT -> "Free flight"
        RouteKind.FREE_TRIANGLE -> "Triangle"
        RouteKind.FAI_TRIANGLE -> "FAI triangle"
    }
}

/**
 * The logbook as a whole: the last twelve months, the years side by side,
 * the months of the year, a typical flight, the kinds of flight, the
 * gliders flown, and the pilot's best flights.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun StatisticsScreen(logbook: LogbookModel, onOpen: (String) -> Unit, onBack: () -> Unit) {
    // Worked out by the core from the logbook as it is now.
    val statistics = remember(logbook.view) { logbook.statistics() }
    var measure by rememberSaveable { mutableStateOf(Measure.Airtime) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Statistics") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding).testTag("statistics"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp),
        ) {
            item {
                SectionHeader("Last twelve months")
                Group {
                    row {
                        Column(Modifier.padding(16.dp)) {
                            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                                Measure.entries.forEachIndexed { index, choice ->
                                    SegmentedButton(
                                        selected = measure == choice,
                                        onClick = { measure = choice },
                                        shape = SegmentedButtonDefaults.itemShape(index, Measure.entries.size),
                                        icon = {},
                                    ) {
                                        Text(choice.title)
                                    }
                                }
                            }
                            Spacer(Modifier.height(16.dp))
                            MonthChart(
                                statistics.months.map { it.month to measure.of(it) },
                                "${measure.title} by month, in ${measure.unit}",
                                Modifier.fillMaxWidth().height(170.dp),
                            )
                        }
                    }
                }
                SectionFooter("In ${measure.unit}.")
            }

            if (statistics.years.isNotEmpty()) {
                item {
                    SectionHeader("Years")
                    Group {
                        for (year in statistics.years) {
                            row {
                                WordsRow(
                                    year.year.toString(),
                                    "${Format.hoursMinutes(year.airtimeS)} h",
                                    StatisticsWords.of(year),
                                    Modifier.testTag("statistics-year-${year.year}"),
                                )
                            }
                        }
                    }
                }
            }

            if (statistics.seasons.isNotEmpty()) {
                item {
                    SectionHeader("Through the year")
                    Group {
                        row {
                            MonthChart(
                                statistics.seasons.map { it.month to measure.of(it) },
                                "${measure.title} by month of the year, in ${measure.unit}",
                                Modifier.fillMaxWidth().height(140.dp).padding(16.dp).testTag("statistics-seasons"),
                            )
                        }
                    }
                    SectionFooter("January to December, all years together, in ${measure.unit}.")
                }
            }

            statistics.typical?.let { typical ->
                item {
                    SectionHeader("A typical flight")
                    Group {
                        row { ValueRow("Airtime", "${Format.hoursMinutes(typical.airtimeS)} h") }
                        typical.distanceM?.let { row { ValueRow("Distance", "${Format.kilometres(it)} km") } }
                        row { ValueRow("Highest", "${Format.metres(typical.highestM.toDouble())} m") }
                        row { ValueRow("Best climb", "${Format.climb(typical.bestClimbMs)} m/s") }
                    }
                    SectionFooter(StatisticsWords.TYPICAL)
                }
            }

            if (statistics.routes.any { it.farthest != null }) {
                item {
                    SectionHeader("Kinds of flight")
                    Group {
                        for (route in statistics.routes) {
                            val farthest = route.farthest ?: continue
                            row {
                                WordsRow(
                                    StatisticsWords.name(route.kind),
                                    "${Format.kilometres(route.distanceM)} km",
                                    StatisticsWords.of(route, farthest),
                                    Modifier.testTag("statistics-route"),
                                    onClick = { onOpen(farthest.flightId) },
                                )
                            }
                        }
                    }
                }
            }

            if (statistics.gliders.isNotEmpty()) {
                item {
                    SectionHeader("Gliders")
                    Group {
                        for (glider in statistics.gliders) {
                            row {
                                ValueRow(
                                    title = glider.glider ?: "Not named",
                                    value = "${Format.flights(glider.flights)} · " +
                                        "${Format.hoursMinutes(glider.airtimeS)} h",
                                )
                            }
                        }
                    }
                }
            }

            item {
                SectionHeader("Your bests")
                Group {
                    best("Longest flight", statistics.longest, onOpen) { "${Format.hoursMinutes(it.toLong())} h" }
                    best("Farthest", statistics.farthest, onOpen) { "${Format.kilometres(it)} km" }
                    best("Most points", statistics.mostPoints, onOpen) { Format.points(it) }
                    best("Highest", statistics.highest, onOpen) { "${Format.metres(it)} m" }
                    best("Best climb", statistics.bestClimb, onOpen) { "${Format.climb(it)} m/s" }
                }
            }
        }
    }
}

/** One best, leading to its flight. */
private fun GroupRows.best(
    title: String,
    best: BestFlight?,
    onOpen: (String) -> Unit,
    text: (Double) -> String,
) {
    if (best == null) return
    row {
        ValueRow(
            title = title,
            detail = Format.day(best.startUnixS),
            value = text(best.value),
            onClick = { onOpen(best.flightId) },
        )
    }
}

/**
 * A row whose second line is too long to stand beside a figure: the name
 * and the figure on one line, the words under them on as many lines as
 * they need.
 */
@Composable
private fun WordsRow(title: String, value: String, words: String, modifier: Modifier = Modifier, onClick: (() -> Unit)? = null) {
    Column(
        modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
            .semantics(mergeDescendants = true) {},
    ) {
        Row {
            Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
            Text(value, style = MaterialTheme.typography.bodyLarge.numbers, color = Venti.colours.textSecondary)
        }
        Text(words, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
    }
}

/** One bar a month, in the accent colour, with the months' initials below. */
@Composable
private fun MonthChart(months: List<Pair<UInt, Double>>, described: String, modifier: Modifier = Modifier) {
    val accent = MaterialTheme.colorScheme.primary
    val grid = MaterialTheme.colorScheme.outlineVariant
    val labels = MaterialTheme.typography.labelSmall.numbers.copy(color = MaterialTheme.colorScheme.onSurfaceVariant)
    val measurer = rememberTextMeasurer()
    val values = months.map { it.second }
    val top = Axis.ceiling(values.maxOrNull() ?: 0.0)

    Canvas(
        modifier.semantics { contentDescription = described },
    ) {
        if (months.isEmpty()) return@Canvas
        val plot = Rect(0f, 6.dp.toPx(), size.width - 40.dp.toPx(), size.height - 18.dp.toPx())
        fun y(value: Double) = plot.bottom - (value / top).toFloat() * plot.height

        for (value in Axis.ticks(0.0, top, wanted = 3)) {
            drawLine(grid, Offset(plot.left, y(value)), Offset(plot.right, y(value)), 1.dp.toPx())
            val text = measurer.measure(Format.plain(value), labels)
            drawText(text, topLeft = Offset(plot.right + 6.dp.toPx(), y(value) - text.size.height / 2f))
        }

        val slot = plot.width / months.size
        val bar = slot * 0.62f
        months.forEachIndexed { index, month ->
            val left = plot.left + slot * index + (slot - bar) / 2
            val height = plot.bottom - y(values[index])
            if (height > 0f) {
                drawRoundRect(
                    accent,
                    topLeft = Offset(left, plot.bottom - height),
                    size = Size(bar, height),
                    cornerRadius = CornerRadius(3.dp.toPx().coerceAtMost(height / 2)),
                )
            }
            val initial = Month.of(month.first.toInt()).getDisplayName(TextStyle.NARROW, Locale.getDefault())
            val text = measurer.measure(initial, labels)
            drawText(
                text,
                topLeft = Offset(left + bar / 2 - text.size.width / 2f, plot.bottom + 3.dp.toPx()),
            )
        }
    }
}
