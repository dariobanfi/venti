package fly.venti.app.settings

import fly.venti.core.gliderClassName
import fly.venti.core.gliderWords
import fly.venti.app.maps.OfflineMapsWords
import fly.venti.app.sites.KeptSitesWords
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
import fly.venti.app.fly.FlySession
import fly.venti.app.wind.WindModel
import fly.venti.core.mapLayersWords
import fly.venti.core.GliderClassKind

/** The pilot's settings, and what the app is made of. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    preferences: Preferences,
    fly: FlySession,
    wind: WindModel,
    account: AccountModel,
    /** The maps kept for flying without a network, in a line. */
    offlineMaps: String,
    /** Opens a screen on top of the settings. */
    onOpen: (SettingsRoute) -> Unit,
    bars: PaddingValues,
    /** Back to the Today tab, where the settings open from. */
    onBack: () -> Unit,
) {
    val links = LocalUriHandler.current
    var pilot by remember { mutableStateOf(preferences.pilot) }
    var glider by remember { mutableStateOf(preferences.glider) }
    var trimSpeed by remember { mutableStateOf(preferences.trimSpeed) }
    var glideRatio by remember { mutableStateOf(preferences.glideRatio) }
    var showsFaiAreas by remember { mutableStateOf(preferences.showsFaiAreas) }
    var showsClimbs by remember { mutableStateOf(preferences.cockpitShowsClimbs) }
    var gliderClass by remember { mutableStateOf(GliderWords.kind(preferences.gliderClass)) }
    var neverLand by remember { mutableStateOf(preferences.neverLand) }
    val names = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false)

    Scaffold(
        modifier = Modifier.padding(bottom = bars.calculateBottomPadding()),
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
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
                    SwitchRow("Vario sound", isOn = !fly.isMuted) { fly.isMuted = !it }
                }
                row {
                    LinkRow(KeptSitesWords.TITLE, R.drawable.ic_chevron_right, Modifier.testTag("kept-sites-link")) {
                        onOpen(SettingsRoute.KeptSites)
                    }
                }
                row {
                    LinkRow("Waypoints", R.drawable.ic_chevron_right) { onOpen(SettingsRoute.Waypoints) }
                }
                row {
                    LinkRow("Airspace", R.drawable.ic_chevron_right) { onOpen(SettingsRoute.Airspace) }
                }
                row {
                    SwitchRow("FAI triangle areas", isOn = showsFaiAreas, modifier = Modifier.testTag("fai-areas")) { isOn ->
                        showsFaiAreas = isOn
                        preferences.showsFaiAreas = isOn
                    }
                }
                row {
                    SwitchRow(mapLayersWords().cockpitClimbs, isOn = showsClimbs, modifier = Modifier.testTag("cockpit-climbs")) { isOn ->
                        showsClimbs = isOn
                        preferences.cockpitShowsClimbs = isOn
                    }
                }
            }
            SectionFooter(
                "The FAI triangle areas are drawn on the cockpit's map once you have flown three kilometres " +
                    "in a straight line: a turnpoint inside either of them makes that line one side of an " +
                    "FAI triangle.",
            )
            SectionFooter(mapLayersWords().cockpitClimbsNote)

            SectionHeader(GliderWords.core.title)
            Group {
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
                row {
                    GliderClassRow(gliderClass) { chosen ->
                        gliderClass = chosen
                        preferences.gliderClass = GliderWords.kept(chosen)
                    }
                }
                row {
                    SwitchRow(GliderWords.core.neverLand, isOn = neverLand, modifier = Modifier.testTag("never-land")) { isOn ->
                        neverLand = isOn
                        preferences.neverLand = isOn
                    }
                }
            }
            SectionFooter("${GliderWords.SPEED_AND_GLIDE} ${GliderWords.core.footer}")

            SectionHeader("Venti")
            Group {
                row {
                    // The account is optional and belongs to the server.
                    LinkRow(
                        "Account",
                        R.drawable.ic_chevron_right,
                        Modifier.testTag("account-link"),
                        value = account.account?.let { "@${it.handle}" } ?: "Not signed in",
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
private fun SwitchRow(title: String, isOn: Boolean, modifier: Modifier = Modifier, onChange: (Boolean) -> Unit) {
    Row(
        modifier = modifier
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

/** The words of the settings' "Glider" section, and how the glider's class is kept. */
object GliderWords {
    /** The core's words of the section: its heading, the class's row, "Never land" and what the two do. */
    val core: fly.venti.core.GliderWords by lazy { gliderWords() }

    /** Under the section, before the core's lines: what the speed and the glide ratio, the app's own two rows, are for. */
    const val SPEED_AND_GLIDE =
        "Your glider's speed with the brakes up and its glide ratio. From them and the wind, the cockpit " +
            "predicts how high you arrive at the place you fly to. The prediction assumes air that neither " +
            "rises nor sinks on the way."

    /** The classes the menu offers, `null` first for none. */
    val CLASSES: List<GliderClassKind?> = listOf(null) + GliderClassKind.entries

    /** A class as the menu names it, in the core's words: "None", "EN A", "CCC". */
    fun name(kind: GliderClassKind?): String = gliderClassName(kind)

    /** The class as the preferences keep it, or `null` for none. */
    fun kept(kind: GliderClassKind?): String? = kind?.name

    /** The class the preferences kept; none for a word they never wrote. */
    fun kind(kept: String?): GliderClassKind? = GliderClassKind.entries.firstOrNull { it.name == kept }
}

/** The glider's class, as a menu: none until the pilot chooses one. */
@Composable
private fun GliderClassRow(chosen: GliderClassKind?, onChoose: (GliderClassKind?) -> Unit) {
    var isOpen by remember { mutableStateOf(false) }
    Box {
        LinkRow(
            GliderWords.core.classTitle,
            R.drawable.ic_arrow_drop_down,
            Modifier.testTag("glider-class"),
            value = GliderWords.name(chosen),
        ) { isOpen = true }
        DropdownMenu(expanded = isOpen, onDismissRequest = { isOpen = false }) {
            for (kind in GliderWords.CLASSES) {
                DropdownMenuItem(
                    text = { Text(GliderWords.name(kind)) },
                    onClick = {
                        isOpen = false
                        onChoose(kind)
                    },
                    trailingIcon = if (kind == chosen) {
                        { Icon(painterResource(R.drawable.ic_check), contentDescription = "Chosen") }
                    } else {
                        null
                    },
                )
            }
        }
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

/** What the settings say that is not the core's to say. */
object SettingsWords {
    /** Under the account and the maps: what Venti is for the app. */
    const val VENTI =
        "The map's wind, sites and forecast come from Venti, and the assistant can look them up. " +
            "With an account your flights are kept there and can be shared; without one, your flights stay on this phone."
}

