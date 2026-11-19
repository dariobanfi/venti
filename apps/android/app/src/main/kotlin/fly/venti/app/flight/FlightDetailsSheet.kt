package fly.venti.app.flight

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.core.FlightAnalysis
import fly.venti.core.FlightScore
import fly.venti.core.FlightView
import fly.venti.core.GlideSummary
import fly.venti.core.RouteKind
import fly.venti.core.ThermalSummary

/**
 * Everything known about a flight, as a list. Opened from the flight panel.
 *
 * Choosing a thermal or a glide closes the sheet and marks that moment on
 * the map and the profile.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FlightDetailsSheet(
    flight: FlightView,
    /** What happened in the flight, once it has been worked out. */
    analysis: FlightAnalysis?,
    /** Called with a position in the flight's track to mark. */
    onSelect: (Int) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Flight details", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text("Done") }
        }
        LazyColumn(
            modifier = Modifier.testTag("flight-details"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp),
        ) {
            item { FlightSection(flight) }
            analysis?.score?.let { score ->
                item { ScoreSection(score) }
            }
            if (analysis != null && analysis.takeoff != null) {
                item { AirSection(analysis) }
                if (analysis.thermals.isNotEmpty()) {
                    item { ThermalsSection(analysis.thermals, flight.startUnixS, onSelect) }
                }
                if (analysis.glides.isNotEmpty()) {
                    item { GlidesSection(analysis.glides, flight.startUnixS, onSelect) }
                }
            }
            item { ExtremesSection(flight) }
            item { ColoursSection() }
            item { RecordingSection(flight) }
        }
    }
}

@Composable
private fun FlightSection(flight: FlightView) {
    SectionHeader("Flight")
    Group {
        row { ValueRow("Date", Format.day(flight.startUnixS)) }
        row { ValueRow("Take-off", Format.time(flight.startUnixS)) }
        row { ValueRow("Landing", Format.time(flight.startUnixS + flight.durationS)) }
        row { ValueRow("Airtime", "${Format.hoursMinutes(flight.durationS)} h") }
        row { ValueRow("Track length", "${Format.kilometres(flight.trackLengthM)} km") }
    }
}

@Composable
private fun ScoreSection(score: FlightScore) {
    SectionHeader("Cross-country")
    Group {
        row { ValueRow("Scored as", name(score)) }
        row { ValueRow("Distance", "${Format.kilometres(score.distanceM)} km") }
        row { ValueRow("Points", Format.points(score.points)) }
        score.closingM?.let { closing ->
            row { ValueRow("Start to finish", "${Format.kilometres(closing)} km") }
        }
    }
    SectionFooter(
        "Worked out by Venti from the World XContest's published rules, at " +
            "${if (score.pointsPerKm == 1.0) "1 point" else "${Format.plain(score.pointsPerKm)} points"} " +
            "per kilometre for this kind of route. The dashed " +
            "line on the map is the scored route. A league's own result can differ a little.",
    )
}

/** "FAI triangle, closed", "Free flight". */
private fun name(score: FlightScore): String {
    val kind = when (score.kind) {
        RouteKind.FREE_FLIGHT -> "Free flight"
        RouteKind.FREE_TRIANGLE -> "Free triangle"
        RouteKind.FAI_TRIANGLE -> "FAI triangle"
    }
    return if (score.closed) "$kind, closed" else kind
}

@Composable
private fun AirSection(analysis: FlightAnalysis) {
    SectionHeader("In the air")
    Group {
        row { ValueRow("In thermals", share(analysis.thermallingS, analysis)) }
        row { ValueRow("On glides", share(analysis.glidingS, analysis)) }
        row { ValueRow("Gained in thermals", "${Format.metres(analysis.heightGainedM)} m") }
    }
    if (analysis.landing == null) {
        SectionFooter("The recording ends in the air.")
    }
}

