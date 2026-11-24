package fly.venti.app.map

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.ChipTick
import fly.venti.app.design.Common
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.forecast.colourOnMap
import fly.venti.core.RegionLevel
import fly.venti.core.ThermalMapLevel

/**
 * A height the forecast's wind can be shown at: above the sea, above the
 * model's ground, or neither for the ground itself.
 */
data class WindLevel(val heightM: Int? = null, val aboveGroundM: Int? = null) {
    constructor(level: ThermalMapLevel) : this(level.heightM, level.aboveGroundM)

    /** Whether this is the wind at the ground, which the take-offs' fans are coloured by. */
    val isGround: Boolean get() = heightM == null && aboveGroundM == null

    /** The end of a choice's test tag: "ground", "250agl", "2000". */
    val tag: String get() = heightM?.toString() ?: aboveGroundM?.let { "${it}agl" } ?: "ground"

    companion object {
        val GROUND = WindLevel()
    }
}

/**
 * The height of the forecast's wind: one small pill at the map's right
 * under the chips that names the height shown ("250 m AGL") and opens the
 * list of heights when tapped (the ground, the heights above it, then those
 * above the sea, by the core's names), the chosen one with the pack's tick.
 * A list always open took a third of a phone's map (docs/design.md, "The
 * wind").
 */
@Composable
fun HeightPill(levels: List<ThermalMapLevel>, chosen: WindLevel, onChoose: (WindLevel) -> Unit, modifier: Modifier = Modifier) {
    var isOpen by remember { mutableStateOf(false) }
    val shown = levels.firstOrNull { WindLevel(it) == chosen } ?: levels.first()
    val colours = Venti.colours
    Box(modifier) {
        Surface(
            onClick = { isOpen = true },
            shape = RoundedCornerShape(VentiRadius.pill),
            color = MaterialTheme.colorScheme.surface,
            shadowElevation = 2.dp,
            modifier = Modifier
                .heightIn(min = VentiSize.touch)
                .testTag("forecast-level")
                .semantics {
                    contentDescription = HEIGHT
                    stateDescription = shown.words
                },
        ) {
            Row(
                modifier = Modifier.padding(start = VentiSpace.s3, end = VentiSpace.s2),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Text(shown.name, style = MaterialTheme.typography.labelLarge.numbers, color = colours.text, maxLines = 1)
                Icon(painterResource(R.drawable.ic_arrow_drop_down), contentDescription = null, tint = colours.textSecondary)
            }
        }
        DropdownMenu(expanded = isOpen, onDismissRequest = { isOpen = false }) {
            for (level in levels) {
                val one = WindLevel(level)
                DropdownMenuItem(
                    text = { Text(level.name, style = MaterialTheme.typography.labelLarge.numbers) },
                    leadingIcon = { if (one == chosen) ChipTick() else Box(Modifier.size(VentiSpace.s4)) },
                    onClick = {
                        isOpen = false
                        onChoose(one)
                    },
                    modifier = Modifier.testTag("forecast-level-${one.tag}"),
                )
            }
        }
    }
}

/** A region of the forecast by region that the pilot tapped. */
data class ChosenRegion(
    val id: String,
    val name: String,
    val level: RegionLevel,
    /** "Not flyable: rain". */
    val words: String,
    /** "From 3 take-offs", "From the model's ground". */
    val detail: String,
)

/**
 * A tapped region: its name, a mark in its level's colour beside the core's
 * words, and where the level comes from. As tall as its three lines, the
 * region in view above it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RegionSheet(region: ChosenRegion, onDismiss: () -> Unit) {
    val colours = Venti.colours
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
        modifier = Modifier.semantics { paneTitle = region.name },
    ) {
        Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = VentiSpace.s4).testTag("forecast-region")) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    region.name,
                    style = MaterialTheme.typography.titleLarge,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("forecast-region-done")) { Text(Common.words.done) }
            }
            Column(
                modifier = Modifier.padding(horizontal = VentiSpace.s5),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Row(
                    modifier = Modifier.testTag("forecast-region-words").clearAndSetSemantics { contentDescription = region.words },
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                ) {
                    Box(
                        Modifier
                            .size(VentiSpace.s3)
                            .background(region.level.colourOnMap, RectangleShape)
                            .border(1.dp, colours.borderStrong, RectangleShape),
                    )
                    Text(region.words, style = MaterialTheme.typography.titleMedium, color = colours.text)
                }
                if (region.detail.isNotEmpty()) {
                    Text(
                        region.detail,
                        style = MaterialTheme.typography.bodyMedium,
                        color = colours.textSecondary,
                        modifier = Modifier.testTag("forecast-region-detail"),
                    )
                }
            }
        }
    }
}

/** What TalkBack calls the height's pill. */
private const val HEIGHT = "Height"
