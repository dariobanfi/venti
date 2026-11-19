package fly.venti.app.fly

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.AppFiles
import fly.venti.app.R
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter

/** The Fly tab: what to check before take-off, and the way into the cockpit. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FlyScreen(fly: FlySession, vario: VarioLink, files: AppFiles, bars: PaddingValues) {
    var isChoosingVario by remember { mutableStateOf(false) }
    var isChoosingReplay by remember { mutableStateOf(false) }

    // Android asks the pilot before the app may use the position. The
    // flight starts whatever the answer: the cockpit says what is missing.
    val askThenStart = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        fly.startFlight()
    }

    fun startReplay(speed: Double) {
        isChoosingReplay = false
        val text = try {
            files.text(AppFiles.SAMPLE_FLIGHT)
        } catch (_: java.io.IOException) {
            fly.problem = "The sample flight is missing from the app."
            return
        }
        fly.startReplay(text, speed)
    }

    Scaffold(
        modifier = Modifier.padding(bottom = bars.calculateBottomPadding()),
        topBar = { TopAppBar(title = { Text("Fly") }) },
        contentWindowInsets = WindowInsets(0),
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 24.dp),
        ) {
            Group {
                row {
                    ChoiceRow(
                        icon = R.drawable.ic_sensors,
                        title = "Vario",
                        value = varioSummary(vario),
                        onClick = { isChoosingVario = true },
                    )
                }
            }
            SectionFooter("Without an external vario the phone's own pressure sensor is used. It is slower.")

            Spacer(Modifier.height(28.dp))
            Button(
                onClick = { askThenStart.launch(permissionsToFly(vario)) },
                modifier = Modifier.fillMaxWidth().height(56.dp),
            ) {
                Text("Start flight", style = MaterialTheme.typography.titleMedium)
            }
            SectionFooter(
                "Recording starts at once and carries on with the screen off. When you finish, the " +
                    "flight is saved to the logbook.",
            )

            Spacer(Modifier.height(28.dp))
            Group {
                row {
                    Box {
                        ChoiceRow(
                            icon = R.drawable.ic_play_arrow,
                            title = "Replay the sample flight",
                            value = "",
                            onClick = { isChoosingReplay = true },
                        )
                        DropdownMenu(expanded = isChoosingReplay, onDismissRequest = { isChoosingReplay = false }) {
                            DropdownMenuItem(text = { Text("In real time") }, onClick = { startReplay(1.0) })
                            DropdownMenuItem(text = { Text("20 times faster") }, onClick = { startReplay(20.0) })
                        }
                    }
                }
            }
            SectionFooter(
                "Plays a recorded flight through the cockpit, to try it on the ground. Nothing is saved.",
            )
        }
    }

    if (isChoosingVario) {
        VarioSheet(vario) { isChoosingVario = false }
    }
}

private fun varioSummary(vario: VarioLink): String = when (val state = vario.state) {
    is VarioState.Connected -> state.name
    is VarioState.Connecting -> "Looking for ${state.name}"
    is VarioState.Unavailable -> "Bluetooth unavailable"
    VarioState.Idle, VarioState.Scanning -> if (vario.hasRememberedVario) "Not in range" else "Phone"
}

/**
 * What Android has to ask the pilot before a flight: the position, to
 * record it; notifications, to show that recording goes on with the screen
 * off; and Bluetooth, if a vario was used before.
 */
private fun permissionsToFly(vario: VarioLink): Array<String> = buildList {
    add(Manifest.permission.ACCESS_FINE_LOCATION)
    add(Manifest.permission.ACCESS_COARSE_LOCATION)
    if (Build.VERSION.SDK_INT >= 33) {
        add(Manifest.permission.POST_NOTIFICATIONS)
    }
    if (vario.hasRememberedVario) {
        addAll(VarioLink.permissions)
    }
}.distinct().toTypedArray()

/** A row that opens a choice: an icon, what it is, and what is chosen now. */
@Composable
private fun ChoiceRow(icon: Int, title: String, value: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 56.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(16.dp))
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Text(
            value,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Icon(
            painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
