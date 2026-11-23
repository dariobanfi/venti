package fly.venti.app.design

import androidx.compose.foundation.layout.size
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import app.venti.design.VentiSpace
import fly.venti.app.R

/**
 * The tick an "on" chip carries before its word: the pack's `check`, in
 * every theme, so that on and off never rest on colour alone
 * (design/DESIGN.md, "Chip"; the owner, 2026-11-23). It says nothing to
 * TalkBack: the chip's own state does.
 */
@Composable
fun ChipTick(modifier: Modifier = Modifier) {
    Icon(painterResource(R.drawable.venti_check), contentDescription = null, modifier = modifier.size(VentiSpace.s4).testTag("chip-tick"))
}

/**
 * A chip of a page that is on or off, Material's filter chip with the
 * pack's tick before its word while it is on.
 */
@Composable
fun OnChip(
    selected: Boolean,
    onClick: () -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    FilterChip(
        selected = selected,
        onClick = onClick,
        label = { Text(label, maxLines = 1) },
        leadingIcon = if (selected) {
            { ChipTick() }
        } else {
            null
        },
        enabled = enabled,
        modifier = modifier,
    )
}
