package fly.venti.app.sites

import kotlinx.coroutines.CancellationException
import fly.venti.core.SiteKind
import fly.venti.core.PublicFlight
import fly.venti.core.SiteFlights
import fly.venti.app.community.CommunityModel
import androidx.annotation.DrawableRes
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.runtime.key
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
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
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.design.StatusChip
import fly.venti.app.design.Venti
import fly.venti.app.design.status
import fly.venti.core.OutlookDay
import fly.venti.core.SiteOutlook
import fly.venti.app.R
import fly.venti.app.alerts.AlertMeRow
import fly.venti.app.alerts.AlertsModel
import fly.venti.app.alerts.LocalAlerts
import fly.venti.app.today.AirChart
import fly.venti.app.today.NearbyLine
import fly.venti.app.today.ThermalLine
import fly.venti.app.design.DisclosureRow
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.app.forecast.ForecastPage
import fly.venti.app.forecast.ForecastPlace
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.wind.StationPage
import fly.venti.app.wind.WindGlyph
import fly.venti.app.wind.WindModel
import fly.venti.core.Credit
import fly.venti.core.SiteConfirmations
import fly.venti.core.SiteView
import fly.venti.core.SiteField
import fly.venti.core.SitePage as SiteAsItStands
import fly.venti.core.SiteWritten
import fly.venti.app.account.AccountModel
import fly.venti.app.community.ReportSheet
import fly.venti.app.community.Reported
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import fly.venti.core.StationNearby
import fly.venti.core.WindSpan
import fly.venti.core.WindStation
import fly.venti.core.stationsNear
import kotlin.math.abs
import kotlin.math.roundToLong

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
    /** The pilot's own take-offs: a site's page marks it as one and unmarks it. */
    own: KeptSitesModel,
    /** Where a site's forecast is asked for. */
    wind: WindModel,
    /** Whether the list is what the pilot opened. */
    showsList: Boolean,
    /** The site whose page shows, if any. */
    chosen: SiteView?,
    onChoose: (SiteView?) -> Unit,
    onDismiss: () -> Unit,
    /** Opens the filter of the sites from their list; `null` where the list has none. */
    onFilter: (() -> Unit)? = null,
    /** Everyone's flights on Venti, for what was flown from a site; `null` for none. */
    community: CommunityModel? = null,
    /** Opens a flight flown from the site, on its own page. */
    onOpenFlight: (PublicFlight) -> Unit = {},
    /** What the site's page leads to beyond itself; `null` where it leads nowhere. */
    actions: SiteActions? = null,
    /**
     * Whose data a site is and of which date, where the page was not opened
     * from the sites on the map: Today's, which has the credit a take-off
     * of the pilot's own was kept with.
     */
    known: SiteSources = SiteSources(),
    /** What the core offers for a site of Venti's own layers, and where each leads; `null` where the page offers none of it. */
    editing: SiteEditing? = null,
    /**
     * The day the page speaks of, by its first moment: the day chosen on
     * Today. Without one, and where the forecast has no such day, the first
     * day with flying hours.
     */
    dayStartUnixS: Long? = null,
) {
    // The site as the server has it now, where it is of Venti's own
    // layers: the map's answer may be an hour old.
    var page by remember(chosen?.id) { mutableStateOf<SiteAsItStands?>(null) }
    var pageProblem by remember(chosen?.id) { mutableStateOf<String?>(null) }
    // Who says they took off there: the core finds the pilot's own among them.
    var confirmations by remember(chosen?.id) { mutableStateOf<SiteConfirmations?>(null) }
    var confirmationsProblem by remember(chosen?.id) { mutableStateOf<String?>(null) }
    // Counts up when both are to be read again: after a confirmation and after one was taken back.
    var readAgain by remember { mutableIntStateOf(0) }
    LaunchedEffect(chosen?.id, chosen?.byPilots?.revision, editing?.model, readAgain) {
        val site = chosen ?: return@LaunchedEffect
        val model = editing?.model ?: return@LaunchedEffect
        if (site.byPilots == null) return@LaunchedEffect
        try {
            page = model.page(site.id)
            pageProblem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            page = null
            pageProblem = model.words(error)
        }
        try {
            confirmations = model.confirmations(site)
            confirmationsProblem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            confirmations = null
            confirmationsProblem = model.words(error)
        }
    }
    // Whether the pilot is being asked why the site is to be removed.
    var isRemoving by remember { mutableStateOf(false) }
    // Whether the form that says "I took off here and it was good" is open.
    var isConfirming by remember { mutableStateOf(false) }
    // Whether the pilot is being asked if their confirmation is to be taken back.
    var isWithdrawing by remember { mutableStateOf(false) }
    // A site or a revision being reported, with its form open.
    var reporting by remember { mutableStateOf<Reported?>(null) }
    // What the pilot has opened from the site's page, the newest last: a
    // wind station near the site, a forecast. Back takes the last away.
    var opened by remember { mutableStateOf(emptyList<FromASite>()) }
    // The sites kept as waypoints while this sheet is open: a page says so
    // also when the pilot comes back to it from the forecast.
    var kept by remember { mutableStateOf(emptySet<String>()) }
    // The list opens at full height, for the search field and the names; a
    // site tapped on the map leaves the map in view.
    // Keyed by which it is: a sheet state kept for the page at half
    // height must not come back for the list, which has no half height.
    val sheet = key(showsList) { rememberModalBottomSheetState(skipPartiallyExpanded = showsList) }
    // A forecast is a table of a whole day, and a station has its charts:
    // they take all the room there is.
    LaunchedEffect(opened.size) {
        if (opened.isNotEmpty()) sheet.expand()
    }
    val back = { opened = opened.dropLast(1) }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheet,
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        when (val top = opened.lastOrNull()) {
            is FromASite.Forecast -> ForecastPage(top.place, wind, onBack = back, onDone = onDismiss)
            is FromASite.Station -> StationPage(
                // The newest reading, if the stations were fetched again
                // while the page was open.
                station = wind.stations.firstOrNull { it.id == top.station.id } ?: top.station,
                wind = wind,
                onForecast = {
                    opened = opened + FromASite.Forecast(ForecastPlace(top.station.name, top.station.position))
                },
                onBack = back,
                onDone = onDismiss,
            )
            is FromASite.Confirmations -> if (editing != null) {
                SiteConfirmationsPage(
                    name = top.site.name,
                    says = editing.model.says,
                    confirmations = confirmations?.takeIf { it.siteId == top.site.id },
                    problem = confirmationsProblem,
                    onTryAgain = { readAgain += 1 },
                    onBack = back,
                    onDone = onDismiss,
                )
            }
            is FromASite.History -> if (editing != null) {
                // By its identifier: what the history offers is the core's to say of the history itself.
                SiteHistoryPage(
                    id = top.id,
                    name = top.name,
                    edit = editing.model,
                    requests = editing.requests,
                    said = editing.said,
                    onBack = back,
                    onDone = onDismiss,
                    onWritten = { written ->
                        // The site as it stands now, on its page, with the core's line.
                        opened = emptyList()
                        editing.onWritten(written)
                    },
                    onReport = { reporting = it },
                )
            }
            null -> if (chosen != null) {
                // Not an answer older than the site at hand, which a correction has just changed.
                val shown = page?.site?.takeIf { (it.byPilots?.revision ?: 0u) >= (chosen.byPilots?.revision ?: 0u) } ?: chosen
                val listed = confirmations?.takeIf { it.siteId == shown.id }
                SitePage(
                    site = shown,
                    // What the core offers for the site to whoever is signed in here: its one answer.
                    edits = editing?.let { given ->
                        given.model.offers(shown, listed)?.let { offers ->
                            SitePageEdits(
                                offers = offers,
                                confirmations = listed,
                                said = given.said,
                                problem = pageProblem,
                                // Where the core offers anything for the site and the phone holds no session.
                                showsSignIn = offers.history.isOffered && !given.model.hasSession,
                                onCorrect = { given.onCorrect(shown) },
                                onRemove = { isRemoving = true },
                                onHistory = { opened = opened + FromASite.History(shown.id, shown.name) },
                                onReport = { reporting = Reported.site(shown.id) },
                                onSignIn = given.onSignIn,
                                onConfirm = { isConfirming = true },
                                onWithdraw = { isWithdrawing = true },
                                onConfirmations = { opened = opened + FromASite.Confirmations(shown) },
                            )
                        }
                    },
                    sources = known.with(sites),
                    own = own,
                    wind = wind,
                    isKept = chosen.id in kept,
                    onKeep = {
                        if (waypoints.add(chosen.name, chosen.position, chosen.elevationM)) {
                            kept = kept + chosen.id
                        }
                    },
                    onStation = { opened = opened + FromASite.Station(it) },
                    // The forecast comes from the server the sites come from.
                    onForecast = { opened = opened + FromASite.Forecast(ForecastPlace(chosen.name, chosen.position)) },
                    onBack = if (showsList) ({ onChoose(null) }) else null,
                    onDone = onDismiss,
                    community = community,
                    onOpenFlight = onOpenFlight,
                    actions = actions,
                    edit = editing?.model,
                    dayStartUnixS = dayStartUnixS,
                )
            } else {
                SiteList(sites, onOpen = onChoose, onDone = onDismiss, onFilter = onFilter)
            }
        }
    }

    if (isRemoving && chosen != null && editing != null) {
        val says = editing.model.says
        SiteWhyDialog(
            title = says.removeQuestion,
            about = says.removeAbout,
            send = says.remove,
            cancel = says.cancel,
            why = editing.model.form.fields.firstOrNull { it.field == SiteField.COMMENT },
            isRemoval = true,
            requests = editing.requests,
            work = { comment ->
                try {
                    val words = editing.model.remove(chosen.id, comment)
                    isRemoving = false
                    // What is left of the site is its history, which brings it back.
                    opened = listOf(FromASite.History(chosen.id, chosen.name))
                    editing.onSaid(words)
                    null
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    editing.model.words(error)
                }
            },
            onDismiss = { isRemoving = false },
        )
    }

    if (isConfirming && chosen != null && editing != null) {
        SiteConfirmSheet(
            site = chosen,
            edit = editing.model,
            requests = editing.requests,
            onConfirmed = { written ->
                isConfirming = false
                // The site with the confirmation counted, and the list read again for the pilot's own.
                page = null
                readAgain += 1
                editing.onWritten(written)
            },
            onDismiss = { isConfirming = false },
        )
    }

    if (isWithdrawing && chosen != null && editing != null) {
        val says = editing.model.says
        SiteWhyDialog(
            title = says.withdrawQuestion,
            about = null,
            send = says.withdraw,
            cancel = says.cancel,
            why = null,
            isRemoval = true,
            requests = editing.requests,
            work = {
                try {
                    val words = editing.model.withdraw(chosen.id)
                    isWithdrawing = false
                    // The answer has no body: the site's count and its list are read anew.
                    readAgain += 1
                    editing.onSaid(words)
                    null
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    editing.model.words(error)
                }
            },
            onDismiss = { isWithdrawing = false },
            asksWhy = false,
        )
    }

    // To whoever runs the server the site is on: the app's Venti.
    if (editing != null) {
        reporting?.let { reported ->
            ReportSheet(reported, editing.account, editing.model.server(), editing.requests, onDismiss = { reporting = null })
        }
    }
}

