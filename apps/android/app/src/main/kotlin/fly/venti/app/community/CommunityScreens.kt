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
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
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
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import androidx.compose.foundation.layout.heightIn
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.logbook.TrackThumbnail
import fly.venti.app.logbook.groupedShape
import fly.venti.core.FlownSiteLine
import fly.venti.core.FoundPilotLine
import fly.venti.core.PublicFlight
import fly.venti.core.gliderClassName
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.RouteKind
import fly.venti.core.pilotsWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/** What the list of everyone's flights says, in words. */
object CommunityWords {
    const val WHOSE_RECKONING =
        "Flights their pilots share with everyone on Venti. Distances and points are " +
            "Venti's own reckoning by the World XContest's rules, not a league's."

    const val NONE_YET = "Flights that pilots share with everyone on Venti are listed here."

    /** The two orders, as the switch above the list names them. */
    fun title(order: PublicFlightsOrder): String = when (order) {
        PublicFlightsOrder.NEWEST -> "Newest"
        PublicFlightsOrder.BEST -> "Best"
    }

    /**
     * The day, the site it took off from where the server names one with
     * the flag of its country, and the glider with its class:
     * "Sat, 15 Aug 2026 · 🇦🇹 Brauneck · EN B · Sample Wing".
     */
    fun detail(flight: PublicFlight): String =
        listOfNotNull(Format.day(flight.startUnixS), place(flight), glider(flight)).joinToString(" · ")

    /** The take-off, with its country's flag before it; the flag alone where the site has no name. */
    fun place(flight: PublicFlight): String? =
        listOfNotNull(Countries.flag(flight.takeoffCountry), flight.takeoffName).joinToString(" ").ifEmpty { null }

    /**
     * The glider's class in the core's name and the glider: "EN B · Sample
     * Wing"; either alone. The class first, so that a long name cut at the
     * row's end still leaves it to read.
     */
    fun glider(flight: PublicFlight): String? =
        listOfNotNull(flight.gliderClass?.let { gliderClassName(it) }, flight.glider).joinToString(" · ").ifEmpty { null }

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
        listOfNotNull(
            flight.pilotCountry?.let(Countries::name),
            Format.day(flight.startUnixS),
            flight.takeoffName,
            flight.takeoffCountry?.let(Countries::name),
            glider(flight),
            distance(flight),
            triangle(flight),
            "${airtime(flight)} in the air",
        ).joinToString(", ")
}

/**
 * Everyone's public flights on the pilot's Venti server: the newest first,
 * or the best. More is fetched as the end of the list comes into view.
 * Over the list is the field that finds a pilot by name: while there is
 * text in it, the pilots the server found stand in the list's place.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PublicFlightsList(
    community: CommunityModel,
    onOpen: (PublicFlight) -> Unit,
    modifier: Modifier = Modifier,
    /** Opens a pilot the search found: the handle and the name. Without it there is no search. */
    onOpenPilot: ((String, String) -> Unit)? = null,
) {
    var wanted by rememberSaveable { mutableStateOf("") }
    val words = remember { pilotsWords() }
    Column(modifier.fillMaxSize()) {
        if (onOpenPilot != null) {
            OutlinedTextField(
                value = wanted,
                onValueChange = { wanted = it },
                placeholder = { Text(words.searchPlaceholder) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_search), contentDescription = null) },
                trailingIcon = {
                    if (wanted.isNotEmpty()) {
                        IconButton(onClick = { wanted = "" }) {
                            Icon(painterResource(R.drawable.ic_close), contentDescription = words.searchClear)
                        }
                    }
                },
                singleLine = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false),
                shape = MaterialTheme.shapes.extraLarge,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2)
                    .testTag("pilot-search"),
            )
            // Asked once the typing has paused: each letter cancels the
            // question of the one before.
            LaunchedEffect(community, wanted) { community.findPilots(wanted) }
        }
        if (onOpenPilot != null && wanted.isNotBlank()) {
            PilotsFound(community.search.takeIf { it.text == wanted }, onOpenPilot)
        } else {
            EveryonesFlights(community, onOpen)
        }
    }
}

