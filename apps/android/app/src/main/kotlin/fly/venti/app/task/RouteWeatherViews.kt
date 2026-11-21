package fly.venti.app.task

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.forecast.ThermalMapModel
import fly.venti.app.forecast.ThermalMapWords
import fly.venti.core.Coordinate
import kotlinx.coroutines.launch

/**
 * The weather along the route, under its legs and score: folded until the
 * pilot opens it, then the day, the start, the speed and the glide, and
 * what the core says of the route, of each place and of each leg.
 */
@Composable
fun RouteWeatherSection(
    weather: RouteWeatherModel,
    /** The thermal map, for its days and Venti's clock. */
    thermalMap: ThermalMapModel,
    places: List<Coordinate>,
    /** The pilot's glide, as the settings keep it. */
    glideRatio: Double,
) {
    val colours = Venti.colours
    val scope = rememberCoroutineScope()
    var isOpen by remember { mutableStateOf(false) }
    var chosenDate by remember { mutableStateOf<String?>(null) }
    var today by remember { mutableStateOf<String?>(null) }
    var start by remember { mutableStateOf<Int?>(null) }
    var speed by remember { mutableStateOf(RouteWeatherWords.SPEED_KMH) }
    var glide by remember { mutableStateOf(glideRatio.coerceIn(RouteWeatherWords.GLIDE_RATIOS)) }

    // The days are the thermal map's: asked for here if the map tab has not yet.
    LaunchedEffect(isOpen) {
        if (!isOpen) return@LaunchedEffect
        if (start == null) {
            val now = thermalMap.clock()
            today = RouteWeatherWords.date(now)
            start = RouteWeatherWords.nextFullHour(now)
        }
        if (thermalMap.map == null && !thermalMap.isAsking) thermalMap.load()
    }
    val days = thermalMap.map?.days.orEmpty()
    val day = days.firstOrNull { it.date == chosenDate }?.date ?: days.firstOrNull()?.date ?: today
    val startAt = start
    LaunchedEffect(isOpen, places, day, startAt) {
        if (isOpen && day != null && startAt != null) weather.show(places, day)
    }
    val forecast = weather.forecast
    val heights = weather.heights
    val along = remember(places, day, startAt, speed, glide, forecast, heights) {
        if (places.size >= 2 && day != null && startAt != null) {
            weather.along(places, RouteWeatherWords.startUnixS(day, startAt), speed, glide)
        } else {
            null
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = VentiSize.touch)
                .testTag("route-weather-toggle")
                .clickable(role = Role.Button, onClick = { isOpen = !isOpen })
                .semantics {
                    heading()
                    stateDescription = if (isOpen) ThermalMapWords.EXPANDED else ThermalMapWords.COLLAPSED
                    if (isOpen) {
                        collapse {
                            isOpen = false
                            true
                        }
                    } else {
                        expand {
                            isOpen = true
                            true
                        }
                    }
                },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
        ) {
            Text(
                RouteWeatherWords.TITLE,
                style = MaterialTheme.typography.titleSmall,
                color = colours.text,
                modifier = Modifier.weight(1f),
            )
            Icon(
                painterResource(R.drawable.ic_arrow_drop_down),
                contentDescription = null,
                tint = colours.text,
                // The arrow points up while what it unfolded can be folded away.
                modifier = Modifier.rotate(if (isOpen) 180f else 0f),
            )
        }
        if (!isOpen) return@Column

        if (days.size > 1) {
            SegmentedPills(
                days.map { it.name },
                days.indexOfFirst { it.date == day }.coerceAtLeast(0),
                Modifier.testTag("route-weather-days"),
                fitToWords = days.size > 3,
                onChoose = { index -> chosenDate = days[index].date },
            )
        }
        startAt?.let { minute ->
            Stepper(RouteWeatherWords.START, RouteWeatherWords.clock(minute)) { steps ->
                start = RouteWeatherWords.stepStart(minute, steps)
            }
        }
        Stepper(RouteWeatherWords.SPEED, "$speed km/h") { steps -> speed = RouteWeatherWords.stepSpeed(speed, steps) }
        Stepper(RouteWeatherWords.GLIDE, Format.glideRatio(glide)) { steps -> glide = RouteWeatherWords.stepGlide(glide, steps) }

        val problem = weather.problem
        when {
            weather.isAsking -> Row(
                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("route-weather-asking"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                CircularProgressIndicator(Modifier.size(VentiSpace.s4), strokeWidth = VentiSpace.s1 / 2, color = colours.primary)
                Text(RouteWeatherWords.ASKING, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
            }
            problem != null -> Column(Modifier.testTag("route-weather-problem")) {
                Text(problem, style = MaterialTheme.typography.bodyMedium, color = colours.text)
                if (problem != RouteWeatherWords.NO_SERVER) {
                    TextButton(
                        onClick = {
                            weather.forgetForecast()
                            if (day != null) scope.launch { weather.show(places, day) }
                        },
                        modifier = Modifier.heightIn(min = VentiSize.touch),
                    ) { Text(RouteWeatherWords.TRY_AGAIN) }
                }
            }
        }

        if (along == null) return@Column
        Text(
            along.words,
            style = MaterialTheme.typography.bodyMedium,
            color = colours.text,
            modifier = Modifier.testTag("route-weather-words"),
        )
        along.points.forEachIndexed { index, point ->
            Column(Modifier.testTag("route-weather-point")) {
                Text(point.words, style = MaterialTheme.typography.bodyMedium, color = colours.text)
                if (point.departureWords.isNotEmpty()) {
                    Text(
                        point.departureWords,
                        style = MaterialTheme.typography.bodySmall,
                        color = colours.textSecondary,
                        modifier = Modifier.testTag("route-weather-departure"),
                    )
                }
            }
            along.legs.getOrNull(index)?.let { leg ->
                Text(
                    leg.words,
                    style = MaterialTheme.typography.bodySmall,
                    color = colours.textSecondary,
                    modifier = Modifier.testTag("route-weather-leg"),
                )
            }
        }
        val credits = listOfNotNull(along.model) + along.credits.map { it.text }
        if (credits.isNotEmpty()) {
            Text(
                credits.joinToString("\n"),
                style = MaterialTheme.typography.bodySmall,
                color = colours.textMuted,
                modifier = Modifier.testTag("route-weather-credits"),
            )
        }
    }
}

/** A setting with a button either side of it to make it less or more, by one step. */
@Composable
internal fun Stepper(title: String, value: String, onStep: (Int) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(title, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.text, modifier = Modifier.weight(1f))
        IconButton(onClick = { onStep(-1) }) {
            Icon(painterResource(R.drawable.ic_remove), contentDescription = "Less: $title")
        }
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium.numbers,
            color = Venti.colours.text,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(min = VentiSpace.s8),
        )
        IconButton(onClick = { onStep(1) }) {
            Icon(painterResource(R.drawable.ic_add), contentDescription = "More: $title")
        }
    }
}
