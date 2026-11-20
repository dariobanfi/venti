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
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
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
import androidx.compose.ui.semantics.Role
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
import fly.venti.app.design.PillShape
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
        val outlooks = places.mapNotNull { it.outlook }
        val isOver = outlooks.isNotEmpty() && outlooks.all { it.days.firstOrNull()?.hours.isNullOrEmpty() }
        day = if (isOver && days.size > 1) 1 else 0
    }
    val shown = ordered(places, day)

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
                headline = headline(today.state, shown, day),
                onOpenSettings = onOpenSettings,
                onOpenAssistant = onOpenAssistant,
            )
            Column(
                modifier = Modifier.fillMaxWidth().padding(VentiSpace.s4),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s4),
            ) {
                when (val state = today.state) {
                    TodayState.NoServer -> Explained(
                        title = "Venti is not connected",
                        words = "The forecast and the take-offs come from Venti. Set its address in the settings.",
                        action = "Open the settings",
                        act = onOpenSettings,
                    )
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
                            DaySwitch(days, day, onChoose = {
                                hasChosenDay = true
                                day = it
                            })
                        }
                        shown.firstOrNull()?.let { first ->
                            TakeoffCard(first, outlookDay(first, day), onClick = { onOpen(first.site) })
                        }
                        Column {
                            shown.drop(1).forEachIndexed { index, other ->
                                if (index > 0) HorizontalDivider(color = Venti.colours.border)
                                TakeoffRow(other, outlookDay(other, day), onClick = { onOpen(other.site) })
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
                        Foot(today)
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

/** The days the forecasts reach over, three at most. */
private fun days(places: List<TodayPlace>): List<OutlookDay> =
    places.mapNotNull { it.outlook?.days }.maxByOrNull { it.size }.orEmpty().take(3)

private fun outlookDay(place: TodayPlace, day: Int): OutlookDay? = place.outlook?.days?.getOrNull(day)

/**
 * The take-offs in the order they are shown: by what the forecast says of
 * the chosen day, the better first, and among equals the nearer. One the
 * server has not answered for yet comes last.
 */
private fun ordered(places: List<TodayPlace>, day: Int): List<TodayPlace> =
    places.sortedWith(
        compareBy<TodayPlace> { place ->
            when (outlookDay(place, day)?.verdict) {
                Flyability.FLYABLE -> 0
                Flyability.MARGINAL -> 1
                Flyability.NOT_FLYABLE -> 2
                null -> 3
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
private fun headline(state: TodayState, shown: List<TodayPlace>, day: Int): String = when (state) {
    TodayState.NoServer -> "No forecast yet"
    is TodayState.Failed -> "No forecast"
    TodayState.Idle, TodayState.Loading -> "Reading the forecast"
    TodayState.Loaded -> when {
        shown.isEmpty() -> "No take-offs near"
        else -> shown.firstOrNull()?.let { outlookDay(it, day) }?.headline ?: "Reading the forecast"
    }
}

/**
 * The valley, with the day and its line, and the corner that opens the
 * assistant and the settings. The art is the design pack's and is light
 * in every appearance, so what stands on it is in the pack's ink.
 */
@Composable
private fun Head(dayLine: String, headline: String, onOpenSettings: () -> Unit, onOpenAssistant: () -> Unit) {
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    Box(modifier = Modifier.fillMaxWidth().height(top + VentiSpace.s8 * 3)) {
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

/** Today, tomorrow and the day after, as the pack's switch: the chosen day on the primary colour. */
@Composable
private fun DaySwitch(days: List<OutlookDay>, chosen: Int, onChoose: (Int) -> Unit) {
    val colours = Venti.colours
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("today-days")
            .background(colours.surfaceRaised, PillShape)
            .padding(VentiSpace.s1)
            .selectableGroup(),
    ) {
        days.forEachIndexed { index, shown ->
            val isChosen = index == chosen
            Box(
                modifier = Modifier
                    .weight(1f)
                    .heightIn(min = VentiSize.touch - VentiSpace.s2)
                    .background(if (isChosen) colours.primary else colours.surfaceRaised, PillShape)
                    .selectable(selected = isChosen, role = Role.Tab, onClick = { onChoose(index) }),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    if (index == 0) "Today" else Format.shortWeekday(shown.startUnixS),
                    style = MaterialTheme.typography.titleMedium,
                    color = if (isChosen) colours.onPrimary else colours.textSecondary,
                )
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
                Text("Reading the forecast", color = colours.textSecondary)
                return@Column
            }
            if (day.why.isNotEmpty()) {
                Text(day.why, color = colours.textSecondary)
            }
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
                    "Wind in km/h at 10 m. The bar is the hour's status: tall for flyable, low for not.",
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
                day?.let { it.why.ifEmpty { it.headline } } ?: "Reading the forecast",
                style = MaterialTheme.typography.labelLarge,
                color = colours.textSecondary,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
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
            }
        }
    }
}

private fun spoken(hour: OutlookHour): String {
    val verdict = when (hour.verdict) {
        Flyability.FLYABLE -> "Flyable"
        Flyability.MARGINAL -> "Marginal"
        Flyability.NOT_FLYABLE -> "Not flyable"
    }
    val wind = if (hour.windKmh.isEmpty()) "" else ", wind ${hour.windKmh} kilometres an hour"
    return if (hour.why.isEmpty()) "$verdict$wind" else "$verdict: ${hour.why}$wind"
}

/**
 * What the reading is, whose data it is, and how fresh: the design
 * language asks a forecast for its update time and its source.
 */
@Composable
private fun Foot(today: TodayModel) {
    val outlook = today.places.firstNotNullOfOrNull { it.outlook }
    val style = MaterialTheme.typography.bodySmall
    val colour = Venti.colours.textSecondary
    Column(modifier = Modifier.fillMaxWidth().testTag("today-foot"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        if (outlook != null && outlook.note.isNotEmpty()) {
            Text(
                "${outlook.note} Run of ${Format.time(outlook.runUnixS)}, looked up at ${Format.time(outlook.fetchedUnixS)}.",
                style = style,
                color = colour,
            )
        }
        Text(
            "Flyable, marginal and not flyable are what the forecast's wind, gusts, rain and cloud say by fixed " +
                "limits. They are not advice to fly.",
            style = style,
            color = colour,
        )
        for (credit in today.credits) {
            Text(credit.text, style = style, color = colour)
        }
    }
}
