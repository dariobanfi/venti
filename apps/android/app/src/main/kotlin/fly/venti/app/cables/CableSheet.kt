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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.platform.LocalUriHandler
import app.venti.design.VentiSize
import fly.venti.core.CableLine
import fly.venti.core.CablesMap
import fly.venti.app.design.Common

/**
 * A cable tapped on a map: what it is, in the core's words, and whose the
 * data is, as a link to its credit (owner decision 19). What the layer
 * cannot promise stands behind the layer's "i" in the layers sheet.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CableSheet(line: CableLine, layer: CablesMap?, onDismiss: () -> Unit) {
    val links = LocalUriHandler.current
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
            for (credit in layer?.credits.orEmpty()) {
                TextButton(
                    onClick = { links.openUri(credit.url) },
                    enabled = credit.url.isNotEmpty(),
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("cable-credit"),
                ) {
                    Text(credit.text, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
    }
}
