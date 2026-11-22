package fly.venti.app.flight

import fly.venti.app.terrain.TerrainModel
import fly.venti.app.settings.Preferences
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.SharedFiles
import fly.venti.app.account.AccountModel
import fly.venti.app.airspace.AirspaceModel
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.community.CommunityModel
import fly.venti.app.community.FlightSocialBar
import fly.venti.app.community.SocialModel
import fly.venti.app.community.ReportSheet
import fly.venti.app.community.Reported
import fly.venti.app.wind.grown
import fly.venti.core.PublicFlight
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import fly.venti.app.design.Common

/**
 * Another pilot's flight, fetched from the server and shown like one of
 * the pilot's own: the track on the map, coloured by climb, with its
 * numbers, its altitude profile and its thermals. Nothing is kept.
 *
 * Its menu is not a flight's of the logbook: there is no file of the
 * pilot's to share or delete, and nothing to replay. It leads to everything
 * else that pilot shares with everyone, hands on the flight's page on the
 * server, and has the form for telling whoever runs the server that the
 * flight should not be there.
 */
@Composable
fun PublicFlightScreen(
    flight: PublicFlight,
    community: CommunityModel,
    airspace: AirspaceModel,
    /** Pilots among each other: the flight's cheers and comments. */
    social: SocialModel,
    /** The pilot's own account, which a report is sent from. */
    account: AccountModel,
    /** The address of the server the flight is on, for the link to its page. */
    server: String?,
    /** Where a report is sent from, so that it goes on when its form is closed. */
    requests: CoroutineScope,
    /** Called to list everything else this pilot shares with everyone. */
    onPilot: () -> Unit,
    /** Where a flight to lay beside this one comes from. */
    compareSources: CompareSources? = null,
    onBack: () -> Unit,
    /** Where the choice of the aerial view is kept. */
    preferences: Preferences? = null,
    /** Where the ground under the flight comes from, for its profile. */
    terrain: TerrainModel? = null,
) {
    val context = LocalContext.current
    var isShowingMenu by remember { mutableStateOf(false) }
    var reporting by remember { mutableStateOf<Reported?>(null) }
    var opened by remember(flight.id) { mutableStateOf<CommunityModel.Opened?>(null) }
    var problem by remember(flight.id) { mutableStateOf<String?>(null) }

    LaunchedEffect(flight.id) {
        try {
            opened = community.open(flight)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = community.words(error)
        }
    }

    val all = airspace.set
    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        val shown = opened
        val failure = problem
        when {
            shown != null -> FlightDetail(
                shown.flight,
                shown.analysis,
                // Another pilot's flight is not checked against airspace:
                // that is a help for one's own review.
                airspaceCheck = null,
                // Up to the height the pilot chose for the maps.
                airspace = remember(shown.flight, all) {
                    airspace.shapes(shown.flight.bounds.grown(0.2), preferences?.mapAirspaceCeilingM)
                },
                compareSources = compareSources,
                preferences = preferences,
                terrain = terrain,
            )
            failure != null -> Failure(failure, Modifier.align(Alignment.Center))
            else -> CircularProgressIndicator(Modifier.align(Alignment.Center))
        }

        // Back, whose flight it is, and its page on the server to hand on.
        val colours = IconButtonDefaults.filledTonalIconButtonColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
            contentColor = MaterialTheme.colorScheme.onSurface,
        )
        Column(Modifier.fillMaxWidth().statusBarsPadding()) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = VentiSpace.s2, vertical = VentiSpace.s1),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                FilledTonalIconButton(onClick = onBack, colors = colours) {
                    Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                }
                Spacer(Modifier.width(VentiSpace.s2))
                Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                    Surface(shape = CircleShape, color = MaterialTheme.colorScheme.surfaceContainerLow, shadowElevation = 2.dp) {
                        Column(
                            Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Text(
                                flight.pilot,
                                style = MaterialTheme.typography.titleMedium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier
                                    .testTag("public-flight-pilot")
                                    .semantics { heading() },
                            )
                            // The site it took off from, where the server names one.
                            flight.takeoffName?.let { takeoff ->
                                Text(
                                    takeoff,
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.testTag("public-flight-takeoff"),
                                )
                            }
                        }
                    }
                }
                Spacer(Modifier.width(VentiSpace.s2))
                Box {
                    FilledTonalIconButton(onClick = { isShowingMenu = true }, colors = colours) {
                        Icon(painterResource(R.drawable.ic_more_vert), contentDescription = Common.words.more)
                    }
                    DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
                        DropdownMenuItem(
                            text = { Text("Flights of ${flight.pilot}") },
                            leadingIcon = { Icon(painterResource(R.drawable.ic_person), contentDescription = null) },
                            onClick = {
                                isShowingMenu = false
                                onPilot()
                            },
                            modifier = Modifier.testTag("public-flight-pilot-flights"),
                        )
                        if (server != null) {
                            DropdownMenuItem(
                                text = { Text("Share the flight's page") },
                                leadingIcon = { Icon(painterResource(R.drawable.ic_share), contentDescription = null) },
                                onClick = {
                                    isShowingMenu = false
                                    SharedFiles.shareText(context, server + flight.pagePath, "Share the flight's page")
                                },
                            )
                        }
                        // For what should not be here: to whoever runs the
                        // server, and to nobody else.
                        val report = Reported.flight(flight.id)
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
            }
            // Under the head: who cheered, the cheer, and the way to the comments.
            FlightSocialBar(
                flight,
                social,
                account,
                server,
                requests,
                // Between the round buttons at the map's two upper corners.
                Modifier.fillMaxWidth().padding(horizontal = VentiSize.touch + VentiSpace.s5),
            )
        }
    }

    reporting?.let { reported ->
        ReportSheet(reported, account, server, requests, onDismiss = { reporting = null })
    }
}
