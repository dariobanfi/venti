package fly.venti.app.map

import androidx.annotation.DrawableRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import fly.venti.app.forecast.ThermalMapWords
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.core.MapLayersWords
import fly.venti.core.mapLayersWords
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.design.Common

/** The words of the map tab, all of them the core's: the sheet's, its layers' and rows' names, the credits. */
object MapWords {
    val core: MapLayersWords by lazy { mapLayersWords() }

    /** Whose the base map and the terrain are, under the credits. */
    val baseCredits: List<String> get() = listOf(core.baseMapCredit, core.terrainCredit).filter { it.isNotEmpty() }
}

/** A layer of the map as a row of the layers sheet: its switch, and a line under its name if there is one to say. */
class MapLayer(
    val title: String,
    @DrawableRes val icon: Int,
    val tag: String,
    val isOn: Boolean,
    /** What the layer says of itself while it is on: how many, why none, whose it is. */
    val notes: List<String> = emptyList(),
    /** What the layer cannot promise, behind its "i": the core's words, folded until the pilot asks. */
    val about: String? = null,
    val onChange: (Boolean) -> Unit,
)

/** A list or a board the map leads to, as a row of the layers sheet. */
class MapList(
    val title: String,
    @DrawableRes val icon: Int,
    val onOpen: () -> Unit,
    /** What the row says beside its name, as the filter's present words. */
    val value: String? = null,
    /** For a test, where a row has more to it than its name. */
    val tag: String? = null,
)

/**
 * The one sheet behind the map's layers button: a switch for every layer
 * laid over what the map shows (the chips choose that), and a row for each
 * list and board the map leads to. What is advanced is
 * here and nowhere on the map itself.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MapLayersSheet(layers: List<MapLayer>, views: List<MapList>, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        // The title and "Done" stay at the top while the rows scroll under them.
        Row(Modifier.padding(horizontal = VentiSpace.s4), verticalAlignment = Alignment.CenterVertically) {
            Text(
                MapWords.core.title,
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.weight(1f).semantics { heading() },
            )
            TextButton(onClick = onDismiss, modifier = Modifier.testTag("layers-close")) {
                Text(Common.words.done)
            }
        }
        Column(
            modifier = Modifier
                .testTag("layers-sheet")
                // The sheet as high as the screen allows, whatever its rows.
                // A sheet whose rows came to about the screen's height and
                // was pulled up kept Compose recomposing and drawing without
                // end, at the edge between resting under the status bar and
                // below it (the UI tests, 2026-11-23).
                .fillMaxHeight()
                // Clear of the navigation bar, so that the last row can be scrolled above it.
                .navigationBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
        ) {
            SectionHeader(MapWords.core.onTheMap)
            Group {
                for (layer in layers) {
                    row { LayerRow(layer) }
                }
            }
            if (views.isNotEmpty()) {
                SectionHeader(MapWords.core.listsTitle)
                Group {
                    for (view in views) {
                        row {
                            if (view.value.isNullOrEmpty()) {
                                // Said to TalkBack by its name, as the round button it was.
                                LinkRow(
                                    view.title,
                                    R.drawable.ic_chevron_right,
                                    Modifier
                                        .semantics { contentDescription = view.title }
                                        .then(view.tag?.let { Modifier.testTag(it) } ?: Modifier),
                                    leading = view.icon,
                                    onClick = view.onOpen,
                                )
                            } else {
                                ListRowWithValue(view)
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * One layer: its sign, its name, what it says of itself under it, and its
 * switch. The row is the switch; a layer with something to say about
 * itself has an "i" beside it, which unfolds that under the row.
 */
