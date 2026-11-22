package fly.venti.app.community

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.logbook.groupedShape
import fly.venti.core.PublicFlight
import fly.venti.core.RankedPilot
import fly.venti.core.RouteKind
import fly.venti.core.pilotsWords
import kotlinx.coroutines.launch

/**
 * The League tab: what the pilots of Venti share with everyone, and the
 * ranking of those pilots. The design language's fifth tab.
 *
 * The lists are the community's own; this screen is the tab around them
 * and the switch between the two.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LeagueScreen(
    community: CommunityModel,
    /** Pilots among each other: the feed of the pilots one follows, for a signed-in pilot. */
    social: SocialModel,
    /** Whether Venti can be asked: always in the app, whose Venti is built in; not in a test without a server. */
    hasServer: Boolean,
    onOpenFlight: (PublicFlight) -> Unit,
    /** Opens a pilot's public flights: the handle, and the name to show until the server has said it. */
    onOpenPilot: (String, String) -> Unit,
    bars: PaddingValues,
) {
    // Which list shows: everyone's flights, those of the pilots one follows, or the ranking.
    var showing by rememberSaveable { mutableStateOf(LeagueList.Everyone) }
    // The feed is a signed-in pilot's: signed out there is no such choice.
    val lists = LeagueList.entries.filter { it != LeagueList.Following || social.isSignedIn }
    val shown = showing.takeIf { it in lists } ?: LeagueList.Everyone
    Scaffold(
        modifier = Modifier.padding(bottom = bars.calculateBottomPadding()),
        topBar = {
            Column {
                TopAppBar(title = { Text("League") })
                if (hasServer) {
                    PrimaryTabRow(selectedTabIndex = lists.indexOf(shown), containerColor = MaterialTheme.colorScheme.surface) {
                        for (list in lists) {
                            Tab(
                                selected = list == shown,
                                onClick = { showing = list },
                                text = {
                                    Text(
                                        when (list) {
                                            LeagueList.Everyone -> social.words.feedEveryone
                                            LeagueList.Following -> social.words.feedFollowing
                                            LeagueList.Ranking -> social.words.feedRanking
                                        },
                                        maxLines = 1,
                                    )
                                },
                                modifier = Modifier.testTag("league-${list.name.lowercase()}"),
                            )
                        }
                    }
                }
            }
        },
        contentWindowInsets = WindowInsets(0),
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            when {
                !hasServer -> Unconnected()
                shown == LeagueList.Ranking -> RankingList(community, onOpenFlight, onOpenPilot)
                shown == LeagueList.Following -> FollowingFeed(social, community, onOpen = onOpenFlight)
                else -> PublicFlightsList(community, onOpen = onOpenFlight, onOpenPilot = onOpenPilot)
            }
        }
    }
}

/** What the League tab says that is not the core's to say. */
object LeagueWords {
    /** While Venti cannot be asked: as the maps for flying say it (`OfflineMapsWords.NO_SERVER`). */
    const val NO_SERVER_TITLE = "Venti could not be reached"
    const val NO_SERVER = "Everyone's flights and the ranking come from Venti, which could not be reached."
}

/** The lists of the League tab, in the order of its switch. */
private enum class LeagueList { Everyone, Following, Ranking }

/**
 * Without Venti there is nothing to list, and the screen says so. Venti's
 * address is built into the app, so only a test is without it.
 */
@Composable
private fun Unconnected() {
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s7),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        Icon(painterResource(R.drawable.venti_league), contentDescription = null, tint = Venti.colours.textSecondary)
        Text(LeagueWords.NO_SERVER_TITLE, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
        Text(
            LeagueWords.NO_SERVER,
            color = Venti.colours.textSecondary,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * The pilots of Venti by the points of their best flights that they share
 * with everyone, in a year or in all years. The rule, the seasons and every
 * word of a row are the core's and the server's. Under each pilot stand the
 * flights that count, each of which opens like any public flight.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RankingList(
    community: CommunityModel,
    onOpenFlight: (PublicFlight) -> Unit,
    onOpenPilot: (String, String) -> Unit,
) {
    val scope = rememberCoroutineScope()
    var isRefreshing by remember { mutableStateOf(false) }
    LaunchedEffect(community) { community.rank() }
    val ranked = community.ranked

    PullToRefreshBox(
        isRefreshing = isRefreshing,
        onRefresh = {
            scope.launch {
                isRefreshing = true
                community.rank()
                isRefreshing = false
            }
        },
        modifier = Modifier.fillMaxSize(),
    ) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag("ranking"),
            contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s1, bottom = VentiSpace.s5),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            if (ranked.seasons.isNotEmpty()) {
                item(key = "seasons") {
                    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().testTag("ranking-season")) {
                        ranked.seasons.forEachIndexed { index, season ->
                            SegmentedButton(
                                selected = season.name == ranked.season,
                                onClick = { scope.launch { community.rank(season.name) } },
                                shape = SegmentedButtonDefaults.itemShape(index, ranked.seasons.size),
                            ) {
                                Text(season.name, maxLines = 1)
                            }
                        }
                    }
                }
            }
            if (ranked.months.isNotEmpty()) {
                // The months of the season that have a ranking, in the
                // core's names. The one that is chosen, tapped again, gives
                // the whole season back.
                item(key = "months") {
                    Row(
                        modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).testTag("ranking-months"),
                        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                    ) {
                        // The whole year first, in the core's name for it: "All of 2026".
                        ranked.periods.firstOrNull { it.name == ranked.season }?.wholeName?.takeIf { it.isNotEmpty() }?.let { whole ->
                            FilterChip(
                                selected = ranked.month == null,
                                onClick = { scope.launch { community.rank(ranked.season, null) } },
                                label = { Text(whole, maxLines = 1) },
                                modifier = Modifier.testTag("ranking-whole-year"),
                            )
                        }
                        for (month in ranked.months) {
                            val isChosen = month.month == ranked.month
                            FilterChip(
                                selected = isChosen,
                                onClick = {
                                    scope.launch { community.rank(ranked.season, if (isChosen) null else month.month) }
                                },
                                label = { Text(month.name, maxLines = 1) },
                                modifier = Modifier.semantics { contentDescription = month.words },
                            )
                        }
                    }
                }
            }
            if (ranked.title.isNotEmpty()) {
                // What the ranking is of: "May 2026", "2026", "All years".
                item(key = "title") {
                    Text(
                        ranked.title,
                        style = MaterialTheme.typography.titleLarge,
                        modifier = Modifier.testTag("ranking-title").semantics { heading() },
                    )
                }
            }
            items(ranked.pilots, key = { it.handle }) { pilot ->
                RankedPilotCard(pilot, onOpenPilot = { onOpenPilot(pilot.handle, pilot.name) }, onOpenFlight = onOpenFlight)
            }
            if (ranked.pilots.isEmpty() && ranked.isLoading) {
                item(key = "loading") {
                    Box(Modifier.fillMaxWidth().padding(vertical = VentiSpace.s5), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator()
                    }
                }
            }
            if (ranked.pilots.isEmpty() && !ranked.isLoading) {
                item(key = "nobody") {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(vertical = VentiSpace.s7),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                    ) {
                        Text(
                            if (ranked.problem == null) "Nobody is ranked" else "The ranking could not be fetched",
                            style = MaterialTheme.typography.titleLarge,
                            textAlign = TextAlign.Center,
                        )
                        Text(
                            // A month without a ranked flight has the core's sentence.
                            ranked.problem ?: if (ranked.month != null) pilotsWords().monthEmpty else RankingWords.nobody(ranked.season),
                            color = Venti.colours.textSecondary,
                            textAlign = TextAlign.Center,
                        )
                    }
                }
            }
            if (ranked.pilots.isNotEmpty()) {
                item(key = "rule") { SectionFooter(ranked.rule) }
            }
        }
    }
}

