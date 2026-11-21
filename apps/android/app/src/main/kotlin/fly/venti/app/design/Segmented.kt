package fly.venti.app.design

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.rememberTextMeasurer
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
    val style = MaterialTheme.typography.titleMedium
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    // What each word needs on one line, with its padding; and the room there is, once it is known.
    val needs = remember(names, style, density) {
        val padding = with(density) { (VentiSpace.s1 * 2).roundToPx() }
        names.map { measurer.measure(it, style, maxLines = 1, softWrap = false).size.width + padding + 1 }
    }
    var room by remember { mutableIntStateOf(0) }
    val shares = names.map { if (fitToWords) it.length.coerceAtLeast(MIN_LETTERS).toFloat() else 1f }
    // A word is never broken in two. Where the usual shares would break
    // one, at a narrow width or a large text size, each segment is as wide
    // as its word asks; where even that is more than the room, each has
    // its word's width and the row scrolls sideways.
    val asUsual = room == 0 || names.indices.all { room * shares[it] / shares.sum() >= needs[it] }
    val scrolls = !asUsual && needs.sum() > room
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(colours.surfaceRaised, PillShape)
            .padding(VentiSpace.s1)
            .onSizeChanged { room = it.width }
            .then(if (scrolls) Modifier.horizontalScroll(rememberScrollState()) else Modifier)
            .selectableGroup(),
    ) {
        names.forEachIndexed { index, name ->
            val isChosen = index == chosen
            Box(
                modifier = when {
                    scrolls -> Modifier.width(with(density) { needs[index].toDp() })
                    asUsual -> Modifier.weight(shares[index])
                    else -> Modifier.weight(needs[index].toFloat())
                }
                    .heightIn(min = VentiSize.touch - VentiSpace.s2)
                    .background(if (isChosen) colours.primary else colours.surfaceRaised, PillShape)
                    .selectable(selected = isChosen, role = Role.Tab, onClick = { onChoose(index) }),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    name,
                    style = style,
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