/**
 * What a site's page needs for a site of Venti's own layers: the model
 * that asks the core what is offered and sends, the account a report goes
 * from, and what the screen around the sheet does when the site changed.
 */
class SiteEditing(
    val model: SiteEditModel,
    val account: AccountModel,
    /** Where requests run, so that one carries on when the sheet is closed. */
    val requests: CoroutineScope,
    /** The core's line about what was just done to the site whose page shows. */
    val said: String?,
    /** Opens the form on the site. */
    val onCorrect: (SiteView) -> Unit,
    /** Leads to the account, for a pilot who is not signed in. */
    val onSignIn: () -> Unit,
    /** A revision was put back or the site was confirmed: the site as it stands, and the core's line. */
    val onWritten: (SiteWritten) -> Unit,
    /** The site was removed or a confirmation taken back: the core's line. */
    val onSaid: (String) -> Unit,
)

/**
 * Whose data the sites are, with what the licences ask to be shown, and the
 * date of that data: said under a site's page, and kept with a site the
 * pilot marks as a take-off of their own.
 */
data class SiteSources(val credits: List<Credit> = emptyList(), val asOfUnixS: Long? = null) {
    /** These and those of the sites on the map, the map's first: its answer is the one the site was tapped in. */
    fun with(sites: SitesModel): SiteSources = SiteSources(sites.credits + credits, sites.asOf ?: asOfUnixS)

