package fly.venti.app.fly

import fly.venti.core.cockpitSourceWords
import fly.venti.core.VarioMark
import fly.venti.app.map.MapCreditsButton
import fly.venti.app.map.MapCreditsSheet
import fly.venti.app.map.MapSetup
import fly.venti.app.map.MapWords
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
import fly.venti.app.design.Palette
import androidx.compose.ui.semantics.selected
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.ui.text.rememberTextMeasurer
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
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.CornerRadius
import app.venti.design.VentiRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.composed
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
import fly.venti.app.map.OverviewMap
import fly.venti.app.today.PilotPosition
import fly.venti.app.task.inALine
import fly.venti.app.account.AccountModel
import fly.venti.app.airspace.AirspaceBanner
import fly.venti.app.cables.CableSheet
import fly.venti.app.cables.CablesModel
import fly.venti.core.AirspaceLevel
import fly.venti.core.CableWarning
import fly.venti.core.TaskLegReading
import fly.venti.core.TaskReadings
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.airspace.AirspaceWarningsSheet
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.numbers
import fly.venti.app.map.MapSurface
import fly.venti.app.settings.Preferences
import fly.venti.app.task.TaskModel
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.waypoints.place
import fly.venti.core.AirspaceCheck
import fly.venti.core.CockpitView
import fly.venti.core.Coordinate
import fly.venti.core.FlightPhase
import fly.venti.app.nearby.HelpWords
import fly.venti.core.LandingOption
import fly.venti.core.LandingReading
import fly.venti.core.LiftCentreView
import fly.venti.core.LiveScore
import fly.venti.core.RouteKind
import fly.venti.core.WayToTarget
import fly.venti.core.WindView
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.unit.IntOffset
import kotlin.math.roundToInt
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.foundation.layout.offset
import androidx.compose.animation.core.FastOutLinearInEasing
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.runtime.mutableIntStateOf

/**
 * The pages of the cockpit, in their order. A swipe across the cockpit
 * turns them, and a tap on the dots in the top line goes to the next.
 */
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

    /**
     * A task's readings, large, over the map: only while a task is flown,
     * and then the fourth and last, as `docs/design.md` and the iPhone have
     * it, so that the first three are where they are without a task.
     */
    Task("Task"),
    ;

    /** The page after this one, round to the first after the last. */
    fun next(hasTask: Boolean): CockpitPage = step(hasTask, 1)

    /** The page before this one, round to the last before the first. */
    fun previous(hasTask: Boolean): CockpitPage = step(hasTask, -1)

    private fun step(hasTask: Boolean, by: Int): CockpitPage {
        val pages = pages(hasTask)
        val here = pages.indexOf(this).takeIf { it >= 0 } ?: return pages.first()
        return pages[(here + by + pages.size) % pages.size]
    }

    companion object {
        /** The pages there are: the task's only while a task is flown. */
        fun pages(hasTask: Boolean): List<CockpitPage> = entries.filter { it != Task || hasTask }
    }
}

