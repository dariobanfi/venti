package fly.venti.app.fly

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.core.AirspaceLevel
import fly.venti.core.AirspacePolicy
import fly.venti.core.AirspaceProximity
import fly.venti.core.LiveScore
import kotlin.math.abs
import kotlin.math.max

/**
 * The simple height line beside the large airspace reading: one vertical
 * scale with the glider's altitude and the airspace's floor and ceiling on
 * it. The heights are the core's (`altitudeM`, `floorM`, `ceilingM`); this
 * only decides what part of the scale is drawn and where each lies on it.
 *
 * The scale is centred on the glider and reaches far enough to show the
 * limits that matter, within bounds: a limit further than [MOST_M] from the
 * glider, a floor at the ground of a session without terrain (the core puts
 * it below sea level), and a ceiling the airspace does not have run off the
 * scale's end.
 */
object AirspaceHeightLine {
    /** Where on the scale things lie: fractions from its foot (0) to its head (1). */
    data class Scale(
        /** The heights at the scale's foot and head, metres above mean sea level. */
        val lowM: Double,
        val highM: Double,
        /** The glider. */
        val glider: Float,
        /** The airspace's floor, clamped to the scale. */
        val floor: Float,
        /** Its ceiling, clamped to the scale. */
        val ceiling: Float,
        /** Whether the floor lies below the scale's foot. */
        val floorRunsOff: Boolean,
        /** Whether the ceiling lies above the scale's head, or there is none. */
        val ceilingRunsOff: Boolean,
    )

    /** The least half-height of the scale: a limit 50 m away is not drawn on the glider. */
    const val LEAST_M = 500.0

    /** The most: a limit further off than this runs off the scale. */
    const val MOST_M = 3000.0

    /** Room beyond the furthest limit shown, so that it does not sit on the scale's end. */
    private const val ROOM = 1.25

    fun of(altitudeM: Double, floorM: Double, ceilingM: Double?): Scale {
        // A floor below sea level is the ground of a session without
        // terrain, which the core puts far below: it does not set the reach.
        val limits = listOfNotNull(floorM.takeIf { it >= 0 }, ceilingM).map { abs(it - altitudeM) }.filter { it <= MOST_M }
        val reach = (limits.maxOrNull()?.times(ROOM) ?: LEAST_M).coerceIn(LEAST_M, MOST_M)
        val low = altitudeM - reach
        val high = altitudeM + reach
        fun at(height: Double) = ((height - low) / (high - low)).coerceIn(0.0, 1.0).toFloat()
        return Scale(
            lowM = low,
            highM = high,
            glider = at(altitudeM),
            floor = at(floorM),
            ceiling = if (ceilingM == null) 1f else at(max(ceilingM, floorM)),
            floorRunsOff = floorM < low,
            ceilingRunsOff = ceilingM == null || ceilingM > high,
        )
    }
}

/** The ground and the ink of a reading: plain, amber or red, as the core says (`level`, `policy`, `silenced`). */
@Composable
private fun proximityColours(reading: AirspaceProximity): Pair<Color, Color> {
    val plain = MaterialTheme.colorScheme.surfaceContainerLow
    // One the pilot has quieted is drawn as the quieted band is.
    if (reading.silenced) return plain to MaterialTheme.colorScheme.onSurfaceVariant
    return when (reading.level) {
        AirspaceLevel.INSIDE, AirspaceLevel.ENTERING -> Venti.colours.dangerFill to Venti.colours.onDangerFill
        AirspaceLevel.VERY_NEAR -> Venti.colours.cautionSoft to Venti.colours.caution
        AirspaceLevel.NEAR -> plain to MaterialTheme.colorScheme.onSurface
        // No warning: plain, and an airspace that only deserves attention in amber.
        null -> plain to if (reading.policy == AirspacePolicy.CAUTION) Venti.colours.caution else MaterialTheme.colorScheme.onSurface
    }
}

/** The airspace sign, or the quieted one. */
private fun sign(reading: AirspaceProximity): Int =
    if (reading.silenced) R.drawable.ic_notifications_off else R.drawable.venti_airspace

/**
 * The airspace that matters next, small, over a corner of the first page's
 * map: where and how high, in the core's words. The name is left to the
 * map and to the large reading. Nothing in it is tapped.
 */
