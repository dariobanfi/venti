package fly.venti.app.wind

import fly.venti.core.stationSheetWords
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import fly.venti.app.R
import fly.venti.app.alerts.AlertMeRow
import fly.venti.app.alerts.AlertsModel
import fly.venti.app.alerts.LocalAlerts
import fly.venti.app.design.DisclosureRow
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.Metric
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.pilotMessage
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.StationAir
import fly.venti.core.StationDay
import fly.venti.core.StationDays
import fly.venti.core.StationHistory
import fly.venti.core.TemperatureStep
import fly.venti.core.stationAir
import fly.venti.core.stationDate
import fly.venti.core.WindStation
import kotlinx.coroutines.CancellationException

private sealed interface History {
    data object Loading : History

    data class Ready(val history: StationHistory) : History

    data class Failed(val message: String) : History
}

/**
 * One wind station: what it measures now, how the last 24 hours went, and
 * whose data it is. Shown in a sheet over the map.
 */
@Composable
fun StationPage(
    station: WindStation,
    wind: WindModel,
    /** Opens the forecast for the station's place. */
    onForecast: () -> Unit,
    /** Back to the list of stations, if that is where the pilot came from. */
    onBack: (() -> Unit)?,
    onDone: () -> Unit,
) {
    var history by remember(station.id) { mutableStateOf<History>(History.Loading) }
    var attempt by remember(station.id) { mutableIntStateOf(0) }
    // What the core reads of the air from this station and the others on the map.
    val air = remember(station, wind.stations) { stationAir(station, wind.stations) }
    // The day chosen, by its date: `null` for the last 24 hours.
    var date by remember(station.id) { mutableStateOf<String?>(null) }
    var day by remember(station.id) { mutableStateOf<Day>(Day.Loading) }
    val days = remember(station.id) { wind.stationDays() }

    var dayAttempt by remember(station.id) { mutableIntStateOf(0) }
    LaunchedEffect(station.id, date, dayAttempt) {
        val chosen = date ?: return@LaunchedEffect
        day = Day.Loading
        day = try {
            Day.Ready(wind.day(station, chosen))
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Day.Failed(if (error is CoreException) error.pilotMessage else "The day could not be loaded. ${error.pilotMessage}")
        }
    }

    LaunchedEffect(station.id, attempt) {
        history = History.Loading
        history = try {
            History.Ready(wind.history(station))
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            History.Failed(
                if (error is CoreException) {
                    error.pilotMessage
                } else {
                    "The history could not be loaded. ${error.pilotMessage}"
                },
            )
        }
    }

    // The air's card and the chart's explanation stand behind a row at the end.
    var showsMoreAir by rememberSaveable(station.id) { mutableStateOf(false) }
    Column {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = if (onBack != null) 4.dp else 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                IconButton(onClick = onBack) {
                    Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                }
            }
            Text(
                station.name,
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onDone) { Text("Done") }
        }
        LazyColumn(
            modifier = Modifier.testTag("station-page"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 32.dp),
        ) {
            item {
                Now(station)
                SectionFooter(ageText(station, wind.now))
            }
            item {
                // The last 24 hours, or a day the pilot chose, back as far as
                // the server keeps a station's readings.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    // A day chosen is named as the app names days once the core has read it.
                    val shown = (day as? Day.Ready)?.day?.takeIf { it.date == date }?.let { Format.day(it.startUnixS) }
                    SectionHeader(if (date == null) "Last 24 hours" else shown ?: date.orEmpty(), Modifier.weight(1f))
                    // Today is the last 24 hours again.
                    DayChoice(days, date ?: days.lastDate, onChoose = { chosen -> date = chosen.takeIf { it != days.lastDate } })
                }
                Group(Modifier.padding(top = 8.dp)) {
                    row {
                        Box(Modifier.fillMaxWidth().padding(16.dp).testTag("station-chart")) {
                            if (date != null) {
                                DayChart(day, onTryAgain = { dayAttempt += 1 })
                                return@Box
                            }
                            when (val current = history) {
                                History.Loading -> CircularProgressIndicator(
                                    Modifier.align(Alignment.Center).padding(vertical = 44.dp),
                                )
                                is History.Ready -> {
                                    val samples = remember(current.history) { WindSample.of(current.history) }
                                    if (samples.size < 2) {
                                        Text(
                                            "Venti has no history for this station yet.",
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    } else {
                                        WindHistoryCharts(samples)
                                    }
                                }
                                is History.Failed -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                    Text(current.message, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    TextButton(onClick = { attempt += 1 }) { Text("Try again") }
                                }
                            }
                        }
                    }
                }
            }
            item {
                // What the weather models say about the same place, and an
                // alarm for this station's wind.
                val alerts = LocalAlerts.current
                Group(Modifier.padding(top = 24.dp)) {
                    row {
                        LinkRow(
                            "Forecast for this place",
                            R.drawable.ic_chevron_right,
                            Modifier.testTag("station-forecast"),
                            leading = R.drawable.venti_forecast,
                            onClick = onForecast,
                        )
                    }
                    if (alerts != null) {
                        row {
                            AlertMeRow(alerts, "station-alert-me") { AlertsModel.windDraft(station.id, station.name) }
                        }
                    }
                }
            }
            // What the core reads of the air from the readings, and how the
            // chart's hours are to be read: behind a row at the end.
            item {
                Group(Modifier.padding(top = 24.dp)) {
                    row {
                        DisclosureRow(
                            stationSheetWords().moreAirTitle,
                            isOpen = showsMoreAir,
                            modifier = Modifier.testTag("station-more-air"),
                            onChange = { showsMoreAir = it },
                        )
                    }
                }
                if (showsMoreAir) {
                    SectionFooter(days.note, Modifier.testTag("station-days-note"))
                    if (air.cloudBase != null || air.temperatureStep != null) Air(air)
                }
            }
            val credit = (history as? History.Ready)?.history?.credits?.firstOrNull()
                ?: wind.credits.firstOrNull { it.source == station.source }
            if (credit != null) {
                item { Source(credit) }
            }
        }
    }
}

