package fly.venti.app.wind

import fly.venti.app.design.Venti
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fly.venti.app.design.Axis
import fly.venti.app.design.Format
import fly.venti.app.design.Palette
import fly.venti.app.design.numbers
import fly.venti.core.StationHistory
import kotlin.math.abs
import kotlin.math.max

/** One reading, in the units the charts draw. */
data class WindSample(
    /** Seconds since 1970. */
    val time: Long,
    /** Mean wind, km/h. */
    val wind: Double?,
    /** Gusts, km/h. */
    val gusts: Double?,
    /** Where the wind comes from, degrees. */
    val from: Double?,
) {
    companion object {
        /** A station's readings in the order they were taken. */
        fun of(history: StationHistory): List<WindSample> = history.readings
            .map { reading ->
                WindSample(
                    time = reading.timeUnixS,
                    wind = reading.speedMs?.let { it * 3.6 },
                    gusts = reading.gustMs?.let { it * 3.6 },
                    from = reading.directionDeg,
                )
            }
            .sortedBy { it.time }

        /** The sample closest to a moment. */
        fun nearest(time: Long, samples: List<WindSample>): WindSample? = samples.minByOrNull { abs(it.time - time) }

        /** The reading under the finger in words, or what the charts show. */
        fun caption(sample: WindSample?): String {
            if (sample == null) return "Wind and gusts in km/h, and where the wind came from."
            return listOfNotNull(
                Format.time(sample.time),
                sample.wind?.let { "${Format.whole(it)} km/h" },
                sample.gusts?.let { "gusts ${Format.whole(it)}" },
                sample.from?.let { "from ${Format.compassPoint(it)}" },
            ).joinToString(" · ")
        }
    }
}

/**
 * A station's last 24 hours: mean wind as a line with the span up to the
 * gusts above it, and below, in a small chart of its own, where the wind
 * came from. Not a second scale on the first chart: degrees and km/h have
 * nothing to do with each other.
 *
 * A finger on either chart reads out that moment in the line above them.
 */
