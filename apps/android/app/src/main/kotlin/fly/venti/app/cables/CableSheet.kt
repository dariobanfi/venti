package fly.venti.app.cables

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import app.venti.design.VentiSpace
import fly.venti.app.design.SectionFooter
import fly.venti.core.CableLine
import fly.venti.core.CablesMap
import fly.venti.app.design.Common

/**
 * A cable tapped on a map: its words and name as the core gives them, whose
 * it is, and the core's sentence on what the layer does not promise.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CableSheet(line: CableLine, layer: CablesMap?, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s7).testTag("cable-sheet")) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    line.words,
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.weight(1f).testTag("cable-words"),
                )
                TextButton(onClick = onDismiss) { Text(Common.words.done) }
            }
            line.name?.let { name ->
                Text(name, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(top = VentiSpace.s2))
            }
            layer?.limits?.let { SectionFooter(it, Modifier.padding(top = VentiSpace.s4).testTag("cable-limits")) }
            layer?.credits.orEmpty().forEach { credit -> SectionFooter(credit.text) }
        }
    }
}
