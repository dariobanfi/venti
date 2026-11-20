package fly.venti.app.flight

import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fly.venti.app.design.Axis
import fly.venti.app.design.Format
import fly.venti.app.design.numbers
import fly.venti.core.FlightView
import fly.venti.core.TrackPoint
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max

/**
 * The flight's altitude over time. Touching or dragging along it sets the
 * cursor, which stays where the finger left it.
 *
 * Android has no chart of its own; this one is drawn here, with altitudes
 * on the trailing edge and times of day along the bottom.
 */
@Composable
fun AltitudeChart(
    flight: FlightView,
    cursor: Int?,
    onCursor: (Int) -> Unit,
    modifier: Modifier = Modifier,
    /** A second flight laid beside this one: a thinner line of its own colour. */
    compared: ComparedFlight? = null,
) {
    val otherColour = CompareColours.onScreen
    val accent = MaterialTheme.colorScheme.primary
    val grid = MaterialTheme.colorScheme.outlineVariant
    val rule = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f)
    val ring = MaterialTheme.colorScheme.surface
    val labels = MaterialTheme.typography.labelSmall.numbers.copy(color = MaterialTheme.colorScheme.onSurfaceVariant)
    val measurer = rememberTextMeasurer()

    val range = remember(flight, compared) { AltitudeAxis.range(flight, compared?.flight) }
    val duration = max(flight.durationS, 1L).toDouble()
    val marked = cursor?.let { flight.track.getOrNull(it) }
    val setCursor by rememberUpdatedState(onCursor)

    Canvas(
        modifier
            .testTag("altitude-chart")
            .semantics {
                contentDescription = "Altitude over time"
                stateDescription =
                    "From ${Format.metres(flight.minAltitudeM)} to ${Format.metres(flight.maxAltitudeM)} metres"
            }
            .pointerInput(flight) {
                val plotWidth = size.width - ALTITUDE_LABELS.toPx()
                fun mark(x: Float) {
                    val seconds = (x / plotWidth).coerceIn(0f, 1f) * duration
                    setCursor(TrackSearch.nearestIndex(flight.track, seconds))
                }
                awaitEachGesture {
                    val down = awaitFirstDown()
                    mark(down.position.x)
                    drag(down.id) { change ->
                        mark(change.position.x)
                        change.consume()
                    }
                }
            },
    ) {
        val plot = Rect(0f, 6.dp.toPx(), size.width - ALTITUDE_LABELS.toPx(), size.height - TIME_LABELS.toPx())
        fun x(seconds: Double) = plot.left + (seconds / duration).toFloat() * plot.width
        fun y(altitude: Double) =
            plot.bottom - ((altitude - range.start) / (range.endInclusive - range.start)).toFloat() * plot.height

        // Altitudes: a line across and the figure at its end.
        for (altitude in Axis.ticks(range.start, range.endInclusive, wanted = 3)) {
            drawLine(grid, Offset(plot.left, y(altitude)), Offset(plot.right, y(altitude)), 1.dp.toPx())
            val text = measurer.measure("${Format.metres(altitude)} m", labels)
            drawText(text, topLeft = Offset(plot.right + 6.dp.toPx(), y(altitude) - text.size.height / 2f))
        }
        // Times of day, without AM or PM: the panel's heading says which.
        for (time in Axis.clockTicks(flight.startUnixS, flight.startUnixS + flight.durationS, most = 4)) {
            val at = x((time - flight.startUnixS).toDouble())
            drawLine(grid, Offset(at, plot.top), Offset(at, plot.bottom), 1.dp.toPx())
            val text = measurer.measure(Format.timeOnAxis(time), labels)
            val left = (at - text.size.width / 2f).coerceIn(plot.left, plot.right - text.size.width)
            drawText(text, topLeft = Offset(left, plot.bottom + 3.dp.toPx()))
        }

        if (flight.profile.size >= 2) {
            val line = Path()
            flight.profile.forEachIndexed { index, point ->
                val at = Offset(x(point.seconds.toDouble()), y(point.altitudeM))
                if (index == 0) line.moveTo(at.x, at.y) else line.lineTo(at.x, at.y)
            }
            val area = Path().apply {
                addPath(line)
                lineTo(x(flight.profile.last().seconds.toDouble()), plot.bottom)
                lineTo(x(flight.profile.first().seconds.toDouble()), plot.bottom)
                close()
            }
            drawPath(
                area,
                Brush.verticalGradient(
                    listOf(accent.copy(alpha = 0.32f), accent.copy(alpha = 0.04f)),
                    startY = plot.top,
                    endY = plot.bottom,
                ),
            )
            drawPath(line, accent, style = Stroke(2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
        }

        // The other flight on this flight's time, cut at the chart's edges.
        if (compared != null && compared.flight.profile.size >= 2) {
            val offset = compared.comparison.offsetS.toDouble()
            clipRect(plot.left, plot.top, plot.right, plot.bottom) {
                val line = Path()
                compared.flight.profile.forEachIndexed { index, point ->
                    val at = Offset(x(point.seconds.toDouble() + offset), y(point.altitudeM))
                    if (index == 0) line.moveTo(at.x, at.y) else line.lineTo(at.x, at.y)
                }
                drawPath(line, otherColour, style = Stroke(1.5.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
            }
            val theirs = marked?.let { compared.point(it.seconds) }
            if (marked != null && theirs != null) {
                val at = Offset(x(marked.seconds.toDouble()), y(theirs.altitudeM))
                drawCircle(ring, 6.dp.toPx(), at)
                drawCircle(otherColour, 4.dp.toPx(), at)
            }
        }

        if (marked != null) {
            val at = Offset(x(marked.seconds.toDouble()), y(marked.altitudeM))
            drawLine(rule, Offset(at.x, plot.top), Offset(at.x, plot.bottom), 1.dp.toPx())
            drawCircle(ring, 7.dp.toPx(), at)
            drawCircle(accent, 5.dp.toPx(), at)
        }
    }
}

/** The room kept for the altitude figures on the trailing edge. */
private val ALTITUDE_LABELS: Dp = 52.dp

/** The room kept for the times along the bottom. */
private val TIME_LABELS: Dp = 18.dp

object AltitudeAxis {
    /**
     * The vertical extent of the chart: the flight's altitudes widened to
     * whole hundreds of metres, and never flat.
     */
    fun range(flight: FlightView, other: FlightView? = null): ClosedFloatingPointRange<Double> {
        val low = floor(minOf(flight.minAltitudeM, other?.minAltitudeM ?: flight.minAltitudeM) / 100) * 100
        val high = ceil(maxOf(flight.maxAltitudeM, other?.maxAltitudeM ?: flight.maxAltitudeM) / 100) * 100
        return low..max(high, low + 100)
    }
}

object TrackSearch {
    /** The position in the track closest to a moment. Binary search: a track is in time order. */
    fun nearestIndex(track: List<TrackPoint>, seconds: Double): Int {
        if (track.isEmpty()) return 0
        var low = 0
        var high = track.lastIndex
        while (low < high) {
            val middle = (low + high) / 2
            if (track[middle].seconds.toDouble() < seconds) {
                low = middle + 1
            } else {
                high = middle
            }
        }
        if (low > 0 && seconds - track[low - 1].seconds.toDouble() < track[low].seconds.toDouble() - seconds) {
            return low - 1
        }
        return low
    }
}
