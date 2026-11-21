package fly.venti.app.today

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import app.venti.design.VentiScaleBreaks
import app.venti.design.VentiSpace
import fly.venti.app.design.Venti
import fly.venti.core.OutlookDay

/** Where a climb stands on the design language's thermal scale. */
object ThermalScale {
    /** The step of the scale for a climb in m/s: 0 below the first break, 5 from the last. */
    fun step(climbMs: Double): Int = VentiScaleBreaks.thermal.count { climbMs >= it }

    /** The height of an hour's bar for a step, dp: it grows with the step. */
    fun barHeight(step: Int): Int = 2 + 2 * step
}

/** The scale's colour for a climb, in the theme in use. */
@Composable
fun thermalColour(climbMs: Double): Color = Venti.colours.thermal[ThermalScale.step(climbMs)]

/**
 * A round mark in the thermal scale's colour for a climb, with a hairline
 * edge: the palest step is close to the surface. Never colours the words
 * beside it.
 */
@Composable
fun ThermalMark(climbMs: Double?, modifier: Modifier = Modifier) {
    ThermalMark(if (climbMs != null) thermalColour(climbMs) else Venti.colours.surface, modifier)
}

/** The same mark in a colour given: a step of the scale as the core names it, as the forecast map draws it. */
@Composable
fun ThermalMark(fill: Color, modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(VentiSpace.s3)
            .background(fill, CircleShape)
            .border(1.dp, Venti.colours.borderStrong, CircleShape),
    )
}

/** The kind of day for thermals, and its line: "Cross-country day", "1.8 m/s to a cloud base of 2900 m, from noon to 17:00". */
@Composable
fun ThermalLine(day: OutlookDay, modifier: Modifier = Modifier) {
    if (day.thermals == null) return
    ThermalLine(day.thermalWords, day.thermalLine, thermalColourOrSurface(day.climbMs), modifier)
}

/**
 * The mark with the kind of day beside it and the thermals' line under it,
 * read by TalkBack as the kind of day and its line: on Today, on a site's
 * page, and on a square of the forecast map.
 */
@Composable
fun ThermalLine(words: String, line: String, mark: Color, modifier: Modifier = Modifier) {
    Row(
        modifier
            .testTag("thermal-line")
            .clearAndSetSemantics {
                contentDescription = words
                stateDescription = line
            },
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalAlignment = Alignment.Top,
    ) {
        ThermalMark(mark, Modifier.padding(top = VentiSpace.s1))
        Column {
            Text(words, style = MaterialTheme.typography.labelLarge, color = Venti.colours.text)
            if (line.isNotEmpty()) {
                Text(line, style = MaterialTheme.typography.labelLarge, color = Venti.colours.textSecondary)
            }
        }
    }
}

@Composable
private fun thermalColourOrSurface(climbMs: Double?): Color =
    if (climbMs != null) thermalColour(climbMs) else Venti.colours.surface
