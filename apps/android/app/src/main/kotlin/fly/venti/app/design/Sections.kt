package fly.venti.app.design

import androidx.compose.foundation.BorderStroke
import androidx.annotation.DrawableRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSpace

// The pieces every list-like screen is made of: a heading, a group of rows
// on one rounded surface, and a line of explanation under it. They are
// Material's own surfaces and type; only the arrangement is ours, so that
// the details, the statistics, the settings and the credits read alike.

/** The heading over a group of rows. */
@Composable
fun SectionHeader(title: String, modifier: Modifier = Modifier) {
    Text(
        title,
        modifier = modifier
            .fillMaxWidth()
            .padding(start = 4.dp, end = 4.dp, top = 24.dp, bottom = 8.dp)
            .semantics { heading() },
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
    )
}

/** A line of explanation under a group of rows. */
@Composable
fun SectionFooter(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        modifier = modifier.fillMaxWidth().padding(start = 4.dp, end = 4.dp, top = 8.dp),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** Collects the rows of a [Group]. */
class GroupRows {
    internal val rows = mutableListOf<@Composable () -> Unit>()

    fun row(content: @Composable () -> Unit) {
        rows += content
    }
}

/** Rows on one card of the design pack: flat, with a line round it and a hairline between rows. */
@Composable
fun Group(modifier: Modifier = Modifier, rows: GroupRows.() -> Unit) {
    val content = GroupRows().apply(rows).rows
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) {
        Column {
            content.forEachIndexed { index, row ->
                if (index > 0) {
                    HorizontalDivider(
                        modifier = Modifier.padding(start = 16.dp),
                        color = MaterialTheme.colorScheme.outlineVariant,
                    )
                }
                row()
            }
        }
    }
}

/**
 * One row: what it is on the left, its value on the right, and an optional
 * second line under each. With `onClick` the whole row can be pressed.
 */
@Composable
fun ValueRow(
    title: String,
    value: String,
    modifier: Modifier = Modifier,
    detail: String? = null,
    valueDetail: String? = null,
    onClick: (() -> Unit)? = null,
) {
    // The title keeps the room of its longest word, and the value takes
    // what is left before it does: a long value (an email address) wraps,
    // never the title letter by letter ("Go / ogl / e" at 360 dp and text
    // at 130 %).
    Layout(
        modifier = modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .heightIn(min = 52.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        content = {
            ValueRowTitle(title, detail)
            ValueRowValue(value, valueDetail)
        },
    ) { (titlePart, valuePart), constraints ->
        val gap = 12.dp.roundToPx()
        val width = constraints.maxWidth
        val titleLeast = titlePart.minIntrinsicWidth(Constraints.Infinity)
        val valued = valuePart.measure(Constraints(maxWidth = (width - gap - titleLeast).coerceAtLeast(0)))
        val titled = titlePart.measure(Constraints(maxWidth = (width - gap - valued.width).coerceAtLeast(0)))
        val height = maxOf(titled.height, valued.height, constraints.minHeight)
        layout(width, height) {
            titled.placeRelative(0, (height - titled.height) / 2)
            valued.placeRelative(width - valued.width, (height - valued.height) / 2)
        }
    }
}

@Composable
private fun ValueRowTitle(title: String, detail: String?) {
    Column {
        Text(title, style = MaterialTheme.typography.bodyLarge)
        if (detail != null) {
            Text(
                detail,
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun ValueRowValue(value: String, valueDetail: String?) {
    Column(horizontalAlignment = Alignment.End) {
        Text(
            value,
            style = MaterialTheme.typography.bodyLarge.numbers,
            color = if (valueDetail == null) {
                MaterialTheme.colorScheme.onSurfaceVariant
            } else {
                MaterialTheme.colorScheme.onSurface
            },
            textAlign = TextAlign.End,
        )
        if (valueDetail != null) {
            Text(
                valueDetail,
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.End,
            )
        }
    }
}

/**
 * A row that leads somewhere: another screen, or the browser. `icon` at its
 * end says which; `leading`, if given, is a picture of what it leads to,
 * and `value` what stands there now.
 */
@Composable
fun LinkRow(
    title: String,
    @DrawableRes icon: Int,
    modifier: Modifier = Modifier,
    @DrawableRes leading: Int? = null,
    value: String? = null,
    /** Whether it can be pressed just now; one that cannot is set in grey, as an `ActionRow` is. */
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .heightIn(min = 52.dp)
            // Room above and below a title of two lines, which large text makes of a sentence.
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null) {
            Icon(painterResource(leading), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.width(16.dp))
        }
        Text(
            title,
            style = MaterialTheme.typography.bodyLarge,
            color = if (enabled) Color.Unspecified else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.38f),
            modifier = Modifier.weight(1f),
        )
        if (value != null) {
            Text(
                value,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
            Spacer(Modifier.width(4.dp))
        }
        Icon(painterResource(icon), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/**
 * A row that unfolds what stands under it and folds it away again: the
 * arrow at its end points down while folded, up while open.
 */
@Composable
fun DisclosureRow(title: String, isOpen: Boolean, modifier: Modifier = Modifier, onChange: (Boolean) -> Unit) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .toggleable(value = isOpen, role = Role.Button, onValueChange = onChange)
            .heightIn(min = 52.dp)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Icon(
            painterResource(fly.venti.app.R.drawable.ic_arrow_drop_down),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.rotate(if (isOpen) 180f else 0f),
        )
    }
}

/**
 * A row that does something when pressed, set in a colour that says what
 * kind of thing. One that cannot be pressed just now is set in grey.
 */
@Composable
fun ActionRow(title: String, colour: Color, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .heightIn(min = 52.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            title,
            style = MaterialTheme.typography.bodyLarge,
            color = if (enabled) colour else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.38f),
        )
    }
}
