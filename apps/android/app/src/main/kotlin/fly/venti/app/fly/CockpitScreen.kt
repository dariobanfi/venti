package fly.venti.app.fly

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.material3.AlertDialog
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.FilledTonalIconToggleButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.contentColorFor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.airspace.AirspaceBanner
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.airspace.AirspaceWarningsSheet
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Palette
import fly.venti.app.design.numbers
import fly.venti.app.map.MapSurface
import fly.venti.app.settings.Preferences
import fly.venti.app.task.TaskModel
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.waypoints.place
import fly.venti.core.AirspaceCheck
import fly.venti.core.CockpitView
import fly.venti.core.FlightPhase
import fly.venti.core.LandingOption
import fly.venti.core.LiftCentreView
import fly.venti.core.LiveScore
import fly.venti.core.RouteKind
import fly.venti.core.WayToTarget
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
    /** The pilot's waypoints, to choose a place to fly to. */
    waypoints: WaypointsModel,
    /** The pilot's airspace: fetched around the first position, and credited under the warnings. */
    airspace: AirspaceModel,
    /** The task the pilot keeps: its cylinders for the map, its turnpoints for the list. */
    tasks: TaskModel,
    /** The pilot's account, through which a flight is shared, and the choice of who may watch. */
    account: AccountModel,
    preferences: Preferences,
    /** The connected vario's name, if one is connected. */
    varioName: String?,
    /** Whether the map draws where to fly for an FAI triangle: the pilot's setting. */
    showsFaiAreas: Boolean,
    /** Called when the pilot has confirmed the end of the session. */
    onFinish: () -> Unit,
) {
    val view = fly.view ?: return
    val isReplay = fly.mode is FlySession.Mode.Replay
    var isConfirmingFinish by remember { mutableStateOf(false) }
    var isChoosingTarget by remember { mutableStateOf(false) }
    var isShowingAirspace by remember { mutableStateOf(false) }
    var isShowingWinds by remember { mutableStateOf(false) }
    var isShowingAhead by remember { mutableStateOf(false) }
    var isShowingTask by remember { mutableStateOf(false) }
    var isShowingLive by remember { mutableStateOf(false) }
    // Whether the screen takes no touches: against a glove, a line or rain
    // pressing a button in flight.
    var isLocked by remember { mutableStateOf(false) }
    // The task is drawn only while the session flies one.
    val taskStatus = fly.task
    val task = tasks.task.takeIf { taskStatus != null }
    // Whether the map shows the whole flight instead of following closely.
    var showsWholeFlight by remember { mutableStateOf(false) }
    val fai = fly.faiAreas.takeIf { showsFaiAreas }
    val map = remember { CockpitMap() }
    val way = fly.way
    val route = fly.score?.route.orEmpty()
    // The airspace the loudest warning is about: the map steps back as far
    // as it takes to show it.
    val airspaceDistance = fly.airspaceWarnings.firstOrNull { !it.silenced }?.distanceM

    KeepScreenOn()
    // With the first position, and a network, the airspace around it is
    // fetched if what the phone keeps is old or from elsewhere. A change
    // reaches the running session by itself.
    val hasPosition = view.position != null
    LaunchedEffect(hasPosition) {
        view.position?.let(airspace::refreshSoon)
    }
    // The cockpit covers the whole app; the back button ends it like the
    // button on screen does, and asks first. Locked, it does nothing.
    BackHandler { if (!isLocked) isConfirmingFinish = true }

    // A surface, so that text without a colour of its own takes the theme's.
    Box(Modifier.fillMaxSize()) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
            Column(Modifier.systemBarsPadding()) {
                StatusLine(view, fly.score, fly.mode, varioName, onReplaySpeed = fly::setReplaySpeed)
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    MapSurface(
                        content = map,
                        modifier = Modifier.fillMaxSize(),
                        onUpdate = {
                            map.airspace = fly.airspace()
                            map.range = fly.range
                            map.show(view, way, route, airspaceDistance, fai, showsWholeFlight, task, taskStatus)
                        },
                    )
                    // The whole flight at a glance, with the scored route and
                    // where to go for an FAI triangle; again for the close view
                    // that follows the glider.
                    Column(
                        modifier = Modifier.align(Alignment.CenterStart).padding(10.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        SmallFloatingActionButton(
                            onClick = { showsWholeFlight = !showsWholeFlight },
                            shape = CircleShape,
                            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                            contentColor = MaterialTheme.colorScheme.onSurface,
                        ) {
                            Icon(
                                painterResource(if (showsWholeFlight) R.drawable.ic_my_location else R.drawable.ic_open_in_full),
                                contentDescription = if (showsWholeFlight) "Follow the glider" else "Show the whole flight",
                            )
                        }
                        // With airspace to warn from, or the ground to draw:
                        // what lies on the line ahead, seen from the side.
                        if (fly.airspaceCheck != AirspaceCheck.NO_AIRSPACE || view.groundElevationM != null) {
                            SmallFloatingActionButton(
                                onClick = { isShowingAhead = true },
                                shape = CircleShape,
                                containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                                contentColor = MaterialTheme.colorScheme.onSurface,
                            ) {
                                Icon(painterResource(R.drawable.ic_elevation), contentDescription = AheadWords.TITLE)
                            }
                        }
                    }
                    Column(
                        // Less room above than the other badges: a badge that
                        // can be pressed is given a taller touch target than it
                        // is drawn, and stands in its middle.
                        modifier = Modifier.align(Alignment.TopEnd).padding(top = 6.dp, end = 10.dp),
                        horizontalAlignment = Alignment.End,
                        verticalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        view.wind?.let { wind ->
                            WindBadge(wind, view.windAgeS, onOpen = { isShowingWinds = true })
                        }
                        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            // A wind that leaves the glider little way into
                            // it: said by the core, and only while it says so.
                            view.strongWind?.let { words -> StrongWindBadge(words) }
                            // While circling: which way the lift is stronger,
                            // in words beside the picture on the map.
                            view.lift?.centre?.let { centre -> LiftBadge(centre) }
                        }
                    }
                    AirspaceBanner(
                        fly.airspaceWarnings,
                        fly.airspaceCheck,
                        onOpen = { isShowingAirspace = true },
                        // Clear of the map's own credits button in the corner.
                        modifier = Modifier.align(Alignment.BottomCenter).padding(start = 44.dp, end = 10.dp, bottom = 10.dp),
                    )
                    // The landing to think of first, and whether the glide
                    // reaches it. A tap lists the places to fly to, each with
                    // its reach.
                    fly.landing?.let { landing ->
                        LandingBadge(
                            landing,
                            onOpen = { isChoosingTarget = true },
                            // A badge that can be pressed stands in the middle
                            // of a taller touch target, as the wind's does.
                            modifier = Modifier.align(Alignment.TopStart).padding(start = 10.dp, top = 6.dp),
                        )
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
                HorizontalDivider()
                // With a task: where the flight stands in it, above the line
                // that says where to fly, which is then its next turnpoint
                // unless the pilot chose another place.
                if (taskStatus != null) {
                    TaskStrip(taskStatus, onOpen = { isShowingTask = true })
                    HorizontalDivider()
                }
                TargetStrip(way, course(view), onChoose = { isChoosingTarget = true })
                Instruments(view, way)
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
                    // Nothing on the screen answers a touch until it is
                    // unlocked by holding a finger on the lock.
                    Spacer(Modifier.width(12.dp))
                    FilledTonalIconButton(onClick = { isLocked = true }, modifier = Modifier.size(56.dp)) {
                        Icon(painterResource(R.drawable.ic_lock_open), contentDescription = "Lock the screen")
                    }
                    // Who watches the flight, and whether it is going out. Not
                    // in a replay: what is replayed is not shared.
                    if (!isReplay) {
                        Spacer(Modifier.width(12.dp))
                        LiveButton(fly.live, onOpen = { isShowingLive = true })
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
        // Locked: a cover over everything takes the touches, and a lock at the
        // foot of the screen opens it when held. The cockpit goes on showing
        // and sounding as before.
        if (isLocked) {
            LockCover(onUnlock = { isLocked = false })
        }
    }

    if (isShowingAirspace) {
        AirspaceWarningsSheet(
            warnings = fly.airspaceWarnings,
            notices = airspace.credits.mapNotNull { it.notice },
            onAcknowledge = fly::acknowledgeAirspace,
            onIgnoreToday = fly::ignoreAirspaceToday,
            onUnsilence = fly::unsilenceAirspace,
            onDismiss = { isShowingAirspace = false },
        )
    }

    if (isShowingAhead) {
        AheadSheet(fly, onDismiss = { isShowingAhead = false })
    }

    if (isShowingWinds) {
        // The running session's bands: the list fills while it is open.
        WindByHeightSheet(view.windsByHeight, onDismiss = { isShowingWinds = false })
    }

    if (isChoosingTarget) {
        TargetPicker(
            waypoints = waypoints,
            position = view.position,
            chosen = fly.chosenPlace,
            reach = fly::reach,
            onChoose = { waypoint -> fly.goTo(waypoint?.place) },
            onDismiss = { isChoosingTarget = false },
        )
    }

    if (isShowingTask) {
        TaskProgressSheet(task, taskStatus, onStep = fly::stepTask, onDismiss = { isShowingTask = false })
    }

    if (isShowingLive) {
        LiveSheet(fly, account, preferences, onDismiss = { isShowingLive = false })
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

/** The direction of travel, once the glider moves fast enough for it to mean something. */
private fun course(view: CockpitView): Double? =
    if ((view.groundSpeedMs ?: 0.0) >= 2) view.courseDeg else null

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
private fun StatusLine(
    view: CockpitView,
    score: LiveScore?,
    mode: FlySession.Mode?,
    varioName: String?,
    /** Called with the speed the pilot chose for a replay. */
    onReplaySpeed: (Double) -> Unit,
) {
    val style = MaterialTheme.typography.labelLarge
    val missing = MaterialTheme.colorScheme.error
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (mode is FlySession.Mode.Replay) {
            // A replay can be held at a moment and run on, slower or faster.
            ReplaySpeedMenu(mode.speed, onChoose = onReplaySpeed)
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
            Text(Format.stopwatch(seconds), style = style.numbers, maxLines = 1, modifier = Modifier.testTag("flight-time"))
        }
        // The cross-country distance so far, once there is a kilometre of
        // it, with a triangle when it scores as one.
        if (score != null && score.distanceM >= 1000) {
            val distance = Format.kilometres(score.distanceM)
            val quiet = MaterialTheme.colorScheme.onSurfaceVariant
            Spacer(Modifier.width(10.dp))
            Row(
                modifier = Modifier.clearAndSetSemantics {
                    contentDescription = "Cross-country distance"
                    stateDescription = "$distance kilometres, ${scoreWords(score)}"
                },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painterResource(
                        if (score.kind == RouteKind.FREE_FLIGHT) R.drawable.ic_arrow_outward else R.drawable.ic_triangle,
                    ),
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = quiet,
                )
                Spacer(Modifier.width(3.dp))
                Text("$distance km", style = style.numbers, color = quiet, maxLines = 1)
            }
            // An open triangle: how far it is to where it began.
            val gap = score.closingM
            if (gap != null && !score.closed && gap >= 100) {
                Spacer(Modifier.width(8.dp))
                Text("${Format.kilometres(gap)} to close", style = style.numbers, color = quiet, maxLines = 1)
            }
        }
        // A gap also when the line is full and the spring between its two
        // halves has nothing left to give.
        Spacer(Modifier.width(8.dp))
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

private fun scoreWords(score: LiveScore): String = when (score.kind) {
    RouteKind.FREE_FLIGHT -> "free flight"
    RouteKind.FREE_TRIANGLE -> if (score.closed) "closed triangle" else "triangle"
    RouteKind.FAI_TRIANGLE -> if (score.closed) "closed FAI triangle" else "FAI triangle"
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
private fun Instruments(view: CockpitView, way: WayToTarget?) {
    Column {
        HorizontalDivider()
        Row(Modifier.height(LARGE_ROW)) {
            Instrument(
                title = "Altitude",
                value = view.altitudeM?.let(Format::metres),
                unit = "m",
                detail = AltitudeWords.under(view.heightAboveGroundM, view.heightAboveTakeoffM),
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
                // Next to the glide the glider is making: the glide it
                // needs to reach the place it is flying to.
                Instrument("Needs", way?.requiredGlide?.let(Format::neededGlide), "", Modifier.weight(1f))
            }
        }
        HorizontalDivider()
    }
}

/** The height of the row with altitude and climb, and of the row below it. Fixed, like the type in them. */
private val LARGE_ROW = 106.dp
private val SMALL_ROW = 74.dp

/** What stands under the altitude. */
object AltitudeWords {
    /**
     * Over the ground where the phone has the ground under the glider;
     * over the take-off where it has not. The ground is a model's: a
     * glider it puts a few metres under the ground stands on it.
     */
    fun under(aboveGroundM: Double?, aboveTakeoffM: Double?): String? =
        aboveGroundM?.let { "${Format.metres(maxOf(it, 0.0))} m over ground" }
            ?: aboveTakeoffM?.let { "${Format.signedMetres(it)} m over take-off" }
}

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
                // The number, and the line under it where it has one.
                stateDescription = listOfNotNull(
                    when {
                        value == null -> "not available"
                        unit.isEmpty() -> value
                        else -> "$value $unit"
                    },
                    detail,
                ).joinToString(", ")
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
 * A small raised surface over the map, for the wind and the flight's phase.
 * `ground` and `ink` are given for one that warns; the others are plain.
 */
@Composable
private fun Badge(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    ground: Color? = null,
    ink: Color? = null,
    content: @Composable RowScope.() -> Unit,
) {
    val row = @Composable {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            content()
        }
    }
    val colour = ground ?: MaterialTheme.colorScheme.surfaceContainerLow
    val content = ink ?: contentColorFor(colour)
    if (onClick == null) {
        Surface(
            modifier = modifier,
            shape = CircleShape,
            color = colour,
            contentColor = content,
            shadowElevation = 3.dp,
            content = row,
        )
    } else {
        // A surface that can be pressed keeps its press inside its shape.
        Surface(
            onClick = onClick,
            modifier = modifier,
            shape = CircleShape,
            color = colour,
            contentColor = content,
            shadowElevation = 3.dp,
            content = row,
        )
    }
}

/**
 * Where the lift is in the thermal being circled in: an arrow toward the
 * side where the climb is stronger and how far to move the circle, or that
 * the circle is centred.
 */
@Composable
private fun LiftBadge(centre: LiftCentreView, modifier: Modifier = Modifier) {
    Badge(
        modifier
            .testTag("lift-badge")
            .clearAndSetSemantics {
                contentDescription = "Lift"
                stateDescription = centre.words
            },
    ) {
        Icon(
            painterResource(if (centre.isCentred) R.drawable.ic_target else R.drawable.ic_arrow_upward),
            contentDescription = null,
            // The map is north-up, so the arrow points the way to move.
            modifier = Modifier.size(18.dp).rotate(if (centre.isCentred) 0f else centre.towardDeg.toFloat()),
        )
        Spacer(Modifier.width(6.dp))
        Text(centre.words, style = MaterialTheme.typography.titleSmall.numbers, color = Color.Unspecified)
    }
}

/**
 * The landing to think of first: its name and the height the glider is
 * expected to arrive above it, or that none of the pilot's landings is
 * within reach. By the glider's own numbers and the wind last measured, in
 * a straight line: the ground in between is not looked at.
 *
 * Plain while there is height to spare, on amber when there is little, on
 * red when no landing is within reach: grounds, not type, as with the other
 * warnings over the map.
 */
@Composable
private fun LandingBadge(landing: LandingOption, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val height = ReachWords.height(landing)
    val tone = ReachTone.of(landing.reach.reach)
    val (ground, ink) = when (tone) {
        ReachTone.Plain -> null to null
        ReachTone.Amber -> Palette.amber to Palette.inkOnAmber
        ReachTone.Red -> Palette.red to Color.White
        ReachTone.Quiet -> null to MaterialTheme.colorScheme.onSurfaceVariant
    }
    Badge(
        modifier
            // Room for the wind's badge at the other corner of a narrow phone.
            .widthIn(max = 210.dp)
            .testTag("landing-badge")
            .clearAndSetSemantics {
                contentDescription = "Landing"
                stateDescription = ReachWords.spoken(landing)
                role = Role.Button
                onClick(label = "List the places to fly to") {
                    onOpen()
                    true
                }
            },
        onClick = onOpen,
        ground = ground,
        ink = ink,
    ) {
        Icon(painterResource(R.drawable.ic_flight_land), contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(6.dp))
        val style = MaterialTheme.typography.titleSmall.numbers
        if (height != null) {
            // The name gives way; the height is always whole.
            Text(
                landing.name,
                style = style,
                color = Color.Unspecified,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Spacer(Modifier.width(6.dp))
            Text(height, style = style, color = Color.Unspecified, maxLines = 1)
        } else {
            Text(ReachWords.NO_LANDING, style = style, color = Color.Unspecified, maxLines = 1)
        }
    }
}

/**
 * A wind that leaves the glider little way into it, in the core's words:
 * "Into the wind 4 km/h". On the amber of an airspace warning that is very
 * near, in both appearances: a warning is not themed.
 */
@Composable
private fun StrongWindBadge(words: String, modifier: Modifier = Modifier) {
    Badge(
        modifier
            .testTag("strong-wind-badge")
            .clearAndSetSemantics {
                contentDescription = "Strong wind"
                stateDescription = words
            },
        ground = Palette.amber,
        ink = Palette.inkOnAmber,
    ) {
        Icon(painterResource(R.drawable.ic_warning), contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(6.dp))
        Text(words, style = MaterialTheme.typography.titleSmall.numbers, color = Color.Unspecified)
    }
}

/** What the wind's badge says beside the speed, and to a screen reader. */
object WindBadgeWords {
    /**
     * The age of a wind once it is five minutes old or more: "12 min",
     * "1 h". A fresh wind has none to show.
     */
    fun age(seconds: Double?): String? {
        if (seconds == null || seconds < SHOWN_FROM_SECONDS) return null
        val minutes = (seconds / 60).toInt()
        return if (minutes < 60) "$minutes min" else "${minutes / 60} h"
    }

    /** "11 kilometres per hour from W, measured 12 min ago" */
    fun spoken(wind: WindView, ageSeconds: Double?): String =
        "${Format.spokenKilometresPerHour(wind.speedMs)} from ${Format.compassPoint(wind.fromDeg)}" +
            age(ageSeconds)?.let { ", measured $it ago" }.orEmpty()

    /** From this age on a wind is shown with its age, seconds. */
    private const val SHOWN_FROM_SECONDS = 300.0
}

/**
 * The wind over the map: an arrow pointing the way the air moves, its
 * speed, and once it is a while old, how old.
 */
@Composable
private fun WindBadge(wind: WindView, ageSeconds: Double?, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val speed = Format.kilometresPerHour(wind.speedMs)
    Badge(
        modifier
            .testTag("wind-badge")
            .clearAndSetSemantics {
                contentDescription = "Wind"
                stateDescription = WindBadgeWords.spoken(wind, ageSeconds)
                role = Role.Button
                onClick(label = "Show the wind by height") {
                    onOpen()
                    true
                }
            },
        // A tap lists the wind of every band of height the glider has circled in.
        onClick = onOpen,
    ) {
        Icon(
            painterResource(R.drawable.ic_arrow_upward),
            contentDescription = null,
            // The arrow shows where the wind goes; the map is north-up.
            modifier = Modifier.size(20.dp).rotate((wind.fromDeg + 180).toFloat()),
        )
        Spacer(Modifier.width(6.dp))
        Text("$speed km/h", style = MaterialTheme.typography.titleMedium.numbers, color = Color.Unspecified)
        // A wind from a while ago is a memory of another place and height:
        // its age stands beside it.
        WindBadgeWords.age(ageSeconds)?.let { age ->
            Spacer(Modifier.width(6.dp))
            Text(
                age,
                style = MaterialTheme.typography.titleSmall.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** How long a finger is held on the lock to open it, milliseconds. */
private const val HOLD_TO_UNLOCK_MILLISECONDS = 800L

/**
 * Lies over the whole cockpit while it is locked and takes every touch, so
 * that nothing beneath it can be pressed. At its foot, a lock that opens it
 * when a finger is held on it.
 */
@Composable
private fun LockCover(onUnlock: () -> Unit) {
    Box(
        Modifier
            .fillMaxSize()
            .pointerInput(Unit) {
                // Every touch ends here, wherever it lands.
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false).consume()
                    do {
                        val event = awaitPointerEvent()
                        event.changes.forEach { it.consume() }
                    } while (event.changes.any { it.pressed })
                }
            }
            .clearAndSetSemantics {},
    ) {
        Surface(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .systemBarsPadding()
                .padding(bottom = 14.dp)
                .testTag("cockpit-unlock")
                .pointerInput(Unit) {
                    awaitEachGesture {
                        awaitFirstDown().consume()
                        // Held long enough: the finger has not come up.
                        val lifted = withTimeoutOrNull(HOLD_TO_UNLOCK_MILLISECONDS) { waitForUpOrCancellation() }
                        if (lifted == null) onUnlock()
                    }
                }
                .clearAndSetSemantics {
                    contentDescription = "Unlock the screen"
                    stateDescription = "Hold to unlock"
                    role = Role.Button
                    onClick(label = "Unlock") {
                        onUnlock()
                        true
                    }
                },
            shape = CircleShape,
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            shadowElevation = 6.dp,
        ) {
            Row(Modifier.padding(horizontal = 20.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(painterResource(R.drawable.ic_lock), contentDescription = null)
                Spacer(Modifier.width(8.dp))
                Text("Locked. Hold to unlock", style = MaterialTheme.typography.titleMedium)
            }
        }
    }
}
