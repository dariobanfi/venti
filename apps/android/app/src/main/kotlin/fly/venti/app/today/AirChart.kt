package fly.venti.app.today

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSpace
import fly.venti.app.design.Format
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.core.ForecastWind
import fly.venti.core.OutlookDay

/**
 * The air over a take-off through one day, as a picture: for every hour how
 * high the thermals go, as a column in the thermal scale's colour of the
 * climb with the climb over it; a cloud on the column where cumulus caps
 * them; the wind at the model's levels as arrows; the take-off's height as
 * a dashed line, and the model's own ground as a band at the foot.
 * Everything in it is the core's; [AirScale] places it.
 */
@Composable
fun AirChart(day: OutlookDay, takeoffM: Double?, modifier: Modifier = Modifier) {
    val colours = Venti.colours
    val measurer = rememberTextMeasurer()
    val caption = MaterialTheme.typography.bodySmall.numbers
    val scale = AirScale.of(day, takeoffM) ?: return
    Column(modifier, verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        Canvas(
            Modifier
                .fillMaxWidth()
                .height(CHART_HEIGHT)
                .testTag("air-chart")
                .semantics {
                    contentDescription = "The air through the day"
                    stateDescription = day.thermalLine.ifEmpty { "No estimate of the thermals" }
                },
        ) {
            val quiet = caption.copy(color = colours.textSecondary)
            val ink = caption.copy(color = colours.text)
            val gutter = 40.dp.toPx()
            val head = 22.dp.toPx()
            val foot = 20.dp.toPx()
            val plot = Rect(gutter, head, size.width, size.height - foot)
            if (plot.width <= 0 || plot.height <= 0 || day.hours.isEmpty()) return@Canvas
            fun y(metres: Double) = plot.top + scale.y(metres, plot.height)

            if (day.hours.any { (it.climbMs ?: 0.0) > 0 }) {
                label(measurer, "m/s", quiet, Offset(plot.left - 6.dp.toPx(), head / 2), Anchor.Right)
            }
            for (line in scale.lines) {
                drawLine(colours.border, Offset(plot.left, y(line)), Offset(plot.right, y(line)), 1.dp.toPx())
                label(measurer, Format.metres(line), quiet, Offset(plot.left - 6.dp.toPx(), y(line)), Anchor.Right)
            }
            day.groundM?.let { ground ->
                drawRect(colours.surfaceRaised, Offset(plot.left, y(ground)), Size(plot.width, plot.bottom - y(ground)))
                drawLine(colours.borderStrong, Offset(plot.left, y(ground)), Offset(plot.right, y(ground)), 1.5.dp.toPx())
            }
            val slot = plot.width / day.hours.size
            day.hours.forEachIndexed { index, hour ->
                val middle = plot.left + slot * (index + 0.5f)
                label(measurer, hour.hour, quiet, Offset(middle, plot.bottom + foot / 2), Anchor.Centre)
                val top = hour.thermalTopM
                val ground = day.groundM
                if (top != null && ground != null && top > ground) {
                    val width = slot * 0.62f
                    val climb = hour.climbMs ?: 0.0
                    drawRoundRect(
                        colours.thermal[ThermalScale.step(climb)],
                        Offset(middle - width / 2, y(top)),
                        Size(width, y(ground) - y(top)),
                        CornerRadius(3.dp.toPx()),
                    )
                    if (hour.thermalCloud) {
                        // Cumulus: a flat-bottomed cloud standing on the column.
                        val cloudTop = Offset(middle - slot * 0.42f, y(top) - 9.dp.toPx())
                        val cloudSize = Size(slot * 0.84f, 9.dp.toPx())
                        drawRoundRect(colours.surface, cloudTop, cloudSize, CornerRadius(4.5.dp.toPx()))
                        drawRoundRect(colours.textSecondary, cloudTop, cloudSize, CornerRadius(4.5.dp.toPx()), style = Stroke(1.5.dp.toPx()))
                    }
                    if (climb > 0) {
                        label(
                            measurer,
                            Format.oneDecimal(climb),
                            ink.copy(fontWeight = FontWeight.SemiBold),
                            Offset(middle, head / 2),
                            Anchor.Centre,
                        )
                    }
                }
                // The wind at 10 m above the model's ground, and at every
                // level well clear of it: the ones just above would lie on
                // top of one another.
                val lowest = hour.winds.firstOrNull()?.altitudeM
                for (level in hour.winds) {
                    if (level.altitudeM != lowest && level.altitudeM <= (lowest ?: 0.0) + 300) continue
                    if (level.altitudeM > scale.ceilingM) continue
                    windArrow(level.wind, Offset(middle, y(level.altitudeM)), wide = slot >= 36.dp.toPx(), measurer, ink, colours.text)
                }
            }
            takeoffM?.let { takeoff ->
                drawLine(
                    colours.text,
                    Offset(plot.left, y(takeoff)),
                    Offset(plot.right, y(takeoff)),
                    1.5.dp.toPx(),
                    pathEffect = PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 4.dp.toPx())),
                )
                label(measurer, "Take-off", ink, Offset(plot.right - 2.dp.toPx(), y(takeoff) - 2.dp.toPx()), Anchor.BottomRight)
            }
        }
        Text(
            AirChartWords.caption(hasTakeoff = takeoffM != null),
            style = MaterialTheme.typography.bodySmall,
            color = colours.textSecondary,
        )
    }
}

