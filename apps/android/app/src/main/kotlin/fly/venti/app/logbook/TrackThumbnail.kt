package fly.venti.app.logbook

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSpace
import fly.venti.app.design.Venti
import fly.venti.core.Coordinate
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min

/**
 * A small drawing of a flight's shape, so flights can be told apart in a
 * list at a glance. It is decoration: the row beside it says which flight
 * it is.
 *
 * `large` is the pilot's own logbook's: a little map of its own, a quiet
 * grid of the theme's border under the track, north up, with the take-off
 * as a hollow dot and the landing as a solid one, as the flight's map has
 * them. The iPhone's and the web's logbooks draw the same.
 */
@Composable
fun TrackThumbnail(outline: List<Coordinate>, modifier: Modifier = Modifier, large: Boolean = false) {
    val colours = Venti.colours
    val shape = MaterialTheme.shapes.let { if (large) it.medium else it.small }
    val frame = if (large) {
        modifier
            .size(VentiSpace.s8)
            .background(colours.bg, shape)
            .border(1.dp, colours.border, shape)
    } else {
        modifier
            .size(52.dp)
            .background(MaterialTheme.colorScheme.surfaceContainerHighest, shape)
    }
    // Decoration: the row says in words what the flight was.
    Canvas(frame.clearAndSetSemantics {}) {
        // The drawing's proportions are the web's: a square of 52 units,
        // the track kept 7 (large) or 8 units from the edge.
        val unit = size.width / 52
        if (large) {
            grid(colours.border)
        }
        val inset = if (large) unit * 7 else 8.dp.toPx()
        val area = Rect(Offset(inset, inset), Size(size.width - 2 * inset, size.height - 2 * inset))
        val points = TrackShape.points(outline, area) ?: return@Canvas
        val path = Path().apply {
            moveTo(points[0].x, points[0].y)
            points.drop(1).forEach { lineTo(it.x, it.y) }
        }
        val width = if (large) unit * 1.75f else 2.dp.toPx()
        drawPath(path, colours.primary, style = Stroke(width = width, cap = StrokeCap.Round, join = StrokeJoin.Round))
        if (large) {
            val radius = unit * 2.25f
            drawCircle(colours.surface, radius, points.first())
            drawCircle(colours.text, radius, points.first(), style = Stroke(width = unit))
            drawCircle(colours.text, radius, points.last())
        }
    }
}

/** Three lines each way, a quarter apart. */
private fun DrawScope.grid(colour: Color) {
    val width = size.width / 104
    for (quarter in 1..3) {
        val x = size.width * quarter / 4
        val y = size.height * quarter / 4
        drawLine(colour, Offset(x, 0f), Offset(x, size.height), width)
        drawLine(colour, Offset(0f, y), Offset(size.width, y), width)
    }
}

object TrackShape {
    /**
     * The outline scaled to fit `area` without distortion, north up.
     * Returns `null` for an outline with fewer than two points.
     */
    fun points(outline: List<Coordinate>, area: Rect): List<Offset>? {
        if (outline.size < 2) return null

        // Degrees of longitude are narrower than degrees of latitude by the
        // cosine of the latitude. Over one flight a single factor is enough.
        val narrowing = cos(Math.toRadians(outline[0].lat))
        val xs = outline.map { it.lon * narrowing }
        val ys = outline.map { it.lat }
        val west = xs.min()
        val east = xs.max()
        val south = ys.min()
        val north = ys.max()

        val scale = min(area.width / max(east - west, 1e-9), area.height / max(north - south, 1e-9))
        val centreX = area.center.x - (west + east) / 2 * scale
        val centreY = area.center.y + (south + north) / 2 * scale
        return outline.indices.map { index ->
            Offset((centreX + xs[index] * scale).toFloat(), (centreY - ys[index] * scale).toFloat())
        }
    }
}
