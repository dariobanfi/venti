package fly.venti.app.fly

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiMapLight
import app.venti.design.VentiSpace
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.core.CockpitView
import kotlin.math.min

/**
 * The thermal view beside the cockpit's map, as the design language and the
 * owner's own layout have it: a close-up of the air the glider is in,
 * drawn, not a map. North is up, the glider in the middle.
 *
 * While circling it shows the core's picture of the lift: the places flown
 * through, each where its air is now, in the climb colours and larger where
 * this thermal climbs better, and the circle to fly next, dashed, with a
 * mark at its middle. On the glide it shows the track behind the glider in
 * the climb colours and the thermals left behind with their climb. In its
 * corners the wind, an arrow that points where the air moves with its
 * speed, and a scale. Under it the core's words about the centre.
 *
 * What is shown is the core's; [CloseUp] only places it.
 */
@Composable
fun ThermalView(view: CockpitView, modifier: Modifier = Modifier) {
    val colours = Venti.colours
    val measurer = rememberTextMeasurer()
    val label = MaterialTheme.typography.labelLarge.numbers.copy(color = colours.text, fontWeight = FontWeight.SemiBold)
    val bands = ClimbBand.entries.associateWith { it.ink }
    val centre = view.lift?.centre?.takeIf { view.circling }
    Column(modifier) {
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .background(colours.surface)
                .border(1.dp, colours.borderStrong)
                .testTag("thermal-view")
                .semantics {
                    contentDescription = "Thermal view"
                    stateDescription = when {
                        view.circling && view.lift != null -> "The lift of the circles flown"
                        else -> "The track behind the glider"
                    }
                },
        ) {
            Canvas(Modifier.fillMaxSize()) {
                val position = view.position ?: return@Canvas
                val circling = view.circling && view.lift != null
                val lift = view.lift
                val places = if (circling) {
                    lift?.points.orEmpty().map { CloseUp.offset(position, it.lat, it.lon) } +
                        lift?.centre?.outline.orEmpty().map { CloseUp.offset(position, it) }
                } else {
                    emptyList()
                }
                val span = CloseUp.span(circling, places)
                val scale = min(size.width, size.height) / span.toFloat()
                val middle = Offset(size.width / 2, size.height / 2)
                fun at(offset: CloseUp.Offset) = Offset(
                    middle.x + offset.eastM.toFloat() * scale,
                    middle.y - offset.northM.toFloat() * scale,
                )

                if (circling && lift != null) {
                    // The places flown through, larger where this thermal climbs better.
                    for (point in lift.points) {
                        val radius = (3 + 5 * point.strength.toFloat()).dp.toPx()
                        drawCircle(bands.getValue(ClimbBand.of(point.climbMs)), radius, at(CloseUp.offset(position, point.lat, point.lon)))
                    }
                    lift.centre?.let { next ->
                        // The circle to fly next, and a mark at its middle.
                        val outline = Path()
                        next.outline.forEachIndexed { index, place ->
                            val spot = at(CloseUp.offset(position, place))
                            if (index == 0) outline.moveTo(spot.x, spot.y) else outline.lineTo(spot.x, spot.y)
                        }
                        drawPath(
                            outline,
                            colours.text,
                            style = Stroke(2.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(8.dp.toPx(), 6.dp.toPx()))),
                        )
                        val mark = at(CloseUp.offset(position, next.position))
                        val arm = 6.dp.toPx()
                        drawLine(colours.text, mark - Offset(arm, 0f), mark + Offset(arm, 0f), 2.dp.toPx(), StrokeCap.Round)
                        drawLine(colours.text, mark - Offset(0f, arm), mark + Offset(0f, arm), 2.dp.toPx(), StrokeCap.Round)
                    }
                } else {
                    // The track behind the glider, in the climb colours.
                    val trail = view.trail
                    for (index in 1 until trail.size) {
                        val from = trail[index - 1]
                        val to = trail[index]
                        drawLine(
                            bands.getValue(ClimbBand.of(to.climbMs)),
                            at(CloseUp.offset(position, from.lat, from.lon)),
                            at(CloseUp.offset(position, to.lat, to.lon)),
                            4.dp.toPx(),
                            StrokeCap.Round,
                        )
                    }
                    // The thermals left behind, with their climb: where to go back to.
                    for (thermal in view.thermals) {
                        val spot = at(CloseUp.offset(position, thermal.position))
                        if (spot.x !in 0f..size.width || spot.y !in 0f..size.height) continue
                        drawCircle(bands.getValue(ClimbBand.of(thermal.climbMs)), 6.dp.toPx(), spot)
                        drawCircle(colours.text, 6.dp.toPx(), spot, style = Stroke(1.5.dp.toPx()))
                        drawLabel(measurer, thermal.label, label, spot + Offset(9.dp.toPx(), -10.dp.toPx()))
                    }
                }
                ownArrow(middle, view.courseDeg)
                wind(view, measurer, label, colours.text)
                scaleBar(span, scale, measurer, label, colours.text)
            }
        }
        // The core's words about the centre, while there are any.
        if (centre != null) {
            Text(
                centre.words,
                style = MaterialTheme.typography.titleMedium.numbers,
                color = colours.text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = VentiSpace.s1).testTag("thermal-words").clearAndSetSemantics {
                    contentDescription = "Lift"
                    stateDescription = centre.words
                },
            )
        }
    }
}

