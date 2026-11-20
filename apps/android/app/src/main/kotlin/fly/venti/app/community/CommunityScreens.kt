package fly.venti.app.community

import androidx.annotation.DrawableRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.design.Format
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.numbers
import fly.venti.app.logbook.TrackThumbnail
import fly.venti.app.logbook.groupedShape
import fly.venti.core.PublicFlight
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.RouteKind
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/** What the list of everyone's flights says, in words. */
object CommunityWords {
    const val WHOSE_RECKONING =
        "Flights their pilots share with everyone on your Venti server. Distances and points are the " +
            "server's own reckoning by the World XContest's rules, not a league's."

    const val NONE_YET = "Flights that pilots on your Venti server share with everyone are listed here."

    /** The two orders, as the switch above the list names them. */
    fun title(order: PublicFlightsOrder): String = when (order) {
        PublicFlightsOrder.NEWEST -> "Newest"
        PublicFlightsOrder.BEST -> "Best"
    }

    /** The day and the glider: "Sat, 15 Aug 2026 · Sample Wing". */
    fun detail(flight: PublicFlight): String {
        val day = Format.day(flight.startUnixS)
        return flight.glider?.let { "$day · $it" } ?: day
    }

    /** The scored distance, or the length of the track of a flight too short to score. */
    fun distance(flight: PublicFlight): String =
        "${Format.kilometres(flight.score?.distanceM ?: flight.trackLengthM)} km"

    fun airtime(flight: PublicFlight): String = "${Format.hoursMinutes(flight.airtimeS)} h"

    /** What a triangle is called, for a flight that scores as one. */
    fun triangle(flight: PublicFlight): String? = when (flight.score?.kind) {
        RouteKind.FAI_TRIANGLE -> "FAI triangle"
        RouteKind.FREE_TRIANGLE -> "Triangle"
        RouteKind.FREE_FLIGHT, null -> null
    }

    /** A row for someone who cannot see it. */
    fun spoken(flight: PublicFlight): String =
        listOfNotNull(detail(flight), distance(flight), triangle(flight), "${airtime(flight)} in the air").joinToString(", ")
}