@Composable
internal fun AirspaceProximityBadge(reading: AirspaceProximity, modifier: Modifier = Modifier) {
    val (ground, ink) = proximityColours(reading)
    // In the airspace the short words are the place alone.
    val lines = if (reading.shortWords == reading.placeWords) listOf(reading.placeWords) else listOf(reading.placeWords, reading.heightWords)
    Badge(
        modifier
            .testTag("airspace-proximity")
            .clearAndSetSemantics {
                contentDescription = "Airspace next"
                stateDescription = listOfNotNull(reading.words, reading.uncertain).joinToString(". ")
            },
        ground = ground,
        ink = ink,
    ) {
        Icon(painterResource(sign(reading)), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
        Spacer(Modifier.width(VentiSpace.s2))
        Column {
            for (line in lines) {
                Text(
                    line,
                    style = fixed(VentiType.label).copy(fontWeight = FontWeight.SemiBold).numbers,
                    color = Color.Unspecified,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/**
 * The airspace that matters next, large, on the map's page: its name,
 * where and how high as two readings beside each other, what is in doubt,
 * the airspace as published, and the height line. A tap lists the
 * warnings, as a tap on the band does and as on the iPhone.
 */
@Composable
internal fun AirspaceProximityStrip(reading: AirspaceProximity, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val (ground, ink) = proximityColours(reading)
    Surface(
        modifier = modifier
            .fillMaxWidth()
            .testTag("airspace-proximity-map")
            .clickable(role = Role.Button, onClick = onOpen)
            .clearAndSetSemantics {
                contentDescription = "Airspace next"
                stateDescription = listOfNotNull(reading.words, reading.detail, reading.uncertain).joinToString(". ")
                role = Role.Button
                // The band's words for the same.
                onClick(label = "Show all warnings") {
                    onOpen()
                    true
                }
            },
        color = ground,
        contentColor = ink,
    ) {
        Row(Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s2)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(painterResource(sign(reading)), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
                    Spacer(Modifier.width(VentiSpace.s2))
                    Text(
                        reading.name,
                        style = fixed(VentiType.heading),
                        color = Color.Unspecified,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s4)) {
                    for (words in listOf(reading.placeWords, reading.heightWords)) {
                        Text(
                            words,
                            modifier = Modifier.weight(1f),
                            style = fixed(VentiType.instrumentM).numbers,
                            color = Color.Unspecified,
                            maxLines = 2,
                        )
                    }
                }
                reading.uncertain?.let { doubt ->
                    Text(doubt, style = fixed(VentiType.label), color = Color.Unspecified, maxLines = 3, overflow = TextOverflow.Ellipsis)
                }
                Text(
                    reading.detail,
                    style = fixed(VentiType.caption),
                    color = Color.Unspecified,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.width(VentiSpace.s3))
            HeightLine(reading, ink, Modifier.width(VentiSpace.s6).height(HEIGHT_LINE))
        }
    }
}

/** How tall the height line is. */
private val HEIGHT_LINE = VentiSpace.s8 + VentiSpace.s5

/**
 * The glider against the airspace's floor and ceiling on one scale: a thin
 * rail, the airspace as a bar in the reading's ink (with an open end where
 * a limit runs off the scale), and the glider as a mark across the rail.
 */
@Composable
private fun HeightLine(reading: AirspaceProximity, ink: Color, modifier: Modifier = Modifier) {
    val scale = AirspaceHeightLine.of(reading.altitudeM, reading.floorM, reading.ceilingM)
    val rail = Venti.colours.textMuted
    val glider = MaterialTheme.colorScheme.onSurface
    Canvas(modifier.fillMaxHeight().clearAndSetSemantics {}) {
        fun y(fraction: Float) = size.height * (1 - fraction)
        val middle = size.width / 2
        val thin = VentiSpace.s1.toPx() / 2
        drawLine(rail, Offset(middle, 0f), Offset(middle, size.height), thin)
        // The airspace's band: solid where its limits are on the scale,
        // dashed where one runs off the end.
        val bar = VentiSpace.s3.toPx()
        val top = y(scale.ceiling)
        val bottom = y(scale.floor)
        if (bottom - top > 0) {
            drawRect(ink, Offset(middle - bar / 2, top), Size(bar, bottom - top))
        }
        val dash = PathEffect.dashPathEffect(floatArrayOf(VentiSpace.s1.toPx(), VentiSpace.s1.toPx()))
        if (scale.ceilingRunsOff) {
            drawLine(ink, Offset(middle, 0f), Offset(middle, top), bar, pathEffect = dash)
        }
        if (scale.floorRunsOff) {
            drawLine(ink, Offset(middle, bottom), Offset(middle, size.height), bar, pathEffect = dash)
        }
        // The glider: a mark across the rail, wider than the band.
        val at = y(scale.glider)
        drawLine(glider, Offset(0f, at), Offset(size.width, at), VentiSpace.s1.toPx())
    }
}

/**
 * The cross-country distance so far beside the map, and for a triangle
 * not closed how far it is to close: the flight page's words.
 */
@Composable
internal fun ScoreBadge(score: LiveScore, modifier: Modifier = Modifier) {
    val words = "${Format.kilometres(score.distanceM)} km · ${FlightPageWords.kind(score)}"
    Badge(
        modifier
            .widthIn(max = SCORE_BADGE_WIDTH)
            .testTag("score-badge")
            .clearAndSetSemantics {
                contentDescription = "Cross-country distance"
                stateDescription = words
            },
    ) {
        Text(
            words,
            style = fixed(VentiType.label).copy(fontWeight = FontWeight.SemiBold).numbers,
            color = Color.Unspecified,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * Clear of the map page's three round buttons at the lower left, on a
 * phone 411 points wide: they take 216 of them.
 */
private val SCORE_BADGE_WIDTH = VentiSpace.s8 * 2 + VentiSpace.s6
