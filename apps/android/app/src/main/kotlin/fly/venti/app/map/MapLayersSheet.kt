package fly.venti.app.map

import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
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
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillExtrusionLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.HillshadeLayer
import org.maplibre.android.style.layers.Layer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.RasterLayer
import org.maplibre.android.style.layers.SymbolLayer

/**
 * The words of the map tab that are the app's own: the names of its
 * layers and lists, which the core does not give, and of the pieces
 * around them.
 */
object MapWords {
    /** The core's words of the sheet: its title and headings, the two layers of the air, what they say when empty. */
    val core: MapLayersWords by lazy { mapLayersWords() }

    const val DONE = "Done"

    const val FORECAST = "Forecast"
    const val THERMALS = "Thermals"
    const val WIND = "Wind"
    const val AIRSPACE = "Airspace"
    const val CABLES = "Cables"
    const val SITES = "Sites"
    const val PILOTS = "Pilots"
    const val AERIAL = "Aerial view"

    const val WIND_STATIONS = "Wind stations"
    const val FLYING_SITES = "Flying sites"
    const val PILOTS_IN_THE_AIR = "Pilots in the air"
    const val FLYING_PLANS = "Flying plans"
}

/** A layer of the map as a row of the layers sheet: its switch, and a line under its name if there is one to say. */
class MapLayer(
    val title: String,
    @DrawableRes val icon: Int,
    val tag: String,
    val isOn: Boolean,
    /** What the layer says of itself while it is on: how many, why none, whose it is. */
    val notes: List<String> = emptyList(),
    val onChange: (Boolean) -> Unit,
)

/** A list or a board the map leads to, as a row of the layers sheet. */
class MapList(val title: String, @DrawableRes val icon: Int, val onOpen: () -> Unit)

/**
 * The one sheet behind the map's layers button: a switch for every layer
 * the map can draw, the four of the chips among them in the same state,
 * and a row for each list and board the map leads to. What is advanced is
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
        Column(
            modifier = Modifier
                .testTag("layers-sheet")
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    MapWords.core.title,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("layers-close")) {
                    Text(MapWords.DONE)
                }
            }
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
                            // Said to TalkBack by its name, as the round button it was.
                            LinkRow(
                                view.title,
                                R.drawable.ic_chevron_right,
                                Modifier.semantics { contentDescription = view.title },
                                leading = view.icon,
                                onClick = view.onOpen,
                            )
                        }
                    }
                }
            }
        }
    }
}

/** One layer: its sign, its name, what it says of itself under it, and its switch. The row is the switch. */
@Composable
private fun LayerRow(layer: MapLayer) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .toggleable(value = layer.isOn, role = Role.Switch, onValueChange = layer.onChange)
            .testTag(layer.tag)
            .heightIn(min = VentiSize.touch)
            .padding(start = VentiSpace.s4, end = VentiSpace.s3, top = VentiSpace.s2, bottom = VentiSpace.s2),
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
                    Text(MapWords.DONE)
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
 * Whose the base map is: the credit of every source that a layer in view
 * draws from, as the style gives it, a line for each. The photographs'
 * sources are among them only while the aerial view shows.
 */
object MapCredits {
    fun of(style: Style): List<String> = runCatching {
        val drawn = style.layers
            .filter { it.visibility.value != Property.NONE }
            .mapNotNull(::sourceOf)
            .toSet()
        style.sources
            .filter { it.id in drawn }
            .map { plain(it.attribution) }
            .filter { it.isNotEmpty() }
            .distinct()
    }.getOrDefault(emptyList())

    /** A credit as the style writes it, with links, as words alone. */
    fun plain(attribution: String): String =
        attribution.replace(Regex("<[^>]*>"), " ").replace("&copy;", "©").replace("&amp;", "&")
            .replace(Regex("\\s+"), " ").trim()

    private fun sourceOf(layer: Layer): String? = when (layer) {
        is FillLayer -> layer.sourceId
        is LineLayer -> layer.sourceId
        is SymbolLayer -> layer.sourceId
        is RasterLayer -> layer.sourceId
        is HillshadeLayer -> layer.sourceId
        is CircleLayer -> layer.sourceId
        is FillExtrusionLayer -> layer.sourceId
        else -> null
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
                    Text(MapWords.DONE)
                }
            }
            Text(line, style = MaterialTheme.typography.bodyLarge.numbers, modifier = Modifier.testTag(tag))
            for (note in notes) {
                Text(note, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
            }
        }
    }
}