/**
 * The cockpit: what the pilot looks at in the air, as the design language's
 * flight screen. It is always in the flight theme: a white ground and
 * near-black ink, one very large reading and four large ones. The screen
 * is for readings: no button at its foot. A swipe turns the pages, or a
 * tap on their dots in the top line, and the flight ends from the top
 * line too, by a finger held on "End".
 *
 * Three pages: the readings over the map, the map with the way to the place
 * flown to, and the flight with what can be switched; while a task is flown
 * a fourth with the task's readings. Warnings stand at the
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
    /** The Bluetooth vario: its state is a mark in the top line, and it is chosen in the settings. */
    vario: VarioLink,
    /** Whether the map draws where to fly for an FAI triangle: the pilot's setting. */
    showsFaiAreas: Boolean,
    /** The region kept on the phone that covers the glider, which the map is drawn from. */
    keptMap: KeptMap? = null,
    /** The cables of the flight's area: fetched around the first position, and drawn on the map. */
    cables: CablesModel? = null,
    /** The maps kept for flying without a network, in a line, for the settings. */
    offlineMaps: String = "",
    /** Called to show the list of the pilot's airspace, from the settings. */
    onOpenAirspace: () -> Unit = {},
    /** Called to show the maps for flying without a network, from the settings. */
    onOpenMaps: () -> Unit = {},
    /** Called to show the pilot's task, from the settings. */
    onOpenTask: () -> Unit = {},
    /** Called with how many times faster the sample flight is to be replayed, from the settings. */
    onReplay: (Double) -> Unit = {},
    /**
     * Opens help after landing, with where the glider landed: offered under
     * "Landed" once the glider is down, never in a replay.
     */
    onAskForHelp: (Coordinate?) -> Unit = {},
    /** Called when the pilot has held the button that ends the session. */
    onFinish: () -> Unit,
) {
    val view = fly.view ?: return
    val isReplay = fly.mode is FlySession.Mode.Replay
    var chosenPage by rememberSaveable { mutableStateOf(CockpitPage.Instruments) }
    // The task's page is there only while a task is flown.
    val hasTask = fly.task != null
    val page = if (chosenPage == CockpitPage.Task && !hasTask) CockpitPage.Instruments else chosenPage
    // How far the page is moved sideways: by the finger, or while it
    // settles on the next one (plan item 16, as on the iPhone).
    val slide = remember { Animatable(0f) }
    var pageWidth by remember { mutableIntStateOf(0) }
    var isSettling by remember { mutableStateOf(false) }
    val pageScope = rememberCoroutineScope()
    // Turns the page as a pager does: the page in front goes off to the
    // side and the next comes in from the other. The dots change with the page.
    val turnPage = { toNext: Boolean ->
        if (!isSettling) {
            isSettling = true
            pageScope.launch {
                try {
                    val width = max(pageWidth, 1).toFloat()
                    slide.animateTo(if (toNext) -width else width, tween(PageSlide.OUT_MILLIS, easing = FastOutLinearInEasing))
                    chosenPage = if (toNext) page.next(hasTask) else page.previous(hasTask)
                    slide.snapTo(if (toNext) width else -width)
                    slide.animateTo(0f, tween(PageSlide.IN_MILLIS, easing = LinearOutSlowInEasing))
                } finally {
                    slide.snapTo(0f)
                    isSettling = false
                }
            }
        }
    }
    // A cable tapped on the map, by its identifier.
    var chosenCable by remember { mutableStateOf<String?>(null) }
    var isChoosingTarget by remember { mutableStateOf(false) }
    var isShowingAirspace by remember { mutableStateOf(false) }
    var isShowingWinds by remember { mutableStateOf(false) }
    // Whether the wind's sheet begins with the wind on the ground: it was
    // opened from that badge or from its warning.
    var windsBeginOnTheGround by remember { mutableStateOf(false) }
    var isShowingAhead by remember { mutableStateOf(false) }
    var isShowingTask by remember { mutableStateOf(false) }
    var isShowingLive by remember { mutableStateOf(false) }
    var isShowingDetails by remember { mutableStateOf(false) }
    // What the Fly tab listed before take-off, behind the one settings button.
    var isShowingSettings by remember { mutableStateOf(false) }
    var isChoosingVario by remember { mutableStateOf(false) }
    // Whether the screen takes no touches: against a glove, a line or rain
    // pressing a button in flight.
    var isLocked by remember { mutableStateOf(false) }
    // Once, the first time a signed-in pilot opens the cockpit: that the
    // flight is shared live (on by default, the owner, 2026-11-23). Kept as
    // said at once, so that it never comes again, tapped or not.
    var showsLiveNotice by remember { mutableStateOf(false) }
    LaunchedEffect(account.isSignedIn) {
        if (LiveNotice.isDue(preferences, isSignedIn = account.isSignedIn, isReplay = isReplay)) {
            preferences.liveNoticeShown = true
            showsLiveNotice = true
        }
    }
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
    // The thermal map on the cockpit's map: on until the pilot switches it
    // off in the settings, a choice apart from the map tab's. Fetched tile
    // by tile for the map in view, never for a region kept for flying
    // without a network.
    val context = LocalContext.current
    val offersThermals = remember(keptMap) { keptMap == null && MapSetup.offersThermals(context) }
    var thermals by remember { mutableStateOf(preferences.cockpitShowsThermals) }
    // The ground: the drawn map or photographs from above, one choice with every other map.
    var aerial by remember { mutableStateOf(preferences.mapShowsAerial) }
    // Who made the photographs in view, as the map last said.
    var photos by remember { mutableStateOf<String?>(null) }
    // Every credit of the map, behind its (i): nothing of it is written over the map.
    var showsCredits by remember { mutableStateOf(false) }
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
    // And one that cannot be read in the sun is not much more: the screen
    // goes all the way up, unless the pilot would rather keep the battery.
    FullBrightness(isOn = !preferences.cockpitKeepsBrightness)
    // With the first position, and a network, the airspace around it is
    // fetched if what the phone keeps is old or from elsewhere. A change
    // reaches the running session by itself.
    // And at once, before any position: around the pilot where the phone
    // knows where they are, else around where they last looked on the map.
    // The core says whether what is kept is due; without a network what
    // was kept stays.
    LaunchedEffect(Unit) {
        val pilot = PilotPosition(context.applicationContext)
        pilot.readIfAllowed()
        airspace.refreshSoon(pilot.place ?: OverviewMap.lastLookedAt)
    }
    val hasPosition = view.position != null
    LaunchedEffect(hasPosition) {
        view.position?.let(airspace::refreshSoon)
        view.position?.let { cables?.refreshSoon(it) }
    }
    // The cockpit covers the whole app and ends only by holding "End": the
    // system's back does nothing here, so that a thumb on the edge of the
    // screen ends no flight.
    BackHandler {}

    Box(Modifier.fillMaxSize()) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(
                Modifier
                    .systemBarsPadding()
                    .testTag("cockpit")
                    .turnsPagesBySwipe(
                        isSettling = { isSettling },
                        onSlide = { by -> pageScope.launch { slide.snapTo(by) } },
                        onLetGo = { across, flung ->
                            if (PageSlide.turns(across, flung, pageWidth.toFloat())) {
                                turnPage(across < 0)
                            } else {
                                pageScope.launch { slide.animateTo(0f, tween(PageSlide.IN_MILLIS)) }
                            }
                        },
                    ),
            ) {
                StatusLine(
                    view,
                    fly.mode,
                    checks = CockpitChecks.of(
                        view,
                        isReplay = isReplay,
                        airspaceCheck = fly.airspaceCheck,
                        varioState = vario.state,
                        varioName = vario.connectedName,
                        mapIsKept = keptMap != null,
                    ),
                    onReplaySpeed = fly::setReplaySpeed,
                    onSettings = { isShowingSettings = true },
                    page = page,
                    pages = CockpitPage.pages(hasTask),
                    onNextPage = { turnPage(true) },
                    onEnd = onFinish,
                    isLocked = isLocked,
                )
                // What the pilot must know now, on every page.
                Alerts(
                    fly,
                    view,
                    isReplay,
                    onAirspace = { isShowingAirspace = true },
                    onGroundWinds = openGroundWinds,
                    onHelp = { onAskForHelp(view.position) },
                )
                if (showsLiveNotice) {
                    LiveNoticeLine(
                        onOpen = {
                            showsLiveNotice = false
                            isShowingLive = true
                        },
                    )
                }
                // The page: the readings, the map and the strips under it.
                // It moves under the finger and settles on the next; what
                // has slid off is not drawn over the lines above.
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .onSizeChanged { pageWidth = it.width }
                        .clipToBounds()
                        .offset { IntOffset(slide.value.roundToInt(), 0) },
                ) {
                when (page) {
                    CockpitPage.Instruments -> InstrumentsPage(view, way, onWinds = openWinds)
                    CockpitPage.Map -> Unit
                    CockpitPage.Task -> TaskPage(view.taskReadings)
                    CockpitPage.Flight -> FlightPage(
                        view,
                        fly.score,
                        isReplay,
                        onLock = { isLocked = true },
                        live = fly.live,
                        onLive = { isShowingLive = true },
                        onDetails = { isShowingDetails = true },
                        onRecordNow = fly::recordNow,
                    )
                }
                CockpitRule()
                // The map alone gives way when a warning takes room at the
                // top: the readings keep their size and their place. On the
                // first page the thermal view stands beside it, on the left,
                // always: on the ground, on a glide and while circling, with
                // the wind (the owner, 2026-11-20 and 2026-11-22).
                val showsThermalView = page == CockpitPage.Instruments
                Row(Modifier.weight(1f).fillMaxWidth().heightIn(min = 120.dp)) {
                    if (showsThermalView) {
                        ThermalView(
                            view,
                            Modifier.weight(THERMAL_VIEW_SHARE).fillMaxHeight().padding(start = VentiSpace.s2, top = VentiSpace.s2, bottom = VentiSpace.s2),
                        )
                    }
                    Box(Modifier.weight(if (showsThermalView) 1 - THERMAL_VIEW_SHARE else 1f).fillMaxHeight()) {
                        MapSurface(
                            content = map,
                            modifier = Modifier.fillMaxSize(),
                            kept = keptMap,
                            onPhotosCredit = { photos = it },
                            onUpdate = {
                                map.airspace = fly.airspace()
                                map.range = fly.range
                                map.gliders = fly.gliders
                                map.cables = cables?.lines.orEmpty()
                                map.onCable = { id -> chosenCable = id }
                                map.closing = fly.score?.closing?.outline.orEmpty()
                                map.takeoffs = fly.takeoffs?.takeoffs.orEmpty().map { it.site }
                                map.others = fly.othersNear?.aircraft.orEmpty()
                                map.climbs = fly.climbsNear?.climbs.orEmpty().map { it.climb }
                                // Beside the thermal view the map keeps to the glide:
                                // the circle is looked at in the close-up.
                                map.closeUp = showsThermalView
                                // Not on the first page's small map beside the
                                // thermal view.
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
                            // On the map's page the line under the map names the
                            // nearest landing instead, once the core has one.
                            if (page != CockpitPage.Map || view.nearestLanding == null) {
                                fly.landing?.let { landing ->
                                    LandingBadge(landing, onOpen = { isChoosingTarget = true })
                                }
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
                            // How many others the networks hear around, in the core's words.
                            if (page == CockpitPage.Map) {
                                fly.othersNear?.countWords?.takeIf { it.isNotEmpty() }?.let { words -> CountBadge(words) }
                            }
                        }
                        Column(
                            modifier = Modifier.align(Alignment.BottomStart).padding(
                                start = VentiSpace.s2,
                                bottom = VentiSpace.s1,
                            ),
                            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                        ) {
                            // No credit is written over the map: the cockpit is an
                            // instrument (owner decision 23). Every credit is behind
                            // the (i) at the lower left, on every page.
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
                            // Light on the photographs, which are dark.
                            MapCreditsButton(
                                tint = if (aerial && page != CockpitPage.Instruments) Palette.haloOnMap else Palette.inkOnMap,
                                onOpen = { showsCredits = true },
                                tag = "cockpit-credits",
                            )
                        }
                        // At the lower right, clear of the (i) at the
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
                                CockpitPage.Map -> Column(
                                    horizontalAlignment = Alignment.End,
                                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                                ) {
                                    // The closing circle of an open triangle, in the core's words, over the distance so far.
                                    fly.score?.closing?.let { closing -> ClosingBadge(closing.words) }
                                    fly.score?.let { ScoreBadge(it) }
                                }
                                CockpitPage.Task, CockpitPage.Flight -> Unit
                            }
                        }
                    }
                }
                // Under the map a rule only where readings follow it.
                if (page == CockpitPage.Map) {
                    CockpitRule()
                    // The airspace that matters next, large: it stays while
                    // a warning is up, and can be about another airspace
                    // than the band, which is about the most urgent warning.
                    view.airspaceProximity?.let { reading ->
                        AirspaceProximityStrip(reading, onOpen = { isShowingAirspace = true })
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    }
                    // With a task: where the flight stands in it, above
                    // the line that says where to fly.
                    if (taskStatus != null) {
                        TaskStrip(taskStatus, onOpen = { isShowingTask = true })
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    }
                    // The nearest landing within glide in this wind, or the
                    // caution that none is: one line, in the core's words.
                    view.nearestLanding?.let { landing ->
                        NearestLandingLine(landing, onOpen = { isChoosingTarget = true })
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    }
                    // The nearest climb of others, only while the pilot has
                    // them switched on: one quiet line, a hint and no warning.
                    fly.climbsNear?.climbs?.firstOrNull()?.let { nearest ->
                        Text(
                            nearest.line,
                            style = MaterialTheme.typography.bodyMedium.numbers,
                            color = Venti.colours.textSecondary,
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag("climb-near")
                                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
                        )
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    }
                    TargetStrip(way, course(view), onChoose = { isChoosingTarget = true })
                    MapReadings(view)
                }
                }
            }
        }
        if (isLocked) {
            LockCover(onUnlock = { isLocked = false })
        }
    }

    if (showsCredits) {
        // Whose everything on the cockpit's map is, in full: the photographs
        // while they show, the thermal map, cables, take-offs, other aircraft,
        // climbs, the airspace with what its providers ask to be said (that it
        // is not certified), the wind on the ground, then the base map: the
        // region kept on the phone where the map draws its files, else the
        // core's lines for the tiles and the terrain.
        val lines = (
            listOfNotNull(
                photos.takeIf { aerial && page != CockpitPage.Instruments },
                MapSetup.THERMALS_CREDIT.takeIf { thermals && offersThermals },
                cables?.creditLine,
            ) +
                fly.takeoffs?.credits.orEmpty().map { it.text } +
                fly.othersNear?.credits.orEmpty().map { it.text } +
                fly.climbsNear?.credits.orEmpty().map { it.text } +
                airspace.credits.flatMap { listOfNotNull(it.text, it.notice) } +
                fly.windBelowCredits.map { it.text } +
                (keptMap?.credits?.takeIf { it.isNotEmpty() } ?: MapWords.baseCredits)
            ).distinct()
        MapCreditsSheet(lines, onDismiss = { showsCredits = false })
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

    chosenCable?.let { id ->
        val line = cables?.line(id)
        if (line == null) {
            chosenCable = null
        } else {
            CableSheet(line, cables.layer, onDismiss = { chosenCable = null })
        }
    }

    if (isShowingAhead) {
        AheadSheet(fly, onDismiss = { isShowingAhead = false })
    }

    if (isShowingWinds) {
        // The running session's bands: the list fills while it is open.
        WindByHeightSheet(
            view.windsByHeight,
            words = view.windWords,
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

    if (isShowingDetails) {
        // The running session's view: the rows follow the flight while it is open.
        CockpitDetailsSheet(view, onDismiss = { isShowingDetails = false })
    }

    if (isShowingSettings) {
        CockpitSettingsSheet(
            vario = varioSummary(vario),
            // Where the maps offer it: never over a region kept on the phone.
            showsThermals = thermals.takeIf { offersThermals },
            onThermals = { on ->
                thermals = on
                preferences.cockpitShowsThermals = on
            },
            airspace = airspace.inALine,
            offlineMaps = offlineMaps,
            task = tasks.task?.inALine,
            live = LiveChoice.stored(preferences).summary(account.isSignedIn),
            // A replay takes the place of a cockpit that waits with nothing
            // written, or of another replay; never of a flight recorded.
            canReplay = isReplay || !view.recording,
            onVario = {
                isShowingSettings = false
                isChoosingVario = true
            },
            onAirspace = {
                isShowingSettings = false
                onOpenAirspace()
            },
            onMaps = {
                isShowingSettings = false
                onOpenMaps()
            },
            onTask = {
                isShowingSettings = false
                onOpenTask()
            },
            onLive = {
                isShowingSettings = false
                isShowingLive = true
            },
            onReplay = { speed ->
                isShowingSettings = false
                if (isReplay) {
                    fly.setReplaySpeed(speed)
                } else {
                    onReplay(speed)
                }
            },
            onDismiss = { isShowingSettings = false },
        )
    }

    if (isChoosingVario) {
        VarioSheet(vario) { isChoosingVario = false }
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

/**
 * The screen all the way up while the cockpit shows, and back to where it
 * was when it leaves. It is this window's brightness, not the phone's
 * setting: nothing is left changed if the app is closed in between.
 */
@Composable
private fun FullBrightness(isOn: Boolean) {
    val window = LocalActivity.current?.window
    DisposableEffect(window, isOn) {
        val brightness = window?.takeIf { isOn }?.let { CockpitBrightness(WindowBrightness(it)) }
        brightness?.turnUp()
        onDispose { brightness?.putBack() }
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
    /** Opens help after landing: offered once landed, not in a replay. */
    onHelp: () -> Unit,
) {
    val notice = PhaseWords.notice(view, isReplay)
    val warning = fly.windWarning
    val isFlying = view.phase == FlightPhase.FLYING
    val hasAirspace = AirspaceWords.shown(fly.airspaceWarnings) != null ||
        AirspaceWords.notChecking(fly.airspaceCheck, isFlying) != null
    val cable = fly.cableWarning
    val carriedBack = view.carriedBack
    if (notice == null && warning == null && !hasAirspace && cable == null && carriedBack == null) return
    // An airspace warning of the level "Inside" stays above the cable's.
    val isInside = fly.airspaceWarnings.any { !it.silenced && it.level == AirspaceLevel.INSIDE }
    Column(
        Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s2),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        if (cable != null && !isInside) CableBanner(cable, onSeen = { fly.acknowledgeCable(cable.id) })
        AirspaceBanner(fly.airspaceWarnings, fly.airspaceCheck, onOpen = onAirspace, isFlying = isFlying)
        if (cable != null && isInside) CableBanner(cable, onSeen = { fly.acknowledgeCable(cable.id) })
        warning?.let { GroundWindBanner(it, onOpen = onGroundWinds, onPutAway = fly::putWindWarningAway) }
        carriedBack?.let { CarriedBackBanner(it) }
        notice?.let { (title, detail) -> PhaseNotice(view, title, detail) }
        // After landing, and only then: pilots nearby may give a lift or a hand.
        if (view.phase == FlightPhase.LANDED && !isReplay) {
            QuietButton(HelpWords.ENTRY, R.drawable.venti_landing, Modifier.fillMaxWidth().testTag("cockpit-help"), onClick = onHelp)
        }
    }
}

/** That the flight is shared live, said once in the cockpit. */
object LiveNotice {
    const val WORDS = "With Venti your flight is shared live for safety. Change it in the settings."

    /**
     * Whether the cockpit says it now: once, for a signed-in pilot whose
     * choice shares the flight, not in a replay.
     */
    fun isDue(preferences: Preferences, isSignedIn: Boolean, isReplay: Boolean): Boolean =
        !preferences.liveNoticeShown && !isReplay && isSignedIn && LiveChoice.stored(preferences).audience != null
}

/**
 * The notice that the flight is shared live: quiet, on the raised surface
 * without a sign of caution; a tap opens live tracking, where it is changed.
 */
@Composable
private fun LiveNoticeLine(onOpen: () -> Unit) {
    Surface(
        modifier = Modifier
            .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s2)
            .fillMaxWidth()
            .heightIn(min = VentiSize.touchFlight)
            .clip(MaterialTheme.shapes.medium)
            .clickable(role = Role.Button, onClickLabel = "Open live tracking", onClick = onOpen)
            .testTag("cockpit-live-notice"),
        shape = MaterialTheme.shapes.medium,
        color = Venti.colours.surfaceRaised,
    ) {
        Row(
            Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(LiveNotice.WORDS, style = fixed(VentiType.label), modifier = Modifier.weight(1f))
            Spacer(Modifier.width(VentiSpace.s2))
            Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
        }
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
        // A provisional flight is not offered as saved: it may yet be
        // taken back, a lift or a drive.
        FlightPhase.LANDED -> "Landed" to view.savesInS?.takeIf { !isReplay && !view.flightProvisional }?.let {
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

/**
 * The line at the top: ready, recording or replaying, how long, what is
 * wrong (each as a sign and a word, gone when put right), the vario's
 * mark, the settings, the pages as dots, and "End". It is as high as a
 * finger needs, so that what is in it can be pressed in flight.
 */
@Composable
private fun StatusLine(
    view: CockpitView,
    mode: FlySession.Mode?,
    /** What is wrong, the most urgent first, and the vario's mark. */
    checks: List<CockpitCheck>,
    /** Called with the speed the pilot chose for a replay. */
    onReplaySpeed: (Double) -> Unit,
    /** Opens the settings: what the Fly tab listed before take-off. */
    onSettings: () -> Unit,
    page: CockpitPage,
    pages: List<CockpitPage>,
    onNextPage: () -> Unit,
    /** Called when the pilot has held "End". */
    onEnd: () -> Unit,
    /** Locked, the lock's own control lies over the dots and "End". */
    isLocked: Boolean,
) {
    val style = fixed(VentiType.label)
    Row(
        modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).padding(start = VentiSpace.s4, end = VentiSpace.s2),
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
        // To the right, before the settings, the dots and "End": what is
        // wrong, and the vario, each a sign. The words follow in the order
        // of urgency for as long as the line has room for them whole; a
        // word that does not fit leaves its sign alone, never cut short.
        BoxWithConstraints(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) {
            val measurer = rememberTextMeasurer()
            val density = LocalDensity.current
            var room = maxWidth - (18.dp + VentiSpace.s2) * checks.size
            var wordsFit = true
            val withWords = checks.map { check ->
                val word = check.word ?: return@map false
                val width = with(density) { measurer.measure(word, style.numbers).size.width.toDp() } + VentiSpace.s1
                wordsFit = wordsFit && width <= room
                if (wordsFit) room -= width
                wordsFit
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2, Alignment.End),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                checks.forEachIndexed { index, check ->
                    CheckMark(check, withWord = withWords[index], style = style)
                }
            }
        }
        Row(Modifier.alpha(if (isLocked) 0f else 1f), verticalAlignment = Alignment.CenterVertically) {
            SettingsButton(onSettings)
            PageDots(page, pages, onNext = onNextPage)
            HoldToEnd(onEnd)
        }
    }
}

/** How a check in the top line looks: as a fault, as a caution, as a quiet state, or plain. */
enum class CheckTone { Danger, Caution, Quiet, Plain }

/**
 * One thing the top line says: a sign, a word where there is one, and the
 * tone. What it is said for a screen reader is `spoken`.
 */
data class CockpitCheck(
    val tag: String,
    @param:DrawableRes val icon: Int,
    val word: String?,
    val tone: CheckTone,
    val spoken: String,
)

/**
 * What the top line says is wrong, as a sign and a word, the most urgent
 * first, and the vario's mark: what the Fly tab once listed before take-off
 * speaks here only when it fails, and goes when it is put right.
 */
object CockpitChecks {
    const val NO_GPS = "No GPS"
    const val NO_AIRSPACE = "No airspace"
    const val NO_MAP = "No offline map"
    const val CONNECTING = "Connecting"
    const val VARIO_LOST = FlySession.VARIO_LOST
    const val NO_BLUETOOTH = "No Bluetooth"

    fun of(
        view: CockpitView,
        isReplay: Boolean,
        airspaceCheck: AirspaceCheck,
        /** The Bluetooth link: only it knows that Bluetooth itself is off or not allowed. */
        varioState: VarioState,
        varioName: String?,
        /** Whether the map is drawn from a region kept on the phone. */
        mapIsKept: Boolean,
    ): List<CockpitCheck> = listOfNotNull(
        // No position for five seconds means the fix is gone.
        if (view.positionAgeS?.let { it > 5 } != false) {
            CockpitCheck("check-gps", R.drawable.ic_location_off, NO_GPS, CheckTone.Danger, NO_GPS)
        } else {
            null
        },
        vario(view, isReplay, varioState, varioName),
        // The cockpit holds no airspace at all: it warns of none.
        if (airspaceCheck == AirspaceCheck.NO_AIRSPACE) {
            CockpitCheck("check-airspace", R.drawable.venti_airspace, NO_AIRSPACE, CheckTone.Caution, NO_AIRSPACE)
        } else {
            null
        },
        // Where the glider is, no region is kept on the phone: without a
        // network there is no map.
        if (!mapIsKept && view.position != null) {
            CockpitCheck("check-map", R.drawable.venti_map, NO_MAP, CheckTone.Caution, NO_MAP)
        } else {
            null
        },
    )

    /**
     * The vario's mark, with the pack's vario sign, from the core's
     * `vario`: plain while connected (with its battery where it says it),
     * quiet while it is looked for, and a fault once it is lost or silent.
     * Nothing where the pilot flies with the phone alone, and nothing in
     * a replay. "No Bluetooth" is the shell's: the core hears only that the
     * link is lost or looked for, not that Bluetooth is off or refused.
     */
    private fun vario(
        view: CockpitView,
        isReplay: Boolean,
        state: VarioState,
        name: String?,
    ): CockpitCheck? {
        if (isReplay) return null
        val icon = R.drawable.venti_vario
        val tag = "check-vario"
        val instrument = view.instrument
        return when (view.vario) {
            VarioMark.NONE_SET_UP -> null
            VarioMark.CONNECTED -> if (instrument?.talking == false) {
                // Connected, and nothing understood for a while: the core's words.
                val words = cockpitSourceWords(view.varioSource, instrument, name, false)
                CockpitCheck(tag, icon, words.short, CheckTone.Danger, words.words)
            } else {
                CockpitCheck(
                    tag,
                    icon,
                    instrument?.batteryPercent?.let { "$it%" },
                    CheckTone.Plain,
                    cockpitSourceWords(view.varioSource, instrument, name, false).words,
                )
            }
            VarioMark.CONNECTING, VarioMark.LOST -> when {
                state is VarioState.Unavailable -> CockpitCheck(tag, icon, NO_BLUETOOTH, CheckTone.Danger, state.reason)
                view.vario == VarioMark.LOST -> CockpitCheck(tag, icon, VARIO_LOST, CheckTone.Danger, VARIO_LOST)
                else -> CockpitCheck(tag, icon, CONNECTING, CheckTone.Quiet, "Vario: $CONNECTING")
            }
        }
    }
}

/** One check of the top line: its sign, and its word where there is room. One element for a screen reader. */
@Composable
private fun CheckMark(check: CockpitCheck, withWord: Boolean, style: TextStyle) {
    val ink = when (check.tone) {
        CheckTone.Danger -> Venti.colours.danger
        CheckTone.Caution -> Venti.colours.caution
        CheckTone.Quiet -> Venti.colours.textMuted
        CheckTone.Plain -> MaterialTheme.colorScheme.onSurface
    }
    Row(
        Modifier.testTag(check.tag).clearAndSetSemantics { contentDescription = check.spoken },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(check.icon), contentDescription = null, modifier = Modifier.size(18.dp), tint = ink)
        val word = check.word
        if (word != null && withWord) {
            Spacer(Modifier.width(VentiSpace.s1))
            Text(
                word,
                style = style.numbers,
                color = ink,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/**
 * The cockpit's one settings button, in the top line beside the dots: what
 * the Fly tab listed before take-off. As high as the line and as wide as a
 * gloved finger needs.
 */
@Composable
private fun SettingsButton(onClick: () -> Unit) {
    Box(
        Modifier
            .heightIn(min = VentiSize.touch)
            .width(VentiSize.touch)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("cockpit-settings")
            .semantics { contentDescription = CockpitSettingsWords.TITLE },
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(R.drawable.ic_settings), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
    }
}

/**
 * The cockpit's pages as dots, the one in view filled. A tap goes to the
 * next page, for a pilot with one hand free or in gloves; a swipe across
 * the cockpit does the same.
 */
@Composable
private fun PageDots(page: CockpitPage, pages: List<CockpitPage>, onNext: () -> Unit) {
    // The flight theme's ink for the page in view, its raised surface for
    // the others: its outline is as dark as its ink.
    val ink = Venti.colours.text
    val quiet = Venti.colours.surfaceRaised
    Row(
        Modifier
            .heightIn(min = VentiSize.touch)
            .clip(PillShape)
            .clickable(onClick = onNext)
            .testTag("cockpit-pages")
            .clearAndSetSemantics {
                contentDescription = "Page"
                stateDescription = page.title
                role = Role.Button
                onClick(label = "Next page") {
                    onNext()
                    true
                }
            }
            .padding(horizontal = VentiSpace.s2),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (each in pages) {
            Box(
                Modifier
                    .size(VentiSpace.s2)
                    .clip(CircleShape)
                    .background(if (each == page) ink else quiet),
            )
        }
    }
}

/** When a drag sideways turns the cockpit's page: the numbers of the slide, apart from the view. */
internal object PageSlide {
    /** How long the page in front takes to go off to the side: the pack's fast motion. */
    const val OUT_MILLIS = VentiMotion.fastMillis

    /** How long the next page takes to come in, or the page to come back: the pack's base motion. */
    const val IN_MILLIS = VentiMotion.baseMillis

    /** How far the drag would have gone on, at the speed it was let go with: a quarter of a second. */
    private const val FLUNG_SECONDS = 0.25f

    /**
     * Whether a drag let go turns the page: a sixth of the page's width
     * along, or flung so that it would have ended half the page along, in
     * the direction it went.
     */
    fun turns(across: Float, flungPerSecond: Float, width: Float): Boolean {
        if (across == 0f || width <= 0f) return false
        if (abs(across) > width / 6f) return true
        val ends = across + flungPerSecond * FLUNG_SECONDS
        return abs(ends) >= width / 2f && (ends < 0) == (across < 0)
    }
}

/**
 * Moves the cockpit's page with a finger dragged across it and, let go,
 * turns it or puts it back: to the left the next, to the right the one
 * before. It watches the touches before what lies under them; once a
 * finger has clearly gone sideways the touch is the slide's, and what it
 * started on (a reading, a button, the map) lets it go. Two fingers are a
 * pinch on the map, never a slide.
 */
private fun Modifier.turnsPagesBySwipe(
    isSettling: () -> Boolean,
    /** How far the page is to stand from its place, pixels, negative to the left. */
    onSlide: (Float) -> Unit,
    /** The finger let go this far along, at this speed, pixels and pixels per second. */
    onLetGo: (Float, Float) -> Unit,
): Modifier = composed {
    val settling by rememberUpdatedState(isSettling)
    val slide by rememberUpdatedState(onSlide)
    val letGo by rememberUpdatedState(onLetGo)
    pointerInput(Unit) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            if (settling()) return@awaitEachGesture
            val speed = VelocityTracker()
            speed.addPosition(down.uptimeMillis, down.position)
            var across = 0f
            var along = 0f
            var isSwipe = false
            var isPinch = false
            val slop = viewConfiguration.touchSlop
            while (true) {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                if (event.changes.count { it.pressed } > 1) {
                    isPinch = true
                    break
                }
                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                val moved = change.position - change.previousPosition
                across += moved.x
                along += moved.y
                speed.addPosition(change.uptimeMillis, change.position)
                if (!isSwipe && abs(across) > slop && abs(across) > 2 * abs(along)) {
                    isSwipe = true
                }
                if (isSwipe) {
                    change.consume()
                    // From where the slide was first seen, so that the page
                    // does not jump by the slop.
                    slide(if (across < 0) across + slop else across - slop)
                }
                if (!change.pressed) break
            }
            if (isSwipe) {
                if (isPinch) letGo(0f, 0f) else letGo(across, speed.calculateVelocity().x)
            }
        }
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
                // For a pilot who set a glider class, while the core's hint
                // is to speed up: its words, in the place of the average.
                note = view.speedToFly?.takeIf { it.speedUp }?.words
                    ?: view.averageClimbMs?.let { "${Format.climb(it)} m/s average" },
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
                // The direction flown, from 3 m/s over the ground: a small
                // arrow and the core's compass word, in the note's own line.
                Reading(
                    "Ground speed",
                    view.groundSpeedMs?.let(Format::kilometresPerHour),
                    "km/h",
                    note = view.directionFlown?.point,
                    noteArrowDeg = view.directionFlown?.degrees,
                    modifier = Modifier.weight(1f).testTag("ground-speed"),
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
    onLock: () -> Unit,
    live: LiveShareView?,
    onLive: () -> Unit,
    /** Opens the readings that have no place on a page. */
    onDetails: () -> Unit,
    onRecordNow: () -> Unit,
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
        // The vario's sound is a switch of the settings, not a button
        // here (the owner, 2026-11-22).
        Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
            // Nothing on the screen answers a touch until it is unlocked
            // by holding a finger on the lock.
            QuietButton("Lock screen", R.drawable.ic_lock, Modifier.weight(1f).testTag("cockpit-lock"), onClick = onLock)
            // Who watches the flight, and whether it is going out. Not in
            // a replay: what is replayed is not shared, nor recorded.
            if (!isReplay) LiveButton(live, Modifier.weight(1f), onOpen = onLive)
        }
        // For a pilot who would rather not wait for the take-off to be noticed.
        if (!isReplay && view.waitingForTakeoff) {
            QuietButton("Record now", R.drawable.ic_record, Modifier.fillMaxWidth().testTag("cockpit-record-now"), onClick = onRecordNow)
        }
        // The readings that have no place on a page. The thermal map is a
        // switch of the settings.
        QuietButton(
            CockpitDetailsWords.core.title,
            R.drawable.ic_info,
            Modifier.fillMaxWidth().testTag("cockpit-details"),
            onClick = onDetails,
        )
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

/**
 * How long a finger stays on "End" to end the session, milliseconds: two
 * thirds of a second (the owner, 2026-11-23; a whole second before). The
 * pill fills over the same time.
 */
private const val HOLD_TO_END_MILLISECONDS = 670

/**
 * "End" in the top line, which ends the session when held for two thirds of a second, so
 * that a glove, a line or a drop of rain ends nothing. It fills while the
 * finger stays, and nothing asks again. TalkBack's double tap ends it,
 * which is deliberate enough. The touch is as high as the line; the pill
 * in it is small, to leave the screen to the readings.
 */
@Composable
private fun HoldToEnd(onEnd: () -> Unit) {
    val filled = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val end by rememberUpdatedState(onEnd)
    val primary = MaterialTheme.colorScheme.primary
    val fill = Venti.colours.primaryPressed
    Box(
        Modifier
            .heightIn(min = VentiSize.touch)
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
                contentDescription = "End"
                stateDescription = "Hold to end"
                role = Role.Button
                onClick(label = "End the session") {
                    end()
                    true
                }
            }
            .padding(start = VentiSpace.s1),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .clip(PillShape)
                .drawBehind {
                    drawRect(primary)
                    drawRect(fill, size = Size(size.width * filled.value, size.height))
                }
                .padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s1),
        ) {
            Text("End", style = fixed(VentiType.label), color = MaterialTheme.colorScheme.onPrimary, maxLines = 1)
        }
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
enum class ReadingSize(
    val number: VentiTypeStyle,
    val unit: VentiTypeStyle,
    /** How much of the number's line its placeholder leaves above and below the bar: about the figures' height is drawn. */
    val placeholderInset: Dp,
) {
    /** The one reading a page may have of this size. */
    VeryLarge(VentiType.instrumentXl, VentiType.heading, VentiSpace.s5),
    Large(VentiType.instrumentL, VentiType.label, VentiSpace.s3),
    Medium(VentiType.instrumentM, VentiType.label, VentiSpace.s2),
}

/**
 * What a reading shows before its first value: a quiet bar in the flight
 * theme's raised surface, as wide as a number of two figures and as high
 * as figures in that type, on the number's baseline, so that nothing moves
 * when the value comes (plan item 15, as on the iPhone). No dashes and no
 * made-up zero; the top line says why ("No GPS").
 */
@Composable
internal fun NumberPlaceholder(style: TextStyle, inset: Dp, modifier: Modifier = Modifier) {
    val bar = Venti.colours.surfaceRaised
    // Two figures in the number's type, unseen, give the bar its size and
    // the line its baseline.
    Text(
        "00",
        modifier = modifier
            .testTag("reading-placeholder")
            .drawBehind {
                val top = inset.toPx().coerceAtMost(size.height / 3)
                drawRoundRect(
                    bar,
                    topLeft = Offset(0f, top),
                    size = Size(size.width, size.height - 2 * top),
                    cornerRadius = CornerRadius(VentiRadius.s.toPx()),
                )
            },
        style = style,
        color = Color.Transparent,
        maxLines = 1,
        softWrap = false,
    )
}

/**
 * One reading of the cockpit: the number, and under it its name. The sizes
 * are the design language's instrument sizes and do not follow the phone's
 * text-size setting: a reading has to be in the same place at the same
 * size every time the pilot glances at it. A value the sensors have not
 * supplied is a quiet bar ([NumberPlaceholder]), never an old number.
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
    /** The value's colour, where it says something by its colour; the text's otherwise. */
    ink: Color = Color.Unspecified,
    /** A small arrow before the note, turned this many degrees clockwise from up: the direction flown. */
    noteArrowDeg: Double? = null,
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
            if (value == null) {
                NumberPlaceholder(fixed(size.number).numbers, size.placeholderInset, Modifier.alignByBaseline())
            } else Text(
                value,
                modifier = Modifier.weight(1f, fill = false).alignByBaseline(),
                color = ink,
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
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (noteArrowDeg != null && note != null) {
                    Icon(
                        painterResource(R.drawable.ic_arrow_upward),
                        contentDescription = null,
                        tint = Venti.colours.textMuted,
                        modifier = Modifier
                            .size(VentiType.label.sizePx.dp)
                            .rotate(noteArrowDeg.toFloat())
                            .testTag("direction-flown"),
                    )
                    Spacer(Modifier.width(VentiSpace.s1))
                }
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

/**
 * The wind carries the glider backwards on a glide, in the core's words:
 * "The wind carries you back: 4 km/h backwards". A band among the cautions
 * at the top of every page, on the amber of the wind's warning.
 */
@Composable
private fun CarriedBackBanner(words: String) {
    val colours = Venti.colours
    Surface(
        color = colours.cautionSoft,
        contentColor = colours.caution,
        shape = MaterialTheme.shapes.medium,
        border = BorderStroke(1.dp, colours.caution),
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = VentiSize.touch)
            .testTag("carried-back")
            .clearAndSetSemantics {
                contentDescription = "Wind"
                stateDescription = words
            },
    ) {
        Row(
            Modifier.padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(painterResource(R.drawable.venti_caution), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
            Spacer(Modifier.width(VentiSpace.s3))
            Text(words, style = fixed(VentiType.heading).numbers)
        }
    }
}

/**
 * The nearest landing within glide in this wind, in the core's words, as
 * one line under the map: "Landing Musterwiese: 2.1 km, +340 m". In the
 * caution colour when none is within glide.
 */
@Composable
private fun NearestLandingLine(landing: LandingReading, onOpen: () -> Unit) {
    val ink = if (landing.withinGlide) MaterialTheme.colorScheme.onSurface else Venti.colours.caution
    Row(
        Modifier
            .fillMaxWidth()
            .testTag("nearest-landing")
            // As the landing's badge on the other pages: a tap lists the places to fly to.
            .clickable(role = Role.Button, onClick = onOpen)
            .clearAndSetSemantics {
                contentDescription = CockpitDetailsWords.core.nearestLanding
                stateDescription = landing.words
                role = Role.Button
                onClick(label = "List the places to fly to") {
                    onOpen()
                    true
                }
            }
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(if (landing.withinGlide) R.drawable.venti_landing else R.drawable.venti_caution),
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = ink,
        )
        Spacer(Modifier.width(VentiSpace.s2))
        Text(
            landing.words,
            style = fixed(VentiType.label).numbers,
            color = ink,
            // One line where it fits; a long caution takes a second before any of it is cut.
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
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
                .align(Alignment.TopEnd)
                .systemBarsPadding()
                // In the top line, over the dots and "End": the readings
                // stay in view while the screen is locked.
                .padding(end = VentiSpace.s2)
                .height(VentiSize.touch)
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
            Row(
                Modifier.padding(horizontal = VentiSpace.s3),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(painterResource(R.drawable.ic_lock), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
                Spacer(Modifier.width(VentiSpace.s2))
                Text("Hold to unlock", style = fixed(VentiType.label))
            }
        }
    }
}

/** The share of the width the thermal view takes beside the first page's map. */
private const val THERMAL_VIEW_SHARE = 0.48f

/**
 * The cable ahead, in the airspace warning's band on the alert red, with
 * the core's words; quieted, plain. "Seen" quiets it for ten minutes, as
 * an airspace warning's "Seen" does.
 */
@Composable
private fun CableBanner(warning: CableWarning, onSeen: () -> Unit) {
    val ground = if (warning.silenced) MaterialTheme.colorScheme.surfaceContainerHigh else Venti.colours.dangerFill
    val ink = if (warning.silenced) MaterialTheme.colorScheme.onSurfaceVariant else Venti.colours.onDangerFill
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("cable-banner")
            .semantics {
                contentDescription = if (warning.silenced) "Cable warning, quieted" else "Cable warning"
                stateDescription = warning.words
            },
        shape = MaterialTheme.shapes.medium,
        color = ground,
        contentColor = ink,
    ) {
        Row(
            Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s2, top = VentiSpace.s2, bottom = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painterResource(if (warning.silenced) R.drawable.ic_notifications_off else R.drawable.venti_danger),
                contentDescription = null,
            )
            Spacer(Modifier.width(VentiSpace.s3))
            Text(warning.words, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
            if (!warning.silenced) {
                TextButton(onClick = onSeen, modifier = Modifier.heightIn(min = VentiSize.touch).testTag("cable-seen")) {
                    Text("Seen", color = ink, style = fixed(VentiType.heading))
                }
            }
        }
    }
}

/** A count over the map, in the core's words: the other networks' aircraft around. */
@Composable
private fun CountBadge(words: String) {
    Badge(
        Modifier.testTag("others-count").clearAndSetSemantics { contentDescription = words },
    ) {
        Icon(painterResource(R.drawable.venti_live), contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(6.dp))
        Text(words, style = fixed(VentiType.label).copy(fontWeight = FontWeight.SemiBold).numbers, color = Color.Unspecified)
    }
}

/** The closing circle of an open triangle, in the core's words, over the distance so far. */
@Composable
private fun ClosingBadge(words: String) {
    Badge(
        Modifier
            .widthIn(max = VentiSpace.s8 * 2 + VentiSpace.s6)
            .testTag("closing-badge")
            .clearAndSetSemantics {
                contentDescription = "Closing circle"
                stateDescription = words
            },
    ) {
        Text(
            words,
            style = fixed(VentiType.label).copy(fontWeight = FontWeight.SemiBold).numbers,
            color = Color.Unspecified,
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * The task's page, as an instrument's competition page: the next
 * turnpoint large, its name, distance, height over what it needs (green
 * over, red under), the glide it needs and the arrival; goal the same in a
 * smaller row; before the start, the start's words large; the speed
 * section's average. Numbers and words are the core's.
 */
@Composable
private fun TaskPage(readings: TaskReadings?) {
    Column(
        Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s2).testTag("task-page"),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
    ) {
        readings?.start?.let { start ->
            Text(
                start.words,
                style = fixed(VentiType.heading),
                modifier = Modifier.testTag("task-start-words"),
            )
        }
        val next = readings?.next
        Text(
            next?.name ?: "– –",
            style = fixed(VentiType.heading),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.testTag("task-next-name"),
        )
        LegReadings(next, ReadingSize.Large, Modifier.testTag("task-next"))
        LegReadings(next, ReadingSize.Large, arrival = true)
        // On the last leg the next turnpoint is the goal: it is not said twice.
        val goal = readings?.goal?.takeIf { next?.isGoal != true }
        goal?.let {
            Text(it.name, style = fixed(VentiType.label), maxLines = 1, overflow = TextOverflow.Ellipsis)
            LegReadings(it, ReadingSize.Medium, Modifier.testTag("task-goal"))
        }
        readings?.speedSectionAverageWords?.let { words ->
            Text(
                words,
                style = fixed(VentiType.label).numbers,
                color = Venti.colours.textMuted,
                modifier = Modifier.testTag("task-speed"),
            )
        }
    }
}

/** One place ahead of the task: distance and height over what it needs, or the glide it needs and the arrival. */
@Composable
private fun LegReadings(leg: TaskLegReading?, size: ReadingSize, modifier: Modifier = Modifier, arrival: Boolean = false) {
    val over = leg?.overNeededM
    val overInk = when {
        over == null -> Color.Unspecified
        over >= 0 -> Venti.colours.good
        else -> Venti.colours.danger
    }
    Row(modifier) {
        if (!arrival) {
            Reading("Distance", leg?.let { Format.kilometres(it.distanceM) }, "km", size = size, modifier = Modifier.weight(1f))
            Reading(
                "Over needed",
                leg?.overNeededWords,
                "",
                size = size,
                ink = overInk,
                modifier = Modifier.weight(1f).testTag("task-over-needed"),
            )
        } else {
            Reading("Glide needed", leg?.glideNeeded?.let(Format::neededGlide), "", size = size, modifier = Modifier.weight(1f))
            Reading(
                "Arrival",
                leg?.arrivesUnixS?.let(Format::time),
                "",
                size = size,
                modifier = Modifier.weight(1f),
            )
        }
    }
}
