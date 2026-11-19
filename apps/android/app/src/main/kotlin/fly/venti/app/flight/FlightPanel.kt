package fly.venti.app.flight

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
import androidx.compose.foundation.shape.RoundedCornerShape
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
import fly.venti.app.R
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Metric
import fly.venti.core.FlightView
import fly.venti.core.FlightScore
import fly.venti.core.TrackPoint

/**
 * The panel over the flight map: when the flight was, its headline numbers,
 * and its altitude profile.
 *
 * Dragging along the profile moves a marker along the track, and the numbers
 * change to that moment. The marker stays where the finger left it, so the
 * pilot can look from the chart to the map; the button in the corner returns
 * to the totals.
 *
 * It is a Material surface, opaque and raised: the map is the content and
 * this is a control above it.
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
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(28.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        shadowElevation = 6.dp,
    ) {
        Column(Modifier.padding(start = 18.dp, end = 18.dp, top = 12.dp, bottom = 18.dp)) {
            Header(flight, cursor, onCursor, isPlaying, onPlaying, onDetails)
            Spacer(Modifier.height(10.dp))
            val moment = cursor?.let { flight.track.getOrNull(it) }
            if (moment != null) Moment(flight, moment) else Totals(flight, score)
            Spacer(Modifier.height(14.dp))
            AltitudeChart(
                flight = flight,
                cursor = cursor,
                onCursor = {
                    // A finger on the chart takes over from the playback.
                    onPlaying(false)
                    onCursor(it)
                },
                modifier = Modifier.fillMaxWidth().height(124.dp),
            )
            Spacer(Modifier.height(12.dp))
            ClimbLegend()
        }
    }
}

@Composable
private fun Header(
    flight: FlightView,
    cursor: Int?,
    onCursor: (Int?) -> Unit,
    isPlaying: Boolean,
    onPlaying: (Boolean) -> Unit,
    onDetails: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(
                Format.day(flight.startUnixS),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                subtitle(flight),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
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

private fun subtitle(flight: FlightView): String {
    val times = "${Format.time(flight.startUnixS)} – ${Format.time(flight.startUnixS + flight.durationS)}"
    val glider = flight.glider
    return if (glider.isNullOrEmpty()) times else "$times · $glider"
}

@Composable
private fun Totals(flight: FlightView, score: FlightScore?) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Metric("Airtime", Format.hoursMinutes(flight.durationS), Modifier.weight(1f), unit = "h")
        // The scored distance, as in the logbook's list. Until it is
        // worked out, and for a flight too short to score, the length of
        // the track.
        if (score != null) {
            Metric("Distance", Format.kilometres(score.distanceM), Modifier.weight(1f), unit = "km")
        } else {
            Metric("Track", Format.kilometres(flight.trackLengthM), Modifier.weight(1f), unit = "km")
        }
        Metric("Highest", Format.metres(flight.maxAltitudeM), Modifier.weight(1f), unit = "m")
        Metric("Best climb", Format.climb(flight.maxClimbMs), Modifier.weight(1f), unit = "m/s")
    }
}

@Composable
private fun Moment(flight: FlightView, point: TrackPoint) {
    val seconds = point.seconds.toLong()
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Metric("Time", Format.time(flight.startUnixS + seconds), Modifier.weight(1f))
        Metric("In the air", Format.hoursMinutes(seconds), Modifier.weight(1f), unit = "h")
        Metric("Altitude", Format.metres(point.altitudeM), Modifier.weight(1f), unit = "m")
        Metric("Climb", Format.climb(point.climbMs), Modifier.weight(1f), unit = "m/s")
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
            contentDescription = "Track colours: blue where the glider sank, grey where it held its " +
                "height, orange and red where it climbed."
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