    /** How old the data is, as the server's API asks a client to say: "As of Wed, 18 Nov 2026." */
    val asOfInWords: String? get() = asOfUnixS?.let { "As of ${Format.day(it)}." }
}

/**
 * What a site's page leads to beyond itself: what the forecast says of the
 * site, the route map with the site as its first place, and the cockpit.
 */
class SiteActions(
    /** What the forecast says about flying from a site; `null` without an answer. */
    val outlook: suspend (SiteView) -> SiteOutlook?,
    /** Whether a flight can be started now: not while one is on. */
    val canStartFlight: () -> Boolean,
    val onPlanRoute: (SiteView) -> Unit,
    /** Opens the cockpit, which waits for the take-off. */
    val onStartFlight: (SiteView) -> Unit,
)

/**
 * The head of a site's page, as the design language's sheet has it: the
 * site in a line, what the forecast says of its first day with flying
 * hours, and the two things to do from here.
 */
@Composable
private fun SiteHead(site: SiteView, day: OutlookDay?, isForTakingOff: Boolean, actions: SiteActions?) {
    val colours = Venti.colours
    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
        Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
            Text(SiteWords.inALine(site), color = colours.textSecondary, modifier = Modifier.testTag("site-in-a-line"))
            day?.let { shown ->
                Text(SiteWords.outlook(shown), color = colours.textSecondary, modifier = Modifier.testTag("site-outlook"))
                // The wind near by where it is much stronger, under the
                // day's own line: a note, and no part of the status.
                NearbyLine(shown.nearby, style = MaterialTheme.typography.bodyMedium)
                ThermalLine(shown)
            }
        }
        if (actions != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                OutlinedButton(
                    onClick = { actions.onPlanRoute(site) },
                    modifier = Modifier.weight(1f).heightIn(min = VentiSize.touch).testTag("site-plan-route"),
                ) { Text("Plan route") }
                if (isForTakingOff && actions.canStartFlight()) {
                    Button(
                        onClick = { actions.onStartFlight(site) },
                        modifier = Modifier.weight(1f).heightIn(min = VentiSize.touch).testTag("site-start-flight"),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = MaterialTheme.colorScheme.tertiary,
                            contentColor = MaterialTheme.colorScheme.onTertiary,
                        ),
                    ) { Text("Start flight") }
                }
            }
        }
    }
}

