package fly.venti.app.fly

import fly.venti.app.maps.OfflineMapsWords
import androidx.compose.material3.ButtonDefaults
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.AppFiles
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.settings.Preferences
import fly.venti.app.task.TaskModel
import fly.venti.app.task.inALine

/** The Fly tab: what to check before take-off, and the way into the cockpit. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FlyScreen(
    fly: FlySession,
    vario: VarioLink,
    airspace: AirspaceModel,
    tasks: TaskModel,
    /** The pilot's account: a flight is shared with others through it. */
    account: AccountModel,
    preferences: Preferences,
    files: AppFiles,
    /** Called to show the list of the pilot's airspace. */
    onOpenAirspace: () -> Unit,
    /** Called to show the pilot's task. */
    onOpenTask: () -> Unit,
    /** Called to show who may watch a flight while it is under way. */
    onOpenLive: () -> Unit,
    /** The maps kept for flying without a network, in a line. */
    offlineMaps: String,
    /** Called to show the maps for flying without a network. */
    onOpenMaps: () -> Unit,
    bars: PaddingValues,
    /** Whether "Start flight" was tapped on another tab, a site's page for one: the cockpit opens. */
    wantsCockpit: Boolean = false,
    /** Called once the cockpit asked for from elsewhere is on its way. */
    onCockpitTaken: () -> Unit = {},
) {
    var isChoosingVario by remember { mutableStateOf(false) }
    var isChoosingReplay by remember { mutableStateOf(false) }

    // Android asks the pilot before the app may use the position. The
    // flight starts whatever the answer: the cockpit says what is missing.
    val askThenStart = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        fly.startFlight()
    }
    // "Start flight" on a site's page: this tab came to the front for it,
    // and starts as its own button does, asking first for what the
    // cockpit needs. The cockpit opens once the answer is in, so no word
    // of the app's own stands over the system's question.
    LaunchedEffect(wantsCockpit) {
        if (wantsCockpit) {
            onCockpitTaken()
            if (fly.mode == null) askThenStart.launch(permissionsToFly(vario))
        }
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
                        icon = R.drawable.venti_vario,
                        title = "Vario",
                        value = varioSummary(vario),
                        onClick = { isChoosingVario = true },
                    )
                }
                // What the cockpit will warn from, to be seen before
                // take-off and not found out in the air.
                row {
                    ChoiceRow(
                        icon = R.drawable.venti_airspace,
                        title = "Airspace",
                        value = airspace.inALine,
                        onClick = onOpenAirspace,
                        modifier = Modifier.testTag("airspace-before-takeoff"),
                    )
                }
                // The map the cockpit will draw without a network.
                row {
                    ChoiceRow(
                        icon = R.drawable.venti_map,
                        title = OfflineMapsWords.ROW,
                        value = offlineMaps,
                        onClick = onOpenMaps,
                        modifier = Modifier.testTag("maps-before-takeoff"),
                    )
                }
                // The task of the day, or a route planned for the flight:
                // the cockpit leads along it.
                row {
                    ChoiceRow(
                        icon = R.drawable.ic_sports_score,
                        title = "Task",
                        value = tasks.task?.inALine ?: "None",
                        onClick = onOpenTask,
                        modifier = Modifier.testTag("task-before-takeoff"),
                    )
                }
                // Who may watch the flight while it is under way. The
                // choice is kept, so it is shown before every start.
                row {
                    ChoiceRow(
                        icon = R.drawable.venti_live,
                        title = "Live tracking",
                        value = LiveChoice.stored(preferences).summary(account.isSignedIn),
                        onClick = onOpenLive,
                        modifier = Modifier.testTag("live-before-takeoff"),
                    )
                }
            }

            Spacer(Modifier.height(28.dp))
            // The screen's one accent: the action that starts a flight.
            Button(
                onClick = { askThenStart.launch(permissionsToFly(vario)) },
                modifier = Modifier.fillMaxWidth().height(56.dp).testTag("open-cockpit"),
                colors = ButtonDefaults.buttonColors(
                    containerColor = MaterialTheme.colorScheme.tertiary,
                    contentColor = MaterialTheme.colorScheme.onTertiary,
                ),
            ) {
                Text(FlyWords.OPEN_COCKPIT, style = MaterialTheme.typography.titleMedium)
            }
            SectionFooter(FlyWords.OPEN_COCKPIT_FOOTER)

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
private fun ChoiceRow(icon: Int, title: String, value: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 56.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(16.dp))
        Text(title, style = MaterialTheme.typography.bodyLarge)
        Spacer(Modifier.width(12.dp))
        // The value takes the room the title leaves, and is cut short
        // before it pushes the title off the row.
        Text(
            value,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.End,
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

/** What the Fly tab says about the cockpit. */
object FlyWords {
    const val OPEN_COCKPIT = "Open the cockpit"
    const val OPEN_COCKPIT_FOOTER =
        "The cockpit waits on the ground and records from the moment you take off, with the screen off too. " +
            "Two minutes after you have landed, the flight is saved to the logbook."
}
