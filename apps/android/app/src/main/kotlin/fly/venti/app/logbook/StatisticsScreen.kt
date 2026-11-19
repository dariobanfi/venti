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

    fun of(month: MonthStatistics): Double = when (this) {
        Airtime -> month.airtimeS / 3600.0
        Distance -> month.distanceM / 1000
        Flights -> month.flights.toDouble()
    }
}

/**
 * The logbook as a whole: the last twelve months, the gliders flown, and
 * the pilot's best flights.
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
            modifier = Modifier.fillMaxSize().padding(padding),
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
                            MonthChart(statistics.months, measure, Modifier.fillMaxWidth().height(170.dp))
                        }
                    }
                }
                SectionFooter("In ${measure.unit}.")
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

/** One bar a month, in the accent colour, with the months' initials below. */
@Composable
private fun MonthChart(months: List<MonthStatistics>, measure: Measure, modifier: Modifier = Modifier) {
    val accent = MaterialTheme.colorScheme.primary
    val grid = MaterialTheme.colorScheme.outlineVariant
    val labels = MaterialTheme.typography.labelSmall.numbers.copy(color = MaterialTheme.colorScheme.onSurfaceVariant)
    val measurer = rememberTextMeasurer()
    val values = months.map(measure::of)
    val top = Axis.ceiling(values.maxOrNull() ?: 0.0)

    Canvas(
        modifier.semantics { contentDescription = "${measure.title} by month, in ${measure.unit}" },
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
            val initial = Month.of(month.month.toInt()).getDisplayName(TextStyle.NARROW, Locale.getDefault())
            val text = measurer.measure(initial, labels)
            drawText(
                text,
                topLeft = Offset(left + bar / 2 - text.size.width / 2f, plot.bottom + 3.dp.toPx()),
            )
        }
    }
}