@Composable
fun WindHistoryCharts(samples: List<WindSample>, modifier: Modifier = Modifier) {
    // The moment under the finger, while it is down.
    var touched by remember { mutableStateOf<Long?>(null) }
    val marked = touched?.let { WindSample.nearest(it, samples) }
    val windColour = Venti.colours.wind[5]
    val gustColour = Venti.colours.wind[2]
    val look = ChartLook(
        grid = MaterialTheme.colorScheme.outlineVariant,
        rule = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f),
        labels = MaterialTheme.typography.labelSmall.numbers.copy(color = MaterialTheme.colorScheme.onSurfaceVariant),
        measurer = rememberTextMeasurer(),
    )
    val start = samples.first().time
    val end = max(samples.last().time, start + 1)
    val touch = Modifier.pointerInput(samples) {
        val plotWidth = size.width - VALUE_LABELS.toPx()
        fun moment(x: Float) = start + ((x / plotWidth).coerceIn(0f, 1f) * (end - start)).toLong()
        awaitEachGesture {
            val down = awaitFirstDown()
            touched = moment(down.position.x)
            drag(down.id) { change ->
                touched = moment(change.position.x)
                change.consume()
            }
            touched = null
        }
    }

    Column(modifier) {
        Text(
            WindSample.caption(marked),
            style = MaterialTheme.typography.bodySmall.numbers,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(10.dp))

        val top = Axis.ceiling(samples.maxOf { max(it.gusts ?: 0.0, it.wind ?: 0.0) }, least = 10.0)
        Canvas(
            Modifier
                .fillMaxWidth()
                .height(170.dp)
                .testTag("wind-chart")
                .semantics {
                    contentDescription = "Wind and gusts over the last 24 hours, in kilometres per hour"
                }
                .then(touch),
        ) {
            val plot = plot()
            fun x(time: Long) = plot.left + (time - start).toFloat() / (end - start) * plot.width
            fun y(speed: Double) = plot.bottom - (speed / top).toFloat() * plot.height

            for (speed in Axis.ticks(0.0, top, wanted = 4)) {
                level(look, plot, y(speed), Format.whole(speed))
            }
            hours(look, plot, start, end, ::x)

            // The span from the mean wind up to the gusts.
            val known = samples.filter { it.wind != null }
            if (known.size >= 2) {
                val span = Path()
                known.forEachIndexed { index, sample ->
                    val at = Offset(x(sample.time), y(max(sample.gusts ?: 0.0, sample.wind ?: 0.0)))
                    if (index == 0) span.moveTo(at.x, at.y) else span.lineTo(at.x, at.y)
                }
                known.asReversed().forEach { sample -> span.lineTo(x(sample.time), y(sample.wind ?: 0.0)) }
                span.close()
                drawPath(span, gustColour)

                val line = Path()
                known.forEachIndexed { index, sample ->
                    val at = Offset(x(sample.time), y(sample.wind ?: 0.0))
                    if (index == 0) line.moveTo(at.x, at.y) else line.lineTo(at.x, at.y)
                }
                drawPath(
                    line,
                    windColour,
                    style = Stroke(2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round),
                )
            }
            marked?.let { drawLine(look.rule, Offset(x(it.time), plot.top), Offset(x(it.time), plot.bottom), 1.dp.toPx()) }
        }

        Spacer(Modifier.height(10.dp))
        Canvas(
            Modifier
                .fillMaxWidth()
                .height(96.dp)
                .semantics { contentDescription = "Wind direction over the last 24 hours" }
                .then(touch),
        ) {
            val plot = plot()
            fun x(time: Long) = plot.left + (time - start).toFloat() / (end - start) * plot.width
            fun y(degrees: Double) = plot.bottom - (degrees / 360).toFloat() * plot.height

            for (degrees in listOf(0.0, 90.0, 180.0, 270.0, 360.0)) {
                level(look, plot, y(degrees), Format.compassPoint(degrees))
            }
            hours(look, plot, start, end, ::x)
            for (sample in samples) {
                val from = sample.from ?: continue
                drawCircle(windColour, 2.dp.toPx(), Offset(x(sample.time), y(from)))
            }
            marked?.let { drawLine(look.rule, Offset(x(it.time), plot.top), Offset(x(it.time), plot.bottom), 1.dp.toPx()) }
        }
    }
}

/** The room kept for the figures on the trailing edge. */
private val VALUE_LABELS: Dp = 36.dp

private class ChartLook(val grid: Color, val rule: Color, val labels: TextStyle, val measurer: TextMeasurer)

/** The part of a chart the data is drawn in: clear of the figures beside and the times below. */
private fun DrawScope.plot() = Rect(0f, 6.dp.toPx(), size.width - VALUE_LABELS.toPx(), size.height - 18.dp.toPx())

/** A line across the chart with its figure at the trailing end. */
private fun DrawScope.level(look: ChartLook, plot: Rect, y: Float, label: String) {
    drawLine(look.grid, Offset(plot.left, y), Offset(plot.right, y), 1.dp.toPx())
    val text = look.measurer.measure(label, look.labels)
    drawText(text, topLeft = Offset(plot.right + 6.dp.toPx(), y - text.size.height / 2f))
}

/** Lines down the chart at round times of day, with the time below. */
private fun DrawScope.hours(look: ChartLook, plot: Rect, start: Long, end: Long, x: (Long) -> Float) {
    for (time in Axis.clockTicks(start, end, most = 4)) {
        val at = x(time)
        drawLine(look.grid, Offset(at, plot.top), Offset(at, plot.bottom), 1.dp.toPx())
        val text = look.measurer.measure(Format.timeOnDayAxis(time), look.labels)
        val left = (at - text.size.width / 2f).coerceIn(plot.left, max(plot.left, plot.right - text.size.width))
        drawText(text, topLeft = Offset(left, plot.bottom + 3.dp.toPx()))
    }
}