@Composable
private fun LayerRow(layer: MapLayer) {
    var showsAbout by remember { mutableStateOf(false) }
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Row(
                modifier = Modifier
                    .weight(1f)
                    .toggleable(value = layer.isOn, role = Role.Switch, onValueChange = layer.onChange)
                    .testTag(layer.tag)
                    .heightIn(min = VentiSize.touch)
                    .padding(start = VentiSpace.s4, end = if (layer.about == null) VentiSpace.s3 else 0.dp, top = VentiSpace.s2, bottom = VentiSpace.s2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painterResource(layer.icon),
                    contentDescription = null,
                    modifier = Modifier.size(VentiSize.icon),
                    tint = Venti.colours.textSecondary,
                )
                Spacer(Modifier.width(VentiSpace.s3))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                    Text(layer.title, style = MaterialTheme.typography.bodyLarge)
                    for (note in layer.notes) {
                        Text(
                            note,
                            style = MaterialTheme.typography.bodyMedium.numbers,
                            color = Venti.colours.textSecondary,
                        )
                    }
                }
                Spacer(Modifier.width(VentiSpace.s3))
                Switch(checked = layer.isOn, onCheckedChange = null)
            }
            if (layer.about != null) {
                IconButton(onClick = { showsAbout = !showsAbout }, modifier = Modifier.size(VentiSize.touch).testTag("${layer.tag}-about")) {
                    Icon(
                        painterResource(R.drawable.ic_info),
                        contentDescription = "${layer.title}: ${ThermalMapWords.KEY_AND_NOTES}",
                        tint = if (showsAbout) MaterialTheme.colorScheme.primary else Venti.colours.textSecondary,
                    )
                }
            }
        }
        if (showsAbout && layer.about != null) {
            Text(
                layer.about,
                style = MaterialTheme.typography.bodySmall,
                color = Venti.colours.textSecondary,
                modifier = Modifier
                    .padding(start = VentiSpace.s4 + VentiSize.icon + VentiSpace.s3, end = VentiSpace.s4, bottom = VentiSpace.s2)
                    .testTag("${layer.tag}-about-text"),
            )
        }
    }
}

/**
 * A list's row that says something beside its name, as the sites' filter
 * says what it asks: under the name, since the words can be long.
 */
@Composable
private fun ListRowWithValue(view: MapList) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = view.onOpen)
            .then(view.tag?.let { Modifier.testTag(it) } ?: Modifier)
            .heightIn(min = VentiSize.touch)
            .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(view.icon),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(VentiSpace.s4))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
            Text(view.title, style = MaterialTheme.typography.bodyLarge)
            Text(
                view.value.orEmpty(),
                style = MaterialTheme.typography.bodyMedium,
                color = Venti.colours.textSecondary,
            )
        }
        Spacer(Modifier.width(VentiSpace.s3))
        Icon(
            painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * The (i) on the map: the one sign of whose data the map shows. A tap
 * opens every credit in full.
 */
@Composable
fun MapCreditsButton(tint: Color, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    IconButton(onClick = onOpen, modifier = modifier.size(VentiSize.touch).testTag("map-credits")) {
        Icon(painterResource(R.drawable.ic_info), contentDescription = MapWords.core.creditsTitle, tint = tint)
    }
}

/**
 * Whose the map and everything on it is: a line for each credit, none cut
 * short, Venti's data first and the base map's under it. The licences ask
 * that each credit can be read: all of them are one tap from the map.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MapCreditsSheet(lines: List<String>, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                // Clear of the navigation bar, so that the last row can be scrolled above it.
                .navigationBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    MapWords.core.creditsTitle,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("credits-close")) {
                    Text(Common.words.done)
                }
            }
            // One element for a test and for TalkBack, its lines its texts.
            Column(
                modifier = Modifier.fillMaxWidth().testTag("wind-credits").semantics(mergeDescendants = true) {},
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                for (line in lines) {
                    Text(line, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                }
            }
            Box(Modifier.heightIn(min = VentiSpace.s2))
        }
    }
}

/**
 * What the map says of a climb or of a cell of where thermals have been,
 * tapped: the core's line, and under it whose it is and what it is.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AirLineSheet(title: String, line: String, notes: List<String>, tag: String, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                // Clear of the navigation bar, so that the last row can be scrolled above it.
                .navigationBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    title,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("air-line-close")) {
                    Text(Common.words.done)
                }
            }
            Text(line, style = MaterialTheme.typography.bodyLarge.numbers, modifier = Modifier.testTag(tag))
            for (note in notes) {
                Text(note, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
            }
        }
    }
}
