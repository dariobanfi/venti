package fly.venti.app.wind

import fly.venti.core.nothingFoundWords
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.EmptyList
import fly.venti.app.design.Format
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.numbers
import fly.venti.app.forecast.ForecastPage
import fly.venti.app.forecast.ForecastPlace
import fly.venti.core.WindMark
import fly.venti.core.WindReading
import fly.venti.core.WindStation
import fly.venti.core.WindSymbol
import fly.venti.app.design.Common

/**
 * The sheet over the map for the wind: the stations in view as a list, or
 * one station's page. A station tapped on the map opens its page alone; one
 * chosen from the list leads back to the list.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WindSheet(
    wind: WindModel,
    /** Whether the list is what the pilot opened. */
    showsList: Boolean,
    /** The station whose page shows, if any. */
    chosen: WindStation?,
    onChoose: (WindStation?) -> Unit,
    onDismiss: () -> Unit,
) {
    // The place whose forecast shows, over the station's page.
    var forecast by remember { mutableStateOf<ForecastPlace?>(null) }
    // The list opens at full height, for the search field and the names; a
    // station tapped on the map leaves the map in view.
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = showsList)
    // A forecast is a table of a whole day: it takes all the room there is.
    LaunchedEffect(forecast) {
        if (forecast != null) sheet.expand()
    }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheet,
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        val place = forecast
        if (place != null) {
            ForecastPage(place, wind, onBack = { forecast = null }, onDone = onDismiss)
        } else if (chosen != null) {
            StationPage(
                // The newest reading, if the stations were fetched again
                // while the page was open.
                station = wind.stations.firstOrNull { it.id == chosen.id } ?: chosen,
                wind = wind,
                onForecast = { forecast = ForecastPlace(chosen.name, chosen.position) },
                onBack = if (showsList) ({ onChoose(null) }) else null,
                onDone = onDismiss,
            )
        } else {
            StationList(wind, onOpen = onChoose, onDone = onDismiss)
        }
    }
}

/** The wind stations around the part of the map in view, as a list. */
@Composable
private fun StationList(wind: WindModel, onOpen: (WindStation) -> Unit, onDone: () -> Unit) {
    var search by rememberSaveable { mutableStateOf("") }
    val wanted = search.trim()
    // By name, the way a pilot looks one up.
    val shown = remember(wind.stations, wanted) {
        wind.stations
            .filter { wanted.isEmpty() || it.name.contains(wanted, ignoreCase = true) }
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })
    }

    Column {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Wind stations", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDone) { Text(Common.words.done) }
        }
        if (wind.stations.isEmpty()) {
            EmptyList(R.drawable.venti_wind, "No stations here", wind.problem ?: "Move the map to an area with wind stations.")
        } else {
            OutlinedTextField(
                value = search,
                onValueChange = { search = it },
                placeholder = { Text("Station") },
                leadingIcon = { Icon(painterResource(R.drawable.ic_search), contentDescription = null) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Words,
                    autoCorrectEnabled = false,
                ),
                shape = MaterialTheme.shapes.extraLarge,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            )
            if (shown.isEmpty()) {
                val nothing = nothingFoundWords(wanted)
                EmptyList(R.drawable.venti_wind, nothing.title, nothing.line)
            } else {
                LazyColumn(Modifier.testTag("station-list"), contentPadding = PaddingValues(bottom = 32.dp)) {
                    items(shown, key = { it.id }) { station ->
                        StationRow(station, wind.now, onClick = { onOpen(station) })
                        HorizontalDivider(
                            modifier = Modifier.padding(start = 64.dp),
                            color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f),
                        )
                    }
                    if (wind.credits.isNotEmpty()) {
                        item {
                            SectionFooter(
                                wind.credits.joinToString("\n") { it.text },
                                Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

/**
 * One station in a line: where the wind blows, its name and height, and the
 * wind in numbers.
 */
@Composable
private fun StationRow(
    station: WindStation,
    /** The current time by the server's clock. */
    now: Long,
    onClick: () -> Unit,
) {
    val mark = station.mark
    val detail = StationWords.detail(station, now)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("station-row")
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 64.dp)
            .padding(horizontal = 20.dp, vertical = 10.dp)
            // One element for a screen reader: the name, then the wind in words.
            .clearAndSetSemantics {
                contentDescription = station.name
                stateDescription = StationWords.spoken(station, mark, detail)
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        WindGlyph(mark)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(
                station.name,
                style = MaterialTheme.typography.bodyLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (detail.isNotEmpty()) {
                Text(
                    detail,
                    style = MaterialTheme.typography.bodyMedium.numbers,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (mark.label.isNotEmpty()) {
            Spacer(Modifier.width(12.dp))
            val unit = MaterialTheme.typography.bodySmall
            val quiet = MaterialTheme.colorScheme.onSurfaceVariant
            Text(
                buildAnnotatedString {
                    append(mark.label)
                    withStyle(SpanStyle(fontSize = unit.fontSize, fontWeight = FontWeight.Normal, color = quiet)) {
                        append(" km/h")
                    }
                },
                style = MaterialTheme.typography.titleMedium.numbers,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
        }
    }
}

/** What a station's row says beside its name, on screen and aloud. */
object StationWords {
    /**
     * The station's height, how old its reading is if that matters, and its
     * network: the core's `line`, built here only because the age is counted
     * on by this phone's running clock, where the core's is as old as the
     * answer.
     */
    fun detail(station: WindStation, now: Long): String {
        val parts = mutableListOf<String>()
        station.elevationM?.let { parts += "${Format.metres(it)} m" }
        val reading = station.latest
        if (reading == null) {
            parts += "no reading"
        } else if (station.isOld) {
            parts += age(station, reading, now)
        }
        station.network?.let { parts += it }
        return parts.joinToString(" · ")
    }

    /**
     * How old a reading is, "25 minutes ago"; or its own time on the clock,
     * "14:05", where the core gives no age because Venti's clock was not
     * known when the stations were read.
     */
    fun age(station: WindStation, reading: WindReading, now: Long): String =
        if (station.ageMin == null) Format.time(reading.timeUnixS) else Format.age(Format.minutes(from = reading.timeUnixS, to = now))

    /** The wind in words, for a screen reader. */
    fun spoken(station: WindStation, mark: WindMark, detail: String): String {
        val reading = station.latest
        val speed = reading?.speedMs
        if (reading == null || speed == null || station.isOld) {
            return "No current reading. $detail"
        }
        val parts = mutableListOf("Wind ${Format.spokenKilometresPerHour(speed)}")
        reading.gustMs?.let { parts += "gusts ${Format.kilometresPerHour(it)}" }
        val from = reading.directionDeg
        if (mark.symbol is WindSymbol.Arrow && from != null) {
            parts += "from ${Format.compassPoint(from)}"
        }
        // The network last, as on screen: two stations can share a name.
        station.network?.let { parts += it }
        return parts.joinToString(", ")
    }
}
