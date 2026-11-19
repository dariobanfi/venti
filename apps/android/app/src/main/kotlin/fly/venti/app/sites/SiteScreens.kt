package fly.venti.app.sites

import androidx.annotation.DrawableRes
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.core.SiteView
import fly.venti.core.WindSpan

/**
 * The sheet over the map for flying sites: the sites in view as a list, or
 * one site's page. A site tapped on the map opens its page alone; one
 * chosen from the list leads back to the list.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SiteSheet(
    sites: SitesModel,
    waypoints: WaypointsModel,
    /** Whether the list is what the pilot opened. */
    showsList: Boolean,
    /** The site whose page shows, if any. */
    chosen: SiteView?,
    onChoose: (SiteView?) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        // The list opens at full height, for the search field and the
        // names; a site tapped on the map leaves the map in view.
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = showsList),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        if (chosen != null) {
            SitePage(
                site = chosen,
                sites = sites,
                waypoints = waypoints,
                onBack = if (showsList) ({ onChoose(null) }) else null,
                onDone = onDismiss,
            )
        } else {
            SiteList(sites, onOpen = onChoose, onDone = onDismiss)
        }
    }
}

/**
 * The flying sites around the part of the map in view, as a list: for
 * looking one up by name, and for pilots who use TalkBack, which cannot
 * reach symbols drawn on a map.
 */
@Composable
private fun SiteList(sites: SitesModel, onOpen: (SiteView) -> Unit, onDone: () -> Unit) {
    var search by rememberSaveable { mutableStateOf("") }
    val wanted = search.trim()
    val shown = remember(sites.sites, wanted) { SiteWords.listed(sites.sites, wanted) }

    Column {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Flying sites", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDone) { Text("Done") }
        }
        if (sites.sites.isEmpty()) {
            EmptyList(
                "No flying sites here",
                sites.problem ?: "Zoom the map in on an area, and the sites its source knows there are listed.",
            )
        } else {
            OutlinedTextField(
                value = search,
                onValueChange = { search = it },
                placeholder = { Text("Site") },
                leadingIcon = { Icon(painterResource(R.drawable.ic_search), contentDescription = null) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Words,
                    autoCorrectEnabled = false,
                ),
                shape = MaterialTheme.shapes.extraLarge,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            )
            if (shown.isEmpty()) {
                EmptyList(
                    "No site of that name",
                    "None of the sites on this part of the map is called “$wanted”.",
                )
            } else {
                LazyColumn(modifier = Modifier.testTag("site-list"), contentPadding = PaddingValues(bottom = 32.dp)) {
                    items(shown, key = { it.id }) { site ->
                        SiteRow(site, onClick = { onOpen(site) })
                        HorizontalDivider(
                            modifier = Modifier.padding(start = 64.dp),
                            color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f),
                        )
                    }
                    item {
                        SectionFooter(
                            SiteWords.underTheList(sites),
                            Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                        )
                    }
                }
            }
        }
    }
}

