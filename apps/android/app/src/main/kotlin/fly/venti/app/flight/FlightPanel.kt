package fly.venti.app.flight

import fly.venti.app.design.numbers
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.platform.testTag
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.clickable
import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.material3.Button
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.sp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.design.Venti
import fly.venti.app.design.text
import fly.venti.core.RouteKind
import fly.venti.app.R
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Metric
import fly.venti.core.FlightView
import fly.venti.core.FlightScore
import fly.venti.core.GroundUnderFlight
import fly.venti.core.TrackPoint
import kotlin.math.max

/**
 * The panel under the flight map, as the design language's flight screen
 * has it: the flight in one large number, what kind of flight it was and
 * when, three readings, its altitude profile, and for the pilot's own
 * flight the button that sends it to a league.
 *
 * Dragging along the profile moves a marker along the track, and the
 * readings change to that moment. The marker stays where the finger left
 * it, so the pilot can look from the chart to the map; the button in the
 * corner returns to the totals.
 *
 * It lies over the lower part of the map from edge to edge, like the page
 * it is, and goes down to the screen's edge; its own padding keeps its
 * content clear of the system's bar.
 */
@Composable
fun FlightPanel(
    flight: FlightView,
    /** How the flight scores, once that is worked out. */
    score: FlightScore?,
    cursor: Int?,
    onCursor: (Int?) -> Unit,
    isPlaying: Boolean,
    onPlaying: (Boolean) -> Unit,
    onDetails: () -> Unit,
    modifier: Modifier = Modifier,
    /** A second flight laid beside this one, if any. */
    compared: ComparedFlight? = null,
    /** Called to open the two flights side by side. */
    onShowComparison: () -> Unit = {},
    /** Hands the flight to a league. Absent for a flight that is not the pilot's own. */
    onSendToLeague: (() -> Unit)? = null,
    /** The ground under the flight, where the phone has it. */
    ground: List<GroundUnderFlight> = emptyList(),
) {
    val colours = Venti.colours
    Column(
        modifier
            .background(colours.bg)
            .drawBehind {
                // A line along its top, where the map ends.
                drawLine(colours.border, Offset(0f, 0f), Offset(size.width, 0f), strokeWidth = 1.dp.toPx())
            }
            .navigationBarsPadding()
            .padding(VentiSpace.s4),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        Header(flight, score, cursor, onCursor, isPlaying, onPlaying, onDetails)
        val moment = cursor?.let { flight.track.getOrNull(it) }
        if (moment != null) Moment(flight, moment, ground) else Totals(flight)
        if (compared != null) {
            Beside(compared, moment, cursor != null, onShowComparison)
        }
        Surface(
            shape = MaterialTheme.shapes.medium,
            color = colours.surface,
            border = BorderStroke(1.dp, colours.border),
        ) {
            Column(Modifier.padding(VentiSpace.s3), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                AltitudeChart(
                    flight = flight,
                    cursor = cursor,
                    onCursor = {
                        // A finger on the chart takes over from the playback.
                        onPlaying(false)
                        onCursor(it)
                    },
                    modifier = Modifier.fillMaxWidth().height(CHART_HEIGHT),
                    compared = compared,
                    ground = ground,
                )
                ClimbLegend()
            }
        }
        if (onSendToLeague != null) {
            Button(
                onClick = onSendToLeague,
                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("send-to-league"),
            ) {
                Text(LeagueWords.TITLE)
            }
        }
    }
}

/** The altitude profile's height: the chart's own, as on the iPhone. */
private val CHART_HEIGHT = VentiSpace.s8 + VentiSpace.s7

@Composable
private fun Header(
    flight: FlightView,
    score: FlightScore?,
    cursor: Int?,
    onCursor: (Int?) -> Unit,
    isPlaying: Boolean,
    onPlaying: (Boolean) -> Unit,
    onDetails: () -> Unit,
) {
    Row(verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f)) {
            // The flight in one number, the one the logbook's list has: the
            // scored distance. Until it is worked out, and for a flight too
            // short to score, the length of the track.
            val kilometres = Format.kilometres(score?.distanceM ?: flight.trackLengthM)
            Text(
                "$kilometres km",
                style = VentiType.hero.text.numbers,
                maxLines = 1,
                autoSize = TextAutoSize.StepBased(
                    minFontSize = VentiType.title.sizePx.sp,
                    maxFontSize = VentiType.hero.sizePx.sp,
                ),
                modifier = Modifier.testTag("flight-headline").clearAndSetSemantics {
                    contentDescription = if (score == null) "Track" else "Distance"
                    stateDescription = "$kilometres kilometres"
                },
            )
            Text(kindAndDay(flight, score), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                subtitle(flight),
                style = MaterialTheme.typography.labelLarge,
                color = Venti.colours.textSecondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (cursor != null) {
            PanelButton("Show flight totals", R.drawable.ic_cancel, MaterialTheme.colorScheme.onSurfaceVariant) {
                onPlaying(false)
                onCursor(null)
            }
        }
        PanelButton(
            title = if (isPlaying) "Pause" else "Play the flight",
            icon = if (isPlaying) R.drawable.ic_pause_circle else R.drawable.ic_play_circle,
        ) {
            onPlaying(!isPlaying)
        }
        PanelButton("Flight details", R.drawable.ic_info, onClick = onDetails)
    }
}