/** What a pilot opens from a site's page, in the same sheet. */
private sealed interface FromASite {
    /** A wind station near the site. */
    data class Station(val station: WindStation) : FromASite

    /** The forecast for the site's place, or for a station's. */
    data class Forecast(val place: ForecastPlace) : FromASite

    /** The history of a site of Venti's own layers, by the site's identifier, under its name. */
    data class History(val id: String, val name: String) : FromASite

    /** Who says they took off at a site. */
    data class Confirmations(val site: SiteView) : FromASite
}

/**
 * The flying sites around the part of the map in view, as a list: for
 * looking one up by name, and for pilots who use TalkBack, which cannot
 * reach symbols drawn on a map.
 */
@Composable
private fun SiteList(sites: SitesModel, onOpen: (SiteView) -> Unit, onDone: () -> Unit, onFilter: (() -> Unit)?) {
    var search by rememberSaveable { mutableStateOf("") }
    val wanted = search.trim()
    // The map's filter is the list's: the sites that fit first, then those
    // whose source does not say, under the core's heading and sentence.
    val filtered = sites.filtered
    val fitting = remember(filtered, wanted) { SiteWords.listed(filtered.fitting, wanted) }
    val unsaid = remember(filtered, wanted) { SiteWords.listed(filtered.unsaid, wanted) }

    Column {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // While a filter is on, the filter in the core's line stands where the title stands.
            Text(
                if (filtered.isOn) filtered.filterWords else "Flying sites",
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.weight(1f).testTag("site-list-title"),
            )
            if (onFilter != null && sites.sites.isNotEmpty()) {
                IconButton(onClick = onFilter, modifier = Modifier.size(VentiSize.touch).testTag("site-list-filter")) {
                    Icon(
                        painterResource(R.drawable.ic_filter),
                        contentDescription = SitesFilterWords.TITLE,
                        tint = if (filtered.isOn) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            TextButton(onClick = onDone) { Text("Done") }
        }
        if (sites.sites.isEmpty()) {
            EmptyList(
                "No flying sites here",
                sites.problem ?: "Zoom the map in on an area, and the sites its source knows there are listed.",
            )
        } else {
            if (filtered.isOn) {
                // What the filter leaves of the sites on this part of the
                // map, and under it how many of them are places from
                // pilots: the core's two lines.
                Text(
                    listOf(filtered.words, filtered.fromPilotsWords).filter { it.isNotEmpty() }.joinToString("\n"),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 20.dp).testTag("site-list-leaves"),
                )
            }
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
            if (fitting.isEmpty() && unsaid.isEmpty() && wanted.isNotEmpty()) {
                EmptyList(
                    "No site of that name",
                    "None of the sites on this part of the map is called “$wanted”.",
                )
            } else {
                LazyColumn(modifier = Modifier.testTag("site-list"), contentPadding = PaddingValues(bottom = 32.dp)) {
                    items(fitting, key = { it.id }) { site ->
                        SiteRow(site, onClick = { onOpen(site) })
                        HorizontalDivider(
                            modifier = Modifier.padding(start = 64.dp),
                            color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f),
                        )
                    }
                    if (unsaid.isNotEmpty()) {
                        // Shown apart, not left out: the core's heading and what it means.
                        item(key = "unsaid") {
                            Column(Modifier.padding(horizontal = 16.dp).testTag("site-list-unsaid")) {
                                SectionHeader(filtered.unsaidTitle)
                                Text(
                                    filtered.unsaidNote,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    modifier = Modifier.padding(start = 4.dp, end = 4.dp, bottom = 8.dp),
                                )
                            }
                        }
                        items(unsaid, key = { it.id }) { site ->
                            SiteRow(site, onClick = { onOpen(site) }, dimmed = true)
                            HorizontalDivider(
                                modifier = Modifier.padding(start = 64.dp),
                                color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f),
                            )
                        }
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
private fun SiteRow(site: SiteView, onClick: () -> Unit, dimmed: Boolean = false) {
    val detail = SiteWords.detail(site)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag(if (dimmed) "site-row-unsaid" else "site-row")
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
        SiteGlyph(site.kind, dimmed = dimmed)
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
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SitePage(
    site: SiteView,
    /** Whose data the site is, and of which date. */
    sources: SiteSources,
    own: KeptSitesModel,
    /** The wind stations the map has, for the wind measured near the site. */
    wind: WindModel,
    /** Whether the pilot has kept the site as a waypoint from this sheet. */
    isKept: Boolean,
    onKeep: () -> Unit,
    /** Opens a wind station near the site. */
    onStation: (WindStation) -> Unit,
    /** Opens the forecast for the site's place. */
    onForecast: () -> Unit,
    /** Back to the list of sites, if that is where the pilot came from. */
    onBack: (() -> Unit)?,
    onDone: () -> Unit,
    community: CommunityModel?,
    onOpenFlight: (PublicFlight) -> Unit,
    actions: SiteActions?,
    /** What the core offers for the site and where each leads; `null` where the page asks for none of it. */
    edits: SitePageEdits? = null,
    /** Where what public flights say about the site is asked for; `null` where the page asks for none of it. */
    edit: SiteEditModel? = null,
    /** The day to speak of, by its first moment; the first day with flying hours without one. */
    dayStartUnixS: Long? = null,
) {
    val links = LocalUriHandler.current
    val scope = rememberCoroutineScope()
    // Whether the site is one of the pilot's own take-offs, as the core
    // says: asked when the page opens and after each keeping or forgetting.
    var isOwn by remember(site.id) { mutableStateOf(false) }
    LaunchedEffect(site.id, own.changes) {
        isOwn = own.isKept(site.id)
    }
    // Whether flights begin here: the forecast's verdict and "Start
    // flight" are for such a place.
    val isForTakingOff = SiteKind.TAKEOFF in site.uses && !site.discouraged
    // What the forecast says about flying from here. Absent for a place
    // to land, without a server, and until the server has answered.
    var outlook by remember(site.id) { mutableStateOf<SiteOutlook?>(null) }
    LaunchedEffect(site.id) {
        if (isForTakingOff && actions != null) outlook = actions.outlook(site)
    }
    val day = outlook?.let { SiteWords.day(it, dayStartUnixS) }
    // The paragraphs on how the page is read stand behind a row at its end.
    var showsHowRead by rememberSaveable(site.id) { mutableStateOf(false) }
    // What the public flights that began here say about the site, as the
    // core reads the server's count: shown only for a take-off, and not
    // at all while it is not read or could not be.
    var flights by remember(site.id) { mutableStateOf<SiteFlights?>(null) }
    LaunchedEffect(site.id) {
        val model = edit ?: return@LaunchedEffect
        flights = try {
            model.flights(site.id)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            null
        }
    }
    // A best flight opens as a public flight opens, once the list of
    // everyone's flights knows it.
    var opening by remember(site.id) { mutableStateOf<String?>(null) }
    LaunchedEffect(opening) {
        val id = opening ?: return@LaunchedEffect
        val known = community ?: return@LaunchedEffect
        try {
            onOpenFlight(known.publicFlightById(id))
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // The flight's own page says why it cannot be shown; without it, nothing opens.
        }
        opening = null
    }
    // The wind measured around the site now, from the stations the map
    // already has, and for each whether it comes from a direction the site
    // works in. Which stations, and whether a wind fits, is the core's.
    val stations = wind.stations
    val near = remember(site, stations) {
        stationsNear(stations, site.position, site.elevationM, site.wind, SiteWords.STATIONS_WITHIN_M, 3u)
    }

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
            // The name with the day's status beside it; where the two do not
            // fit in a line, on a narrow screen or with large text, the
            // status goes under the name and the name keeps the width.
            FlowRow(
                modifier = Modifier.weight(1f),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
                itemVerticalAlignment = Alignment.CenterVertically,
            ) {
                Column {
                    Text(
                        site.name,
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.semantics { heading() },
                    )
                    // The site's tier in the core's word or two: "From pilots", "From a partner".
                    site.byPilots?.label?.takeIf { it.isNotEmpty() }?.let { tier ->
                        Text(
                            tier,
                            style = MaterialTheme.typography.labelLarge,
                            color = Venti.colours.textSecondary,
                            modifier = Modifier.testTag("site-tier"),
                        )
                    }
                }
                day?.let { shown ->
                    StatusChip(shown.verdict.status, shown.status, Modifier.testTag("site-status"))
                }
            }
            TextButton(onClick = onDone) { Text("Done") }
        }
        LazyColumn(
            modifier = Modifier.testTag("site-page"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // What was just done to the site, in the core's line.
            edits?.said?.let { words ->
                item { SiteSaid(words) }
            }
            item {
                SiteHead(site, day, isForTakingOff, actions)
            }
            // The air over the take-off through that day, where the model's
            // ground is known.
            if (day != null && day.groundM != null) {
                item {
                    AirChart(day, site.elevationM)
                }
            }
            item {
                Group {
                    row { ValueRow("Where", Format.place(site.position.lat, site.position.lon)) }
                    row { WindRow(site) }
                }
            }
            // A site of Venti's own layers: who added it and who changed
            // it last, and the core's notice of what its tier is worth;
            // then who took off here, and the pilot's own word on it.
            site.byPilots?.let { by ->
                item { SiteByPilotsRows(by) }
                if (edits != null && edits.showsConfirmed(by)) {
                    item { SiteConfirmedRows(by, edits) }
                }
            }
            // What public flights say about it, under the confirmations: for a take-off of any layer.
            val read = flights
            if (read != null && read.isTakeoff && edit != null) {
                item { SiteFlightsSection(read, edit.flightsWords, onOpen = { line -> opening = line.id }) }
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
                                    R.drawable.venti_caution,
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
            if (near.isNotEmpty()) {
                item {
                    SectionHeader("Wind measured nearby")
                    Group {
                        for (station in near) {
                            row { StationNearbyRow(station, wind.now, onClick = { onStation(station.station) }) }
                        }
                    }
                    SectionFooter(SiteWords.NEARBY_CAUTION)
                }
            }
            item {
                // The forecast, and for a take-off an alarm for its kind of day.
                val alerts = LocalAlerts.current.takeIf { isForTakingOff }
                Group {
                    row {
                        LinkRow(
                            "Forecast for this place",
                            R.drawable.ic_chevron_right,
                            Modifier.testTag("site-forecast"),
                            leading = R.drawable.venti_forecast,
                            onClick = onForecast,
                        )
                    }
                    if (alerts != null) {
                        row {
                            AlertMeRow(alerts, "site-alert-me") {
                                AlertsModel.dayDraft(site.id, site.name, site.position.lat, site.position.lon)
                            }
                        }
                    }
                }
            }
            item {
                Group {
                    row {
                        OwnRow(isOwn) { keep ->
                            scope.launch {
                                if (keep) own.keep(site, sources.credits, sources.asOfUnixS) else own.forget(site.id)
                            }
                        }
                    }
                    row {
                        KeepRow(isKept, onKeep)
                    }
                }
                SectionFooter("As a waypoint the cockpit can navigate to it, also without a network.")
                own.problem?.let { SectionFooter(it, Modifier.testTag("site-own-problem")) }
            }
            // What is said of it beyond the name and the wind, as it is
            // written there: the description under the core's heading of
            // whose words it is, the core's sentences, and somebody else's
            // web page, which opens outside the app.
            val said = SiteWords.said(site)
            val website = SiteWords.website(site)
            if (site.description != null || said.isNotEmpty() || website != null) {
                item {
                    if (site.descriptionTitle.isNotEmpty()) {
                        SectionHeader(site.descriptionTitle, Modifier.testTag("site-description-title"))
                    }
                    Group(Modifier.testTag("site-more")) {
                        site.description?.let { words ->
                            row { SaidRow(words, Modifier.testTag("site-description")) }
                        }
                        for (sentence in said) {
                            row { SaidRow(sentence) }
                        }
                        if (website != null) {
                            row {
                                WebsiteRow(website) {
                                    // No app that opens web pages: nothing to do.
                                    runCatching { links.openUri(website) }
                                }
                            }
                        }
                    }
                }
            }
            // What the layers of the picture and the wind near by are and
            // are not, in the core's words, while the page shows them.
            val layersNote = outlook?.layersNote.orEmpty().takeIf { day?.groundM != null }.orEmpty()
            val nearbyNote = outlook?.nearbyNote.orEmpty().takeIf { day != null }.orEmpty()
            if (edits != null && edits.showsEntries) {
                item { SiteEditRows(edits) }
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
            }
            // How the page is read, what a site on a map is worth, whose
            // data it is and under which licence: one tap away, behind a
            // row at the end, in the same words.
            item {
                Group {
                    row {
                        DisclosureRow(
                            SiteWords.HOW_READ,
                            isOpen = showsHowRead,
                            modifier = Modifier.testTag("site-how-read"),
                            onChange = { showsHowRead = it },
                        )
                    }
                }
                if (showsHowRead) {
                    Column(Modifier.testTag("site-air-notes")) {
                        if (layersNote.isNotEmpty()) SectionFooter(layersNote, Modifier.testTag("site-layers-note"))
                        if (nearbyNote.isNotEmpty()) SectionFooter(nearbyNote, Modifier.testTag("site-nearby-note"))
                        SectionFooter(SiteWords.footnote(site, sources), Modifier.testTag("site-footnote"))
                    }
                }
            }
        }
    }
}

/**
 * A wind station near a site: its wind as the map draws it, how far and
 * which way it stands from the site and how much higher or lower, and
 * whether its wind comes from a direction the site works in.
 */
@Composable
private fun StationNearbyRow(
    near: StationNearby,
    /** The current time by the server's clock. */
    now: Long,
    onClick: () -> Unit,
) {
    val mark = near.station.mark
    val whereabouts = SiteWords.whereabouts(near)
    val fit = SiteWords.fit(near)
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("site-station")
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 64.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        WindGlyph(mark)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                near.station.name,
                style = MaterialTheme.typography.bodyLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(whereabouts, style = MaterialTheme.typography.bodyMedium.numbers, color = quiet)
            if (fit != null) {
                Text(fit, style = MaterialTheme.typography.bodyMedium, color = quiet)
            }
        }
        Spacer(Modifier.width(8.dp))
        if (mark.label.isNotEmpty()) {
            val unit = MaterialTheme.typography.bodySmall
            Text(
                buildAnnotatedString {
                    append(mark.label)
                    withStyle(SpanStyle(fontSize = unit.fontSize, fontWeight = FontWeight.Normal, color = quiet)) {
                        append(" km/h")
                    }
                },
                style = MaterialTheme.typography.titleMedium.numbers,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
        } else {
            // What stands in place of a wind that is not current.
            Text(SiteWords.age(near.station, now), style = MaterialTheme.typography.bodyMedium, color = quiet)
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

/**
 * Marks the site as one of the pilot's own take-offs, and unmarks it: the
 * row of the waypoint below it, as a switch. Today lists the pilot's own first.
 */
@Composable
private fun OwnRow(isOwn: Boolean, onChange: (Boolean) -> Unit) {
    val colour = if (isOwn) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.primary
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("site-own")
            .toggleable(value = isOwn, role = Role.Switch, onValueChange = onChange)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(if (isOwn) R.drawable.ic_check else R.drawable.ic_add),
            contentDescription = null,
            tint = colour,
        )
        Spacer(Modifier.width(VentiSpace.s4))
        Text(
            if (isOwn) SiteWords.OWN else SiteWords.KEEP_AS_OWN,
            style = MaterialTheme.typography.bodyLarge,
            color = colour,
        )
    }
}

/** Something a site's source says of it, as text: never read as markup. */
@Composable
private fun SaidRow(words: String, modifier: Modifier = Modifier) {
    Text(
        words,
        style = MaterialTheme.typography.bodyLarge,
        modifier = modifier.fillMaxWidth().padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
    )
}

/** The site's web page, by its address: it opens in the browser, outside the app. */
@Composable
private fun WebsiteRow(address: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("site-website")
            .clickable(role = Role.Button, onClickLabel = "Open in the browser", onClick = onClick)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            address,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.primary,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(VentiSpace.s3))
        Icon(
            painterResource(R.drawable.ic_open_in_new),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
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
    /** "Take-off 1550 m, wind from N to NE" */
    fun inALine(site: SiteView): String {
        var line = site.kindInWords
        site.elevationM?.let { line += " ${Format.metres(it)} m" }
        if (site.windInWords.isNotEmpty()) line += ", wind from ${site.windInWords}"
        return line
    }

    /** The first day the forecast has flying hours for; the first day it has at all where none has any. */
    fun firstFlyingDay(outlook: SiteOutlook): OutlookDay? =
        outlook.days.firstOrNull { it.hours.isNotEmpty() } ?: outlook.days.firstOrNull()

    /** The day that begins at the moment asked for, where the forecast has it; else the first with flying hours. */
    fun day(outlook: SiteOutlook, startUnixS: Long?): OutlookDay? =
        startUnixS?.let { wanted -> outlook.days.firstOrNull { it.startUnixS == wanted } } ?: firstFlyingDay(outlook)

    /** The row at the end of a site's page that unfolds how it is read, whose data it is and under which licence. */
    const val HOW_READ = "How this is read"

    /** "Today: Good from noon. Gusts to 26 km/h after 15:00", with the day as the core names it. */
    fun outlook(day: OutlookDay): String =
        if (day.why.isEmpty()) "${day.name}: ${day.headline}" else "${day.name}: ${day.headline}. ${day.why}"

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
        // A place from pilots says so first, in the core's label for its tier.
        site.byPilots?.takeIf { it.fromPilots }?.label?.takeIf { it.isNotEmpty() }?.let { parts += it }
        // Without a name of its own, the name already says what it is.
        if (site.isNamed) parts += site.kindInWords
        site.elevationM?.let { parts += "${Format.metres(it)} m" }
        if (site.windInWords.isNotEmpty()) parts += "wind ${site.windInWords}"
        // A site of which nothing else is known is at least somewhere.
        return if (parts.isEmpty()) Format.place(site.position.lat, site.position.lon) else parts.joinToString(" · ")
    }

    /** How far from a site a wind station still says something about it, metres. */
    const val STATIONS_WITHIN_M = 15_000.0

    /** What a station near a site is worth, said under the stations. */
    const val NEARBY_CAUTION =
        "A station near a site is not the site: it may stand on a summit, in the valley or behind a ridge. " +
            "A direction that fits says nothing about the wind's strength or its gusts."

    /** Where a station stands, seen from the site: "2.1 km NE · 300 m higher". */
    fun whereabouts(near: StationNearby): String {
        val parts = mutableListOf("${Format.kilometres(near.distanceM)} km ${Format.compassPoint(near.bearingDeg)}")
        val higher = near.higherByM
        if (higher != null && abs(higher) >= 50) {
            val rounded = (abs(higher) / 10).roundToLong() * 10.0
            parts += "${Format.metres(rounded)} m ${if (higher > 0) "higher" else "lower"}"
        }
        return parts.joinToString(" · ")
    }

    /**
     * Whether the station's wind comes from a direction the site works in,
     * with the direction; `null` when that cannot be said. It is a fact
     * about a direction, and no word on whether the site can be flown.
     */
    fun fit(near: StationNearby): String? {
        val fits = near.fits ?: return null
        val from = near.station.latest?.directionDeg ?: return null
        val point = Format.compassPoint(from)
        return if (fits) "From $point: a direction the site works in" else "From $point: not a direction the site works in"
    }

    /** What stands in place of a station's wind when it is not current. */
    fun age(station: WindStation, now: Long): String {
        val reading = station.latest ?: return "no reading"
        return Format.age(Format.minutes(from = reading.timeUnixS, to = now))
    }

    /** Under the list: what a site on a map is worth, whose data it is, and how old. */
    fun underTheList(sites: SitesModel): String =
        (listOf(CAUTION) + sites.credits.map { it.text } + listOfNotNull(sites.asOfInWords)).joinToString("\n")

    /**
     * Under a site's page: what a site on a map is worth, the notice and
     * the credit of the site's source, and how old its data is.
     */
    fun footnote(site: SiteView, sources: SiteSources): String {
        val credit = sources.credits.firstOrNull { it.source == site.source }
        return listOfNotNull(CAUTION, credit?.notice, credit?.text, sources.asOfInWords).joinToString("\n\n")
    }

    /** On the switch that marks a site as one of the pilot's own take-offs, off and on. */
    const val KEEP_AS_OWN = "Keep as my take-off"
    const val OWN = "Kept"

    /**
     * The core's sentences about a fee, rules for guests and a time not to
     * fly, those that are not empty, in that order. Nothing is worked out
     * from them.
     */
    fun said(site: SiteView): List<String> =
        listOf(site.feeInWords, site.guestsInWords, site.noFlightInWords).filter { it.isNotEmpty() }

    /**
     * The site's web page, if its source names one: an `http` or `https`
     * address, which is all the core hands on and all that is opened.
     */
    fun website(site: SiteView): String? =
        site.website?.takeIf { it.startsWith("https://") || it.startsWith("http://") }

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
