package fly.venti.app.fly

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilledTonalIconToggleButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Palette
import fly.venti.app.design.numbers
import fly.venti.app.map.MapSurface
import fly.venti.core.CockpitView
import fly.venti.core.FlightPhase
import fly.venti.core.VarioSourceKind
import fly.venti.core.WindView
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/**
 * The cockpit: what the pilot looks at in the air.
 *
 * Few numbers, very large, in fixed places. The map follows the glider by
 * itself. While the core says the glider is circling in lift, the lower row
 * changes from cruise numbers to thermal numbers and the map zooms in;
 * nothing needs to be touched in flight.
 */
@Composable
fun CockpitScreen(
    fly: FlySession,
    /** The connected vario's name, if one is connected. */
    varioName: String?,
    /** Called when the pilot has confirmed the end of the session. */
    onFinish: () -> Unit,
) {
    val view = fly.view ?: return
    val isReplay = fly.mode is FlySession.Mode.Replay
    var isConfirmingFinish by remember { mutableStateOf(false) }
    val map = remember { CockpitMap() }

    KeepScreenOn()
    // The cockpit covers the whole app; the back button ends it like the
    // button on screen does, and asks first.
    BackHandler { isConfirmingFinish = true }

    // A surface, so that text without a colour of its own takes the theme's.
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.systemBarsPadding()) {
            StatusLine(view, fly.mode, varioName)
            Box(Modifier.weight(1f).fillMaxWidth()) {
                MapSurface(content = map, modifier = Modifier.fillMaxSize(), onUpdate = { map.show(view) })
                if (view.position != null) {
                    GliderMark(Modifier.align(Alignment.Center))
                }
                view.wind?.let { wind ->
                    WindBadge(wind, Modifier.align(Alignment.TopEnd).padding(10.dp))
                }
                if (view.phase != FlightPhase.FLYING) {
                    Badge(Modifier.align(Alignment.TopCenter).padding(10.dp)) {
                        Text(
                            if (view.phase == FlightPhase.GROUND) "Waiting for take-off" else "Landed",
                            style = MaterialTheme.typography.titleMedium,
                        )
                    }
                }
            }
            Instruments(view)
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedButton(
                    onClick = { isConfirmingFinish = true },
                    modifier = Modifier.weight(1f).height(56.dp),
                ) {
                    Text(if (isReplay) "End replay" else "Finish flight", style = MaterialTheme.typography.titleMedium)
                }
                Spacer(Modifier.width(12.dp))
                FilledTonalIconToggleButton(
                    checked = fly.isMuted,
                    onCheckedChange = { fly.isMuted = it },
                    modifier = Modifier.size(56.dp),
                ) {
                    Icon(
                        painterResource(if (fly.isMuted) R.drawable.ic_volume_off else R.drawable.ic_volume_up),
                        contentDescription = "Mute the vario",
                    )
                }
            }
        }
    }

    if (isConfirmingFinish) {
        AlertDialog(
            onDismissRequest = { isConfirmingFinish = false },
            title = { Text(if (isReplay) "End the replay?" else "Finish the flight?") },
            text = if (isReplay) {
                null
            } else {
                { Text("Recording stops and the flight is added to the logbook.") }
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingFinish = false
                        onFinish()
                    },
                ) {
                    Text(if (isReplay) "Close the cockpit" else "Save flight")
                }
            },
            dismissButton = { TextButton(onClick = { isConfirmingFinish = false }) { Text("Cancel") } },
        )
    }
}

/** A cockpit that dims and locks is no use: the screen stays on while it shows. */
@Composable
private fun KeepScreenOn() {
    val view = LocalView.current
    DisposableEffect(view) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
}

