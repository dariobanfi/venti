package fly.venti.app.live

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.community.ReportSheet
import fly.venti.app.community.Reported
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.core.LiveFollowView
import fly.venti.core.LiveGlider
import kotlinx.coroutines.CoroutineScope

/** What the list and a glider's page say apart from their numbers. */
object GliderWords {
    const val CAUTION =
        "Positions are as each pilot's app sent them. A glider nothing was heard of for a while has most " +
            "likely no network: it is not known to have landed."

    const val MORE = "There are more than the server lists at once. Zoom the map in. "

    const val GONE =
        "This glider is no longer on this part of the map: its pilot stopped showing the flight, or it has " +
            "left the area."

    /** "1,596 m · −1.0 m/s · Now" */
    fun detail(glider: LiveGlider, age: String): String = buildList {
        add("${Format.metres(glider.altitudeM.toDouble())} m")
        val climb = glider.climbMs
        if (!glider.landed && climb != null) {
            add("${Format.climb(climb)} m/s")
        }
        add(age)
    }.joinToString(" · ")

    /** What stands under the list. */
    fun underTheList(isTruncated: Boolean): String = if (isTruncated) MORE + CAUTION else CAUTION

    /** A glider's page is named by its pilot's handle. */
    fun title(glider: LiveGlider?): String = glider?.let { "@${it.handle}" } ?: "Glider"
}

/**
 * The gliders on the map as a list: for reading them side by side, and for
 * pilots who use TalkBack. Choosing one closes the list and opens its page
 * over the map, where its flight is drawn.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GliderListSheet(live: LiveModel, onChoose: (String) -> Unit, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("In the air", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text("Done") }
        }
        val gliders = live.gliders
        if (gliders.isEmpty()) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(
                    painterResource(R.drawable.ic_paragliding),
                    contentDescription = null,
                    modifier = Modifier.size(40.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text("Nobody in the air here", style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
                Text(
                    "Pilots who show their flight to everyone appear on this part of the map while they fly.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        } else {
            LazyColumn(modifier = Modifier.testTag("live-list"), contentPadding = PaddingValues(bottom = 32.dp)) {
                items(gliders, key = { it.id }) { glider ->
                    GliderRow(glider, age = live.ageInWords(glider), onClick = { onChoose(glider.id) })
                    HorizontalDivider(
                        modifier = Modifier.padding(start = 64.dp),
                        color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f),
                    )
                }
                item {
                    SectionFooter(
                        GliderWords.underTheList(live.isTruncated),
                        Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    )
                }
            }
        }
    }
}

/** One glider in the list: the pilot, and how high, how it climbs and how old the position is. */
@Composable
private fun GliderRow(glider: LiveGlider, age: String, onClick: () -> Unit) {
    val detail = GliderWords.detail(glider, age)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("live-row")
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 64.dp)
            .padding(horizontal = 20.dp, vertical = 10.dp)
            // One element for a screen reader: the pilot, then the rest.
            .clearAndSetSemantics {
                contentDescription = glider.name
                stateDescription = detail
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(if (glider.landed) R.drawable.ic_man else R.drawable.ic_paragliding),
            contentDescription = null,
            // The map's own green for one that flies, in both appearances.
            tint = if (glider.landed || glider.quiet) {
                MaterialTheme.colorScheme.onSurfaceVariant
            } else {
                Color(0xFF000000 or LiveSymbols.GREEN.toLong())
            },
        )
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(glider.name, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
 * One glider's page, at half height over the map: who flies it, where it
 * is and how old that is, and the way to its track on the server's page.
 * While it is open the flight is asked for, and the map behind it draws it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GliderSheet(
    /** The track's identifier. The page shows the glider as the newest answer has it. */
    id: String,
    live: LiveModel,
    /** The pilot's own account, which a report is sent from. */
    account: AccountModel,
    /** The address of the server the gliders come from, for the link to its page. */
    server: String?,
    /** Where a report is sent from, so that it goes on when its form is closed. */
    requests: CoroutineScope,
    onDismiss: () -> Unit,
) {
    // The flight's track is asked for while its page is open, and no longer.
    LaunchedEffect(id) {
        live.follow(id)
    }
    var reporting by remember { mutableStateOf<Reported?>(null) }
    val glider = live.glider(id)
    val flight = live.followed?.takeIf { it.glider.id == id }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        // As tall as the sheet can be, so that it stops at half height and
        // the flight shows above it, however few rows there are.
        Box(Modifier.fillMaxHeight()) {
            Column {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        GliderWords.title(glider),
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = onDismiss) { Text("Done") }
                }
                Column(
                    Modifier
                        .testTag("live-glider")
                        .verticalScroll(rememberScrollState())
                        .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 32.dp),
                ) {
                    if (glider != null) {
                        GliderPage(
                            glider,
                            age = live.ageInWords(glider),
                            flight,
                            server,
                            onReport = { reporting = Reported.liveTrack(glider.id) },
                        )
                    } else {
                        Text(
                            live.followProblem ?: GliderWords.GONE,
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(horizontal = 4.dp, vertical = 16.dp),
                        )
                    }
                }
            }
        }
    }

    reporting?.let { reported ->
        ReportSheet(reported, account, server, requests, onDismiss = { reporting = null })
    }
}

@Composable
private fun GliderPage(glider: LiveGlider, age: String, flight: LiveFollowView?, server: String?, onReport: () -> Unit) {
    val links = LocalUriHandler.current
    Group {
        row { ValueRow("Pilot", glider.name) }
        // Read as one by a screen reader, and found as one by a test.
        row { ValueRow("Position", age, Modifier.testTag("live-age").semantics(mergeDescendants = true) {}) }
        row { ValueRow("Altitude", "${Format.metres(glider.altitudeM.toDouble())} m") }
        if (!glider.landed) {
            glider.climbMs?.let { climb -> row { ValueRow("Climb", "${Format.climb(climb)} m/s") } }
            glider.groundSpeedMs?.let { speed -> row { ValueRow("Speed", "${Format.kilometresPerHour(speed)} km/h") } }
            glider.courseDeg?.let { course -> row { ValueRow("Flying", Format.compassPoint(course)) } }
        }
        // The flight so far, whose line is on the map.
        if (flight != null) {
            row {
                ValueRow(
                    "Shown for",
                    Format.stopwatch(flight.durationS),
                    Modifier.testTag("live-duration").semantics(mergeDescendants = true) {},
                )
            }
            row { ValueRow("Highest", "${Format.metres(flight.highestM.toDouble())} m") }
        }
    }
    SectionFooter(GliderWords.CAUTION)
    if (server != null) {
        Group(Modifier.padding(top = 16.dp)) {
            row {
                LinkRow("Follow the flight in the browser", R.drawable.ic_open_in_new) {
                    links.openUri(server + glider.pagePath)
                }
            }
        }
        SectionFooter("The server's page shows the whole track and keeps itself up to date.")
    }
    // For a track that should not be here: to whoever runs the server,
    // and to nobody else.
    Group(Modifier.padding(top = 16.dp)) {
        row {
            LinkRow("Report this track", R.drawable.ic_flag, Modifier.testTag("report-track"), onClick = onReport)
        }
    }
}
