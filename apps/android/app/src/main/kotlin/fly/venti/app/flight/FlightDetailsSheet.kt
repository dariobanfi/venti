package fly.venti.app.flight

import fly.venti.app.design.LinkRow
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.airspace.AirspaceLayers
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.core.AirspaceCover
import fly.venti.core.AirspaceCrossing
import fly.venti.core.FlightAirspace
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
    /** The flight checked against the airspace the app has, once that is done. */
    airspace: FlightAirspace?,
    /** Called with a position in the flight's track to mark. */
    onSelect: (Int) -> Unit,
    onDismiss: () -> Unit,
    /** Whether a second flight lies beside this one. */
    isCompared: Boolean = false,
    /** Called to lay a flight beside this one, or to open the two side by side; `null` where neither can be. */
    onCompare: (() -> Unit)? = null,
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
            if (onCompare != null) {
                item {
                    Spacer(Modifier.height(16.dp))
                    Group {
                        row {
                            LinkRow(
                                if (isCompared) CompareWords.SIDE_BY_SIDE_ROW else CompareWords.LAY_BESIDE,
                                icon = R.drawable.ic_chevron_right,
                                leading = R.drawable.venti_track,
                                modifier = Modifier.testTag("compare-row"),
                                onClick = onCompare,
                            )
                        }
                    }
                }
            }
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
            if (airspace != null) {
                item { AirspaceSection(airspace, flight.startUnixS, onSelect) }
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

/** What the list says about a flight checked against airspace. */
object FlightAirspaceWords {
    const val NOT_CHECKED = "Not checked: the app has no airspace where this flight was flown."
    const val NONE_ENTERED = "The flight entered none of the airspace the app has."

    /** What stands in place of the stretches when there are none to list, or `null`. */
    fun summary(airspace: FlightAirspace): String? = when {
        airspace.cover == AirspaceCover.NONE -> NOT_CHECKED
        airspace.crossings.isEmpty() -> NONE_ENTERED
        else -> null
    }

    /** What the check can and cannot say. */
    fun caveat(cover: AirspaceCover): String {
        val limits = "Checked against the airspace the app has today, not the airspace of the day of the " +
            "flight, and without knowing what was active. A help for your own review, nothing more."
        return when (cover) {
            AirspaceCover.NONE ->
                "Add an airspace file or connect Venti in the settings, and the flight is checked " +
                    "against what the app then has."
            AirspaceCover.PARTLY ->
                "Part of this flight lies where the app has no airspace; of that part nothing is known. $limits"
            AirspaceCover.WHOLE -> limits
        }
    }

    fun name(crossing: AirspaceCrossing): String = crossing.name.ifEmpty { "Unnamed airspace" }

    /** "Class D control zone · GND to 9000 ft AMSL". */
    fun kind(crossing: AirspaceCrossing): String = "${crossing.kind} · ${crossing.lower} to ${crossing.upper}"

    /** When the flight went in, and between which heights it was inside: "10:12 · 1,704 to 1,727 m". */
    fun moment(crossing: AirspaceCrossing, flightStart: Long): String =
        "${Format.time(flightStart + crossing.entry.seconds.toLong())} · " +
            "${Format.metres(crossing.lowestM)} to ${Format.metres(crossing.highestM)} m"

    /** How long it stayed inside: "0:41 min". */
    fun duration(crossing: AirspaceCrossing): String = "${Format.minutesSeconds(crossing.seconds.toLong())} min"
}

/**
 * Where the flight was inside airspace the app has, or possibly inside.
 * Choosing a stretch marks where it began.
 */
@Composable
private fun AirspaceSection(airspace: FlightAirspace, flightStart: Long, onSelect: (Int) -> Unit) {
    SectionHeader("Airspace")
    Group {
        for (crossing in airspace.crossings) {
            row { AirspaceCrossingRow(crossing, flightStart) { onSelect(crossing.entry.trackIndex.toInt()) } }
        }
        FlightAirspaceWords.summary(airspace)?.let { summary ->
            row {
                Text(
                    summary,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
    SectionFooter(FlightAirspaceWords.caveat(airspace.cover))
}

/** One stretch of the flight inside an airspace, or possibly inside it. */
@Composable
private fun AirspaceCrossingRow(crossing: AirspaceCrossing, flightStart: Long, onClick: () -> Unit) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("airspace-crossing-row")
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            painterResource(if (crossing.uncertain == null) R.drawable.venti_caution else R.drawable.ic_help),
            contentDescription = null,
            tint = Color(0xFF000000 or AirspaceLayers.rgb(crossing.policy).toLong()),
        )
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(FlightAirspaceWords.name(crossing), style = MaterialTheme.typography.bodyLarge)
            Text(FlightAirspaceWords.kind(crossing), style = MaterialTheme.typography.bodyMedium, color = quiet)
            Text(
                FlightAirspaceWords.moment(crossing, flightStart),
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = quiet,
            )
            crossing.uncertain?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
        }
        Spacer(Modifier.width(8.dp))
        Text(FlightAirspaceWords.duration(crossing), style = MaterialTheme.typography.bodyLarge.numbers)
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