/** Said in place of the list when it would be empty. */
@Composable
private fun EmptyList(title: String, text: String) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            painterResource(R.drawable.ic_location_off),
            contentDescription = null,
            modifier = Modifier.size(40.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(title, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
        Text(
            text,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

/** One site in a line: its sign, its name, and what else is known of it. */
@Composable
private fun SiteRow(site: SiteView, onClick: () -> Unit) {
    val detail = SiteWords.detail(site)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("site-row")
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 64.dp)
            .padding(horizontal = 20.dp, vertical = 10.dp)
            // One element for a screen reader: the name, then the rest.
            .clearAndSetSemantics {
                contentDescription = site.name
                stateDescription = detail
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SiteGlyph(site.kind)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(site.name, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                detail,
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/**
 * One flying site: what its source says about it, what that is worth, and
 * a way to keep it as a waypoint. Shown in a sheet over the map.
 */
@Composable
private fun SitePage(
    site: SiteView,
    sites: SitesModel,
    waypoints: WaypointsModel,
    /** Back to the list of sites, if that is where the pilot came from. */
    onBack: (() -> Unit)?,
    onDone: () -> Unit,
) {
    val links = LocalUriHandler.current
    var isKept by remember(site.id) { mutableStateOf(false) }

    Column {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = if (onBack != null) 4.dp else 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                IconButton(onClick = onBack) {
                    Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                }
            }
            Text(
                site.name,
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onDone) { Text("Done") }
        }
        LazyColumn(
            modifier = Modifier.testTag("site-page"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            item {
                Group {
                    row { ValueRow("What it is", site.kindInWords) }
                    site.elevationM?.let { elevation ->
                        row { ValueRow("Height", "${Format.metres(elevation)} m") }
                    }
                    row { WindRow(site) }
                }
            }
            if (site.official == true || site.discouraged || site.paragliding == false) {
                item {
                    Group {
                        if (site.official == true) {
                            row { MarkRow(R.drawable.ic_verified, "Marked as an official site") }
                        }
                        if (site.discouraged) {
                            row {
                                MarkRow(
                                    R.drawable.ic_warning,
                                    "Marked as unsafe or discouraged",
                                    MaterialTheme.colorScheme.error,
                                )
                            }
                        }
                        if (site.paragliding == false) {
                            row { MarkRow(R.drawable.ic_block, "Marked as not for paragliders") }
                        }
                    }
                }
            }
            item {
                Group {
                    row {
                        KeepRow(isKept) {
                            isKept = waypoints.add(site.name, site.position, site.elevationM)
                        }
                    }
                }
                SectionFooter("As a waypoint the cockpit can navigate to it, also without a network.")
            }
            item {
                site.url?.let { address ->
                    Group {
                        row {
                            LinkRow("See or correct it at the source", R.drawable.ic_open_in_new) {
                                links.openUri(address)
                            }
                        }
                    }
                }
                SectionFooter(SiteWords.footnote(site, sites))
            }
        }
    }
}

/** The wind a site works in: in words and as wedges on a small compass, or that the source does not say. */
@Composable
private fun WindRow(site: SiteView) {
    Row(
        modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("Wind it works in", style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Spacer(Modifier.width(12.dp))
        Text(
            site.windInWords.ifEmpty { "Not given" },
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.End,
        )
        if (site.wind.isNotEmpty()) {
            Spacer(Modifier.width(10.dp))
            WindRose(site.wind)
        }
    }
}

/** Something the source marks the site as. */
@Composable
private fun MarkRow(@DrawableRes icon: Int, text: String, colour: Color = MaterialTheme.colorScheme.onSurface) {
    Row(
        modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = colour)
        Spacer(Modifier.width(16.dp))
        Text(text, style = MaterialTheme.typography.bodyLarge, color = colour)
    }
}

/** The button that keeps the site as a waypoint, and says so once it has. */
@Composable
private fun KeepRow(isKept: Boolean, onKeep: () -> Unit) {
    val colour = if (isKept) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.primary
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("site-keep")
            .clickable(enabled = !isKept, role = Role.Button, onClick = onKeep)
            .heightIn(min = 52.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(if (isKept) R.drawable.ic_check else R.drawable.ic_add),
            contentDescription = null,
            tint = colour,
        )
        Spacer(Modifier.width(16.dp))
        Text(
            if (isKept) "In your waypoints" else "Add to my waypoints",
            style = MaterialTheme.typography.bodyLarge,
            color = colour,
        )
    }
}

/**
 * The wind directions a site works in, as wedges on a small compass: north
 * at the top, each wedge where the wind may come from.
 */
@Composable
fun WindRose(spans: List<WindSpan>, modifier: Modifier = Modifier) {
    val ring = MaterialTheme.colorScheme.outline
    val wedge = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.75f)
    Canvas(modifier.size(30.dp)) {
        val radius = size.minDimension / 2 - 1.dp.toPx()
        val corner = Offset(center.x - radius, center.y - radius)
        val box = Size(2 * radius, 2 * radius)
        for (span in spans) {
            val (start, sweep) = SiteWords.wedge(span)
            drawArc(wedge, start, sweep, useCenter = true, topLeft = corner, size = box)
        }
        drawCircle(ring, radius, style = Stroke(1.dp.toPx()))
    }
}

/** What the sites' screens say, apart from what the core has worded. */
object SiteWords {
    /** What a site on the map is worth, said wherever sites are listed. */
    const val CAUTION =
        "A site on the map is not permission to fly there. Ask the club or the owner, and look at the place."

    /** Those with a name first, by name; then the rest by what they are. */
    fun listed(sites: List<SiteView>, wanted: String): List<SiteView> =
        sites
            .filter { wanted.isEmpty() || it.name.contains(wanted, ignoreCase = true) }
            .sortedWith(compareBy<SiteView> { !it.isNamed }.thenBy(String.CASE_INSENSITIVE_ORDER) { it.name })

    /** What a row says under the name: what it is, how high, and the wind it works in. */
    fun detail(site: SiteView): String {
        val parts = mutableListOf<String>()
        // Without a name of its own, the name already says what it is.
        if (site.isNamed) parts += site.kindInWords
        site.elevationM?.let { parts += "${Format.metres(it)} m" }
        if (site.windInWords.isNotEmpty()) parts += "wind ${site.windInWords}"
        return if (parts.isEmpty()) "No more is known" else parts.joinToString(" · ")
    }

    /** Under the list: what a site on a map is worth, whose data it is, and how old. */
    fun underTheList(sites: SitesModel): String =
        (listOf(CAUTION) + sites.credits.map { it.text } + listOfNotNull(sites.asOfInWords)).joinToString("\n")

    /**
     * Under a site's page: what a site on a map is worth, the notice and
     * the credit of the site's source, and how old its data is.
     */
    fun footnote(site: SiteView, sites: SitesModel): String {
        val credit = sites.credits.firstOrNull { it.source == site.source }
        return listOfNotNull(CAUTION, credit?.notice, credit?.text, sites.asOfInWords).joinToString("\n\n")
    }

    /**
     * Where a span's wedge starts and how far it sweeps, in degrees as a
     * canvas measures them: from east, clockwise. Compass degrees run
     * clockwise from north, and a span may run across north. A single
     * direction is drawn as the sixteenth of the compass it names.
     */
    fun wedge(span: WindSpan): Pair<Float, Float> {
        val turned = span.toDeg - span.fromDeg
        val width = if (turned >= 360) 360.0 else (turned % 360 + 360) % 360
        val (from, sweep) = if (width == 0.0) span.fromDeg - 11.25 to 22.5 else span.fromDeg to width
        return (from - 90).toFloat() to sweep.toFloat()
    }
}
