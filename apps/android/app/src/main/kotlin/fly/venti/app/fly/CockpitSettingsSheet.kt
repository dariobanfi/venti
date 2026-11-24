package fly.venti.app.fly

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.os.PowerManager
import android.provider.Settings
import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Column
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.SectionFooter
import fly.venti.core.mapLayersWords
import fly.venti.core.pagesWords
import fly.venti.app.design.Common
import fly.venti.app.design.Group
import fly.venti.app.maps.OfflineMapsWords
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue

/** The words of the cockpit's settings that are not the core's. */
object CockpitSettingsWords {
    const val TITLE = "Settings"
    const val VARIO = "Vario"
    val THERMALS: String get() = mapLayersWords().thermalMap
    const val VARIO_FOOT = "The phone beeps in flight, and not while a connected vario beeps itself."
    const val REPLAY_FOOT = "Plays a recorded flight through the cockpit, to try it on the ground. Nothing is saved."
    const val AIRSPACE = "Airspace"
    const val TASK = "Task"
    const val LIVE = "Live tracking"
    const val REPLAY = "Replay the sample flight"
    const val IN_REAL_TIME = "In real time"
    const val FASTER = "20 times faster"
    const val NONE = "None"
    const val BATTERY = "Battery optimisation"
    const val BATTERY_LINE = "Android may stop a recording with the screen off. Allow Venti to run."
}

/**
 * What the Fly tab listed before the cockpit opened, now behind the one
 * settings button in the cockpit's top line: the vario, the thermal map, what
 * the cockpit warns from, the map it draws without a network, the task,
 * who watches the flight, and a replay to try the cockpit on the ground.
 * Where Android may stop the app with the screen off, a row leads to
 * Android's own page for it; there is no pop-up.
 * In the flight theme, every row as high as a gloved finger needs. A row
 * that leads to a list of its own leaves the cockpit for it; its back
 * arrow comes back here.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CockpitSettingsSheet(
    /** What the vario row says: the vario chosen, and how it stands. */
    vario: String,
    /** Whether the cockpit's map shows the thermal map; `null` where the maps do not offer it. */
    showsThermals: Boolean?,
    onThermals: (Boolean) -> Unit,
    /** What the cockpit warns from, in a line. */
    airspace: String,
    /** The maps kept for flying without a network, in a line. */
    offlineMaps: String,
    /** The task flown, in a line, or `null` without one. */
    task: String?,
    /** Who watches the flight, in a line. */
    live: String,
    /** Whether a replay may begin: nothing is being recorded. */
    canReplay: Boolean,
    onVario: () -> Unit,
    onAirspace: () -> Unit,
    onMaps: () -> Unit,
    onTask: () -> Unit,
    onLive: () -> Unit,
    /** The settings' page "Glider and cockpit". */
    onGliderAndCockpit: () -> Unit,
    /** Called with how many times faster than it was flown the sample flight is to be replayed. */
    onReplay: (Double) -> Unit,
    onDismiss: () -> Unit,
) {
    var isChoosingReplay by remember { mutableStateOf(false) }
    val context = LocalContext.current
    // Asked again whenever the app comes back, from Android's page too.
    var isOptimised by remember { mutableStateOf(isBatteryOptimised(context)) }
    LifecycleResumeEffect(Unit) {
        isOptimised = isBatteryOptimised(context)
        onPauseOrDispose {}
    }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(CockpitSettingsWords.TITLE, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(
                onClick = onDismiss,
                modifier = Modifier.heightIn(min = VentiSize.touchFlight).testTag("cockpit-settings-close"),
            ) { Text(Common.words.done, style = MaterialTheme.typography.titleMedium) }
        }
        LazyColumn(
            modifier = Modifier.testTag("cockpit-settings-sheet"),
            contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s6),
        ) {
            item {
                Group {
                    row { SettingRow(R.drawable.venti_vario, CockpitSettingsWords.VARIO, vario, "cockpit-settings-vario", onVario) }
                }
            }
            // When the phone sounds is the core's: in flight only, and not
            // while a connected vario beeps itself.
            item { SectionFooter(CockpitSettingsWords.VARIO_FOOT) }
            // On the cockpit's map, at 40 %: fetched tile by tile for
            // the map in view, never kept for flying without a network.
            if (showsThermals != null) {
                item { Spacer(Modifier.padding(top = VentiSpace.s5)) }
                item {
                    Group {
                        row {
                            SwitchSettingRow(
                                R.drawable.venti_thermal,
                                CockpitSettingsWords.THERMALS,
                                isOn = showsThermals,
                                tag = "cockpit-thermals",
                                onChange = onThermals,
                            )
                        }
                    }
                }
            }
            // Android's battery optimisation may stop a recording with the
            // screen off: its own page, only while Venti is under it.
            if (isOptimised) {
                item { Spacer(Modifier.padding(top = VentiSpace.s5)) }
                item {
                    Group {
                        row {
                            SettingRow(
                                R.drawable.ic_battery,
                                CockpitSettingsWords.BATTERY,
                                "",
                                "cockpit-battery",
                                line = CockpitSettingsWords.BATTERY_LINE,
                                onClick = {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
                                    } catch (_: ActivityNotFoundException) {
                                        // A phone without the page: nothing to open.
                                    }
                                },
                            )
                        }
                    }
                }
            }
            item { Spacer(Modifier.padding(top = VentiSpace.s5)) }
            item {
                Group {
                    // What the cockpit will warn from, to be seen before
                    // take-off and not found out in the air.
                    row { SettingRow(R.drawable.venti_airspace, CockpitSettingsWords.AIRSPACE, airspace, "airspace-before-takeoff", onAirspace) }
                    // The map the cockpit will draw without a network.
                    row { SettingRow(R.drawable.venti_map, OfflineMapsWords.ROW, offlineMaps, "maps-before-takeoff", onMaps) }
                    // The task of the day, or a route planned for the flight.
                    row {
                        SettingRow(
                            R.drawable.ic_sports_score,
                            CockpitSettingsWords.TASK,
                            task ?: CockpitSettingsWords.NONE,
                            "task-before-takeoff",
                            onTask,
                        )
                    }
                    // Who may watch the flight while it is under way.
                    row { SettingRow(R.drawable.venti_live, CockpitSettingsWords.LIVE, live, "live-before-takeoff", onLive) }
                    // The glider's numbers and what the cockpit shows, as on the iPhone.
                    row {
                        SettingRow(
                            R.drawable.venti_wing,
                            pagesWords().gliderAndCockpit,
                            "",
                            "cockpit-settings-glider",
                            onGliderAndCockpit,
                        )
                    }
                }
            }
            // To try the cockpit on the ground, while nothing is recorded.
            if (canReplay) {
                item { Spacer(Modifier.padding(top = VentiSpace.s5)) }
                item {
                    Group {
                        row {
                            Box {
                                SettingRow(
                                    R.drawable.ic_play_arrow,
                                    CockpitSettingsWords.REPLAY,
                                    "",
                                    "cockpit-replay",
                                    onClick = { isChoosingReplay = true },
                                )
                                DropdownMenu(expanded = isChoosingReplay, onDismissRequest = { isChoosingReplay = false }) {
                                    DropdownMenuItem(
                                        text = { Text(CockpitSettingsWords.IN_REAL_TIME, style = MaterialTheme.typography.titleMedium) },
                                        modifier = Modifier.heightIn(min = VentiSize.touchFlight),
                                        onClick = {
                                            isChoosingReplay = false
                                            onReplay(1.0)
                                        },
                                    )
                                    DropdownMenuItem(
                                        text = { Text(CockpitSettingsWords.FASTER, style = MaterialTheme.typography.titleMedium) },
                                        modifier = Modifier.heightIn(min = VentiSize.touchFlight),
                                        onClick = {
                                            isChoosingReplay = false
                                            onReplay(20.0)
                                        },
                                    )
                                }
                            }
                        }
                    }
                }
                item { SectionFooter(CockpitSettingsWords.REPLAY_FOOT) }
            }
        }
    }
}

