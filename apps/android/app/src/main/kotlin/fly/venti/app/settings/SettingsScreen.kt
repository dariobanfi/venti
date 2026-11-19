package fly.venti.app.settings

import androidx.annotation.DrawableRes
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fly.venti.app.BuildConfig
import fly.venti.app.R
import fly.venti.app.SettingsRoute
import fly.venti.app.design.Format
import fly.venti.app.design.numbers
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.fly.FlySession
import fly.venti.app.wind.WindModel

/** The pilot's settings, and what the app is made of. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    preferences: Preferences,
    fly: FlySession,
    wind: WindModel,
    /** Opens a screen on top of the settings. */
    onOpen: (SettingsRoute) -> Unit,
    bars: PaddingValues,
) {
    val links = LocalUriHandler.current
    var pilot by remember { mutableStateOf(preferences.pilot) }
    var glider by remember { mutableStateOf(preferences.glider) }
    var trimSpeed by remember { mutableStateOf(preferences.trimSpeed) }
    var glideRatio by remember { mutableStateOf(preferences.glideRatio) }
    val names = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false)

    Scaffold(
        modifier = Modifier.padding(bottom = bars.calculateBottomPadding()),
        topBar = { TopAppBar(title = { Text("Settings") }) },
        contentWindowInsets = WindowInsets(0),
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 24.dp),
        ) {
            SectionHeader("Pilot", Modifier.padding(top = 0.dp))
            OutlinedTextField(
                value = pilot,
                onValueChange = {
                    pilot = it
                    preferences.pilot = it
                },
                label = { Text("Your name") },
                singleLine = true,
                keyboardOptions = names,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(
                value = glider,
                onValueChange = {
                    glider = it
                    preferences.glider = it
                },
                label = { Text("Glider") },
                singleLine = true,
                keyboardOptions = names,
                modifier = Modifier.fillMaxWidth(),
            )
            SectionFooter("Written into the flights you record. They stay on this phone.")

            SectionHeader("Cockpit")
            Group {
                row {
                    SwitchRow("Vario sound", isOn = !fly.isMuted) { fly.isMuted = !it }
                }
                row {
                    LinkRow("Waypoints", R.drawable.ic_chevron_right) { onOpen(SettingsRoute.Waypoints) }
                }
                row {
                    StepperRow(
                        "Trim speed",
                        value = "${Format.whole(trimSpeed)} km/h",
                        onLess = { trimSpeed = (trimSpeed - 1).coerceIn(TRIM_SPEEDS) },
                        onMore = { trimSpeed = (trimSpeed + 1).coerceIn(TRIM_SPEEDS) },
                        onChanged = { preferences.trimSpeed = trimSpeed },
                    )
                }
                row {
                    StepperRow(
                        "Glide ratio",
                        value = Format.glideRatio(glideRatio),
                        onLess = { glideRatio = (glideRatio - 0.5).coerceIn(GLIDE_RATIOS) },
                        onMore = { glideRatio = (glideRatio + 0.5).coerceIn(GLIDE_RATIOS) },
                        onChanged = { preferences.glideRatio = glideRatio },
                    )
                }
            }
            SectionFooter(
                "Your glider's speed with the brakes up and its glide ratio. From them and the wind, the " +
                    "cockpit predicts how high you arrive at the place you fly to. The prediction assumes " +
                    "air that neither rises nor sinks on the way.",
            )

            SectionHeader("Live wind")
            OutlinedTextField(
                value = wind.address,
                onValueChange = wind::use,
                label = { Text("Server address") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    keyboardType = KeyboardType.Uri,
                ),
                modifier = Modifier.fillMaxWidth(),
            )
            SectionFooter(wind.addressNote)

            SectionHeader("About")
            Group {
                row { ValueRow("Version", BuildConfig.VERSION_NAME) }
                row { LinkRow("Data and licences", R.drawable.ic_chevron_right) { onOpen(SettingsRoute.Credits) } }
                row {
                    LinkRow("Source code", R.drawable.ic_open_in_new) {
                        links.openUri("https://github.com/dariobanfi/venti")
                    }
                }
            }
        }
    }
}

/** A row with a switch; pressing anywhere on it flips the switch. */
@Composable
private fun SwitchRow(title: String, isOn: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .toggleable(value = isOn, role = Role.Switch, onValueChange = onChange)
            .heightIn(min = 56.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Switch(checked = isOn, onCheckedChange = null)
    }
}

/** What a glider's trim speed can be set to, km/h, and its glide ratio. */
private val TRIM_SPEEDS = 25.0..60.0
private val GLIDE_RATIOS = 4.0..14.0

/** A row with a number and a button either side of it to make it less or more. */
@Composable
private fun StepperRow(title: String, value: String, onLess: () -> Unit, onMore: () -> Unit, onChanged: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(start = 16.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        IconButton(
            onClick = {
                onLess()
                onChanged()
            },
        ) {
            Icon(painterResource(R.drawable.ic_remove), contentDescription = "Less: $title")
        }
        Text(
            value,
            style = MaterialTheme.typography.bodyLarge.numbers,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(min = 72.dp),
        )
        IconButton(
            onClick = {
                onMore()
                onChanged()
            },
        ) {
            Icon(painterResource(R.drawable.ic_add), contentDescription = "More: $title")
        }
    }
}

/** A row that leads somewhere: another screen, or the browser. */
@Composable
private fun LinkRow(title: String, @DrawableRes icon: Int, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 52.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Icon(painterResource(icon), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
