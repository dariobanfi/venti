package fly.venti.app.settings

import app.venti.design.VentiSpace
import fly.venti.core.pagesWords
import fly.venti.app.maps.OfflineMapsWords
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fly.venti.app.BuildConfig
import fly.venti.app.R
import fly.venti.app.SettingsRoute
import fly.venti.app.account.AccountModel
import fly.venti.app.design.Format
import fly.venti.app.design.numbers
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.wind.WindModel
import fly.venti.core.mapLayersWords
import fly.venti.core.GliderClassKind
import fly.venti.app.design.Common

/** The pilot's settings, and what the app is made of. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    preferences: Preferences,
    wind: WindModel,
    account: AccountModel,
    /** The maps kept for flying without a network, in a line. */
    offlineMaps: String,
    /** What the cockpit warns from, in a line: whose airspace, and the files. */
    airspace: String,
    /** Opens a screen on top of the settings. */
    onOpen: (SettingsRoute) -> Unit,
    bars: PaddingValues,
    /** Back to the Today tab, where the settings open from. */
    onBack: () -> Unit,
) {
    val links = LocalUriHandler.current
    var pilot by remember { mutableStateOf(preferences.pilot) }
    var glider by remember { mutableStateOf(preferences.glider) }
    val names = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false)

    Scaffold(
        modifier = Modifier.padding(bottom = bars.calculateBottomPadding()),
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
            )
        },
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
                    LinkRow("Waypoints", R.drawable.ic_chevron_right) { onOpen(SettingsRoute.Waypoints) }
                }
                row {
                    LinkRow(
                        "Airspace",
                        R.drawable.ic_chevron_right,
                        Modifier.testTag("airspace-link"),
                        value = airspace,
                    ) { onOpen(SettingsRoute.Airspace) }
                }
                row {
                    // The glider, the FAI triangle areas and the cockpit's
                    // other switches: on a page of their own.
                    LinkRow(
                        pagesWords().gliderAndCockpit,
                        R.drawable.ic_chevron_right,
                        Modifier.testTag("cockpit-settings-link"),
                    ) { onOpen(SettingsRoute.GliderAndCockpit) }
                }
            }

            SectionHeader("Venti")
            Group {
                row {
                    // The account is optional and belongs to the server.
                    LinkRow(
                        "Account",
                        R.drawable.ic_chevron_right,
                        Modifier.testTag("account-link"),
                        // A session the server ended says so, as on the iPhone and the web.
                        value = account.account?.let { if (account.needsSignIn) "Signed out" else "@${it.handle}" } ?: "Not signed in",
                    ) {
                        onOpen(SettingsRoute.Account)
                    }
                }
                row {
                    LinkRow(
                        OfflineMapsWords.TITLE,
                        R.drawable.ic_chevron_right,
                        Modifier.testTag("maps-link"),
                        value = offlineMaps,
                    ) {
                        onOpen(SettingsRoute.OfflineMaps)
                    }
                }
            }
            SectionFooter(SettingsWords.VENTI)

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
internal fun SwitchRow(title: String, isOn: Boolean, modifier: Modifier = Modifier, onChange: (Boolean) -> Unit) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .toggleable(value = isOn, role = Role.Switch, onValueChange = onChange)
            .heightIn(min = 56.dp)
            // Room above and below a title of two lines, which large text makes of a long one.
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Switch(checked = isOn, onCheckedChange = null)
    }
}

/** What the settings say that is not the core's to say. */
object SettingsWords {
    /** Under the account and the maps: what Venti is for the app. */
    const val VENTI =
        "The map's wind, sites and forecast come from Venti, and the assistant can look them up. " +
            "With an account your flights are kept there and can be shared; without one, your flights stay on this phone."
}