/** What the picture's marks are. */
object AirChartWords {
    fun caption(hasTakeoff: Boolean): String =
        "How high the thermals go hour by hour, in the colour of the climb to expect, with the climb in m/s over " +
            "each and a cloud where cumulus caps them. The arrows are the wind at the model's levels, in km/h. " +
            (if (hasTakeoff) "The dashed line is the take-off. " else "") +
            "The band at the foot is the model's ground, which is smoother than the terrain. Heights in metres " +
            "above sea level."
}

private val CHART_HEIGHT = VentiSpace.s8 * 4

private enum class Anchor { Right, Centre, BottomRight }

private fun DrawScope.label(measurer: TextMeasurer, text: String, style: TextStyle, at: Offset, anchor: Anchor) {
    val laid = measurer.measure(text, style)
    val w = laid.size.width.toFloat()
    val h = laid.size.height.toFloat()
    val topLeft = when (anchor) {
        Anchor.Right -> Offset(at.x - w, at.y - h / 2)
        Anchor.Centre -> Offset(at.x - w / 2, at.y - h / 2)
        Anchor.BottomRight -> Offset(at.x - w, at.y - h)
    }
    drawText(laid, topLeft = topLeft)
}

/** A wind as an arrow that points where the air goes, with its speed where there is room; a dot without a direction. */
private fun DrawScope.windArrow(wind: ForecastWind, place: Offset, wide: Boolean, measurer: TextMeasurer, style: TextStyle, ink: Color) {
    val centre = Offset(place.x - if (wide) 8.dp.toPx() else 0f, place.y)
    val from = wind.fromDeg
    if (from != null) {
        rotate((from + 180).toFloat(), pivot = centre) {
            val shape = Path().apply {
                moveTo(centre.x, centre.y - 7.dp.toPx())
                lineTo(centre.x + 4.5.dp.toPx(), centre.y + 6.dp.toPx())
                lineTo(centre.x, centre.y + 3.dp.toPx())
                lineTo(centre.x - 4.5.dp.toPx(), centre.y + 6.dp.toPx())
                close()
            }
            drawPath(shape, ink)
        }
    } else {
        drawCircle(ink, 2.5.dp.toPx(), centre)
    }
    if (wide) {
        val laid = measurer.measure(wind.label, style)
        drawText(laid, topLeft = Offset(centre.x + 8.dp.toPx(), centre.y - laid.size.height / 2f))
    }
}
