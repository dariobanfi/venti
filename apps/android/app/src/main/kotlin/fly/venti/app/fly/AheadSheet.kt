package fly.venti.app.fly

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import fly.venti.app.airspace.AirspaceLayers
import fly.venti.app.design.Axis
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.numbers
import fly.venti.core.AirspaceAhead
import fly.venti.core.AirspaceAheadSpan
import fly.venti.core.AirspacePolicy
import kotlin.math.round

/** What the side view says that is not the core's to say, and where its line ends in the picture. */
object AheadWords {
    const val TITLE = "Ahead, from the side"

    const val CAUTION =
        "From the airspace this app holds, on a straight line over the ground. The app does not have the " +
            "height of the ground here: a limit given above ground is drawn as far as it may reach. Airspace " +
            "a paraglider needs nothing for is left out. The warnings stay what they are."

    const val CAUTION_WITH_GROUND =
        "From the airspace this app holds and the map's model of the ground, on a straight line. The model " +
            "is of the bare ground, about 13 m to a point: it knows no trees, cables or buildings, and the " +
            "glide is the one drawn, not the one the air will give. Airspace a paraglider needs nothing for " +
            "is left out. The warnings stay what they are."

    const val NOTHING_YET = "Nothing to show yet: the side view needs a position, a height and a way to look."

    const val UNCERTAIN = "Its height could not be placed exactly."

    /** How far off an airspace begins on the line: "Here", "In 620 m", "In 4.2 km". */
    fun place(fromM: Double): String = when {
        fromM <= 0 -> "Here"
        fromM < 995 -> "In ${Format.metres(round(fromM / 10) * 10)} m"
        else -> "In ${Format.kilometres(fromM)} km"
    }

    /** That the glider's line runs through an airspace; nothing when it does not. */
    fun onLine(span: AirspaceAheadSpan): String? = when {
        !span.inTheWay -> null
        span.uncertain -> "Possibly on your line"
        else -> "On your line"
    }

    /** One airspace on the line for someone who cannot see its row. */
    fun spoken(span: AirspaceAheadSpan): String = listOfNotNull(
        "${span.kind}, ${span.lower} to ${span.upper}.",
        UNCERTAIN.takeIf { span.uncertain },
        place(span.fromM) + ".",
        onLine(span)?.let { "$it." },
    ).joinToString(" ")

    /**
     * Where the glider's line ends in the picture, as kilometres from the
     * glider and a height: where it meets the ground, at the far end, or
     * where it leaves the picture's foot. `null` when there is no line.
     */
    fun lineEnd(ahead: AirspaceAhead): Pair<Double, Double>? {
        if (ahead.lineEndM >= ahead.altitudeM) return null
        val reach = ahead.distanceM / 1000
        val fall = ahead.altitudeM - ahead.lineEndM
        ahead.lineMeetsGroundM?.let { meets ->
            return meets / 1000 to ahead.altitudeM - fall * meets / ahead.distanceM
        }
        if (ahead.lineEndM >= ahead.bottomM) return reach to ahead.lineEndM
        return reach * (ahead.altitudeM - ahead.bottomM) / fall to ahead.bottomM
    }
}