/** What the ranking says in words of its own. */
object RankingWords {
    /** Why a season has no pilots, and what would change that. */
    fun nobody(season: String): String =
        if (season.isNotEmpty() && season.all(Char::isDigit)) {
            "Nobody shares a flight of $season that scores. Choose another year, or all years."
        } else {
            "A pilot is ranked once one of their flights is shared with everyone and long enough to score."
        }
}

/** A pilot's place, and under it the flights that count. */
@Composable
private fun RankedPilotCard(pilot: RankedPilot, onOpenPilot: () -> Unit, onOpenFlight: (PublicFlight) -> Unit) {
    val colours = Venti.colours
    Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerLow) {
        Column {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("ranked-pilot")
                    .clickable(role = Role.Button, onClick = onOpenPilot)
                    .heightIn(min = VentiSize.touch)
                    .padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s3)
                    .clearAndSetSemantics {
                        contentDescription = "Place ${Format.count(pilot.place.toLong())}, ${pilot.name}"
                        stateDescription = "${Format.points(pilot.points)} points, ${pilot.counted}"
                    },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    Format.count(pilot.place.toLong()),
                    style = MaterialTheme.typography.headlineSmall.numbers,
                    fontWeight = FontWeight.Bold,
                    // The first three are the ones looked for.
                    color = if (pilot.place <= 3u) colours.accentText else colours.textSecondary,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.widthIn(min = VentiSize.icon + VentiSpace.s1),
                )
                Spacer(Modifier.width(VentiSpace.s3))
                Column(Modifier.weight(1f)) {
                    Text(pilot.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(
                        pilot.counted,
                        style = MaterialTheme.typography.bodyMedium,
                        color = colours.textSecondary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Spacer(Modifier.width(VentiSpace.s2))
                Column(horizontalAlignment = Alignment.End) {
                    Text(Format.points(pilot.points), style = MaterialTheme.typography.titleLarge.numbers, fontWeight = FontWeight.Bold)
                    Text("points", style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                }
            }
            for (flight in pilot.flights) {
                HorizontalDivider(color = colours.border)
                RankedFlightLine(flight, onClick = { onOpenFlight(flight) })
            }
        }
    }
}

/** One of the flights that count for a pilot: its day, its distance and its points. */
@Composable
private fun RankedFlightLine(flight: PublicFlight, onClick: () -> Unit) {
    val colours = Venti.colours
    val score = flight.score
    val triangle = CommunityWords.triangle(flight)
    val points = score?.let { Format.points(it.points) } ?: "–"
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("ranked-flight")
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s3)
            .clearAndSetSemantics {
                contentDescription = Format.day(flight.startUnixS)
                stateDescription = listOfNotNull(triangle, CommunityWords.distance(flight), "$points points").joinToString(", ")
            },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        Text(
            Format.day(flight.startUnixS),
            style = MaterialTheme.typography.bodyMedium,
            color = colours.textSecondary,
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        if (score != null && score.kind != RouteKind.FREE_FLIGHT) {
            Icon(painterResource(R.drawable.venti_triangle), contentDescription = null, tint = colours.textSecondary, modifier = Modifier.size(VentiSpace.s4))
        }
        Text(CommunityWords.distance(flight), style = MaterialTheme.typography.bodyMedium.numbers)
        Text(
            points,
            style = MaterialTheme.typography.bodyMedium.numbers,
            textAlign = TextAlign.End,
            modifier = Modifier.widthIn(min = VentiSpace.s7),
        )
    }
}
