package fly.venti.app.nearby

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.alerts.AlertNotifications
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.map.OverviewMap
import fly.venti.app.sites.Choice
import fly.venti.app.today.PilotPosition
import fly.venti.core.NearbyChoice
import fly.venti.core.nearbySettingsWords
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/** What the settings for pilots nearby say that is not the core's to say. */
object NearbyWords {
    const val TITLE = "Pilots nearby"
    const val WHERE_THE_MAP_IS = "Where the map is"

    /** The distances to choose from, in kilometres: the server takes 1 to 100. */
    val RADII = listOf(10u, 30u, 50u, 100u)

    fun radius(km: UInt): String = "$km km"

    fun around(place: String): String = "Around $place"
}

/**
 * The account's two settings for what pilots nearby do, both off until the
 * pilot turns them on (owner decision 15), under the core's words: new open
 * plans near a place, with how far around it; and whether pilots nearby may
 * ask the pilot for help. Turning one on asks Android, the first time, for
 * the permission to notify, and for help also for the position.
 */
@Composable
fun NearbySettings(model: NearbyModel, scope: CoroutineScope) {
    val words = remember { nearbySettingsWords() }
    val context = LocalContext.current
    LaunchedEffect(model, model.isSignedIn) { model.loadSettings() }
    val allow = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {}
    // Asked the first time something would be sent, never at start.
    val askFor: (Boolean) -> Unit = { position ->
        val wanted = buildList {
            if (Build.VERSION.SDK_INT >= 33 && !AlertNotifications.areAllowed(context)) add(Manifest.permission.POST_NOTIFICATIONS)
            if (position && !PilotPosition(context.applicationContext).isAllowed) {
                add(Manifest.permission.ACCESS_FINE_LOCATION)
                add(Manifest.permission.ACCESS_COARSE_LOCATION)
            }
        }
        if (wanted.isNotEmpty()) allow.launch(wanted.toTypedArray())
    }
    val settings = model.settings
    val change: (NearbyChoice) -> Unit = { choice -> scope.launch { model.change(choice) } }

    SectionHeader(NearbyWords.TITLE)
    Group(Modifier.testTag("nearby-settings")) {
        row {
            Column {
                SwitchRow(
                    words[0],
                    words[1],
                    isOn = settings?.openPlansNear != null,
                    enabled = settings != null && !model.isSaving,
                    modifier = Modifier.testTag("nearby-open-plans"),
                ) { on ->
                    val now = settings ?: return@SwitchRow
                    if (on) askFor(false)
                    change(now.copy(openPlansNear = if (on) OverviewMap.lastLookedAt else null))
                }
                val place = settings?.openPlansNear
                if (settings != null && place != null) {
                    Column(
                        Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s3),
                        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                    ) {
                        Text(
                            NearbyWords.around(HelpWords.place(place)),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.testTag("nearby-place"),
                        )
                        Row(
                            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                            horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                        ) {
                            for (km in NearbyWords.RADII) {
                                Choice(
                                    NearbyWords.radius(km),
                                    isOn = settings.openPlansRadiusKm == km,
                                    tag = "nearby-radius-$km",
                                ) { if (!model.isSaving) change(settings.copy(openPlansRadiusKm = km)) }
                            }
                        }
                        TextButton(
                            onClick = { change(settings.copy(openPlansNear = OverviewMap.lastLookedAt)) },
                            enabled = !model.isSaving,
                            modifier = Modifier.heightIn(min = VentiSize.touch).testTag("nearby-place-map"),
                        ) { Text(NearbyWords.WHERE_THE_MAP_IS) }
                    }
                }
            }
        }
        row {
            SwitchRow(
                words[2],
                words[3],
                isOn = settings?.mayBeAskedForHelp == true,
                enabled = settings != null && !model.isSaving,
                modifier = Modifier.testTag("nearby-help"),
            ) { on ->
                val now = settings ?: return@SwitchRow
                if (on) askFor(true)
                change(now.copy(mayBeAskedForHelp = on))
            }
        }
    }
    model.settingsProblem?.let { SectionFooter(it, Modifier.testTag("nearby-problem")) }
}

/** A row with a switch and its line under the title; pressing anywhere on it flips the switch. */
@Composable
private fun SwitchRow(
    title: String,
    line: String,
    isOn: Boolean,
    enabled: Boolean,
    modifier: Modifier = Modifier,
    onChange: (Boolean) -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .toggleable(value = isOn, enabled = enabled, role = Role.Switch, onValueChange = onChange)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(line, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Spacer(Modifier.width(VentiSpace.s3))
        Switch(checked = isOn, onCheckedChange = null, enabled = enabled)
    }
}
