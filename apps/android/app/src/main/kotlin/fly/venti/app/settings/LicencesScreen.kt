package fly.venti.app.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.core.net.toUri
import app.venti.design.VentiSpace
import fly.venti.app.AssetFiles
import fly.venti.app.R
import fly.venti.app.design.Common
import fly.venti.app.design.DisclosureRow
import fly.venti.app.design.Group
import fly.venti.app.design.SectionHeader

/**
 * "Open-source licences", a page of "Data and licences": the notices the
 * licences of the software inside the app ask to go with it. Three parts,
 * each folded until opened, so that the short ones are not below the
 * core's long list: the core's crates (the generated file, as it is), the
 * map engine with its licence and copyright lines, and the app's
 * libraries with the full text of each licence once.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LicencesScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val texts = remember {
        val files = AssetFiles(context)
        fun read(path: String) = runCatching { LicenceText.paragraphs(files.text(path)) }.getOrDefault(emptyList())
        LicenceTexts(
            core = read(OpenSourceLicences.CORE_NOTICES),
            mapLibre = read(OpenSourceLicences.MAPLIBRE_TEXT),
            apache = read(OpenSourceLicences.APACHE_TEXT),
            bsd3 = read(OpenSourceLicences.BSD_3_TEXT),
        )
    }
    var coreOpen by rememberSaveable { mutableStateOf(false) }
    var mapOpen by rememberSaveable { mutableStateOf(false) }
    var librariesOpen by rememberSaveable { mutableStateOf(false) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(LicenceWords.TITLE) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            Modifier.fillMaxSize().padding(padding).testTag("licences"),
            contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s5),
        ) {
            part(LicenceWords.CORE, "licences-core", coreOpen) { coreOpen = it }
            if (coreOpen) paragraphs(texts.core)

            part(LicenceWords.MAP_ENGINE, "licences-maplibre", mapOpen) { mapOpen = it }
            if (mapOpen) {
                item { LibraryLine(OpenSourceLicences.mapLibre) }
                paragraphs(texts.mapLibre)
            }

            part(LicenceWords.LIBRARIES, "licences-libraries", librariesOpen) { librariesOpen = it }
            if (librariesOpen) {
                items(OpenSourceLicences.libraries) { library -> LibraryLine(library) }
                item { SectionHeader(LicenceWords.APACHE) }
                paragraphs(texts.apache)
                item { SectionHeader(LicenceWords.BSD_3) }
                paragraphs(texts.bsd3)
                item {
                    SectionHeader(LicenceWords.SDK_LICENSE)
                    Link(OpenSourceLicences.SDK_LICENSE_LINK)
                }
            }
        }
    }
}

private class LicenceTexts(
    val core: List<LicenceParagraph>,
    val mapLibre: List<LicenceParagraph>,
    val apache: List<LicenceParagraph>,
    val bsd3: List<LicenceParagraph>,
)

/** A part's row, which opens and folds it. */
private fun LazyListScope.part(title: String, tag: String, isOpen: Boolean, onChange: (Boolean) -> Unit) {
    item(key = tag) {
        Group(Modifier.padding(top = VentiSpace.s4)) {
            row { DisclosureRow(title, isOpen, Modifier.testTag(tag), onChange) }
        }
    }
}

/** A licence file, a paragraph to an item: the core's is long. */
private fun LazyListScope.paragraphs(paragraphs: List<LicenceParagraph>) {
    items(paragraphs) { paragraph ->
        if (paragraph.isHeading) {
            Text(
                paragraph.text,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = VentiSpace.s4, bottom = VentiSpace.s1)
                    .semantics { heading() },
            )
        } else {
            Text(
                paragraph.text,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.fillMaxWidth().padding(vertical = VentiSpace.s2),
            )
        }
    }
}

/** One library: its name, and under it the version and the licence. */
@Composable
private fun LibraryLine(library: OpenSourceLicences.Library) {
    Column(Modifier.fillMaxWidth().padding(vertical = VentiSpace.s2).testTag("licences-library")) {
        Text(library.name, style = MaterialTheme.typography.bodyLarge)
        Text(
            library.line,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun Link(link: String) {
    val opener = LocalUriHandler.current
    val host = link.toUri().host ?: return
    Text(
        host,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier
            .clickable(role = Role.Button) { opener.openUri(link) }
            .padding(vertical = VentiSpace.s2),
    )
}