/** One chosen day of the station's readings. */
private sealed interface Day {
    data object Loading : Day

    data class Ready(val day: StationDay) : Day

    data class Failed(val message: String) : Day
}

/** A chosen day's chart over the whole day, or the core's words where it has no readings. */
@Composable
private fun DayChart(day: Day, onTryAgain: () -> Unit) {
    when (day) {
        Day.Loading -> Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(Modifier.padding(vertical = 44.dp))
        }
        is Day.Failed -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(day.message, color = MaterialTheme.colorScheme.onSurfaceVariant)
            TextButton(onClick = onTryAgain) { Text("Try again") }
        }
        is Day.Ready -> {
            val samples = remember(day.day) { WindSample.of(day.day.readings) }
            val empty = day.day.emptyWords
            if (empty != null || samples.size < 2) {
                Text(
                    empty ?: "Venti has no history for this station yet.",
                    modifier = Modifier.testTag("station-day-empty"),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                WindHistoryCharts(samples, span = day.day.startUnixS..day.day.endUnixS)
            }
        }
    }
}

/**
 * The button that chooses a day: Material's date picker, limited to the
 * days the core names. Dates pass as the core writes them; the picker's
 * own moment (midnight UTC of the day) is named by the core as well.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DayChoice(days: StationDays, shown: String, onChoose: (String) -> Unit) {
    var isOpen by remember { mutableStateOf(false) }
    TextButton(onClick = { isOpen = true }, modifier = Modifier.testTag("station-day")) {
        Icon(painterResource(R.drawable.ic_calendar_month), contentDescription = null)
        Text(shown, modifier = Modifier.padding(start = 8.dp))
    }
    if (!isOpen) return
    val picker = rememberDatePickerState(
        initialSelectedDateMillis = StationDayWords.millis(shown),
        selectableDates = object : SelectableDates {
            override fun isSelectableDate(utcTimeMillis: Long): Boolean =
                StationDayWords.date(utcTimeMillis).let { it >= days.firstDate && it <= days.lastDate }

            override fun isSelectableYear(year: Int): Boolean =
                year.toString() >= days.firstDate.take(4) && year.toString() <= days.lastDate.take(4)
        },
    )
    DatePickerDialog(
        onDismissRequest = { isOpen = false },
        confirmButton = {
            TextButton(
                onClick = {
                    isOpen = false
                    picker.selectedDateMillis?.let { onChoose(StationDayWords.date(it)) }
                },
                modifier = Modifier.testTag("station-day-ok"),
            ) { Text("Show") }
        },
        dismissButton = { TextButton(onClick = { isOpen = false }) { Text("Cancel") } },
    ) {
        DatePicker(picker)
    }
}


/** A date picker's moments and the core's dates, one into the other. */
object StationDayWords {
    /** The date a picker means by its moment, midnight UTC of the day, as the core names it. */
    fun date(utcMillis: Long): String = stationDate(utcMillis / 1000, 0)

