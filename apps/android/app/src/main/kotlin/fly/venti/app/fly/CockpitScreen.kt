package fly.venti.app.fly

import fly.venti.app.map.MapSetup
import androidx.compose.ui.platform.LocalContext
import fly.venti.core.LiveShareView
import fly.venti.core.DaylightLeft
import fly.venti.app.airspace.AirspaceWords
import fly.venti.app.design.PillShape
import app.venti.design.VentiMotion
import kotlinx.coroutines.launch
import androidx.annotation.DrawableRes
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.Animatable
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.alpha
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.background
import androidx.compose.runtime.saveable.rememberSaveable
import fly.venti.app.maps.KeptMap
import fly.venti.core.GroundWindStation
import fly.venti.core.GroundWindAlert
import androidx.compose.material3.IconButton
import androidx.compose.foundation.clickable
import app.venti.design.VentiSpace
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.BorderStroke
import fly.venti.app.design.text
import app.venti.design.VentiSize
import app.venti.design.VentiTypeStyle
import app.venti.design.VentiType
import fly.venti.app.design.Venti
import fly.venti.app.design.PhotosCredit
import androidx.compose.ui.semantics.selected
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

/** The pages of the cockpit. "Next page" goes around them. */
enum class CockpitPage(
    /** The page in a word, for a pilot who has the screen read out. */
    val title: String,
) {
    /** The readings over the map: what the pilot flies by. */
    Instruments("Instruments"),

    /** The map as large as it gets, with the way to the place flown to. */
    Map("Map"),

    /** How the flight stands, and what can be switched. */
    Flight("Flight"),
    ;

    val next: CockpitPage get() = entries[(ordinal + 1) % entries.size]
}

