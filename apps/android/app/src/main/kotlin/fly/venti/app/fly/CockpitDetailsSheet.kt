package fly.venti.app.fly

import fly.venti.core.cockpitDetailsWords
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.numbers
import fly.venti.core.CockpitView

/** The cockpit's details sheet in words, all of them the core's: its title, the names of its rows, its note. */
object CockpitDetailsWords {
    /** The core's words, read once. */
    val core: fly.venti.core.CockpitDetailsWords by lazy { cockpitDetailsWords() }

    /**
     * The rows there are now, each a name and the core's words, in the
     * order of the sheet. A reading the core does not give has no row; the
     * air's own rise and fall and the speed to fly are the core's only for
     * a pilot who set a glider class.
     */
    fun rows(view: CockpitView): List<Pair<String, String>> = listOfNotNull(
        view.airspeedWords?.let { core.airspeed to it },
        view.headingWords?.let { core.heading to it },
        view.altitudeSourceWords?.let { core.altitude to it },
        view.qnhWords?.let { core.qnh to it },
        view.nettoWords?.let { core.air to it },
        view.speedToFly?.let { core.speedToFly to it.words },
    )
}

/**
 * The readings that have no place on a page of the cockpit: the estimated
 * airspeed and heading, where the altitude comes from, the QNH, and for a
 * pilot who set a glider class what the air does and the speed to fly. All
 * in the core's words; opened from the flight's page.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CockpitDetailsSheet(view: CockpitView, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(CockpitDetailsWords.core.title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss, modifier = Modifier.testTag("cockpit-details-close")) { Text("Done") }
        }
        val rows = CockpitDetailsWords.rows(view)
        LazyColumn(
            modifier = Modifier.testTag("cockpit-details-sheet"),
            contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s6),
        ) {
            item {
                if (rows.isEmpty()) {
                    Text(
                        CockpitDetailsWords.core.noneYet,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s4),
                    )
                } else {
                    Group {
                        for ((name, words) in rows) {
                            row { DetailRow(name, words) }
                        }
                    }
                }
            }
            item { SectionFooter(CockpitDetailsWords.core.note) }
        }
    }
}

/** One reading: its name, and under it the core's words. */
@Composable
private fun DetailRow(name: String, words: String) {
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(min = VentiSize.touch)
            .testTag("cockpit-detail")
            .clearAndSetSemantics {
                contentDescription = name
                stateDescription = words
            }
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
    ) {
        Text(name, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(words, style = MaterialTheme.typography.bodyLarge.numbers)
    }
}
