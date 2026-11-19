package fly.venti.app.logbook

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import fly.venti.core.Coordinate
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min

/**
 * A small drawing of a flight's shape, so flights can be told apart in a
 * list at a glance.
 */
@Composable
fun TrackThumbnail(outline: List<Coordinate>, modifier: Modifier = Modifier) {
    val line = MaterialTheme.colorScheme.primary
    Canvas(
        modifier
            .size(52.dp)
            .background(MaterialTheme.colorScheme.surfaceContainerHighest, RoundedCornerShape(12.dp))
            // Decoration: the row says in words what the flight was.
            .clearAndSetSemantics {},
    ) {
        val inset = 8.dp.toPx()
        val area = Rect(Offset(inset, inset), Size(size.width - 2 * inset, size.height - 2 * inset))
        val points = TrackShape.points(outline, area) ?: return@Canvas
        val path = Path().apply {
            moveTo(points[0].x, points[0].y)
            points.drop(1).forEach { lineTo(it.x, it.y) }
        }
        drawPath(path, line, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
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
