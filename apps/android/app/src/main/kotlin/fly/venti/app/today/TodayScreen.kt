package fly.venti.app.today

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiBrand
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Metric
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.StatusChip
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.design.status
import fly.venti.app.design.text
import fly.venti.app.settings.Preferences
import fly.venti.app.wind.WindModel
import fly.venti.core.Coordinate
import fly.venti.core.Flyability
import fly.venti.core.OutlookDay
import fly.venti.core.OutlookHour
import fly.venti.core.SiteView
import kotlinx.coroutines.launch

/**
 * The Today tab, as the design language's Today screen has it: the day in
 * a line over the valley, and under it the take-offs around with what the
 * forecast says about flying from each. The take-off that looks best is a
 * card with its readings and its hours; the others are rows with their
 * status. The assistant and the settings open from its corner.
 *
 * What a forecast says about a take-off is the core's reading of a weather
 * model, and the screen says so at its foot: the model, when Venti last
 * looked for a newer run, and that the words are not advice to fly.
 */
@Composable
fun TodayScreen(
    today: TodayModel,
    wind: WindModel,
    preferences: Preferences,
    /** Opens a take-off's page. */
    onOpen: (SiteView) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenAssistant: () -> Unit,
    bars: PaddingValues,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val position = remember { PilotPosition(context.applicationContext) }
    // Which of the forecast's days is shown: 0 is today. Until the pilot
    // has chosen, an evening shows tomorrow, since today has no flying
    // hours left.
    var day by rememberSaveable { mutableIntStateOf(0) }
    var hasChosenDay by rememberSaveable { mutableStateOf(false) }
    var isRefreshing by remember { mutableStateOf(false) }
    var nearPilot by remember { mutableStateOf(preferences.todayNearPilot) }
    val place = position.place.takeIf { nearPilot } ?: HOME

    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { answer ->
        if (answer.values.any { it }) {
            position.isDenied = false
            position.readIfAllowed()
        } else {
            position.isDenied = true
        }
    }
    fun useMyPosition() {
        nearPilot = true
        preferences.todayNearPilot = true
        if (position.isAllowed) position.readIfAllowed() else ask.launch(PilotPosition.PERMISSIONS)
    }

    // Asked when the tab is first shown, when the pilot's position
    // arrives, when Venti's address changes, and again after a while.
    LaunchedEffect(place, wind.address) {
        if (nearPilot) position.readIfAllowed()
        if (!today.isFresh(place)) today.load(place)
    }
    val places = today.places
    val days = days(places)
    LaunchedEffect(places) {
        if (hasChosenDay) return@LaunchedEffect
        // The first day that has flying hours left: in the evening that is tomorrow.
        day = days.indexOfFirst { it.hours.isNotEmpty() }.coerceAtLeast(0)
    }
    val chosen = days.getOrNull(day)
    val shown = ordered(places, chosen)

    PullToRefreshBox(
        isRefreshing = isRefreshing,
        onRefresh = {
            scope.launch {
                isRefreshing = true
                today.load(place)
                isRefreshing = false
            }
        },
        modifier = Modifier.fillMaxSize().background(Venti.colours.bg).padding(bottom = bars.calculateBottomPadding()),
    ) {
        Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            Head(
                dayLine = dayLine(days, day, isNearPilot = position.place != null && nearPilot),
                headline = headline(today.state, shown, chosen),
                thermals = shown.firstOrNull()?.let { outlookDay(it, chosen) }?.thermalWords.orEmpty(),
                onOpenSettings = onOpenSettings,
                onOpenAssistant = onOpenAssistant,
            )
            Column(
                modifier = Modifier.fillMaxWidth().padding(VentiSpace.s4),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s4),
            ) {
                when (val state = today.state) {
                    is TodayState.Failed -> Explained(
                        title = "The forecast could not be read",
                        words = state.message,
                        action = "Try again",
                        act = { scope.launch { today.load(place) } },
                    )
                    TodayState.Idle, TodayState.Loading -> Box(
                        modifier = Modifier.fillMaxWidth().padding(top = VentiSpace.s6),
                        contentAlignment = Alignment.Center,
                    ) {
                        CircularProgressIndicator()
                    }
                    TodayState.Loaded -> if (places.isEmpty()) {
                        Explained(
                            title = "No take-offs within 30 km",
                            words = if (position.place == null || !nearPilot) {
                                "These would be the take-offs around the Tegernsee. Venti knows none there yet."
                            } else {
                                "Venti knows no take-off around where you are."
                            },
                            action = null,
                            act = {},
                        )
                    } else {
                        if (days.size > 1) {
                            SegmentedPills(days.map { it.name }, day, Modifier.testTag("today-days"), onChoose = {
                                hasChosenDay = true
                                day = it
                            })
                        }
                        shown.firstOrNull()?.let { first ->
                            TakeoffCard(first, outlookDay(first, chosen), onClick = { onOpen(first.site) })
                        }
                        Column {
                            shown.drop(1).forEachIndexed { index, other ->
                                if (index > 0) HorizontalDivider(color = Venti.colours.border)
                                TakeoffRow(other, outlookDay(other, chosen), onClick = { onOpen(other.site) })
                            }
                        }
                        if (!(nearPilot && position.place != null) && !position.isDenied) {
                            OutlinedButton(
                                onClick = ::useMyPosition,
                                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch),
                            ) {
                                Text("Use my position")
                            }
                        }
                        Foot(today, shown.firstOrNull()?.let { outlookDay(it, chosen) })
                    }
                }
            }
        }
    }
}

