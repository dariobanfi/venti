package fly.venti.app.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import fly.venti.app.R
import fly.venti.app.design.Group
import fly.venti.app.design.SectionHeader
import fly.venti.app.wind.WindModel

/**
 * Whose data and software the app uses, with the notices their licences
 * ask for.
 *
 * The map, the wind and the app itself are credited in the same words as
 * on the iPhone. The libraries are this app's own: what an Android app is
 * built from differs.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CreditsScreen(wind: WindModel, onBack: () -> Unit) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Data and licences") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 24.dp),
        ) {
            SectionHeader("Map", Modifier.padding(top = 0.dp))
            Group {
                row {
                    Credit(
                        "Base map",
                        "© OpenStreetMap contributors, under the Open Database License. Tiles from " +
                            "OpenFreeMap in the OpenMapTiles schema, © OpenMapTiles.",
                        link = "https://www.openstreetmap.org/copyright",
                    )
                }
                row {
                    Credit(
                        "Terrain",
                        "Mapterhorn, built from the open elevation data of many survey agencies. The " +
                            "full list is on its attribution page.",
                        link = "https://mapterhorn.com/attribution",
                    )
                }
                row {
                    Credit(
                        "Labels",
                        "Noto Sans, under the SIL Open Font License.",
                        link = "https://openfontlicense.org",
                    )
                }
                row {
                    Credit(
                        "Map engine",
                        "MapLibre Native, under the BSD 2-Clause License.",
                        link = "https://maplibre.org",
                    )
                }
            }

            SectionHeader("Wind")
            Group {
                // Whose stations the server delivers is the server's to
                // say: the credits come with its answers.
                if (wind.credits.isEmpty()) {
                    row {
                        Text(
                            "Wind stations come from the Venti server set in the settings. Each " +
                                "station's page names whose data it is and under which licence.",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                        )
                    }
                } else {
                    for (source in wind.credits) {
                        row { Credit(source.provider, "${source.text}. Under ${source.licence}.", link = source.url) }
                    }
                }
            }

            SectionHeader("Venti")
            Group {
                row {
                    Credit(
                        "This app",
                        "Free software under the GNU Affero General Public License, version 3 or " +
                            "later. You may use, study, share and change it.",
                        link = "https://www.gnu.org/licenses/agpl-3.0.html",
                    )
                }
            }

            SectionHeader("Libraries")
            Group {
                row {
                    Credit(
                        "Interface",
                        "Jetpack Compose and AndroidX, Kotlin and its libraries, the Material Symbols " +
                            "icons, JNA, and OkHttp, Gson and Timber inside the map engine, under the " +
                            "Apache License 2.0.",
                        link = "https://www.apache.org/licenses/LICENSE-2.0",
                    )
                }
                row {
                    Credit(
                        "Bridge to the core",
                        "UniFFI, under the Mozilla Public License 2.0.",
                        link = "https://www.mozilla.org/MPL/2.0/",
                    )
                }
            }
        }
    }
}

@Composable
private fun Credit(title: String, text: String, link: String) {
    val links = LocalUriHandler.current
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 2.dp),
        )
        Text(
            link.toUri().host ?: link,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier
                .padding(top = 2.dp)
                .clickable(role = Role.Button) { links.openUri(link) }
                .padding(vertical = 6.dp),
        )
    }
}