/** Words on the view, clipped to it. */
private fun DrawScope.drawLabel(measurer: TextMeasurer, text: String, style: TextStyle, at: Offset) {
    val laid = measurer.measure(text, style)
    val x = at.x.coerceIn(0f, (size.width - laid.size.width).coerceAtLeast(0f))
    val y = at.y.coerceIn(0f, (size.height - laid.size.height).coerceAtLeast(0f))
    drawText(laid, topLeft = Offset(x, y))
}

/** The pilot's own glider as the map draws it: a cream arrow with a dark ring where it flies, a dot in the same pair without a course. */
private fun DrawScope.ownArrow(at: Offset, courseDeg: Double?) {
    val cream = VentiMapLight.ownPosition
    val ring = VentiMapLight.ownPositionRing
    if (courseDeg == null) {
        drawCircle(cream, 8.dp.toPx(), at)
        drawCircle(ring, 8.dp.toPx(), at, style = Stroke(2.5.dp.toPx()))
        return
    }
    val side = 34.dp.toPx()
    val arrow = Path().apply {
        moveTo(at.x, at.y - side * 0.4f)
        lineTo(at.x + side * 0.32f, at.y + side * 0.36f)
        lineTo(at.x, at.y + side * 0.18f)
        lineTo(at.x - side * 0.32f, at.y + side * 0.36f)
        close()
    }
    rotate(courseDeg.toFloat(), pivot = at) {
        drawPath(arrow, cream)
        drawPath(arrow, ring, style = Stroke(3.dp.toPx(), join = StrokeJoin.Round))
    }
}

/** The wind in the upper left corner: an arrow that points where the air moves, and its speed. */
private fun DrawScope.wind(view: CockpitView, measurer: TextMeasurer, style: TextStyle, ink: Color) {
    val wind = view.wind ?: return
    val inset = 8.dp.toPx()
    val length = 22.dp.toPx()
    val middle = Offset(inset + length / 2, inset + length / 2)
    rotate((wind.fromDeg + 180).toFloat(), pivot = middle) {
        val tip = middle - Offset(0f, length / 2)
        val tail = middle + Offset(0f, length / 2)
        drawLine(ink, tail, tip, 2.5.dp.toPx(), StrokeCap.Round)
        drawLine(ink, tip, tip + Offset(-6.dp.toPx(), 7.dp.toPx()), 2.5.dp.toPx(), StrokeCap.Round)
        drawLine(ink, tip, tip + Offset(6.dp.toPx(), 7.dp.toPx()), 2.5.dp.toPx(), StrokeCap.Round)
    }
    drawLabel(measurer, "${Format.kilometresPerHour(wind.speedMs)} km/h", style, Offset(inset * 2 + length, inset))
}

/** The scale in the lower right corner: a bar of a round length, and its length. */
private fun DrawScope.scaleBar(span: Double, scale: Float, measurer: TextMeasurer, style: TextStyle, ink: Color) {
    val length = CloseUp.scaleBar(span)
    val inset = 8.dp.toPx()
    val bar = length.toFloat() * scale
    val laid = measurer.measure(CloseUp.scaleWords(length), style)
    val y = size.height - inset
    val right = size.width - inset
    drawLine(ink, Offset(right - bar, y), Offset(right, y), 2.5.dp.toPx(), StrokeCap.Round)
    drawText(laid, topLeft = Offset(right - laid.size.width, y - laid.size.height - 4.dp.toPx()))
}
