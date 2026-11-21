package fly.venti.app.design

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import app.venti.design.VentiSize
import app.venti.design.VentiSpace

/**
 * The pack's segmented control (`.v-seg`) for two to four choices, such as
 * the days of a forecast: a pill on the raised surface, the chosen segment
 * on the primary colour. TalkBack reads each segment as a tab, and which
 * one is chosen.
 */
@Composable
fun SegmentedPills(
    names: List<String>,
    chosen: Int,
    modifier: Modifier = Modifier,
    /**
     * Each segment as wide as its word needs, in proportion, instead of an
     * equal share: for four days, where "Tomorrow" would break in a
     * quarter of a narrow card.
     */
    fitToWords: Boolean = false,
    onChoose: (Int) -> Unit,
) {
    val colours = Venti.colours
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(colours.surfaceRaised, PillShape)
            .padding(VentiSpace.s1)
            .selectableGroup(),
    ) {
        names.forEachIndexed { index, name ->
            val isChosen = index == chosen
            Box(
                modifier = Modifier
                    .weight(if (fitToWords) name.length.coerceAtLeast(MIN_LETTERS).toFloat() else 1f)
                    .heightIn(min = VentiSize.touch - VentiSpace.s2)
                    .background(if (isChosen) colours.primary else colours.surfaceRaised, PillShape)
                    .selectable(selected = isChosen, role = Role.Tab, onClick = { onChoose(index) }),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    name,
                    style = MaterialTheme.typography.titleMedium,
                    color = if (isChosen) colours.onPrimary else colours.textSecondary,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(horizontal = VentiSpace.s1),
                )
            }
        }
    }
}

/** The least share a segment fitted to its word takes, in letters: room for its padding. */
private const val MIN_LETTERS = 4
