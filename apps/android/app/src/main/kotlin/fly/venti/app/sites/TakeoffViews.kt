package fly.venti.app.sites

import fly.venti.app.community.FlightTermsLink
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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
import fly.venti.app.R
import fly.venti.app.design.Group
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.Venti
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.account.AccountModel
import fly.venti.core.Bounds
import fly.venti.core.CandidateOffer
import fly.venti.core.Coordinate
import fly.venti.core.FlownMonth
import fly.venti.core.SiteBestFlightLine
import fly.venti.core.SiteCandidateView
import fly.venti.core.SiteFlights
import fly.venti.core.SiteFlightsWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/**
 * What the public flights that began at a take-off say about it, on its
 * page: the core's count line or its line for none, what is counted, the
 * twelve months as bars, the season, the best flights as rows that open
 * the flight, and the terms the flights are given under. Nothing at all
 * for a site that is not a take-off. Every word is the core's.
 */
@Composable
fun SiteFlightsSection(flights: SiteFlights, words: SiteFlightsWords, onOpen: (SiteBestFlightLine) -> Unit) {
    if (!flights.isTakeoff) return
    Column(Modifier.testTag("site-flights")) {
        SectionHeader(words.title)
        Group {
            row {
                Column(
                    Modifier.fillMaxWidth().padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
                ) {
                    Text(
                        flights.words.ifEmpty { flights.nobodyWords },
                        style = MaterialTheme.typography.bodyLarge,
                        modifier = Modifier.testTag("site-flights-words"),
                    )
                    if (flights.about.isNotEmpty()) {
                        Text(flights.about, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
                    }
                }
            }
            if (flights.flights > 0u && flights.months.isNotEmpty()) {
                row {
                    Column(
                        Modifier.fillMaxWidth().padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
                        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                    ) {
                        Text(
                            words.monthsTitle,
                            style = MaterialTheme.typography.labelLarge,
                            color = Venti.colours.textSecondary,
                            modifier = Modifier.semantics { heading() },
                        )
                        MonthBars(flights.months, Modifier.testTag("site-flights-months"))
                        if (flights.seasonWords.isNotEmpty()) {
                            Text(
                                flights.seasonWords,
                                style = MaterialTheme.typography.bodyMedium,
                                modifier = Modifier.testTag("site-flights-season"),
                            )
                        }
                    }
                }
            }
        }
        if (flights.best.isNotEmpty()) {
            SectionHeader(words.bestTitle)
            Group {
                for (line in flights.best) {
                    row {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable(role = Role.Button) { onOpen(line) }
                                .heightIn(min = VentiSize.touch)
                                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2)
                                .testTag("site-flight"),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(line.words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                            Icon(
                                painterResource(R.drawable.ic_chevron_right),
                                contentDescription = null,
                                tint = Venti.colours.textSecondary,
                            )
                        }
                    }
                }
            }
        }
        FlightTermsLink(flights.terms, Modifier.testTag("site-flights-terms"))
    }
}

/**
 * The twelve months as a small row of bars, January first: each bar as
 * tall as its share of the busiest month, in the accent colour with the
 * corners of the logbook's chart of the months, and the month's short name
 * under it. Each month is one element for a screen reader, in the core's
 * words.
 */
@Composable
fun MonthBars(months: List<FlownMonth>, modifier: Modifier = Modifier) {
    val accent = MaterialTheme.colorScheme.primary
    val track = Venti.colours.border
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
        for (month in months) {
            Column(
                modifier = Modifier.weight(1f).clearAndSetSemantics { contentDescription = month.words },
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Box(Modifier.fillMaxWidth().height(VentiSize.touch), contentAlignment = Alignment.BottomCenter) {
                    // A hairline for a month nobody flew in, so that the row reads as twelve.
                    Box(Modifier.fillMaxWidth().height(VentiSpace.s1 / 4).background(track))
                    val share = month.share.coerceIn(0.0, 1.0).toFloat()
                    if (share > 0f) {
                        Box(
                            Modifier
                                .fillMaxWidth()
                                .fillMaxHeight(share)
                                .background(accent, RoundedCornerShape(topStart = VentiSpace.s1 * 0.75f, topEnd = VentiSpace.s1 * 0.75f)),
                        )
                    }
                }
                Text(
                    month.name,
                    style = MaterialTheme.typography.labelSmall,
                    color = Venti.colours.textSecondary,
                    maxLines = 1,
                    softWrap = false,
                )
            }
        }
    }
}