/** A duration with its share of the airtime: "0:07 h, 24%". */
private fun share(seconds: Long, analysis: FlightAnalysis): String {
    val time = "${Format.hoursMinutes(seconds)} h"
    if (analysis.airtimeS <= 0) return time
    return "$time, ${Format.percent(seconds.toDouble() / analysis.airtimeS)}"
}

/** Every thermal: when, how much height, how fast, and the wind it showed. */
@Composable
private fun ThermalsSection(thermals: List<ThermalSummary>, flightStart: Long, onSelect: (Int) -> Unit) {
    SectionHeader("Thermals")
    Group {
        for (thermal in thermals) {
            row {
                ValueRow(
                    title = Format.time(flightStart + thermal.entry.seconds.toLong()),
                    detail = detail(thermal),
                    value = "${Format.signedMetres(thermal.gainM)} m",
                    valueDetail = "${Format.climb(thermal.climbMs)} m/s",
                    onClick = { onSelect(thermal.entry.trackIndex.toInt()) },
                    modifier = Modifier.testTag("thermal-row"),
                )
            }
        }
    }
}

private fun detail(thermal: ThermalSummary): String {
    val minutes = Format.minutesSeconds(thermal.exit.seconds.toLong() - thermal.entry.seconds.toLong())
    val wind = thermal.wind ?: return "$minutes min"
    val speed = Format.kilometresPerHour(wind.speedMs)
    return "$minutes min · wind ${Format.compassPoint(wind.fromDeg)} $speed km/h"
}

/** Every glide: when, how far, at what glide ratio and speed. */
@Composable
private fun GlidesSection(glides: List<GlideSummary>, flightStart: Long, onSelect: (Int) -> Unit) {
    SectionHeader("Glides")
    Group {
        for (glide in glides) {
            row {
                ValueRow(
                    title = Format.time(flightStart + glide.start.seconds.toLong()),
                    detail = "${Format.kilometresPerHour(glide.speedMs)} km/h",
                    value = "${Format.kilometres(glide.distanceM)} km",
                    valueDetail = glide.glideRatio?.let { "glide ${Format.glideRatio(it)}" } ?: "no height lost",
                    onClick = { onSelect(glide.start.trackIndex.toInt()) },
                )
            }
        }
    }
}

@Composable
private fun ExtremesSection(flight: FlightView) {
    SectionHeader("Altitude and climb")
    Group {
        row { ValueRow("Highest", "${Format.metres(flight.maxAltitudeM)} m") }
        row { ValueRow("Lowest", "${Format.metres(flight.minAltitudeM)} m") }
        row { ValueRow("Best climb", "${Format.climb(flight.maxClimbMs)} m/s") }
        row { ValueRow("Strongest sink", "${Format.climb(flight.maxSinkMs)} m/s") }
    }
}

/** The exact meaning of each colour of the track, strongest climb first. */
@Composable
private fun ColoursSection() {
    SectionHeader("Track colours")
    Group {
        for (band in ClimbBand.entries.reversed()) {
            row {
                Row(
                    modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.width(22.dp).height(6.dp).background(band.color, CircleShape))
                    Spacer(Modifier.width(14.dp))
                    Text(band.label, style = MaterialTheme.typography.bodyLarge)
                }
            }
        }
    }
    SectionFooter(
        "Climb and sink along the track are averaged over ten seconds. Best climb and strongest sink " +
            "are averaged over half a minute.",
    )
}

@Composable
private fun RecordingSection(flight: FlightView) {
    SectionHeader("Recording")
    Group {
        flight.pilot?.let { pilot -> row { ValueRow("Pilot", pilot) } }
        flight.glider?.let { glider -> row { ValueRow("Glider", glider) } }
        row { ValueRow("Recorder", flight.recorder) }
        row { ValueRow("Positions", Format.count(flight.track.size.toLong())) }
        if (flight.skippedLines > 0u) {
            row { ValueRow("Unreadable lines", Format.count(flight.skippedLines.toLong())) }
        }
    }
}