/**
 * Everyone's public flights on the pilot's Venti server: the newest first,
 * or the best. More is fetched as the end of the list comes into view.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PublicFlightsList(community: CommunityModel, onOpen: (PublicFlight) -> Unit, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var isRefreshing by remember { mutableStateOf(false) }
    // Fetched when the list is first looked at, and again for another server.
    LaunchedEffect(community) {
        community.loadIfNeeded()
    }
    val flights = community.flights

    PullToRefreshBox(
        isRefreshing = isRefreshing,
        onRefresh = {
            scope.launch {
                isRefreshing = true
                community.refresh()
                isRefreshing = false
            }
        },
        modifier = modifier.fillMaxSize(),
    ) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag("public-flights"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
        ) {
            item(key = "order") {
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
                    PublicFlightsOrder.entries.forEachIndexed { index, order ->
                        SegmentedButton(
                            selected = order == community.order,
                            onClick = { scope.launch { community.show(order) } },
                            shape = SegmentedButtonDefaults.itemShape(index, PublicFlightsOrder.entries.size),
                            modifier = Modifier.testTag("public-flights-${order.name.lowercase()}"),
                        ) {
                            Text(CommunityWords.title(order))
                        }
                    }
                }
            }
            if (flights.isEmpty() && !community.isLoading) {
                item(key = "none") {
                    NoFlights("No public flights yet", community.problem ?: CommunityWords.NONE_YET, R.drawable.venti_league)
                }
            }
            itemsIndexed(flights, key = { _, flight -> flight.id }) { index, flight ->
                PublicFlightRow(
                    flight,
                    shape = groupedShape(index, flights.size),
                    tag = "public-flight-row",
                    onOpen = { onOpen(flight) },
                )
                if (index < flights.lastIndex) {
                    Spacer(Modifier.size(2.dp))
                } else {
                    // The last row has come into view: fetch what follows it.
                    LaunchedEffect(flight.id) {
                        community.loadMore()
                    }
                }
            }
            if (community.isLoading && !isRefreshing) {
                item(key = "loading") {
                    Box(Modifier.fillMaxWidth().padding(vertical = 20.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator(Modifier.size(28.dp))
                    }
                }
            }
            if (flights.isNotEmpty()) {
                // Whose reckoning the distances and points are.
                item(key = "reckoning") {
                    SectionFooter(CommunityWords.WHOSE_RECKONING, Modifier.testTag("public-flights-reckoning"))
                }
            }
        }
    }
}

/**
 * One pilot's public flights, the newest first: reached from the menu of
 * one of that pilot's flights.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PilotFlightsScreen(
    handle: String,
    /** The pilot's name as the flight it was opened from had it. */
    name: String,
    community: CommunityModel,
    /** The pilot's own account, which a report is sent from. */
    account: AccountModel,
    /** The address of the server this pilot's page is on. */
    server: String?,
    /** Where a report is sent from, so that it goes on when its form is closed. */
    requests: CoroutineScope,
    onOpen: (PublicFlight) -> Unit,
    onBack: () -> Unit,
) {
    LaunchedEffect(handle) {
        community.showPilot(handle)
    }
    var isShowingMenu by remember { mutableStateOf(false) }
    var reporting by remember { mutableStateOf<Reported?>(null) }
    val pilot = community.pilot?.takeIf { it.handle == handle }
    val flights = pilot?.flights.orEmpty()

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                    }
                },
                actions = {
                    Box {
                        IconButton(onClick = { isShowingMenu = true }) {
                            Icon(painterResource(R.drawable.ic_more_vert), contentDescription = "More")
                        }
                        DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
                            val report = Reported.pilot(handle)
                            DropdownMenuItem(
                                text = { Text(report.title) },
                                leadingIcon = { Icon(painterResource(R.drawable.ic_flag), contentDescription = null) },
                                onClick = {
                                    isShowingMenu = false
                                    reporting = report
                                },
                            )
                        }
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding).testTag("pilot-flights"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp),
        ) {
            item(key = "handle") { SectionHeader("@$handle") }
            if (flights.isEmpty() && pilot?.isLoading == false && (pilot.isLoaded || pilot.problem != null)) {
                item(key = "none") {
                    NoFlights(
                        "No public flights",
                        pilot.problem ?: "This pilot shares no flight with everyone at the moment.",
                        R.drawable.ic_person,
                    )
                }
            }
            itemsIndexed(flights, key = { _, flight -> flight.id }) { index, flight ->
                PublicFlightRow(
                    flight,
                    shape = groupedShape(index, flights.size),
                    tag = "pilot-flight-row",
                    onOpen = { onOpen(flight) },
                )
                if (index < flights.lastIndex) {
                    Spacer(Modifier.size(2.dp))
                } else {
                    // The last row has come into view: fetch what follows it.
                    LaunchedEffect(flight.id) {
                        community.morePilotFlights()
                    }
                }
            }
            if (pilot == null || pilot.isLoading) {
                item(key = "loading") {
                    Box(Modifier.fillMaxWidth().padding(vertical = 20.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator(Modifier.size(28.dp))
                    }
                }
            }
        }
    }

    reporting?.let { reported ->
        ReportSheet(reported, account, server, requests, onDismiss = { reporting = null })
    }
}

/** Said in place of a list when it is empty. */
@Composable
private fun NoFlights(title: String, text: String, @DrawableRes icon: Int) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            painterResource(icon),
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

/**
 * One public flight in the list: its shape, who flew it and when, and how
 * far. Set like a row of the pilot's own logbook, with the pilot where the
 * logbook has the day.
 */
@Composable
internal fun PublicFlightRow(
    flight: PublicFlight,
    shape: Shape,
    /** What a test finds the row by: which list it is a row of. */
    tag: String,
    onOpen: () -> Unit,
) {
    val triangle = CommunityWords.triangle(flight)
    Surface(shape = shape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag(tag)
                .clickable(role = Role.Button, onClick = onOpen)
                .padding(horizontal = 12.dp, vertical = 10.dp)
                // One element for a screen reader: the pilot, then the rest.
                .clearAndSetSemantics {
                    contentDescription = flight.pilot
                    stateDescription = CommunityWords.spoken(flight)
                },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TrackThumbnail(flight.outline)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    flight.pilot,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    CommunityWords.detail(flight),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.width(8.dp))
            Column(horizontalAlignment = Alignment.End) {
                // The scored cross-country distance; a triangle is marked.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (triangle != null) {
                        Icon(
                            painterResource(R.drawable.venti_triangle),
                            contentDescription = triangle,
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.width(3.dp))
                    }
                    Text(CommunityWords.distance(flight), style = MaterialTheme.typography.titleMedium.numbers)
                }
                Text(
                    CommunityWords.airtime(flight),
                    style = MaterialTheme.typography.bodyMedium.numbers,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}