/**
 * The line ahead, seen from the side: the airspace the glider would fly
 * under, into or over in the next ten kilometres and, where the phone has
 * it, the ground, with the glide drawn through both.
 *
 * It is a picture to plan a glide with. The warnings are the banner's, with
 * their margins and their sound; this neither replaces them nor knows more
 * than they do. It reads the running session, so it follows the glider
 * while it is open.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AheadSheet(fly: FlySession, onDismiss: () -> Unit) {
    // Asked anew with each of the cockpit's refreshes.
    val view = fly.view
    val ahead = remember(view) { if (view == null) null else fly.airspaceAhead() }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(AheadWords.TITLE, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text("Done") }
        }
        Column(
            Modifier
                .testTag("ahead")
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
        ) {
            if (ahead == null) {
                Text(
                    AheadWords.NOTHING_YET,
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 16.dp),
                )
            } else {
                Column(Modifier.padding(start = 4.dp, end = 4.dp, top = 8.dp)) {
                    Text(
                        ahead.words,
                        style = MaterialTheme.typography.titleMedium,
                        modifier = Modifier.testTag("ahead-words"),
                    )
                    ahead.groundWords?.let { ground ->
                        Text(
                            ground,
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Medium,
                            modifier = Modifier.testTag("ahead-ground").padding(top = 2.dp),
                        )
                    }
                    Text(
                        "${ahead.toward} · ${ahead.lineWords}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 2.dp),
                    )
                }
                AheadChart(ahead, Modifier.fillMaxWidth().padding(top = 12.dp).height(240.dp))
                SectionFooter(if (ahead.ground.isEmpty()) AheadWords.CAUTION else AheadWords.CAUTION_WITH_GROUND)
                if (ahead.spans.isNotEmpty()) {
                    SectionHeader("On this line")
                    Group {
                        for (span in ahead.spans) {
                            row { AheadRow(span) }
                        }
                    }
                }
            }
        }
    }
}

/** An airspace's colour by how the app treats it: the map's. */
private fun colour(policy: AirspacePolicy) = Color(0xFF000000 or AirspaceLayers.rgb(policy).toLong())

/** The ground: the colour of earth, quiet under the airspace's. */
private val GROUND = Color(0xFF857057).copy(alpha = 0.55f)

/** Room for the heights at the picture's trailing edge, and for the distances under it. */
private val HEIGHT_LABELS = 56.dp
private val DISTANCE_LABELS = 18.dp

/**
 * Height against distance: the ground a filled shape along the foot, each
 * airspace a block, the glider a dot at the left, its line running from it.
 */
