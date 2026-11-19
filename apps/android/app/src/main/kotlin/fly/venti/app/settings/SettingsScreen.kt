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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
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
import androidx.compose.ui.unit.dp
import fly.venti.app.BuildConfig
import fly.venti.app.R
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.fly.FlySession

/** The pilot's settings, and what the app is made of. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(preferences: Preferences, fly: FlySession, onCredits: () -> Unit, bars: PaddingValues) {
    val links = LocalUriHandler.current
    var pilot by remember { mutableStateOf(preferences.pilot) }
    var glider by remember { mutableStateOf(preferences.glider) }
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
            }

            SectionHeader("About")
            Group {
                row { ValueRow("Version", BuildConfig.VERSION_NAME) }
                row { LinkRow("Map data and licences", R.drawable.ic_chevron_right, onClick = onCredits) }
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
