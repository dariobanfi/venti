package fly.venti.app.sites

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.settings.Preferences
import fly.venti.app.today.PilotPosition
import fly.venti.app.waypoints.RowDivider
import fly.venti.core.KeptSite
import fly.venti.core.KeptSites
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/**
 * The pilot's own take-offs, all of them: the sites marked on their pages.
 * Which they are, their order, each one's distance and what is said under
 * the list are the core's, read from the phone alone; the screen lists
 * them and lets the pilot forget one.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun KeptSitesScreen(kept: KeptSitesModel, preferences: Preferences, onBack: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Distances are from where the pilot is, once Today may use the
    // position; nothing asks for it here. Without it the list is by name.
    val position = remember { PilotPosition(context.applicationContext) }
    val place = position.place.takeIf { preferences.todayNearPilot }
    var list by remember { mutableStateOf<KeptSites?>(null) }
    var forgetting by remember { mutableStateOf<KeptSite?>(null) }
    LaunchedEffect(Unit) {
        if (preferences.todayNearPilot) position.readIfAllowed()
    }
    LaunchedEffect(place, kept.changes) {
        list = kept.all(place)
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(KeptSitesWords.TITLE) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
            )
        },
    ) { padding ->
        val shown = list
        when {
            // Read from the phone in a moment: nothing to say in between.
            shown == null -> kept.problem?.let { problem ->
                Text(
                    problem,
                    color = Venti.colours.textSecondary,
                    modifier = Modifier.padding(padding).padding(VentiSpace.s4),
                )
            }
            shown.sites.isEmpty() -> Column(
                modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = VentiSpace.s6),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2, Alignment.CenterVertically),
            ) {
                Icon(
                    painterResource(R.drawable.venti_takeoff),
                    contentDescription = null,
                    modifier = Modifier.size(VentiSpace.s7),
                    tint = Venti.colours.textSecondary,
                )
                Text(KeptSitesWords.NONE, style = MaterialTheme.typography.headlineSmall, textAlign = TextAlign.Center)
                Text(
                    KeptSitesWords.HOW,
                    style = MaterialTheme.typography.bodyLarge,
                    color = Venti.colours.textSecondary,
                    textAlign = TextAlign.Center,
                )
            }
            else -> LazyColumn(
                modifier = Modifier.fillMaxSize().padding(padding).testTag("kept-sites"),
                contentPadding = PaddingValues(bottom = VentiSpace.s5),
            ) {
                items(shown.sites, key = { it.site.id }) { site ->
                    KeptRow(site, onForget = { forgetting = site })
                    RowDivider()
                }
                item {
                    SectionFooter(
                        KeptSitesWords.under(shown),
                        Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2).testTag("kept-sites-note"),
                    )
                }
            }
        }
    }

    forgetting?.let { site ->
        AlertDialog(
            onDismissRequest = { forgetting = null },
            title = { Text(KeptSitesWords.forgetTitle(site)) },
            text = { Text(KeptSitesWords.FORGET_TEXT) },
            confirmButton = {
                TextButton(
                    onClick = {
                        forgetting = null
                        scope.launch { kept.forget(site.site.id) }
                    },
                    modifier = Modifier.testTag("kept-forget-confirm"),
                ) {
                    Text(KeptSitesWords.FORGET, color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { forgetting = null }) { Text(Common.words.cancel) } },
        )
    }
}

/** One of the pilot's own sites in a line: its sign, its name, what is known of it, and the button that forgets it. */
@Composable
private fun KeptRow(kept: KeptSite, onForget: () -> Unit) {
    val detail = KeptSitesWords.detail(kept)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("kept-site")
            .heightIn(min = VentiSize.touch)
            .padding(start = VentiSpace.s4, end = VentiSpace.s1, top = VentiSpace.s2, bottom = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SiteGlyph(kept.site.kind)
        Spacer(Modifier.width(VentiSpace.s4))
        Column(
            // One element for a screen reader: the name, then the rest.
            Modifier.weight(1f).clearAndSetSemantics {
                contentDescription = kept.site.name
                stateDescription = detail
            },
        ) {
            Text(kept.site.name, style = MaterialTheme.typography.bodyLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(
                detail,
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(onClick = onForget, modifier = Modifier.testTag("kept-forget")) {
            Icon(
                painterResource(R.drawable.ic_delete),
                contentDescription = "${KeptSitesWords.FORGET} ${kept.site.name}",
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** What the list of the pilot's own take-offs says, apart from what the core has worded. */
object KeptSitesWords {
    /** The list's name, in the settings and over the list: the core's name for the group on Today. */
    const val TITLE = "Your take-offs"

    const val NONE = "No take-offs of your own yet"
    const val HOW = "Open a site on the map or on Today and choose “${SiteWords.KEEP_AS_OWN}”. Today then lists it first."

    const val FORGET = "Forget"
    const val FORGET_TEXT = "It will no longer be among your take-offs. The site stays on the map."

    fun forgetTitle(kept: KeptSite): String = "Forget ${kept.site.name}?"

    /**
     * Under a name: how far it is and which way, when the list was asked
     * for from a place; what the site is, its height and its wind; and the
     * date its source's data was of when the phone last saw it.
     */
    fun detail(kept: KeptSite): String {
        val parts = mutableListOf<String>()
        val distance = kept.distanceM
        val bearing = kept.bearingDeg
        if (distance != null && bearing != null) {
            parts += "${Format.kilometres(distance)} km ${Format.compassPoint(bearing)}"
        }
        parts += SiteWords.detail(kept.site)
        val line = parts.joinToString(" · ")
        return kept.asOfUnixS?.let { "$line\nAs of ${Format.day(it)}." } ?: line
    }

    /** Under the list: the core's note on where the list is kept, and whose data the sites are. */
    fun under(kept: KeptSites): String = (listOf(kept.note) + kept.credits.map { it.text }).joinToString("\n\n")
}
