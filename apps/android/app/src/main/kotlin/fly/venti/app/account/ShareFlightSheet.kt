package fly.venti.app.account

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.SharedFiles
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Group
import fly.venti.app.design.SectionHeader
import fly.venti.core.FlightVisibility
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * Who may see a flight, and the link to it. Nothing is ever shared by
 * default: a flight is private on the server until the pilot says
 * otherwise here.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareFlightSheet(
    /** The flight's identifier in the logbook. */
    flight: String,
    account: AccountModel,
    /** Where requests run, so that one carries on when the sheet is closed. */
    scope: CoroutineScope,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    var sharing by remember(flight) { mutableStateOf(account.sharing(flight)) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Share flight", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text("Done") }
        }
        Column(
            Modifier
                .testTag("share-flight")
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
        ) {
            val current = sharing
            when {
                !account.isSignedIn -> Text(
                    "To share a flight with a link, sign in to Venti under Settings, Account.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 16.dp),
                )
                current != null -> {
                    SectionHeader("Who can see this flight", Modifier.testTag("share-who"))
                    Group {
                        for (visibility in listOf(FlightVisibility.PRIVATE, FlightVisibility.UNLISTED, FlightVisibility.PUBLIC)) {
                            row {
                                VisibilityRow(
                                    visibility,
                                    isChosen = current.visibility == visibility,
                                    enabled = !account.isWorking,
                                ) {
                                    scope.launch { account.share(flight, visibility)?.let { sharing = it } }
                                }
                            }
                        }
                    }
                    if (current.visibility != FlightVisibility.PRIVATE) {
                        SectionHeader("Link")
                        Group {
                            row {
                                SelectionContainer {
                                    Text(
                                        current.link,
                                        style = MaterialTheme.typography.bodyMedium,
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .testTag("share-link")
                                            .padding(horizontal = 16.dp, vertical = 14.dp),
                                    )
                                }
                            }
                            row {
                                ActionRow("Share the link", MaterialTheme.colorScheme.primary) {
                                    SharedFiles.shareText(context, current.link, "Share the link")
                                }
                            }
                        }
                    }
                }
                else -> Group(Modifier.padding(top = 16.dp)) {
                    row {
                        Text(
                            "This flight is not on Venti yet.",
                            style = MaterialTheme.typography.bodyLarge,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 14.dp),
                        )
                    }
                    row {
                        ActionRow("Sync now", MaterialTheme.colorScheme.primary, enabled = !account.isWorking) {
                            scope.launch {
                                account.sync(force = true, quietly = false)
                                sharing = account.sharing(flight)
                            }
                        }
                    }
                }
            }
        }
    }

    // What the server refused, said where the pilot asked for it.
    account.problem?.let { problem ->
        AlertDialog(
            onDismissRequest = { account.problem = null },
            title = { Text("Share flight") },
            text = { Text(problem) },
            confirmButton = { TextButton(onClick = { account.problem = null }) { Text("OK") } },
        )
    }
}

/** One answer to "who can see this flight", with what it means; the chosen one is ticked. */
@Composable
private fun VisibilityRow(visibility: FlightVisibility, isChosen: Boolean, enabled: Boolean, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("share-${visibility.key}")
            .selectable(selected = isChosen, enabled = enabled, role = Role.RadioButton, onClick = onClick)
            .heightIn(min = 64.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(visibility.title, style = MaterialTheme.typography.bodyLarge)
            Text(
                visibility.explanation,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(12.dp))
        if (isChosen) {
            Icon(painterResource(R.drawable.ic_check), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        } else {
            Spacer(Modifier.width(24.dp))
        }
    }
}