/** What kind of flight it was and on which day: "FAI triangle · Sat, 4 Jul 2026". The day alone until it is scored. */
internal fun kindAndDay(flight: FlightView, score: FlightScore?): String {
    val day = Format.day(flight.startUnixS, Format.zone(flight.timeZone))
    score ?: return day
    val kind = when (score.kind) {
        RouteKind.FREE_FLIGHT -> "Free flight"
        RouteKind.FREE_TRIANGLE -> if (score.closed) "Closed triangle" else "Triangle"
        RouteKind.FAI_TRIANGLE -> if (score.closed) "Closed FAI triangle" else "FAI triangle"
    }
    return "$kind · $day"
}

private fun subtitle(flight: FlightView): String {
    val zone = Format.zone(flight.timeZone)
    val times = "${Format.time(flight.startUnixS, zone)} – ${Format.time(flight.startUnixS + flight.durationS, zone)}"
    val glider = flight.glider
    return if (glider.isNullOrEmpty()) times else "$times · $glider"
}

/** Three readings under the number: the distance is that number. */
@Composable
private fun Totals(flight: FlightView) {
    Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        Metric("Airtime", Format.hoursMinutes(flight.durationS), Modifier.weight(1f), unit = "h")
        Metric("Highest", Format.metres(flight.maxAltitudeM), Modifier.weight(1f), unit = "m")
        Metric("Best climb", Format.climb(flight.maxClimbMs), Modifier.weight(1f), unit = "m/s")
    }
}

@Composable
private fun Moment(flight: FlightView, point: TrackPoint, ground: List<GroundUnderFlight>) {
    val seconds = point.seconds.toLong()
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Metric("Time", Format.time(flight.startUnixS + seconds, Format.zone(flight.timeZone)), Modifier.weight(1f))
        // How high over the ground the glider was, where the ground is
        // known; else how long it had been in the air.
        val above = GroundAtMoment.heightAbove(point.seconds, ground)
        if (above != null) {
            Metric("Over ground", Format.metres(max(above, 0.0)), Modifier.weight(1f), unit = "m")
        } else {
            Metric("In the air", Format.hoursMinutes(seconds), Modifier.weight(1f), unit = "h")
        }
        Metric("Altitude", Format.metres(point.altitudeM), Modifier.weight(1f), unit = "m")
        Metric("Climb", Format.climb(point.climbMs), Modifier.weight(1f), unit = "m/s")
    }
}

/**
 * The line that names the flight laid beside this one, and says where its
 * glider was at the marked moment, or how the two lie to each other. A tap
 * opens the two side by side.
 */
@Composable
private fun Beside(compared: ComparedFlight, moment: TrackPoint?, isMarked: Boolean, onShow: () -> Unit) {
    val colour = CompareColours.onScreen
    val words = moment?.let { compared.words(it) }
        ?: if (isMarked) CompareWords.NOT_IN_THE_AIR else compared.comparison.words
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .testTag("compare-line")
            .clickable(role = Role.Button, onClick = onShow)
            .clearAndSetSemantics {
                contentDescription = compared.name
                stateDescription = words
                role = Role.Button
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(18.dp).height(4.dp).background(colour, CircleShape))
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(compared.name, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                words,
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** An icon button in the panel's corner, with a full-size touch target. */
@Composable
private fun PanelButton(
    title: String,
    @DrawableRes icon: Int,
    tint: Color = MaterialTheme.colorScheme.primary,
    onClick: () -> Unit,
) {
    IconButton(onClick = onClick) {
        Icon(painterResource(icon), contentDescription = title, tint = tint)
    }
}

/** What the colours of the track mean, in one line. */
@Composable
fun ClimbLegend(modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth().clearAndSetSemantics {
            contentDescription = "Track colours: teal where the glider sank, grey where it held its " +
                "height, coral and red where it climbed."
        },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val style = MaterialTheme.typography.labelMedium
        val colour = MaterialTheme.colorScheme.onSurfaceVariant
        Text("Sink", style = style, color = colour)
        Spacer(Modifier.width(8.dp))
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            for (band in ClimbBand.entries) {
                Box(Modifier.weight(1f).height(6.dp).background(band.color, CircleShape))
            }
        }
        Spacer(Modifier.width(8.dp))
        Text("Climb", style = style, color = colour)
    }
}