/**
 * Where the take-offs are looked for until the pilot asks for those
 * around them: the middle of the first region, between the Tegernsee and
 * the Achensee.
 */
private val HOME = Coordinate(lat = 47.6, lon = 11.75)

/**
 * The days the forecasts reach over, three at most. The core gives four
 * since 2026-11-21, but at 360 dp with text at 130 % a fourth breaks
 * "Tomorrow" in two in the switch (tried on 2026-11-21).
 */
private fun days(places: List<TodayPlace>): List<OutlookDay> =
    places.mapNotNull { it.outlook?.days }.maxByOrNull { it.size }.orEmpty().take(3)

/**
 * What a take-off's forecast says of the chosen day: the day that begins at
 * the same moment, wherever it stands in that forecast.
 */
private fun outlookDay(place: TodayPlace, chosen: OutlookDay?): OutlookDay? =
    chosen?.let { day -> place.outlook?.days?.firstOrNull { it.startUnixS == day.startUnixS } }

/**
 * What stands in place of a day's line for a take-off that has none: that
 * the server is still being asked, or that its forecast does not reach the day.
 */
private fun waitingWords(place: TodayPlace): String =
    if (place.isAsked) "No forecast for this day" else "Reading the forecast"

/**
 * The take-offs in the order they are shown: by what the forecast says of
 * the chosen day, the better first, and among equals the nearer. One the
 * server has not answered for yet comes last.
 */
private fun ordered(places: List<TodayPlace>, day: OutlookDay?): List<TodayPlace> =
    places.sortedWith(
        compareBy<TodayPlace> { place ->
            val shown = outlookDay(place, day)
            // Flyable days first, the better thermals first among them.
            when (shown?.verdict) {
                Flyability.FLYABLE -> ThermalScale.rank(shown.thermals)
                Flyability.MARGINAL -> 4
                Flyability.NOT_FLYABLE -> 5
                null -> 6
            }
        }.thenBy { it.distanceM },
    )

/** "Saturday, near you", or the day alone while the take-offs are those around the middle of the first region. */
private fun dayLine(days: List<OutlookDay>, day: Int, isNearPilot: Boolean): String {
    val start = days.getOrNull(day)?.startUnixS ?: (System.currentTimeMillis() / 1000)
    val name = Format.weekday(start)
    return if (isNearPilot) "$name, near you" else name
}

/** The day in a line: the best take-off's. Before there is one, what the screen is waiting for. */
private fun headline(state: TodayState, shown: List<TodayPlace>, day: OutlookDay?): String = when (state) {
    is TodayState.Failed -> "No forecast"
    TodayState.Idle, TodayState.Loading -> "Reading the forecast"
    TodayState.Loaded -> when {
        shown.isEmpty() -> "No take-offs near"
        else -> shown.firstOrNull()?.let { outlookDay(it, day) }?.headline
            ?: if (shown.all { it.isAsked }) "No forecast" else "Reading the forecast"
    }
}

/**
 * The valley, with the day and its line, and the corner that opens the
 * assistant and the settings. The art is the design pack's and is light
 * in every appearance, so what stands on it is in the pack's ink.
 */