/** The pilots a search found, each of which opens that pilot's page, and the core's line for the result. */
@Composable
private fun PilotsFound(search: CommunityModel.PilotSearch?, onOpenPilot: (String, String) -> Unit) {
    val pilots: List<FoundPilotLine> = search?.pilots.orEmpty()
    LazyColumn(
        modifier = Modifier.fillMaxSize().testTag("pilots-found"),
        contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s1, bottom = VentiSpace.s5),
    ) {
        if (pilots.isNotEmpty()) {
            item(key = "pilots") {
                Group {
                    for (line in pilots) {
                        row {
                            PilotLine(
                                line.name,
                                line.picturePath,
                                title = line.words,
                                modifier = Modifier.testTag("pilot-found"),
                                onClick = { onOpenPilot(line.handle, line.name) },
                            )
                        }
                    }
                }
            }
        }
        if (search != null && search.words.isNotEmpty()) {
            item(key = "words") {
                if (pilots.isEmpty()) {
                    // Nobody by this name, too few letters, or no answer: in the words there are for it.
                    Text(
                        search.words,
                        style = MaterialTheme.typography.bodyLarge,
                        color = Venti.colours.textSecondary,
                        textAlign = TextAlign.Center,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s7)
                            .testTag("pilots-found-words"),
                    )
                } else {
                    SectionFooter(search.words, Modifier.testTag("pilots-found-words"))
                }
            }
        }
        if (search == null || (search.isLoading && pilots.isEmpty() && search.words.isEmpty())) {
            item(key = "loading") {
                Box(Modifier.fillMaxWidth().padding(vertical = VentiSpace.s5), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(Modifier.size(VentiSpace.s6))
                }
            }
        }
    }
}