/** The line at the top: is it recording, how long, and are the sensors there. */
@Composable
private fun StatusLine(view: CockpitView, mode: FlySession.Mode?, varioName: String?) {
    val style = MaterialTheme.typography.labelLarge
    val missing = MaterialTheme.colorScheme.error
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (mode is FlySession.Mode.Replay) {
            Icon(painterResource(R.drawable.ic_play_arrow), contentDescription = null, Modifier.size(18.dp))
            Spacer(Modifier.width(4.dp))
            Text(if (mode.speed == 1.0) "Replay" else "Replay ×${mode.speed.toInt()}", style = style, maxLines = 1)
        } else if (view.recording) {
            Icon(
                painterResource(R.drawable.ic_record),
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = missing,
            )
            Spacer(Modifier.width(4.dp))
            Text("Recording", style = style, maxLines = 1)
        }
        view.flightTimeS?.let { seconds ->
            Spacer(Modifier.width(8.dp))
            Text(Format.stopwatch(seconds), style = style.numbers, maxLines = 1)
        }
        Spacer(Modifier.weight(1f))
        // No position for five seconds means the fix is gone.
        if (view.positionAgeS?.let { it > 5 } != false) {
            Icon(
                painterResource(R.drawable.ic_location_off),
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = missing,
            )
            Spacer(Modifier.width(4.dp))
            Text("No position", style = style, color = missing, maxLines = 1)
            Spacer(Modifier.width(8.dp))
        }
        // An external vario that was talking and has stopped.
        val varioIsLost = view.instrument?.talking == false
        Text(
            varioText(view, mode, varioName),
            style = style,
            color = if (varioIsLost) missing else MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

private fun varioText(view: CockpitView, mode: FlySession.Mode?, varioName: String?): String {
    view.instrument?.let { instrument ->
        val name = varioName ?: "Vario"
        if (!instrument.talking) return "$name silent"
        return instrument.batteryPercent?.let { "$name $it%" } ?: name
    }
    // In a replay the pressure is the recording's, not this phone's.
    if (mode is FlySession.Mode.Replay) return "Recorded data"
    return when (view.varioSource) {
        VarioSourceKind.PRESSURE -> "Phone barometer"
        VarioSourceKind.SATELLITE -> "Satellite vario"
        VarioSourceKind.INSTRUMENT, null -> "No vario yet"
    }
}

/**
 * The numbers. The upper pair never changes; the lower row is for cruising
 * or for thermalling.
 */
@Composable
private fun Instruments(view: CockpitView) {
    Column {
        HorizontalDivider()
        Row(Modifier.height(LARGE_ROW)) {
            Instrument(
                title = "Altitude",
                value = view.altitudeM?.let(Format::metres),
                unit = "m",
                detail = view.heightAboveTakeoffM?.let { "${Format.signedMetres(it)} m over take-off" },
                isLarge = true,
                modifier = Modifier.weight(1f),
            )
            VerticalDivider()
            Row(Modifier.weight(1f).padding(start = 12.dp)) {
                VarioBar(view.climbMs)
                Instrument(
                    title = "Climb",
                    value = view.climbMs?.let(Format::climb),
                    unit = "m/s",
                    detail = view.averageClimbMs?.let { "${Format.climb(it)} average" },
                    isLarge = true,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        HorizontalDivider()
        Row(Modifier.height(SMALL_ROW)) {
            if (view.circling) {
                Instrument("Gained", view.thermal?.let { Format.signedMetres(it.gainM) }, "m", Modifier.weight(1f))
                VerticalDivider()
                Instrument("Thermal", view.thermal?.let { Format.climb(it.climbMs) }, "m/s", Modifier.weight(1f))
                VerticalDivider()
                Instrument(
                    title = "Wind",
                    value = view.wind?.let { Format.kilometresPerHour(it.speedMs) },
                    unit = view.wind?.let { "km/h ${Format.compassPoint(it.fromDeg)}" } ?: "km/h",
                    modifier = Modifier.weight(1f),
                )
            } else {
                Instrument(
                    "Speed", view.groundSpeedMs?.let(Format::kilometresPerHour), "km/h", Modifier.weight(1f),
                )
                VerticalDivider()
                Instrument("Glide", view.glideRatio?.let(Format::glideRatio), "", Modifier.weight(1f))
                VerticalDivider()
                Instrument(
                    "Take-off", view.distanceToTakeoffM?.let(Format::kilometres), "km", Modifier.weight(1f),
                )
            }
        }
        HorizontalDivider()
    }
}

/** The height of the row with altitude and climb, and of the row below it. Fixed, like the type in them. */
private val LARGE_ROW = 106.dp
private val SMALL_ROW = 74.dp

/**
 * One number in the cockpit. The sizes are fixed on purpose, and do not
 * follow the phone's text-size setting: an instrument has to be in the same
 * place at the same size every time the pilot glances at it.
 */
@Composable
private fun Instrument(
    title: String,
    /** The value as text, or `null` while the sensors have not supplied it. */
    value: String?,
    unit: String,
    modifier: Modifier = Modifier,
    detail: String? = null,
    isLarge: Boolean = false,
) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    val valueSize = if (isLarge) 46.dp else 32.dp
    Column(
        modifier
            .padding(horizontal = 14.dp, vertical = 8.dp)
            .clearAndSetSemantics {
                contentDescription = title
                stateDescription = when {
                    value == null -> "not available"
                    unit.isEmpty() -> value
                    else -> "$value $unit"
                }
            },
    ) {
        Text(title, style = fixed(12.dp, FontWeight.SemiBold), color = quiet, maxLines = 1)
        Row {
            Text(
                value ?: "–",
                modifier = Modifier.weight(1f, fill = false).alignByBaseline(),
                style = fixed(valueSize, FontWeight.SemiBold).numbers,
                maxLines = 1,
                softWrap = false,
                // A long value is set smaller before it is cut off; the
                // row keeps its height.
                autoSize = TextAutoSize.StepBased(
                    minFontSize = fixedSize(valueSize / 2),
                    maxFontSize = fixedSize(valueSize),
                    stepSize = fixedSize(1.dp),
                ),
            )
            if (unit.isNotEmpty()) {
                Text(
                    unit,
                    modifier = Modifier.padding(start = 4.dp).alignByBaseline(),
                    style = fixed(if (isLarge) 17.dp else 14.dp, FontWeight.Medium),
                    color = quiet,
                    maxLines = 1,
                    softWrap = false,
                )
            }
        }
        if (isLarge) {
            Text(
                detail ?: " ",
                style = fixed(13.dp, FontWeight.Normal).numbers,
                color = quiet,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** A text size that ignores the phone's text-size setting. */
@Composable
private fun fixedSize(size: Dp): TextUnit = with(LocalDensity.current) { size.toSp() }

@Composable
private fun fixed(size: Dp, weight: FontWeight): TextStyle =
    TextStyle(fontSize = fixedSize(size), lineHeight = fixedSize(size * 1.2f), fontWeight = weight)

/**
 * Climb and sink as a bar that grows up or down from the middle, in the
 * colours the track has on the map. Full scale is 5 m/s.
 */
@Composable
private fun VarioBar(climb: Double?) {
    val groove = MaterialTheme.colorScheme.surfaceContainerHighest
    Canvas(Modifier.width(10.dp).fillMaxHeight().padding(vertical = 10.dp).clearAndSetSemantics {}) {
        val half = size.height / 2
        val rate = climb ?: 0.0
        val length = max((min(abs(rate) / 5, 1.0) * half).toFloat(), 2.dp.toPx())
        val round = CornerRadius(size.width / 2)
        drawRoundRect(groove, cornerRadius = round)
        drawRoundRect(
            ClimbBand.of(rate).color,
            topLeft = Offset(0f, if (rate >= 0) half - length else half),
            size = Size(size.width, length),
            cornerRadius = CornerRadius(3.dp.toPx()),
        )
    }
}

/**
 * The glider: a dot in the accent colour with a white ring, in the middle
 * of the map, which the map keeps under it. The same mark as the one a
 * finished flight's map puts at the moment being inspected.
 */
@Composable
private fun GliderMark(modifier: Modifier = Modifier) {
    Canvas(modifier.size(23.dp).clearAndSetSemantics {}) {
        drawCircle(Color.White)
        drawCircle(Palette.accentOnMap, radius = 9.dp.toPx())
    }
}

/** A small raised surface over the map, for the wind and the flight's phase. */
@Composable
private fun Badge(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Surface(
        modifier = modifier,
        shape = CircleShape,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        shadowElevation = 3.dp,
    ) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            content()
        }
    }
}

/** The wind over the map: an arrow pointing the way the air moves, and its speed. */
@Composable
private fun WindBadge(wind: WindView, modifier: Modifier = Modifier) {
    val speed = Format.kilometresPerHour(wind.speedMs)
    Badge(
        modifier.clearAndSetSemantics {
            contentDescription = "Wind"
            stateDescription = "$speed kilometres per hour from ${Format.compassPoint(wind.fromDeg)}"
        },
    ) {
        Icon(
            painterResource(R.drawable.ic_arrow_upward),
            contentDescription = null,
            // The arrow shows where the wind goes; the map is north-up.
            modifier = Modifier.size(20.dp).rotate((wind.fromDeg + 180).toFloat()),
        )
        Spacer(Modifier.width(6.dp))
        Text("$speed km/h", style = MaterialTheme.typography.titleMedium.numbers, color = Color.Unspecified)
    }
}