@Composable
private fun AheadChart(ahead: AirspaceAhead, modifier: Modifier = Modifier) {
    val grid = MaterialTheme.colorScheme.outlineVariant
    val ink = MaterialTheme.colorScheme.onSurface
    val accent = MaterialTheme.colorScheme.primary
    val ring = MaterialTheme.colorScheme.surface
    val labels = MaterialTheme.typography.labelSmall.numbers.copy(color = MaterialTheme.colorScheme.onSurfaceVariant)
    val names = MaterialTheme.typography.labelSmall.copy(color = ink, fontWeight = FontWeight.SemiBold)
    val measurer = rememberTextMeasurer()

    Canvas(
        modifier
            .testTag("ahead-chart")
            .semantics {
                contentDescription = "The line ahead, from the side"
                stateDescription = listOfNotNull(ahead.words, ahead.groundWords).joinToString(". ")
            },
    ) {
        // The glider's dot stands on the leading edge: room for it.
        val plot = Rect(8.dp.toPx(), 6.dp.toPx(), size.width - HEIGHT_LABELS.toPx(), size.height - DISTANCE_LABELS.toPx())
        val reach = ahead.distanceM.coerceAtLeast(1.0)
        val heights = (ahead.topM - ahead.bottomM).coerceAtLeast(1.0)
        fun x(metres: Double) = plot.left + (metres / reach).coerceIn(0.0, 1.0).toFloat() * plot.width
        fun y(height: Double) =
            plot.bottom - ((height.coerceIn(ahead.bottomM, ahead.topM) - ahead.bottomM) / heights).toFloat() * plot.height

        // Heights: a line across and the figure at its end.
        for (height in Axis.ticks(ahead.bottomM, ahead.topM, wanted = 4)) {
            drawLine(grid, Offset(plot.left, y(height)), Offset(plot.right, y(height)), 1.dp.toPx())
            val text = measurer.measure("${Format.metres(height)} m", labels)
            drawText(text, topLeft = Offset(plot.right + 6.dp.toPx(), y(height) - text.size.height / 2f))
        }
        // Distances from the glider, along the foot.
        for (kilometres in Axis.ticks(0.0, reach / 1000, wanted = 5)) {
            val at = x(kilometres * 1000)
            drawLine(grid, Offset(at, plot.top), Offset(at, plot.bottom), 1.dp.toPx())
            val whole = kilometres % 1.0 == 0.0
            val figure = if (whole) kilometres.toInt().toString() else Format.kilometres(kilometres * 1000)
            val text = measurer.measure("$figure km", labels)
            val left = (at - text.size.width / 2f).coerceIn(0f, plot.right - text.size.width)
            drawText(text, topLeft = Offset(left, plot.bottom + 3.dp.toPx()))
        }

        // The ground, where the phone has it.
        if (ahead.ground.size >= 2) {
            val shape = Path().apply {
                moveTo(x(ahead.ground.first().distanceM), plot.bottom)
                for (place in ahead.ground) {
                    lineTo(x(place.distanceM), y(place.elevationM))
                }
                lineTo(x(ahead.ground.last().distanceM), plot.bottom)
                close()
            }
            drawPath(shape, GROUND)
        }

        // Each airspace from its floor to its ceiling, with its name. What
        // could not be placed exactly is drawn fainter.
        for (span in ahead.spans) {
            val left = x(span.fromM)
            val right = x(span.toM)
            val top = y(span.ceilingM ?: ahead.topM)
            val bottom = y(span.floorM)
            if (right <= left || bottom <= top) continue
            drawRect(
                colour(span.policy).copy(alpha = if (span.uncertain) 0.2f else 0.36f),
                topLeft = Offset(left, top),
                size = Size(right - left, bottom - top),
            )
            val room = (right - left - 8.dp.toPx()).toInt()
            if (room >= 24.dp.toPx()) {
                val name = measurer.measure(
                    span.name,
                    names,
                    overflow = TextOverflow.Ellipsis,
                    maxLines = 1,
                    constraints = Constraints(maxWidth = room),
                )
                if (name.size.height + 3.dp.toPx() <= bottom - top) {
                    drawText(name, topLeft = Offset(left + (right - left - name.size.width) / 2f, top + 3.dp.toPx()))
                }
            }
        }

        // The glider's line, dashed, and the glider.
        val glider = Offset(x(0.0), y(ahead.altitudeM))
        AheadWords.lineEnd(ahead)?.let { (kilometres, height) ->
            drawLine(
                ink,
                glider,
                Offset(x(kilometres * 1000), y(height)),
                strokeWidth = 2.dp.toPx(),
                cap = StrokeCap.Round,
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx())),
            )
        }
        drawCircle(ring, 7.dp.toPx(), glider)
        drawCircle(accent, 5.5.dp.toPx(), glider)
    }
}

/** One airspace on the line: what it is, how far, and whether the line runs through it. */
@Composable
private fun AheadRow(span: AirspaceAheadSpan) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("ahead-row")
            .heightIn(min = 60.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            // One element for a screen reader: the name, then the rest.
            .clearAndSetSemantics {
                contentDescription = span.name
                stateDescription = AheadWords.spoken(span)
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(span.name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
            Text("${span.kind} · ${span.lower} to ${span.upper}", style = MaterialTheme.typography.bodyMedium, color = quiet)
            if (span.uncertain) {
                Text(AheadWords.UNCERTAIN, style = MaterialTheme.typography.bodyMedium, color = quiet)
            }
        }
        Spacer(Modifier.width(8.dp))
        Column(horizontalAlignment = Alignment.End) {
            Text(
                AheadWords.place(span.fromM),
                style = MaterialTheme.typography.bodyLarge.numbers,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
            AheadWords.onLine(span)?.let { words ->
                Text(
                    words,
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = FontWeight.SemiBold,
                    color = colour(AirspacePolicy.AVOID),
                    maxLines = 1,
                )
            }
        }
    }
}