    /** The picker's moment for a date the core named. */
    fun millis(date: String): Long =
        java.time.LocalDate.parse(date).atStartOfDay(java.time.ZoneOffset.UTC).toInstant().toEpochMilli()
}

/**
 * What the core reads of the air from this station's readings: the cloud
 * base, as a reading with its words and its note, and the temperature step
 * to a station much higher or lower nearby, with what it is said for.
 */
@Composable
private fun Air(air: StationAir) {
    SectionHeader("The air")
    Group(Modifier.testTag("station-air")) {
        air.cloudBase?.let { base ->
            row {
                Column(Modifier.fillMaxWidth().padding(16.dp).testTag("station-cloud-base")) {
                    Metric(base.words, Format.metres(base.baseM), unit = "m")
                    Text(
                        base.note,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                }
            }
        }
        air.temperatureStep?.let { step ->
            row {
                Column(
                    Modifier.fillMaxWidth().padding(16.dp).testTag("station-temperature-step"),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(StationAirWords.between(step), style = MaterialTheme.typography.bodyLarge)
                    Text(step.words, style = MaterialTheme.typography.titleMedium)
                    Text(step.rule, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(step.note, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

/** The words around the core's numbers for the temperature step. */
object StationAirWords {
    /** "Patscherkofel, 8.2 km away and 1673 m higher: −0.60 °C per 100 m". */
    fun between(step: TemperatureStep): String {
        val way = if (step.higherByM >= 0) "higher" else "lower"
        return "${step.otherName}, ${Format.kilometres(step.distanceM)} km away and " +
            "${Format.metres(kotlin.math.abs(step.higherByM))} m $way: ${Format.perHundredMetres(step.per100mC)} °C per 100 m"
    }
}

/** What the station measures now, as six numbers. */
@Composable
private fun Now(station: WindStation) {
    val reading = station.latest
    Group {
        row {
            Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val third = Modifier.weight(1f)
                Metric(
                    "Wind",
                    reading?.speedMs?.let(Format::kilometresPerHour) ?: "–",
                    third,
                    unit = "km/h",
                )
                Metric(
                    "Gusts",
                    reading?.gustMs?.let(Format::kilometresPerHour) ?: "–",
                    third,
                    unit = "km/h",
                )
                Metric(
                    "From",
                    reading?.directionDeg?.let(Format::compassPoint) ?: "–",
                    third,
                    unit = reading?.directionDeg?.let { "${Format.whole(it)}°" }.orEmpty(),
                )
            }
        }
        row {
            Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val third = Modifier.weight(1f)
                Metric(
                    "Temperature",
                    reading?.temperatureC?.let(Format::temperature) ?: "–",
                    third,
                    unit = "°C",
                )
                Metric("Height", station.elevationM?.let(Format::metres) ?: "–", third, unit = "m")
                Metric("Measured", reading?.let { Format.time(it.timeUnixS) } ?: "–", third)
            }
        }
    }
}

/** How old the reading is, and what that means if it is old. */
private fun ageText(station: WindStation, now: Long): String {
    val reading = station.latest ?: return "Venti has no reading from this station."
    val age = Format.age(Format.minutes(from = reading.timeUnixS, to = now))
    return if (station.isOld) {
        "Measured $age. Too old to say what the wind is doing now."
    } else {
        "Measured $age."
    }
}

/** Whose data the station's is, worded as the provider asks, with the licence. */
@Composable
private fun Source(credit: Credit) {
    val links = LocalUriHandler.current
    SectionHeader("Source")
    Group {
        row {
            Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
                Text(credit.text, style = MaterialTheme.typography.bodyLarge)
                Text(
                    "${credit.provider}, under ${credit.licence}.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 2.dp),
                )
                Text(
                    credit.url.toUri().host ?: credit.url,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier
                        .padding(top = 2.dp)
                        .clickable(role = Role.Button) { links.openUri(credit.url) }
                        .heightIn(min = 32.dp)
                        .padding(vertical = 6.dp),
                )
            }
        }
    }
}
