package fly.venti.app.design

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.sp
import app.venti.design.VentiType

/**
 * One number with its unit and what it measures: the building block of the
 * logbook totals and the flight panel.
 *
 * The value is the design language's middle instrument size, with digits of
 * one width, so a value that changes while the pilot drags along the chart
 * does not jitter. The unit is smaller and grey beside it; what it measures
 * is a label below.
 */
@Composable
fun Metric(caption: String, value: String, modifier: Modifier = Modifier, unit: String = "") {
    val unitStyle = MaterialTheme.typography.labelLarge
    val unitColour = Venti.colours.textSecondary
    Column(
        modifier = modifier.clearAndSetSemantics {
            // One element for a screen reader: "Airtime, 0:29 h".
            contentDescription = caption
            stateDescription = if (unit.isEmpty()) value else "$value $unit"
        },
    ) {
        Text(
            text = buildAnnotatedString {
                append(value)
                if (unit.isNotEmpty()) {
                    withStyle(
                        SpanStyle(
                            fontSize = unitStyle.fontSize,
                            fontWeight = FontWeight.Medium,
                            color = unitColour,
                        ),
                    ) { append(" $unit") }
                }
            },
            style = VentiType.instrumentM.text,
            maxLines = 1,
            softWrap = false,
            // Three or four of these share a phone's width; a long value is
            // set smaller before it is cut off.
            autoSize = TextAutoSize.StepBased(
                minFontSize = VentiType.label.sizePx.sp,
                maxFontSize = VentiType.instrumentM.sizePx.sp,
                stepSize = 1.sp,
            ),
        )
        Text(
            text = caption,
            style = MaterialTheme.typography.labelLarge,
            color = Venti.colours.textSecondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