@Composable
private fun Head(
    dayLine: String,
    headline: String,
    /** What the best take-off's day is for thermals: "Cross-country day". Empty for none. */
    thermals: String,
    onOpenSettings: () -> Unit,
    onOpenAssistant: () -> Unit,
) {
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    Box(modifier = Modifier.fillMaxWidth().height(top + VentiSpace.s8 * 3).testTag("today-head")) {
        Image(
            painterResource(R.drawable.venti_art_valley_strip),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier.fillMaxSize(),
        )
        Row(modifier = Modifier.align(Alignment.TopEnd).statusBarsPadding()) {
            val ink = IconButtonDefaults.iconButtonColors(contentColor = VentiBrand.Ink)
            IconButton(onClick = onOpenAssistant, colors = ink, modifier = Modifier.testTag("today-assistant")) {
                Icon(painterResource(R.drawable.ic_auto_awesome), contentDescription = "Assistant")
            }
            IconButton(onClick = onOpenSettings, colors = ink, modifier = Modifier.testTag("today-settings")) {
                Icon(painterResource(R.drawable.ic_settings), contentDescription = "Settings")
            }
        }
        Column(
            modifier = Modifier.fillMaxWidth().padding(top = top + VentiSpace.s7, start = VentiSpace.s4, end = VentiSpace.s4),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(dayLine, style = VentiType.label.text, color = VentiBrand.Ink)
            Text(
                headline,
                style = VentiType.hero.text,
                color = VentiBrand.Ink,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag("today-headline").semantics { heading() },
            )
            if (thermals.isNotEmpty()) {
                Text(thermals, style = VentiType.label.text, color = VentiBrand.Ink, modifier = Modifier.testTag("today-thermals"))
            }
        }
    }
}

/** A state of the screen in words, with the one thing to do about it. */
@Composable
private fun Explained(title: String, words: String, action: String?, act: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(top = VentiSpace.s5),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
        Text(words, color = Venti.colours.textSecondary, textAlign = TextAlign.Center)
        if (action != null) {
            Button(onClick = act, modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch)) {
                Text(action)
            }
        }
    }
}

/** The first take-off: its status, three readings, and its hours. */
@Composable
private fun TakeoffCard(place: TodayPlace, day: OutlookDay?, onClick: () -> Unit) {
    val colours = Venti.colours
    Surface(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth().testTag("today-card"),
        shape = MaterialTheme.shapes.medium,
        color = colours.surface,
        border = BorderStroke(1.dp, colours.border),
    ) {
        Column(modifier = Modifier.padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    place.site.name,
                    style = MaterialTheme.typography.titleLarge,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(VentiSpace.s2))
                day?.let { StatusChip(it.verdict.status, it.status) }
            }
            if (day == null) {
                Text(waitingWords(place), color = colours.textSecondary)
                return@Column
            }
            if (day.why.isNotEmpty()) {
                Text(day.why, color = colours.textSecondary)
            }
            ThermalLine(day)
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                Metric(
                    caption = if (day.windFrom.isEmpty()) "Wind" else "Wind from ${day.windFrom}",
                    value = day.windKmh.ifEmpty { "– –" },
                    unit = "km/h",
                    modifier = Modifier.weight(1f),
                )
                Metric(caption = "Gusts", value = day.gustKmh.ifEmpty { "– –" }, unit = "km/h", modifier = Modifier.weight(1f))
                Metric(
                    caption = "Cloud base",
                    value = day.cloudBaseM?.let(Format::metres) ?: "– –",
                    unit = "m",
                    modifier = Modifier.weight(1f),
                )
            }
            if (day.hours.isNotEmpty()) {
                HourStrip(day.hours)
                Text(
                    "Wind in km/h at 10 m. The bar is the hour's status: tall for flyable, low for not." +
                        if (showsThermals(day.hours)) {
                            " Under it the climb to expect in thermals, in m/s, with a bar that grows with it."
                        } else {
                            ""
                        },
                    style = MaterialTheme.typography.bodySmall,
                    color = colours.textSecondary,
                )
            }
        }
    }
}

/** Another take-off: its name, what limits its day, and its status. */
@Composable
private fun TakeoffRow(place: TodayPlace, day: OutlookDay?, onClick: () -> Unit) {
    val colours = Venti.colours
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("today-row")
            .heightIn(min = VentiSize.touch)
            .clickable(onClick = onClick)
            .padding(vertical = VentiSpace.s3),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        Icon(painterResource(R.drawable.venti_site), contentDescription = null, tint = colours.text, modifier = Modifier.size(VentiSize.icon))
        Column(modifier = Modifier.weight(1f)) {
            Text(place.site.name, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                // What limits the day; for a day nothing limits, its line.
                day?.let { it.why.ifEmpty { it.headline } } ?: waitingWords(place),
                style = MaterialTheme.typography.labelLarge,
                color = colours.textSecondary,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (day != null && day.thermalWords.isNotEmpty()) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    ThermalMark(day.climbMs)
                    Text(day.thermalWords, style = MaterialTheme.typography.labelLarge, color = colours.textSecondary)
                }
            }
        }
        day?.let { StatusChip(it.verdict.status, it.status) }
    }
}

/**
 * The day's hours side by side: the hour, where the wind goes, its speed,
 * and a bar that is the hour's status by its height as well as its colour.
 */
