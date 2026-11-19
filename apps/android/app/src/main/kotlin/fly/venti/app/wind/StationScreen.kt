package fly.venti.app.wind

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
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.Metric
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.pilotMessage
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.StationHistory
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
                SectionHeader("Last 24 hours")
                Group {
                    row {
                        Box(Modifier.fillMaxWidth().padding(16.dp)) {
                            when (val current = history) {
                                History.Loading -> CircularProgressIndicator(
                                    Modifier.align(Alignment.Center).padding(vertical = 44.dp),
                                )
                                is History.Ready -> {
                                    val samples = remember(current.history) { WindSample.of(current.history) }
                                    if (samples.size < 2) {
                                        Text(
                                            "The server has no history for this station yet.",
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
                // What the weather models say about the same place.
                Group(Modifier.padding(top = 24.dp)) {
                    row {
                        LinkRow(
                            "Forecast for this place",
                            R.drawable.ic_chevron_right,
                            Modifier.testTag("station-forecast"),
                            leading = R.drawable.ic_partly_cloudy_day,
                            onClick = onForecast,
                        )
                    }
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
    val reading = station.latest ?: return "The server has no reading from this station."
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
