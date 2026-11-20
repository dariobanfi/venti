package fly.venti.app.design

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.core.Flyability

/** The three kinds of status the design language has, each its icon, its word's colour and its ground. */
enum class StatusKind { Good, Caution, Danger }

/**
 * The pack's status (`.v-status`): icon, word and colour together, never
 * the colour alone, so that it reads without colour too.
 */
@Composable
fun StatusChip(kind: StatusKind, word: String, modifier: Modifier = Modifier) {
    val colours = Venti.colours
    val (ink, ground, sign) = when (kind) {
        StatusKind.Good -> Triple(colours.good, colours.goodSoft, R.drawable.venti_good)
        StatusKind.Caution -> Triple(colours.caution, colours.cautionSoft, R.drawable.venti_caution)
        StatusKind.Danger -> Triple(colours.danger, colours.dangerSoft, R.drawable.venti_danger)
    }
    Row(
        modifier = modifier
            .clearAndSetSemantics { contentDescription = word }
            .background(ground, RoundedCornerShape(VentiRadius.s))
            .heightIn(min = VentiSize.icon + VentiSpace.s1)
            .padding(start = VentiSpace.s2, end = VentiSpace.s3),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(sign), contentDescription = null, tint = ink, modifier = Modifier.size(VentiSize.icon - VentiSpace.s1))
        Text(word, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold, color = ink, maxLines = 1)
    }
}

/** The status for what a forecast says about flying. */
val Flyability.status: StatusKind
    get() = when (this) {
        Flyability.FLYABLE -> StatusKind.Good
        Flyability.MARGINAL -> StatusKind.Caution
        Flyability.NOT_FLYABLE -> StatusKind.Danger
    }