/** The list itself: the switch between the two orders, the flights, and whose reckoning they are. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun EveryonesFlights(community: CommunityModel, onOpen: (PublicFlight) -> Unit) {
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
        modifier = Modifier.fillMaxSize(),
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
 * One pilot's page: what their public flights add up to, in all years or
 * in one, and under it the flights, the newest first. Reached from one of
 * that pilot's flights, from the ranking, and from the search by name.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PilotFlightsScreen(
    handle: String,
    /** The pilot's name as the flight it was opened from had it. */
    name: String,
    community: CommunityModel,
    /** Pilots among each other: who follows this pilot, and the button that does. */
    social: SocialModel,
    /** The pilot's own account, which a report is sent from. */
    account: AccountModel,
    /** The address of the server this pilot's page is on. */
    server: String?,
    /** Where a report is sent from, so that it goes on when its form is closed. */
    requests: CoroutineScope,
    onOpen: (PublicFlight) -> Unit,
    onBack: () -> Unit,
    /** Opens the page of a take-off the pilot flew from; `null` where there is none to open. */
    onOpenSite: ((FlownSiteLine) -> Unit)? = null,
) {
    LaunchedEffect(handle) {
        community.showPilotTotals(handle)
    }
    var isShowingMenu by remember { mutableStateOf(false) }
    var reporting by remember { mutableStateOf<Reported?>(null) }
    val pilot = community.pilot?.takeIf { it.handle == handle }
    val flights = pilot?.flights.orEmpty()
    val totals = community.pilotTotals?.takeIf { it.handle == handle }
    // The picture as the pilot's totals name it, or as another list did before they came.
    val picture = totals?.statistics?.let { it.picturePath } ?: PilotPictures.of(handle)
    // The year the totals are of: 0 for all years, as the core numbers them.
    var year by rememberSaveable(handle) { mutableIntStateOf(0) }
    // The flights under the totals are that season's too, by the bounds the core gives it.
    val season = totals?.statistics?.seasons?.firstOrNull { it.year == year }
    LaunchedEffect(handle, season?.fromUnixS, season?.toUnixS) {
        community.showPilot(handle, season?.fromUnixS, season?.toUnixS)
    }
    // A total that is one flight's opens that flight, once the list of
    // everyone's flights knows it.
    var opening by remember(handle) { mutableStateOf<String?>(null) }
    LaunchedEffect(opening) {
        val id = opening ?: return@LaunchedEffect
        try {
            onOpen(community.publicFlightById(id))
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // Taken back since the totals were added up: nothing opens.
        }
        opening = null
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(totals?.statistics?.name ?: name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
                actions = {
                    Box {
                        IconButton(onClick = { isShowingMenu = true }) {
                            Icon(painterResource(R.drawable.ic_more_vert), contentDescription = Common.words.more)
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
                            // The picture alone, where the pilot shows one.
                            if (picture != null) {
                                val reportPicture = Reported.picture(handle)
                                DropdownMenuItem(
                                    text = { Text(reportPicture.title) },
                                    leadingIcon = { Icon(painterResource(R.drawable.ic_flag), contentDescription = null) },
                                    onClick = {
                                        isShowingMenu = false
                                        reporting = reportPicture
                                    },
                                    modifier = Modifier.testTag("pilot-report-picture"),
                                )
                            }
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
            item(key = "face") {
                val shownName = totals?.statistics?.name ?: name
                Row(
                    Modifier.fillMaxWidth().padding(top = VentiSpace.s2).testTag("pilot-face"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s4),
                ) {
                    PilotFace(shownName, picture, size = FaceSize.Large)
                    Text("@$handle", style = MaterialTheme.typography.titleMedium, color = Venti.colours.textSecondary)
                }
            }
            item(key = "handle") { Spacer(Modifier.size(VentiSpace.s2)) }
            item(key = "follows") {
                FollowLine(handle, social, requests, Modifier.padding(horizontal = VentiSpace.s1).padding(bottom = VentiSpace.s3))
            }
            totals?.statistics?.let { statistics ->
                item(key = "totals") {
                    PilotTotalsSection(
                        statistics,
                        year = year,
                        onYear = { year = it },
                        onOpenFlight = { opening = it },
                        onOpenSite = onOpenSite,
                        modifier = Modifier.padding(bottom = VentiSpace.s4),
                    )
                }
            }
            // Why the totals are missing, unless the flights are missing for the same reason.
            totals?.problem?.takeIf { pilot?.problem == null }?.let { problem ->
                item(key = "totals-problem") {
                    SectionFooter(problem, Modifier.padding(bottom = VentiSpace.s4).testTag("pilot-totals-problem"))
                }
            }
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
    /** A line under the row, the feed's cheers and comments; none when empty. */
    under: String? = null,
    onOpen: () -> Unit,
) {
    val triangle = CommunityWords.triangle(flight)
    Surface(shape = shape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .testTag(tag)
                .clickable(role = Role.Button, onClick = onOpen)
                .padding(horizontal = 12.dp, vertical = 10.dp)
                // One element for a screen reader: the pilot, then the rest.
                .clearAndSetSemantics {
                    contentDescription = flight.pilot
                    stateDescription = spokenWith(under, CommunityWords.spoken(flight))
                },
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TrackThumbnail(flight.outline)
                Spacer(Modifier.width(VentiSpace.s3))
                Column(Modifier.weight(1f)) {
                    // The pilot's face and country by their name, once they set one.
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        PilotFace(flight.pilot, flight.pilotPicture, size = FaceSize.Small)
                        Spacer(Modifier.width(VentiSpace.s2))
                        Text(
                            flight.pilot,
                            style = MaterialTheme.typography.titleMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        if (flight.pilotCountry != null) {
                            Spacer(Modifier.width(VentiSpace.s1))
                            Flag(flight.pilotCountry, style = MaterialTheme.typography.titleMedium)
                        }
                    }
                    // Two lines, so that the glider and its class are not cut off.
                    Text(
                        CommunityWords.detail(flight),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Spacer(Modifier.width(VentiSpace.s2))
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
                            Spacer(Modifier.width(VentiSpace.s1))
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
            if (!under.isNullOrEmpty()) {
                Text(
                    under,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Venti.colours.textSecondary,
                    modifier = Modifier.padding(top = VentiSpace.s2),
                )
            }
        }
    }
}

/**
 * A pilot in a list: their face, the title and the core's line under it
 * where there is one, opening their page. One element for a screen reader,
 * which reads its words.
 */
@Composable
internal fun PilotLine(
    name: String,
    picturePath: String?,
    title: String,
    modifier: Modifier = Modifier,
    line: String? = null,
    initials: String? = null,
    onClick: () -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2)
            .semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PilotFace(name, picturePath, size = FaceSize.Medium, initials = initials)
        Spacer(Modifier.width(VentiSpace.s3))
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            if (!line.isNullOrEmpty()) {
                Text(line, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
            }
        }
        Spacer(Modifier.width(VentiSpace.s2))
        Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = Venti.colours.textSecondary)
    }
}