/**
 * The cockpit: what the pilot looks at in the air, as the design language's
 * flight screen. It is always in the flight theme: a white ground and
 * near-black ink, one very large reading, four large ones and two buttons.
 *
 * Three pages: the readings over the map, the map with the way to the place
 * flown to, and the flight with what can be switched. Warnings stand at the
 * top of every page, and the map alone gives way to them. Nothing needs to
 * be touched in flight: while the core says the glider is circling in lift
 * the readings change from cruise numbers to thermal numbers and the map
 * zooms in.
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
    /** The region kept on the phone that covers the glider, which the map is drawn from. */
    keptMap: KeptMap? = null,
    /** Called when the pilot has held the button that ends the session. */
    onFinish: () -> Unit,
) {
    val view = fly.view ?: return
    val isReplay = fly.mode is FlySession.Mode.Replay
    var page by rememberSaveable { mutableStateOf(CockpitPage.Instruments) }
    var isChoosingTarget by remember { mutableStateOf(false) }
    var isShowingAirspace by remember { mutableStateOf(false) }
    var isShowingWinds by remember { mutableStateOf(false) }
    // Whether the wind's sheet begins with the wind on the ground: it was
    // opened from that badge or from its warning.
    var windsBeginOnTheGround by remember { mutableStateOf(false) }
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
    // Drawn from the region kept on the phone wherever one covers the
    // glider: the map is made anew when the glider comes into one or
    // leaves it.
    val map = remember(keptMap) { CockpitMap() }
    // The thermal map on the cockpit's map, one choice with the map tab's;
    // never for a region kept for flying without a network.
    val context = LocalContext.current
    val offersThermals = remember(keptMap) { keptMap == null && MapSetup.offersThermals(context) }
    var thermals by remember { mutableStateOf(preferences.mapShowsThermals) }
    // The ground: the drawn map or photographs from above, one choice with every other map.
    var aerial by remember { mutableStateOf(preferences.mapShowsAerial) }
    // Who made the photographs in view, as the map last said.
    var photos by remember { mutableStateOf<String?>(null) }
    val way = fly.way
    val route = fly.score?.route.orEmpty()
    // The airspace the loudest warning is about: the map steps back as far
    // as it takes to show it.
    val airspaceDistance = fly.airspaceWarnings.firstOrNull { !it.silenced }?.distanceM
    val openWinds = {
        windsBeginOnTheGround = false
        isShowingWinds = true
    }
    val openGroundWinds = {
        windsBeginOnTheGround = true
        isShowingWinds = true
    }

    KeepScreenOn()
    // With the first position, and a network, the airspace around it is
    // fetched if what the phone keeps is old or from elsewhere. A change
    // reaches the running session by itself.
    val hasPosition = view.position != null
    LaunchedEffect(hasPosition) {
        view.position?.let(airspace::refreshSoon)
    }
    // The cockpit covers the whole app and ends only by "Hold to end": the
    // system's back does nothing here, so that a thumb on the edge of the
    // screen ends no flight.
    BackHandler {}

    Box(Modifier.fillMaxSize()) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(Modifier.systemBarsPadding()) {
                StatusLine(view, fly.mode, varioName, onReplaySpeed = fly::setReplaySpeed, mapIsKept = keptMap != null)
                // What the pilot must know now, on every page.
                Alerts(
                    fly,
                    view,
                    isReplay,
                    onAirspace = { isShowingAirspace = true },
                    onGroundWinds = openGroundWinds,
                )
                when (page) {
                    CockpitPage.Instruments -> InstrumentsPage(view, way, onWinds = openWinds)
                    CockpitPage.Map -> Unit
                    CockpitPage.Flight -> FlightPage(
                        view,
                        fly.score,
                        isReplay,
                        isMuted = fly.isMuted,
                        onMute = { fly.isMuted = !fly.isMuted },
                        onLock = { isLocked = true },
                        live = fly.live,
                        onLive = { isShowingLive = true },
                        onRecordNow = fly::recordNow,
                        showsThermals = thermals.takeIf { offersThermals },
                        onThermals = {
                            thermals = !thermals
                            preferences.mapShowsThermals = thermals
                        },
                    )
                }
                CockpitRule()
                // The map alone gives way when a warning takes room at the
                // top: the readings keep their size and their place. On the
                // first page the thermal view stands beside it, on the left.
                Row(Modifier.weight(1f).fillMaxWidth().heightIn(min = 120.dp)) {
                    if (page == CockpitPage.Instruments) {
                        ThermalView(
                            view,
                            Modifier.weight(THERMAL_VIEW_SHARE).fillMaxHeight().padding(start = VentiSpace.s2, top = VentiSpace.s2, bottom = VentiSpace.s2),
                        )
                    }
                    Box(Modifier.weight(if (page == CockpitPage.Instruments) 1 - THERMAL_VIEW_SHARE else 1f).fillMaxHeight()) {
                        MapSurface(
                            content = map,
                            modifier = Modifier.fillMaxSize(),
                            kept = keptMap,
                            onPhotosCredit = { photos = it },
                            onUpdate = {
                                map.airspace = fly.airspace()
                                map.range = fly.range
                                map.gliders = fly.gliders
                                // Beside the thermal view the map keeps to the glide:
                                // the circle is looked at in the close-up.
                                map.closeUp = page == CockpitPage.Instruments
                                // Not on the first page's small map beside the
                                // thermal view, which the credit would cover.
                                map.showAerial(aerial && page != CockpitPage.Instruments)
                                map.showThermals(thermals && offersThermals)
                                map.show(view, way, route, airspaceDistance, fai, showsWholeFlight, task, taskStatus)
                            },
                        )
                        // The landing to think of first, and daylight, at the
                        // upper left.
                        Column(
                            modifier = Modifier.align(Alignment.TopStart).padding(VentiSpace.s2),
                            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                        ) {
                            fly.daylight?.let { DaylightBadge(it) }
                            fly.landing?.let { landing ->
                                LandingBadge(landing, onOpen = { isChoosingTarget = true })
                            }
                        }
                        Column(
                            modifier = Modifier.align(Alignment.TopEnd).padding(VentiSpace.s2),
                            horizontalAlignment = Alignment.End,
                            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                        ) {
                            // On the first page the wind is among the readings.
                            if (page != CockpitPage.Instruments) {
                                view.wind?.let { wind -> WindBadge(wind, view.windAgeS, onOpen = openWinds) }
                            }
                            // A wind that leaves the glider little way into it.
                            view.strongWind?.let { words -> StrongWindBadge(words) }
                            // The wind on the ground: the station that speaks
                            // for the landing, else the nearest.
                            fly.windBelow?.stations?.firstOrNull()?.let { station ->
                                GroundWindBadge(station, onOpen = openGroundWinds)
                            }
                            // While circling: which way the lift is stronger. On
                            // the first page the thermal view says so under it.
                            if (page != CockpitPage.Instruments) {
                                view.lift?.centre?.let { centre -> LiftBadge(centre) }
                            }
                        }
                        Column(
                            // Clear of the map's own credits in the corner.
                            modifier = Modifier.align(Alignment.BottomStart).padding(
                                start = VentiSpace.s2,
                                bottom = VentiSpace.s6,
                            ),
                            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                        ) {
                            // Who made the photographs, by name, while they show.
                            if (aerial && page != CockpitPage.Instruments) {
                                PhotosCredit(photos)
                            }
                            if (page == CockpitPage.Map) Row(
                                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                            ) {
                                // The whole flight at a glance; again for the
                                // close view that follows the glider.
                                MapButton(
                                    if (showsWholeFlight) "Follow the glider" else "Show the whole flight",
                                    if (showsWholeFlight) R.drawable.ic_my_location else R.drawable.ic_open_in_full,
                                ) { showsWholeFlight = !showsWholeFlight }
                                // With airspace to warn from, or the ground to
                                // draw: what lies on the line ahead, seen from
                                // the side.
                                if (fly.airspaceCheck != AirspaceCheck.NO_AIRSPACE || view.groundElevationM != null) {
                                    MapButton(AheadWords.TITLE, R.drawable.venti_glide) { isShowingAhead = true }
                                }
                                // The ground under everything: the drawn map,
                                // or photographs from above.
                                MapButton(
                                    "Aerial view",
                                    R.drawable.venti_layers,
                                    Modifier.testTag("cockpit-aerial"),
                                    isOn = aerial,
                                ) {
                                    aerial = !aerial
                                    preferences.mapShowsAerial = aerial
                                }
                            }
                        }
                        // At the lower right, clear of the credits at the
                        // lower left and the landing at the upper left: on
                        // the first page the airspace that matters next,
                        // small; on the map's page the distance so far.
                        Box(
                            Modifier.align(Alignment.BottomEnd).padding(
                                start = VentiSpace.s7,
                                end = VentiSpace.s2,
                                bottom = VentiSpace.s2,
                            ),
                        ) {
                            when (page) {
                                CockpitPage.Instruments -> view.airspaceProximity?.let { AirspaceProximityBadge(it) }
                                CockpitPage.Map -> fly.score?.let { ScoreBadge(it) }
                                CockpitPage.Flight -> Unit
                            }
                        }
                    }
                }
                CockpitRule()
                if (page == CockpitPage.Map) {
                    // The airspace that matters next, large: it stays while
                    // a warning is up, and can be about another airspace
                    // than the band, which is about the most urgent warning.
                    view.airspaceProximity?.let { reading ->
                        AirspaceProximityStrip(reading)
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    }
                    // With a task: where the flight stands in it, above
                    // the line that says where to fly.
                    if (taskStatus != null) {
                        TaskStrip(taskStatus, onOpen = { isShowingTask = true })
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    }
                    TargetStrip(way, course(view), onChoose = { isChoosingTarget = true })
                    MapReadings(view)
                }
                // The two buttons, in the same place on every page. Locked,
                // the lock stands in their place.
                Foot(
                    page,
                    onNext = { page = page.next },
                    onEnd = onFinish,
                    modifier = Modifier.alpha(if (isLocked) 0f else 1f),
                )
            }
        }
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
        WindByHeightSheet(
            view.windsByHeight,
            ground = fly.windBelow,
            groundCredits = fly.windBelowCredits,
            groundFirst = windsBeginOnTheGround,
            onDismiss = { isShowingWinds = false },
        )
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