/**
 * A place on the map's layer "Take-offs without a site", tapped: the
 * core's title, its line, its height and its season, and, where the core
 * offers it to this pilot, the offer with its two buttons.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CandidateSheet(
    candidate: SiteCandidateView,
    words: SiteFlightsWords,
    offer: CandidateOffer?,
    onAdd: (CandidateOffer) -> Unit,
    onSetAside: (CandidateOffer) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                .testTag("candidate-sheet")
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    words.candidateTitle,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("candidate-done")) { Text(Common.words.done) }
            }
            Text(candidate.words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("candidate-words"))
            if (candidate.altitudeWords.isNotEmpty()) {
                Text(candidate.altitudeWords, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
            }
            if (candidate.months.isNotEmpty()) {
                MonthBars(candidate.months, Modifier.testTag("candidate-months"))
            }
            if (candidate.seasonWords.isNotEmpty()) {
                Text(candidate.seasonWords, style = MaterialTheme.typography.bodyMedium)
            }
            offer?.let { offered ->
                CandidateOfferCard(offered, onAdd = { onAdd(offered) }, onSetAside = { onSetAside(offered) })
            }
        }
    }
}

/**
 * The core's offer to add a place this pilot took off at: its sentence,
 * "Add this place", which opens the form on the offer's draft, and "Not
 * this one", which sets it aside on this phone.
 */
@Composable
fun CandidateOfferCard(offer: CandidateOffer, onAdd: () -> Unit, onSetAside: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth().testTag("candidate-offer"),
        shape = RoundedCornerShape(VentiRadius.m),
        color = Venti.colours.surfaceRaised,
    ) {
        Column(Modifier.padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
            Text(offer.words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("candidate-offer-words"))
            if (offer.flight.words.isNotEmpty()) {
                Text(offer.flight.words, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                Button(
                    onClick = onAdd,
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("candidate-add"),
                ) { Text(offer.addWords) }
                OutlinedButton(
                    onClick = onSetAside,
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("candidate-set-aside"),
                ) { Text(offer.dismissWords) }
            }
        }
    }
}

/**
 * On Today, under the take-offs: at most one offer of the core's to add a
 * place this pilot took off at, among the places no site names in the part
 * of the map the take-offs lie in. Nothing while the core offers none; a
 * partner is offered none on the phone.
 */
@Composable
fun TodayCandidateOffer(
    takeoffs: List<Coordinate>,
    edit: SiteEditModel,
    account: AccountModel,
    requests: CoroutineScope,
    onAdd: (SiteFormDraft) -> Unit,
) {
    val area = remember(takeoffs) { CandidateArea.around(takeoffs) } ?: return
    var offer by remember(area) { mutableStateOf<CandidateOffer?>(null) }
    // Counts up with each "Not this one", for the next offer.
    var setAside by remember { mutableIntStateOf(0) }
    LaunchedEffect(area, account.account, setAside) {
        offer = try {
            edit.candidateOffers(edit.candidates(area)).firstOrNull()
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            null
        }
    }
    val shown = offer ?: return
    CandidateOfferCard(
        shown,
        onAdd = { edit.adding()?.let { adding -> onAdd(SiteFormDraft.offered(shown, adding)) } },
        onSetAside = {
            offer = null
            requests.launch {
                edit.setAside(shown.candidate.position)
                setAside += 1
            }
        },
        modifier = Modifier.testTag("today-candidate-offer"),
    )
}

/** The part of the map Today's take-offs lie in, as the places no site names are asked for. */
object CandidateArea {
    /** Room round the take-offs, degrees: a box of one take-off is no box. */
    const val MARGIN_DEG = 0.02

    /** The box round the places with a margin; `null` without any. Whether it is too large to ask about is the core's. */
    fun around(places: List<Coordinate>): Bounds? {
        if (places.isEmpty()) return null
        return Bounds(
            south = places.minOf { it.lat } - MARGIN_DEG,
            west = places.minOf { it.lon } - MARGIN_DEG,
            north = places.maxOf { it.lat } + MARGIN_DEG,
            east = places.maxOf { it.lon } + MARGIN_DEG,
        )
    }
}
