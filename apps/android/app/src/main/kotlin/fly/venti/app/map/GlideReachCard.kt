package fly.venti.app.map

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.FloatElevation
import fly.venti.app.design.Venti
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.task.Stepper

/**
 * The entry in what a long press on the map opens: what a straight glide
 * reaches from the place pressed.
 */
@Composable
fun GlideReachEntry(onOpen: () -> Unit, modifier: Modifier = Modifier) {
    Group(modifier.padding(horizontal = VentiSpace.s4)) {
        row {
            LinkRow(
                GlideReachWords.ENTRY,
                R.drawable.ic_chevron_right,
                Modifier.testTag("glide-reach-here"),
                leading = R.drawable.venti_glide,
                onClick = onOpen,
            )
        }
    }
}

/**
 * Over the lower part of the map while an area is drawn: the core's
 * answer in words, the glide, the start height and the wind, the core's
 * note of what it does not know, and "Clear". Every change asks the
 * core again.
 */
@Composable
fun GlideReachCard(model: GlideReachModel, onClear: () -> Unit, modifier: Modifier = Modifier) {
    val colours = Venti.colours
    val answer = model.answer
    // At most half the screen: the area is drawn above it.
    val tallest = with(LocalDensity.current) { (LocalWindowInfo.current.containerSize.height / 2).toDp() }
    Surface(
        modifier = modifier.fillMaxWidth().testTag("glide-reach-card"),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surface,
        shadowElevation = FloatElevation,
    ) {
        Column(
            Modifier.heightIn(max = tallest).verticalScroll(rememberScrollState()).padding(VentiSpace.s4),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    GlideReachWords.TITLE,
                    style = MaterialTheme.typography.titleSmall,
                    color = colours.text,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                if (model.isAsking) {
                    CircularProgressIndicator(
                        Modifier.size(VentiSpace.s4).testTag("glide-reach-asking"),
                        strokeWidth = VentiSpace.s1 / 2,
                        color = colours.primary,
                    )
                }
                TextButton(
                    onClick = onClear,
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("glide-reach-clear"),
                ) { Text(GlideReachWords.CLEAR) }
            }
            if (answer != null) {
                Text(
                    answer.words,
                    style = MaterialTheme.typography.bodyMedium,
                    color = colours.text,
                    modifier = Modifier.testTag("glide-reach-words"),
                )
            } else if (model.isAsking) {
                Text(GlideReachWords.ASKING, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
            }
            Stepper(GlideReachWords.GLIDE, GlideReachWords.glide(model.glide)) { steps ->
                model.glide = GlideReachWords.stepGlide(model.glide, steps)
            }
            Stepper(GlideReachWords.START, GlideReachWords.start(model.startHeightM)) { steps ->
                model.startHeightM = GlideReachWords.stepStart(model.startHeightM, answer?.groundM, steps)
            }
            val wind = model.wind
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = VentiSize.touch)
                    .toggleable(
                        value = wind != null,
                        role = Role.Switch,
                        onValueChange = { on -> model.wind = if (on) GlideReachWords.FIRST_WIND else null },
                    )
                    .testTag("glide-reach-wind"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(GlideReachWords.WIND, style = MaterialTheme.typography.bodyMedium, color = colours.text, modifier = Modifier.weight(1f))
                Switch(checked = wind != null, onCheckedChange = null)
            }
            if (wind != null) {
                Stepper(GlideReachWords.WIND_FROM, GlideReachWords.windFrom(wind)) { steps ->
                    model.wind = GlideReachWords.stepWindFrom(wind, steps)
                }
                Stepper(GlideReachWords.WIND_SPEED, GlideReachWords.windSpeed(wind)) { steps ->
                    model.wind = GlideReachWords.stepWindSpeed(wind, steps)
                }
            }
            if (answer != null) {
                Text(
                    answer.note,
                    style = MaterialTheme.typography.bodySmall,
                    color = colours.textMuted,
                    modifier = Modifier.testTag("glide-reach-note"),
                )
            }
        }
    }
}