/** The line the design language draws between the readings and the map. */
@Composable
private fun CockpitRule() {
    Box(Modifier.fillMaxWidth().height(2.dp).background(MaterialTheme.colorScheme.outlineVariant))
}

/** What the pilot must know now: airspace, a strong wind at the landing, and where the flight is in its life. */
@Composable
private fun Alerts(
    fly: FlySession,
    view: CockpitView,
    isReplay: Boolean,
    onAirspace: () -> Unit,
    onGroundWinds: () -> Unit,
) {
    val notice = PhaseWords.notice(view, isReplay)
    val warning = fly.windWarning
    val hasAirspace = AirspaceWords.shown(fly.airspaceWarnings) != null || AirspaceWords.notChecking(fly.airspaceCheck) != null
    if (notice == null && warning == null && !hasAirspace) return
    Column(
        Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s2),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        AirspaceBanner(fly.airspaceWarnings, fly.airspaceCheck, onOpen = onAirspace)
        warning?.let { GroundWindBanner(it, onOpen = onGroundWinds, onPutAway = fly::putWindWarningAway) }
        notice?.let { (title, detail) -> PhaseNotice(view, title, detail) }
    }
}

/** Where the flight is in its life, in words, while it is not in the air. */
object PhaseWords {
    /** The title and the line under it, or `null` in the air. */
    fun notice(view: CockpitView, isReplay: Boolean): Pair<String, String?>? = when (view.phase) {
        FlightPhase.FLYING -> null
        FlightPhase.GROUND -> when {
            view.waitingForTakeoff -> "Ready for take-off" to "Recording starts by itself."
            view.recording -> "Recording" to "Waiting for take-off."
            else -> "Waiting for take-off" to null
        }
        FlightPhase.LANDED -> "Landed" to view.savesInS?.takeIf { !isReplay }?.let {
            "The flight is saved in ${Format.minutesSeconds(it)}."
        }
    }
}

@Composable
private fun PhaseNotice(view: CockpitView, title: String, detail: String?) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = VentiSize.touch)
            .testTag("cockpit-phase")
            .clearAndSetSemantics {
                contentDescription = title
                detail?.let { stateDescription = it }
            },
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Row(
            Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painterResource(if (view.phase == FlightPhase.LANDED) R.drawable.venti_landing else R.drawable.venti_takeoff),
                contentDescription = null,
                modifier = Modifier.size(VentiSize.icon),
            )
            Spacer(Modifier.width(VentiSpace.s3))
            Column {
                Text(title, style = fixed(VentiType.heading))
                detail?.let { Text(it, style = fixed(VentiType.label).numbers) }
            }
        }
    }
}

