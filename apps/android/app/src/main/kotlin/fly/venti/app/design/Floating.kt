package fly.venti.app.design

import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace

/**
 * How high what floats over a map stands: the pack's float shadow. Only
 * what floats over a map has one.
 */
val FloatElevation: Dp = VentiSpace.s1

/** The height the pack draws a chip at; its tap area is the touch target's. */
private val CHIP_HEIGHT = VentiSpace.s6 + VentiSpace.s1

/**
 * The pack's chip (`.v-chip`): switches a layer of the map on and off.
 * Drawn 36 high, a pill, the primary ground with its ink when on, the
 * surface with the float shadow when off, and 48 high to the finger.
 */
@Composable
fun LayerChip(
    title: String,
    @DrawableRes icon: Int,
    isOn: Boolean,
    onChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colours = Venti.colours
    Box(
        modifier = modifier
            .heightIn(min = VentiSize.touch)
            .toggleable(value = isOn, role = Role.Switch, onValueChange = onChange),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            shape = PillShape,
            color = if (isOn) colours.primary else colours.surface,
            contentColor = if (isOn) colours.onPrimary else colours.text,
            shadowElevation = FloatElevation,
        ) {
            Row(
                modifier = Modifier.height(CHIP_HEIGHT).padding(start = VentiSpace.s3, end = VentiSpace.s4),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(VentiSize.icon - VentiSpace.s1 - VentiSpace.s1 / 2))
                Text(title, style = MaterialTheme.typography.labelLarge, maxLines = 1)
            }
        }
    }
}

/**
 * The pack's round button (`.v-btn--round`): a sign alone on the surface,
 * floating over a map, 48 across. Its description is said to TalkBack.
 */
@Composable
fun RoundButton(
    @DrawableRes icon: Int,
    description: String,
    modifier: Modifier = Modifier,
    tint: Color = Venti.colours.text,
    /** For a button that switches something on: then its sign is in the link colour, and it says it is selected. */
    isOn: Boolean? = null,
    /**
     * For a button that is on while a choice behind it is: then it is
     * drawn as the selected chip is, the primary ground with its ink.
     */
    fillsWhenOn: Boolean = false,
    onClick: () -> Unit,
) {
    val filled = fillsWhenOn && isOn == true
    Surface(
        onClick = onClick,
        modifier = modifier.size(VentiSize.touch).then(
            if (isOn != null) Modifier.semantics { selected = isOn } else Modifier,
        ),
        shape = PillShape,
        color = if (filled) Venti.colours.primary else Venti.colours.surface,
        contentColor = when {
            filled -> Venti.colours.onPrimary
            isOn == true -> Venti.colours.link
            else -> tint
        },
        shadowElevation = FloatElevation,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(painterResource(icon), contentDescription = description)
        }
    }
}

/**
 * The round button that switches the maps between the drawn ground and the
 * aerial view: photographs from above, with the names and everything the
 * app draws left on them. One choice for every map of the app.
 */
@Composable
fun AerialButton(isOn: Boolean, onChange: (Boolean) -> Unit, modifier: Modifier = Modifier) {
    RoundButton(
        fly.venti.app.R.drawable.venti_layers,
        "Aerial view",
        modifier.testTag("map-aerial"),
        isOn = isOn,
        onClick = { onChange(!isOn) },
    )
}

/**
 * Who made the photographs of the aerial view, by name only, in a line at
 * a map's corner while they show; their own full words are behind the
 * map's credits button and on the credits screen.
 */
@Composable
fun PhotosCredit(
    /** The line, from the map ([MapSurface]'s `onPhotosCredit`); nothing is shown without one. */
    credit: String?,
    modifier: Modifier = Modifier,
) {
    if (credit == null) return
    Surface(
        modifier = modifier.testTag("photos-credit"),
        shape = MaterialTheme.shapes.small,
        color = Venti.colours.surface.copy(alpha = 0.85f),
    ) {
        Text(
            credit,
            style = MaterialTheme.typography.labelSmall,
            color = Venti.colours.textSecondary,
            maxLines = 2,
            modifier = Modifier.padding(horizontal = VentiSpace.s2, vertical = VentiSpace.s1),
        )
    }
}