@Composable
private fun HourStrip(hours: List<OutlookHour>) {
    val colours = Venti.colours
    // The climbs only where there is something to climb in: a row of
    // "0.0" adds nothing to a line that says "None to climb in".
    val showsClimb = showsThermals(hours)
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom) {
        for (hour in hours) {
            Column(
                modifier = Modifier.weight(1f).clearAndSetSemantics {
                    contentDescription = "${hour.hour} o'clock"
                    stateDescription = spoken(hour)
                },
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Text(hour.hour, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                val from = hour.windFromDeg
                if (from != null) {
                    // The arrow points where the wind goes.
                    Icon(
                        painterResource(R.drawable.venti_arrow_wind),
                        contentDescription = null,
                        tint = colours.text,
                        modifier = Modifier.size(VentiSpace.s4).rotate((from + 180).toFloat()),
                    )
                } else {
                    Spacer(Modifier.size(VentiSpace.s4))
                }
                Text(
                    hour.windKmh.ifEmpty { "–" },
                    style = MaterialTheme.typography.labelLarge.numbers,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                )
                Box(modifier = Modifier.fillMaxWidth().height(VentiSpace.s3), contentAlignment = Alignment.BottomCenter) {
                    val (colour, height) = when (hour.verdict) {
                        Flyability.FLYABLE -> colours.good to VentiSpace.s3
                        Flyability.MARGINAL -> colours.caution to VentiSpace.s2
                        Flyability.NOT_FLYABLE -> colours.danger to VentiSpace.s1
                    }
                    Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 1.dp).height(height).background(colour))
                }
                if (showsClimb) {
                    // The climb to expect in thermals, and a bar that grows with it.
                    val climb = hour.climbMs
                    Text(
                        climb?.let(Format::oneDecimal) ?: "–",
                        style = MaterialTheme.typography.labelLarge.numbers,
                        maxLines = 1,
                    )
                    Box(modifier = Modifier.fillMaxWidth().height(VentiSpace.s3), contentAlignment = Alignment.BottomCenter) {
                        if (climb != null) {
                            val step = ThermalScale.step(climb)
                            Box(
                                modifier = Modifier.fillMaxWidth().padding(horizontal = 1.dp)
                                    .height(ThermalScale.barHeight(step).dp)
                                    .background(colours.thermal[step]),
                            )
                        }
                    }
                }
            }
        }
    }
}

/** Whether some hour of the day has a climb above zero: then the strip shows the climbs. */
internal fun showsThermals(hours: List<OutlookHour>): Boolean = hours.any { (it.climbMs ?: 0.0) > 0 }

private fun spoken(hour: OutlookHour): String {
    val verdict = when (hour.verdict) {
        Flyability.FLYABLE -> "Flyable"
        Flyability.MARGINAL -> "Marginal"
        Flyability.NOT_FLYABLE -> "Not flyable"
    }
    val wind = if (hour.windKmh.isEmpty()) "" else ", wind ${hour.windKmh} kilometres an hour"
    val climb = hour.climbMs
    val thermals = when {
        climb == null -> ""
        climb > 0 -> ", thermals of ${Format.oneDecimal(climb)} metres a second"
        else -> ", no thermals to climb in"
    }
    return if (hour.why.isEmpty()) "$verdict$wind$thermals" else "$verdict: ${hour.why}$wind$thermals"
}

/**
 * What the reading is, whose data it is, and how fresh: the design
 * language asks a forecast for its update time and its source.
 */
@Composable
private fun Foot(
    today: TodayModel,
    /** The day shown, the best take-off's: it is read from the model that has the most of it. */
    day: OutlookDay?,
) {
    val outlook = today.places.firstNotNullOfOrNull { it.outlook }
    val style = MaterialTheme.typography.bodySmall
    val colour = Venti.colours.textSecondary
    Column(modifier = Modifier.fillMaxWidth().testTag("today-foot"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        // The note and the run of the day shown, else the outlook's own.
        val note = day?.note?.takeIf { it.isNotEmpty() } ?: outlook?.note.orEmpty()
        val run = day?.runUnixS?.takeIf { day.note.isNotEmpty() } ?: outlook?.runUnixS
        if (outlook != null && note.isNotEmpty() && run != null) {
            Text(
                "$note Run of ${Format.time(run)}, looked up at ${Format.time(outlook.fetchedUnixS)}.",
                style = style,
                color = colour,
            )
        }
        Text(
            "Flyable, marginal and not flyable are what the forecast's wind, gusts, rain and cloud say by fixed " +
                "limits, and the kind of day is what its heat and temperatures say of thermals. They are not " +
                "advice to fly.",
            style = style,
            color = colour,
        )
        for (credit in today.credits) {
            Text(credit.text, style = style, color = colour)
        }
    }
}