/** The line at the top: ready, recording or replaying, how long, and where the climb comes from. */
@Composable
private fun StatusLine(
    view: CockpitView,
    mode: FlySession.Mode?,
    varioName: String?,
    /** Called with the speed the pilot chose for a replay. */
    onReplaySpeed: (Double) -> Unit,
    /** Whether the map is drawn from a region kept on the phone, and so stays when the network goes. */
    mapIsKept: Boolean = false,
) {
    val style = fixed(VentiType.label)
    val danger = Venti.colours.danger
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        when {
            // A replay can be held at a moment and run on, slower or faster.
            mode is FlySession.Mode.Replay -> ReplaySpeedMenu(mode.speed, onChoose = onReplaySpeed)
            view.recording -> {
                Icon(
                    painterResource(R.drawable.ic_record),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = Venti.colours.accentText,
                )
                Spacer(Modifier.width(VentiSpace.s1))
                Text("Recording", style = style, maxLines = 1)
            }
            view.waitingForTakeoff -> Text("Ready", style = style, maxLines = 1, modifier = Modifier.testTag("cockpit-ready"))
        }
        view.flightTimeS?.let { seconds ->
            Spacer(Modifier.width(VentiSpace.s2))
            Text(Format.stopwatch(seconds), style = style.numbers, maxLines = 1, modifier = Modifier.testTag("flight-time"))
        }
        Spacer(Modifier.width(VentiSpace.s2))
        Spacer(Modifier.weight(1f))
        // No position for five seconds means the fix is gone.
        if (view.positionAgeS?.let { it > 5 } != false) {
            Icon(painterResource(R.drawable.ic_location_off), contentDescription = null, modifier = Modifier.size(18.dp), tint = danger)
            Spacer(Modifier.width(VentiSpace.s1))
            Text("No GPS", style = style, color = danger, maxLines = 1)
            Spacer(Modifier.width(VentiSpace.s2))
        }
        // The map is drawn from the phone: an arrow in a circle says so.
        if (mapIsKept) {
            Icon(
                painterResource(R.drawable.ic_map_kept),
                contentDescription = "Map from this phone",
                modifier = Modifier.size(18.dp).testTag("map-kept"),
            )
            Spacer(Modifier.width(VentiSpace.s2))
        }
        // An external vario that was talking and has stopped.
        val varioIsLost = view.instrument?.talking == false
        Text(
            varioText(view, mode, varioName),
            style = style,
            color = if (varioIsLost) danger else MaterialTheme.colorScheme.onSurface,
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
 * The readings of the first page, as the design language's flight screen
 * has them: the climb very large beside its scale, and four large ones in
 * fixed places. Altitude and wind never move; the other two are for
 * cruising or for thermalling.
 */
@Composable
private fun InstrumentsPage(view: CockpitView, way: WayToTarget?, onWinds: () -> Unit) {
    Column(Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s2)) {
        Row(verticalAlignment = Alignment.Top) {
            Reading(
                "Climb",
                view.climbMs?.let(Format::climb),
                "m/s",
                note = view.averageClimbMs?.let { "${Format.climb(it)} m/s average" },
                size = ReadingSize.VeryLarge,
                modifier = Modifier.weight(1f),
            )
            VarioScale(view.climbMs, Modifier.padding(top = VentiSpace.s3))
        }
        Row {
            Reading(
                "Altitude",
                view.altitudeM?.let(Format::metres),
                "m",
                note = AltitudeWords.under(view.heightAboveGroundM, view.heightAboveTakeoffM),
                modifier = Modifier.weight(1f),
            )
            if (view.circling) {
                Reading("Gained", view.thermal?.let { Format.signedMetres(it.gainM) }, "m", modifier = Modifier.weight(1f))
            } else {
                // Under it the course, in the core's words: none while the
                // glider moves too slowly for a course to mean something.
                Reading(
                    "Ground speed",
                    view.groundSpeedMs?.let(Format::kilometresPerHour),
                    "km/h",
                    note = view.courseWords,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        Row {
            if (view.circling) {
                Reading("Thermal", view.thermal?.let { Format.climb(it.climbMs) }, "m/s", modifier = Modifier.weight(1f))
            } else {
                // Under the glide the glider is making: the glide it needs
                // to reach the place it is flying to.
                Reading(
                    "Glide",
                    view.glideRatio?.let(Format::glideRatio),
                    "",
                    note = way?.requiredGlide?.let { "Needs ${Format.glideRatio(it)}" },
                    modifier = Modifier.weight(1f),
                )
            }
            // The wind as last measured while circling. One from a while
            // ago is a memory of another place and height: its age stands
            // under it. A tap lists the wind by height.
            val wind = view.wind
            Reading(
                wind?.let { "Wind from ${Format.compassPoint(it.fromDeg)}" } ?: "Wind",
                wind?.let { Format.kilometresPerHour(it.speedMs) },
                "km/h",
                note = WindBadgeWords.age(view.windAgeS)?.let { "$it old" },
                spoken = "Wind",
                onClick = onWinds,
                modifier = Modifier.weight(1f).testTag("wind-reading"),
            )
        }
    }
}

/** The readings under the map of the second page: three, of middle size. */
@Composable
private fun MapReadings(view: CockpitView) {
    Row(Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s1)) {
        Reading("Climb", view.climbMs?.let(Format::climb), "m/s", size = ReadingSize.Medium, modifier = Modifier.weight(1f))
        Reading("Altitude", view.altitudeM?.let(Format::metres), "m", size = ReadingSize.Medium, modifier = Modifier.weight(1f))
        Reading(
            "Ground speed",
            view.groundSpeedMs?.let(Format::kilometresPerHour),
            "km/h",
            size = ReadingSize.Medium,
            modifier = Modifier.weight(1f),
        )
    }
}

/** What the flight's page says of the distance: how it scores, and how far it is to close. */
object FlightPageWords {
    /** "Free flight", "Triangle, 1.2 km to close", "Closed FAI triangle". */
    fun kind(score: LiveScore): String {
        val kind = when (score.kind) {
            RouteKind.FREE_FLIGHT -> "Free flight"
            RouteKind.FREE_TRIANGLE -> if (score.closed) "Closed triangle" else "Triangle"
            RouteKind.FAI_TRIANGLE -> if (score.closed) "Closed FAI triangle" else "FAI triangle"
        }
        val gap = score.closingM
        return if (gap != null && !score.closed && gap >= 100) "$kind, ${Format.kilometres(gap)} km to close" else kind
    }
}

/** The third page: how the flight stands, and what can be switched. None of its buttons ends anything. */
@Composable
private fun FlightPage(
    view: CockpitView,
    score: LiveScore?,
    isReplay: Boolean,
    isMuted: Boolean,
    onMute: () -> Unit,
    onLock: () -> Unit,
    live: LiveShareView?,
    onLive: () -> Unit,
    onRecordNow: () -> Unit,
    /** Whether the thermal map shows on the map; `null` where the maps do not offer it. */
    showsThermals: Boolean? = null,
    onThermals: () -> Unit = {},
) {
    Column(
        Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s3),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        Row {
            Reading("Flight time", view.flightTimeS?.let(Format::stopwatch), "", modifier = Modifier.weight(1f))
            // The cross-country distance so far, and how it scores.
            Reading(
                "Distance",
                score?.let { Format.kilometres(it.distanceM) },
                "km",
                note = score?.let(FlightPageWords::kind),
                spoken = "Cross-country distance",
                modifier = Modifier.weight(1f),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
            QuietButton(
                if (isMuted) "Unmute vario" else "Mute vario",
                if (isMuted) R.drawable.ic_volume_off else R.drawable.ic_volume_up,
                Modifier.weight(1f).testTag("cockpit-mute"),
                onClick = onMute,
            )
            // Nothing on the screen answers a touch until it is unlocked
            // by holding a finger on the lock.
            QuietButton("Lock screen", R.drawable.ic_lock, Modifier.weight(1f).testTag("cockpit-lock"), onClick = onLock)
        }
        // Who watches the flight, and whether it is going out. Not in a
        // replay: what is replayed is not shared, nor recorded.
        if (!isReplay) {
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                LiveButton(live, Modifier.weight(1f), onOpen = onLive)
                // For a pilot who would rather not wait for the take-off to
                // be noticed.
                if (view.waitingForTakeoff) {
                    QuietButton("Record now", R.drawable.ic_record, Modifier.weight(1f).testTag("cockpit-record-now"), onClick = onRecordNow)
                }
            }
        }
        // The thermal map on the cockpit's map, where the maps offer it:
        // what can be switched is on this page.
        if (showsThermals != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                QuietButton(
                    if (showsThermals) "Hide thermal map" else "Show thermal map",
                    R.drawable.venti_thermal,
                    Modifier.weight(1f).testTag("cockpit-thermals"),
                    onClick = onThermals,
                )
            }
        }
    }
}

/** A button of the pack's quiet kind, as high as a gloved finger needs in flight. */
@Composable
private fun QuietButton(title: String, @DrawableRes icon: Int, modifier: Modifier = Modifier, onClick: () -> Unit) {
    OutlinedButton(
        onClick = onClick,
        modifier = modifier.heightIn(min = VentiSize.touchFlight),
        border = BorderStroke(1.5.dp, MaterialTheme.colorScheme.outline),
        contentPadding = PaddingValues(horizontal = VentiSpace.s3),
    ) {
        Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(VentiSpace.s2))
        Text(title, style = fixed(VentiType.label).copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The two buttons of the cockpit, in the same place on every page. */
@Composable
private fun Foot(page: CockpitPage, onNext: () -> Unit, onEnd: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        OutlinedButton(
            onClick = onNext,
            modifier = Modifier
                .weight(1f)
                .height(VentiSize.touchFlight)
                .testTag("cockpit-next-page")
                .semantics { stateDescription = page.title },
            border = BorderStroke(1.5.dp, MaterialTheme.colorScheme.outline),
        ) {
            Text("Next page", style = fixed(VentiType.heading), maxLines = 1)
        }
        HoldToEnd(onEnd, Modifier.weight(1f))
    }
}

/** How long a finger stays on "Hold to end" to end the session, milliseconds. */
private const val HOLD_TO_END_MILLISECONDS = 1_000

/**
 * The button that ends the session: held for a second, so that a glove, a
 * line or a drop of rain ends nothing. It fills while the finger stays, and
 * nothing asks again. TalkBack's double tap ends it, which is deliberate
 * enough.
 */
@Composable
private fun HoldToEnd(onEnd: () -> Unit, modifier: Modifier = Modifier) {
    val filled = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val end by rememberUpdatedState(onEnd)
    val primary = MaterialTheme.colorScheme.primary
    val fill = Venti.colours.primaryPressed
    Box(
        modifier
            .height(VentiSize.touchFlight)
            .clip(PillShape)
            .drawBehind {
                drawRect(primary)
                drawRect(fill, size = Size(size.width * filled.value, size.height))
            }
            .testTag("cockpit-end")
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown()
                    val filling = scope.launch {
                        filled.animateTo(1f, tween(HOLD_TO_END_MILLISECONDS, easing = LinearEasing))
                    }
                    val lifted = withTimeoutOrNull(HOLD_TO_END_MILLISECONDS.toLong()) { waitForUpOrCancellation() }
                    filling.cancel()
                    if (lifted == null) {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        end()
                    }
                    scope.launch { filled.animateTo(0f, tween(VentiMotion.fastMillis)) }
                }
            }
            .clearAndSetSemantics {
                contentDescription = "Hold to end"
                role = Role.Button
                onClick(label = "End the session") {
                    end()
                    true
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text("Hold to end", style = fixed(VentiType.heading), color = MaterialTheme.colorScheme.onPrimary, maxLines = 1)
    }
}

/**
 * A round button over the map, as large as a finger in a glove needs. With
 * an edge: flight mode has no shadows. One that switches something on has
 * its sign in the link colour while on, and says it is selected.
 */
@Composable
private fun MapButton(
    title: String,
    @DrawableRes icon: Int,
    modifier: Modifier = Modifier,
    isOn: Boolean? = null,
    onClick: () -> Unit,
) {
    Surface(
        onClick = onClick,
        shape = CircleShape,
        color = MaterialTheme.colorScheme.surface,
        contentColor = if (isOn == true) Venti.colours.link else MaterialTheme.colorScheme.onSurface,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
        modifier = modifier.size(VentiSize.touchFlight).semantics {
            contentDescription = title
            if (isOn != null) selected = isOn
        },
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
        }
    }
}

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

/** The sizes of the cockpit's readings: the pack's instrument styles. */
enum class ReadingSize(val number: VentiTypeStyle, val unit: VentiTypeStyle) {
    /** The one reading a page may have of this size. */
    VeryLarge(VentiType.instrumentXl, VentiType.heading),
    Large(VentiType.instrumentL, VentiType.label),
    Medium(VentiType.instrumentM, VentiType.label),
}

/**
 * One reading of the cockpit: the number, and under it its name. The sizes
 * are the design language's instrument sizes and do not follow the phone's
 * text-size setting: a reading has to be in the same place at the same
 * size every time the pilot glances at it. A value the sensors have not
 * supplied is two dashes, never an old number.
 */
@Composable
private fun Reading(
    name: String,
    /** The value as text, or `null` while the sensors have not supplied it. */
    value: String?,
    unit: String,
    modifier: Modifier = Modifier,
    /** What is worth knowing beside it, under the name. */
    note: String? = null,
    size: ReadingSize = ReadingSize.Large,
    /** The name as it is read out, where the one shown says more than a name. */
    spoken: String? = null,
    onClick: (() -> Unit)? = null,
) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    val pressable = if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier
    Column(
        modifier
            .then(pressable)
            .clearAndSetSemantics {
                contentDescription = spoken ?: name
                stateDescription = listOfNotNull(
                    when {
                        value == null -> "not available"
                        unit.isEmpty() -> value
                        else -> "$value $unit"
                    },
                    note,
                ).joinToString(", ")
                if (onClick != null) {
                    role = Role.Button
                    onClick(label = "Show the wind by height") {
                        onClick()
                        true
                    }
                }
            }
            .padding(vertical = VentiSpace.s1),
    ) {
        Row {
            Text(
                value ?: "– –",
                modifier = Modifier.weight(1f, fill = false).alignByBaseline(),
                style = fixed(size.number).numbers,
                maxLines = 1,
                softWrap = false,
                // A long value is set smaller before it is cut off; the
                // reading keeps its height.
                autoSize = TextAutoSize.StepBased(
                    minFontSize = fixedSize(size.number.sizePx.dp * 0.6f),
                    maxFontSize = fixedSize(size.number.sizePx.dp),
                    stepSize = fixedSize(1.dp),
                ),
            )
            if (unit.isNotEmpty() && value != null) {
                Text(
                    unit,
                    modifier = Modifier.padding(start = VentiSpace.s1).alignByBaseline(),
                    style = fixed(size.unit).copy(fontWeight = FontWeight.SemiBold),
                    color = quiet,
                    maxLines = 1,
                    softWrap = false,
                )
            }
        }
        Text(name, style = fixed(VentiType.label), maxLines = 1, overflow = TextOverflow.Ellipsis)
        // Its line is kept when there is nothing to say, so that the
        // readings below stay where they are.
        if (size != ReadingSize.Medium) {
            Text(
                note ?: " ",
                style = fixed(VentiType.label).copy(fontWeight = FontWeight.Normal).numbers,
                color = Venti.colours.textMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** A text size that ignores the phone's text-size setting. */
@Composable
internal fun fixedSize(size: Dp): TextUnit = with(LocalDensity.current) { size.toSp() }

/**
 * One of the pack's type styles at a size that ignores the phone's
 * text-size setting: its pixels taken as the screen's own units.
 */
@Composable
internal fun fixed(style: VentiTypeStyle): TextStyle =
    style.text.copy(fontSize = fixedSize(style.sizePx.dp), lineHeight = fixedSize(style.lineHeightPx.dp))

/**
 * Climb and sink at a glance, as the design language draws it: three steps
 * above a line for the climb and three below for the sink, each lit in its
 * colour of the vario scale once the glider climbs or sinks that fast. It
 * is read by its shape: more steps, more climb.
 */
@Composable
private fun VarioScale(climb: Double?, modifier: Modifier = Modifier) {
    val colours = Venti.colours
    val band = climb?.let(ClimbBand::of) ?: ClimbBand.Level
    val up = listOf(ClimbBand.StrongClimb, ClimbBand.GoodClimb, ClimbBand.Climb)
    val down = listOf(ClimbBand.LightSink, ClimbBand.Sink, ClimbBand.StrongSink)
    Column(modifier.width(44.dp).clearAndSetSemantics {}, verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
        for (step in up) {
            val lit = band.ordinal >= step.ordinal
            Box(Modifier.fillMaxWidth().height(12.dp).background(if (lit) colours.vario[step.ordinal] else colours.surfaceRaised))
        }
        Box(Modifier.fillMaxWidth().height(3.dp).background(colours.text))
        for (step in down) {
            val lit = band.ordinal <= step.ordinal
            Box(Modifier.fillMaxWidth().height(12.dp).background(if (lit) colours.vario[step.ordinal] else colours.surfaceRaised))
        }
    }
}

/** How much daylight is left, over the map once it is less than two hours. */
object DaylightWords {
    /** What to show, if anything, and whether it is past sunset. */
    fun of(daylight: DaylightLeft): Pair<String, Boolean>? {
        val toSunset = daylight.minutesToSunset
        if (toSunset != null && toSunset >= 0) {
            if (toSunset >= 120) return null
            return "Sunset in ${Format.hoursMinutes(toSunset * 60)}" to false
        }
        val toDusk = daylight.minutesToDusk
        if (toDusk != null && toDusk >= 0) return "Night in ${Format.hoursMinutes(toDusk * 60)}" to true
        if (toSunset == null && toDusk == null) return null
        return "Night" to true
    }
}

@Composable
private fun DaylightBadge(daylight: DaylightLeft) {
    val (words, isLate) = DaylightWords.of(daylight) ?: return
    Badge(
        Modifier.clearAndSetSemantics {
            contentDescription = "Daylight"
            stateDescription = words
        },
        ink = if (isLate) Venti.colours.danger else null,
    ) {
        Icon(painterResource(R.drawable.ic_bedtime), contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(6.dp))
        Text(words, style = fixed(VentiType.label).copy(fontWeight = FontWeight.SemiBold).numbers, color = Color.Unspecified)
    }
}

/**
 * A small raised surface over the map, for the wind and the flight's phase.
 * `ground` and `ink` are given for one that warns; the others are plain.
 */
@Composable
internal fun Badge(
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
            // Flight mode has no shadows: a line in the ink sets it off the map.
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
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
            // Flight mode has no shadows: a line in the ink sets it off the map.
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
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
        ReachTone.Amber -> Venti.colours.cautionSoft to Venti.colours.caution
        ReachTone.Red -> Venti.colours.dangerFill to Venti.colours.onDangerFill
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
        Icon(painterResource(R.drawable.venti_landing), contentDescription = null, modifier = Modifier.size(18.dp))
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
        ground = Venti.colours.cautionSoft,
        ink = Venti.colours.caution,
    ) {
        Icon(painterResource(R.drawable.venti_caution), contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(6.dp))
        Text(words, style = MaterialTheme.typography.titleSmall.numbers, color = Color.Unspecified)
    }
}

/**
 * The wind one station measured on the ground: a flag, an arrow where the
 * air moves there, mean and gusts, and how far off the station is when it
 * neither speaks for the landing nor stands near. Strong for a landing, it
 * is on the ground of a warning, with a warning sign.
 */
@Composable
private fun GroundWindBadge(station: GroundWindStation, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val strong = station.isStrong
    Badge(
        modifier
            .testTag("ground-wind-badge")
            .clearAndSetSemantics {
                contentDescription = if (station.isAtLanding) "Wind at the landing" else "Wind on the ground"
                stateDescription = "${station.name}, ${station.wind}, ${station.age}"
                role = Role.Button
                onClick(label = "Show the wind stations around") {
                    onOpen()
                    true
                }
            },
        onClick = onOpen,
        ground = if (strong) Venti.colours.dangerSoft else null,
        ink = if (strong) Venti.colours.danger else null,
    ) {
        Icon(
            painterResource(if (strong) R.drawable.venti_caution else R.drawable.venti_windsock),
            contentDescription = null,
            modifier = Modifier.size(18.dp),
        )
        station.fromDeg?.let { from ->
            Spacer(Modifier.width(4.dp))
            Icon(
                painterResource(R.drawable.venti_arrow_wind),
                contentDescription = null,
                modifier = Modifier.size(18.dp).rotate((from + 180).toFloat()),
            )
        }
        Spacer(Modifier.width(6.dp))
        Text(station.badge, style = MaterialTheme.typography.titleSmall.numbers, color = Color.Unspecified, maxLines = 1)
    }
}

/**
 * A strong wind where the pilot means to land: what a station there
 * measured, in the core's words. A tap lists the stations around; the cross
 * puts it away.
 */
@Composable
private fun GroundWindBanner(alert: GroundWindAlert, onOpen: () -> Unit, onPutAway: () -> Unit) {
    val colours = Venti.colours
    Surface(
        color = colours.cautionSoft,
        contentColor = colours.caution,
        shape = MaterialTheme.shapes.medium,
        border = BorderStroke(1.dp, colours.caution),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Row(
                modifier = Modifier
                    .weight(1f)
                    .testTag("ground-wind-warning")
                    .clickable(onClick = onOpen)
                    .clearAndSetSemantics {
                        contentDescription = alert.title
                        stateDescription = alert.detail
                        role = Role.Button
                        onClick(label = "Show the wind stations around") {
                            onOpen()
                            true
                        }
                    }
                    .padding(start = 12.dp, top = 10.dp, bottom = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(painterResource(R.drawable.venti_windsock), contentDescription = null, modifier = Modifier.size(28.dp))
                Spacer(Modifier.width(10.dp))
                Column {
                    Text(alert.title, style = MaterialTheme.typography.titleMedium)
                    Text(alert.detail, style = MaterialTheme.typography.bodyMedium.numbers)
                }
            }
            IconButton(onClick = onPutAway, modifier = Modifier.size(VentiSize.touchFlight)) {
                Icon(painterResource(R.drawable.ic_close), contentDescription = "Put the warning away")
            }
        }
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
            painterResource(R.drawable.venti_arrow_wind),
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
            },
    ) {
        Surface(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .systemBarsPadding()
                // In the place of the two buttons at the foot.
                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
                .fillMaxWidth()
                .height(VentiSize.touchFlight)
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
            border = BorderStroke(1.5.dp, MaterialTheme.colorScheme.outline),
        ) {
            Row(horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
                Icon(painterResource(R.drawable.ic_lock), contentDescription = null)
                Spacer(Modifier.width(VentiSpace.s2))
                Text("Locked. Hold to unlock", style = fixed(VentiType.heading))
            }
        }
    }
}

/** The share of the width the thermal view takes beside the first page's map. */
private const val THERMAL_VIEW_SHARE = 0.48f
