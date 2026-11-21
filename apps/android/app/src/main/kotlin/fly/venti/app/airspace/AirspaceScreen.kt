package fly.venti.app.airspace

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.numbers
import fly.venti.app.logbook.FlightFiles
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.ImportNoticeDialog
import fly.venti.core.AirspaceFile
import fly.venti.core.KeptAirspace
import kotlinx.coroutines.launch

/**
 * The pilot's airspace: what the app keeps from the pilot's server, and the
 * files the pilot brings in. Where each comes from, how old it is, and what
 * it left out.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AirspaceScreen(
    airspace: AirspaceModel,
    /** Whether there is Venti to ask: always in the app, not in a test that names none. */
    hasServer: Boolean,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var notice by remember { mutableStateOf<ImportNotice?>(null) }

    // Airspace files have no type Android knows, so any file can be picked;
    // the core says which ones hold airspace.
    val chooseFiles = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) {
            scope.launch {
                notice = airspace.importFiles(uris.map { FlightFiles.picked(context, it, what = "an airspace file") })
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Airspace") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 24.dp),
        ) {
            airspace.problem?.let { problem ->
                Row(Modifier.padding(horizontal = 4.dp, vertical = 8.dp), verticalAlignment = Alignment.Top) {
                    Icon(
                        painterResource(R.drawable.venti_caution),
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.error,
                    )
                    Spacer(Modifier.width(12.dp))
                    Text(problem, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error)
                }
            }
            SectionHeader("From Venti")
            Group {
                for (layer in airspace.kept) {
                    row { KeptRow(layer) }
                }
                airspace.serverProblem?.let { problem ->
                    row { Note(problem, R.drawable.ic_cloud_off, MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                row {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .toggleable(
                                value = airspace.usesServer,
                                role = Role.Switch,
                                onValueChange = { isOn ->
                                    if (isOn) airspace.useServer() else airspace.stopUsingServer()
                                },
                            )
                            .heightIn(min = 56.dp)
                            .padding(horizontal = 16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            "Airspace from Venti",
                            style = MaterialTheme.typography.bodyLarge,
                            modifier = Modifier.weight(1f),
                        )
                        // The row is the switch: it takes the press and says the state.
                        Switch(checked = airspace.usesServer, onCheckedChange = null)
                    }
                }
            }
            SectionFooter(AirspaceModel.serverNote(airspace.usesServer, hasServer, airspace.kept.isNotEmpty()))

            SectionHeader("Your files")
            Group {
                for (file in airspace.files) {
                    row { FileRow(file, onDelete = { airspace.delete(file) }) }
                }
                row {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable(role = Role.Button) { chooseFiles.launch(arrayOf("*/*")) }
                            .heightIn(min = 56.dp)
                            .padding(horizontal = 16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            painterResource(R.drawable.ic_file_open),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                        )
                        Spacer(Modifier.width(16.dp))
                        Text(
                            "Add an airspace file",
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
            }
            SectionFooter(
                "Venti reads airspace files in OpenAir format, the kind other flight instruments use. It " +
                    "draws their airspace on the maps and warns in the cockpit when you come near, are " +
                    "about to enter or are inside.",
            )
            SectionFooter(
                "It knows the airspace listed here and no other. Where none of it reaches, the cockpit " +
                    "says that nothing is checked there; an old file it cannot tell from a new one. It " +
                    "does not know which areas are active today. For Germany and Austria it knows the " +
                    "rules a paraglider flies under; anywhere else it goes by an airspace's class and " +
                    "type alone. Every warning is advice: you remain responsible for where you fly.",
            )
        }
    }

    ImportNoticeDialog(notice) { notice = null }
}

/**
 * The airspace of one provider as kept from the server: how much, how old,
 * whose countries, and the credit and the notice its provider asks for.
 */
@Composable
private fun KeptRow(layer: KeptAirspace) {
    Column(
        Modifier
            .fillMaxWidth()
            .testTag("airspace-from-server")
            .semantics(mergeDescendants = true) {}
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(layer.provider, style = MaterialTheme.typography.bodyLarge)
        Text(
            AirspaceModel.detail(layer),
            style = MaterialTheme.typography.bodyMedium.numbers,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        AirspaceModel.countriesNote(layer)?.let { note ->
            Text(note, style = MaterialTheme.typography.bodyMedium)
        }
        if (!layer.saysWhatItCovers) {
            Note(AirspaceModel.COVERS_UNKNOWN, R.drawable.venti_caution, MaterialTheme.colorScheme.error, inRow = true)
        }
        layer.credit?.let { credit ->
            credit.notice?.let { notice ->
                Note(notice, R.drawable.venti_caution, MaterialTheme.colorScheme.onSurface, inRow = true)
            }
            Text(
                "${credit.text} Licence: ${credit.licence}.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** A line with a small sign before it. On a row of its own it brings the row's padding along. */
@Composable
private fun Note(text: String, icon: Int, colour: Color, inRow: Boolean = false) {
    Row(
        modifier = if (inRow) Modifier else Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(18.dp), tint = colour)
        Spacer(Modifier.width(8.dp))
        Text(text, style = MaterialTheme.typography.bodyMedium, color = colour)
    }
}

/** One file: its name, what it holds, and in red what of it could not be read. Press and hold to delete it. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun FileRow(file: AirspaceFile, onDelete: () -> Unit) {
    var isShowingMenu by remember { mutableStateOf(false) }
    Box {
        Column(
            Modifier
                .fillMaxWidth()
                .testTag("airspace-file")
                .combinedClickable(
                    onClick = {},
                    onLongClick = { isShowingMenu = true },
                    onLongClickLabel = "Delete",
                )
                .padding(horizontal = 16.dp, vertical = 12.dp),
        ) {
            Text(file.name, style = MaterialTheme.typography.bodyLarge)
            Text(
                AirspaceModel.detail(file),
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            AirspaceModel.leftOutNote(file)?.let { note ->
                Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.Top) {
                    Icon(
                        painterResource(R.drawable.venti_caution),
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.error,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(note, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error)
                }
            }
        }
        DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
            DropdownMenuItem(
                text = { Text("Delete", color = MaterialTheme.colorScheme.error) },
                onClick = {
                    isShowingMenu = false
                    onDelete()
                },
            )
        }
    }
}