/** A row with a switch, as high as a gloved finger needs; the whole row takes the press. */
@Composable
private fun SwitchSettingRow(@DrawableRes icon: Int, title: String, isOn: Boolean, tag: String, onChange: (Boolean) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .toggleable(value = isOn, role = Role.Switch, onValueChange = onChange)
            .heightIn(min = VentiSize.touchFlight)
            .testTag(tag)
            .padding(horizontal = VentiSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(icon),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(VentiSize.icon),
        )
        Spacer(Modifier.width(VentiSpace.s4))
        Text(title, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
        Switch(checked = isOn, onCheckedChange = null)
    }
}

/** Whether Android's battery optimisation holds Venti: it may then stop a recording with the screen off. */
internal fun isBatteryOptimised(context: Context): Boolean {
    val power = context.getSystemService(PowerManager::class.java) ?: return false
    return !power.isIgnoringBatteryOptimizations(context.packageName)
}

/**
 * A row that opens a choice: a sign, what it is, and what is chosen now, as
 * high as a gloved finger needs; `line` is one short line under the title.
 */
@Composable
private fun SettingRow(
    @DrawableRes icon: Int,
    title: String,
    value: String,
    tag: String,
    onClick: () -> Unit,
    line: String? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = VentiSize.touchFlight)
            .testTag(tag)
            .padding(horizontal = VentiSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(icon),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(VentiSize.icon),
        )
        Spacer(Modifier.width(VentiSpace.s4))
        if (line == null) {
            Text(title, style = MaterialTheme.typography.titleMedium)
        } else {
            Column(Modifier.weight(1f).padding(vertical = VentiSpace.s2)) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(line, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Spacer(Modifier.width(VentiSpace.s3))
        // The value takes the room the title leaves, and is cut short
        // before it pushes the title off the row.
        if (line == null) Text(
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

/** The vario row's words: the vario chosen and how it stands, or the phone's own sensors. */
internal fun varioSummary(vario: VarioLink): String = when (val state = vario.state) {
    is VarioState.Connected -> state.name
    is VarioState.Connecting -> "Looking for ${state.name}"
    is VarioState.Unavailable -> "Bluetooth unavailable"
    VarioState.Idle, VarioState.Scanning -> if (vario.hasRememberedVario) "Not in range" else "Phone"
}
