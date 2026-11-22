package fly.venti.app.fly

import android.content.Context
import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow

/** The list of Bluetooth devices to pick a vario from. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VarioSheet(vario: VarioLink, onDismiss: () -> Unit) {
    val context = LocalContext.current

    // Android asks the pilot before the app may use Bluetooth. The search
    // starts whatever the answer: the list says so if it was "no".
    val askThenScan = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        if (vario.connectedName == null) {
            vario.scan()
        }
    }
    LaunchedEffect(Unit) {
        askThenScan.launch(VarioLink.permissions)
    }
    DisposableEffect(Unit) {
        onDispose { vario.stopScan() }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Vario", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text("Done") }
        }
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp)) {
            val state = vario.state
            if (state is VarioState.Unavailable) {
                item { Unavailable(state.reason) }
            }
            val name = when (state) {
                is VarioState.Connected -> state.name
                is VarioState.Connecting -> state.name
                else -> null
            }
            if (name != null) {
                item {
                    SectionHeader("Your vario")
                    Group {
                        row { ValueRow(name, if (vario.connectedName == null) "Looking for it" else "Connected") }
                        row {
                            ActionRow("Forget this vario", MaterialTheme.colorScheme.error) {
                                vario.forget()
                                vario.scan()
                            }
                        }
                    }
                }
            }
            if (vario.connectedName != null) {
                item { WhatItSends(vario, context) }
            }
            item { NearbyDevices(vario) }
        }
    }
}

@Composable
private fun Unavailable(reason: String) {
    Group(Modifier.padding(top = 8.dp)) {
        row {
            Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.venti_caution),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.error,
                )
                Spacer(Modifier.width(16.dp))
                Text(reason, style = MaterialTheme.typography.bodyLarge)
            }
        }
    }
}

/** What the connected device has sent lately, and a way to share it. */
@Composable
private fun WhatItSends(vario: VarioLink, context: Context) {
    SectionHeader("What it sends")
    Group {
        row { ValueRow("Received", "${Format.count(vario.report.bytes.toLong())} bytes") }
        if (vario.report.lines.isNotEmpty()) {
            row {
                Column(Modifier.padding(horizontal = 16.dp, vertical = 10.dp)) {
                    for (line in vario.report.lines) {
                        Text(
                            line,
                            style = MaterialTheme.typography.bodySmall,
                            fontFamily = FontFamily.Monospace,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
        row {
            ActionRow("Share a report", MaterialTheme.colorScheme.primary) {
                share(context, vario.reportText)
            }
        }
    }
    SectionFooter(
        "If the cockpit shows no climb from this vario, share the report with the project: it says " +
            "what the device offers and sends.",
    )
}

@Composable
private fun NearbyDevices(vario: VarioLink) {
    Row(verticalAlignment = Alignment.Bottom) {
        SectionHeader("Nearby devices", Modifier.weight(1f))
        if (vario.state == VarioState.Scanning) {
            CircularProgressIndicator(Modifier.padding(bottom = 8.dp, end = 4.dp).size(16.dp), strokeWidth = 2.dp)
        }
    }
    if (vario.devices.isNotEmpty()) {
        Group {
            for (device in vario.devices) {
                row {
                    ValueRow(
                        title = device.name,
                        value = if (device.looksLikeAVario) "Vario" else "",
                        onClick = { vario.connect(device) },
                    )
                }
            }
        }
    }
    SectionFooter(
        "Switch the vario on and choose it here, not in the phone's Bluetooth settings. Venti reads " +
            "LK8EX1, OpenVario, XC Tracer, LXWP0 and BlueFly data. Without an external vario the " +
            "phone's own pressure sensor is used. It is slower.",
    )
}

/** Offers the report to other apps as text: mail, a messenger, a note. */
private fun share(context: Context, report: String) {
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_SUBJECT, "Venti vario report")
        putExtra(Intent.EXTRA_TEXT, report)
    }
    context.startActivity(Intent.createChooser(send, "Share a report"))
}
