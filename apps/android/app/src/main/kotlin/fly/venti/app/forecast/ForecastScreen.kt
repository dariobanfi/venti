package fly.venti.app.forecast

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LocalDarkAppearance
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindModel
import fly.venti.app.wind.ink
import fly.venti.app.wind.windDart
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.ForecastDay
import fly.venti.core.ForecastView
import fly.venti.core.ForecastWind
import fly.venti.core.ModelForecast
import kotlinx.coroutines.CancellationException
import kotlin.math.roundToInt

/** A place whose forecast is asked for: what it is called and where it is. */
data class ForecastPlace(val name: String, val position: Coordinate)

/**
 * The forecast for a place the pilot pressed on, on the map: a sheet of its
 * own, named by where the place is. At half height first, like a site's and
 * a station's page: the place that was pressed stays in view above it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ForecastSheet(place: Coordinate, wind: WindModel, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        // As tall as the sheet can be from the start. A sheet stops at half
        // height only when what it holds is taller than that, and it holds
        // little while the forecast is being fetched: it would open at its
        // full height then, and stay there once the table is in it.
        Box(Modifier.fillMaxHeight()) {
            ForecastPage(
                ForecastPlace(Format.place(place.lat, place.lon), place),
                wind,
                onBack = null,
                onDone = onDismiss,
            )
        }
    }
}

private sealed interface Phase {
    data object Loading : Phase

    data class Loaded(val forecast: ForecastView) : Phase

    data class Failed(val message: String) : Phase
}

/**
 * What the weather models say about one place: the wind through the day at
 * the ground and higher up, the cloud base, cloud, rain and temperature,
 * hour by hour. Shown in the sheet of the site or the station it was opened
 * from, or in a sheet of its own.
 *
 * It shows magnitudes and no verdict. Whether a day can be flown depends on
 * the pilot, the wing and the place, and a model's cell of a few kilometres
 * knows none of them.
 */
@Composable
fun ForecastPage(
    place: ForecastPlace,
    wind: WindModel,
    /** Back to the page the forecast was opened from, if there is one. */
    onBack: (() -> Unit)?,
    onDone: () -> Unit,
) {
    var phase by remember(place) { mutableStateOf<Phase>(Phase.Loading) }
    var attempt by remember(place) { mutableIntStateOf(0) }
    // The model looked at, by its identifier: the finest one at first.
    var model by remember(place) { mutableStateOf<String?>(null) }
    // The day looked at, by its first moment.
    var day by remember(place) { mutableStateOf<Long?>(null) }

    LaunchedEffect(place, attempt) {
        phase = Phase.Loading
        phase = try {
            Phase.Loaded(wind.forecast(place.position))
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Phase.Failed(
                if (error is CoreException) {
                    error.pilotMessage
                } else {
                    "The server could not be reached. ${error.pilotMessage}"
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
                place.name,
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onDone) { Text("Done") }
        }
        when (val current = phase) {
            Phase.Loading -> Box(Modifier.fillMaxWidth().padding(vertical = 96.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
            is Phase.Failed -> NoForecast("No forecast", current.message, onRetry = { attempt += 1 })
            is Phase.Loaded -> {
                val models = current.forecast.models
                val shown = models.firstOrNull { it.model == model } ?: models.firstOrNull()
                if (shown == null) {
                    NoForecast("No forecast for this place", "None of the server's weather models reaches it.", onRetry = null)
                } else {
                    Loaded(current.forecast, shown, day, onDay = { day = it }, onModel = { model = it })
                }
            }
        }
    }
}

/** Said in place of the forecast when there is none. */
@Composable
private fun NoForecast(title: String, text: String, onRetry: (() -> Unit)?) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            painterResource(R.drawable.ic_cloud_off),
            contentDescription = null,
            modifier = Modifier.size(40.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(title, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
        Text(
            text,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        if (onRetry != null) {
            TextButton(onClick = onRetry) { Text("Try again") }
        }
    }
}

/** The days to pick from, the table of the chosen one, and which model it is from. */
@Composable
private fun Loaded(
    forecast: ForecastView,
    shown: ModelForecast,
    day: Long?,
    onDay: (Long) -> Unit,
    onModel: (String) -> Unit,
) {
    val now = remember { System.currentTimeMillis() / 1000 }
    val days = remember(shown) { ForecastRows.days(shown) }
    val chosen = days.firstOrNull { it.startUnixS == day } ?: ForecastRows.today(days, now)

    Column(
        Modifier
            .testTag("forecast")
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 32.dp),
    ) {
        if (days.size > 1 && chosen != null) {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().testTag("forecast-day")) {
                days.forEachIndexed { index, each ->
                    SegmentedButton(
                        selected = each.startUnixS == chosen.startUnixS,
                        onClick = { onDay(each.startUnixS) },
                        shape = SegmentedButtonDefaults.itemShape(index, days.size),
                        modifier = Modifier.testTag("forecast-day-$index"),
                    ) {
                        Text(ForecastRows.name(each, now))
                    }
                }
            }
        }
        if (chosen != null) {
            SectionHeader("Wind in km/h, and the sky")
            Group {
                row { ForecastTable(shown, chosen, Modifier.padding(start = 12.dp, top = 10.dp, bottom = 10.dp)) }
            }
            SectionFooter(ForecastRows.caveat(shown))
        }
        Spacer(Modifier.height(24.dp))
        Group {
            row {
                if (forecast.models.size > 1) {
                    ModelRow(forecast.models, shown, onModel)
                } else {
                    ValueRow("Weather model", shown.name)
                }
            }
            row { ValueRow("Model run", Format.dayAndTime(shown.runUnixS)) }
            shown.cellElevationM?.let { ground ->
                row { ValueRow("The model's ground here", "${Format.metres(ground)} m") }
            }
        }
        SectionFooter(forecast.credits.joinToString("\n") { it.text })
    }
}

/** Which weather model the table is from, and a menu of the others: one at a time, never a blend. */
@Composable
private fun ModelRow(models: List<ModelForecast>, shown: ModelForecast, onChoose: (String) -> Unit) {
    var isOpen by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("forecast-model")
            .clickable(role = Role.DropdownList) { isOpen = true }
            .heightIn(min = 52.dp)
            .padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("Weather model", style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        // The menu opens where the chosen model stands, at the row's end.
        Box {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    shown.name,
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Icon(
                    painterResource(R.drawable.ic_arrow_drop_down),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            DropdownMenu(expanded = isOpen, onDismissRequest = { isOpen = false }) {
                for (model in models) {
                    DropdownMenuItem(
                        text = { Text(if (model.grid.isEmpty()) model.name else "${model.name}, ${model.grid}") },
                        onClick = {
                            isOpen = false
                            onChoose(model.model)
                        },
                        leadingIcon = {
                            if (model.model == shown.model) {
                                Icon(painterResource(R.drawable.ic_check), contentDescription = "Chosen")
                            } else {
                                Spacer(Modifier.size(24.dp))
                            }
                        },
                    )
                }
            }
        }
    }
}

/**
 * The hours of one day as a table: a row for each height the wind is given
 * at, the highest at the top, then the sky. The row names stay in place and
 * the hours scroll.
 *
 * The table is sized in the units of text, so that it grows with the
 * pilot's text size.
 */
@Composable
fun ForecastTable(model: ModelForecast, day: ForecastDay, modifier: Modifier = Modifier) {
    val hours = remember(day) { ForecastRows.hours(day) }
    val rows = remember(model, hours) { ForecastRows.of(model, hours) }
    val (names, cell, row, head) = with(LocalDensity.current) {
        listOf(NAMES_WIDTH.toDp(), CELL_WIDTH.toDp(), ROW_HEIGHT.toDp(), HEAD_HEIGHT.toDp())
    }

    Row(modifier.testTag("forecast-table")) {
        // The names of the rows. A screen reader hears them with their rows.
        Column(Modifier.width(names).clearAndSetSemantics {}) {
            Spacer(Modifier.height(head))
            for (each in rows) {
                Column(Modifier.height(row), verticalArrangement = Arrangement.Center) {
                    Text(each.name, style = MaterialTheme.typography.labelLarge, maxLines = 1)
                    if (each.detail.isNotEmpty()) {
                        Text(
                            each.detail,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
        Column(Modifier.horizontalScroll(rememberScrollState()).padding(end = 12.dp)) {
            Row(Modifier.clearAndSetSemantics {}) {
                for (hour in hours) {
                    Box(Modifier.size(cell, head), contentAlignment = Alignment.Center) {
                        Text(
                            ForecastRows.clock(hour.timeUnixS),
                            style = MaterialTheme.typography.labelMedium.numbers,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            for (each in rows) {
                Row(
                    // One element for a screen reader: the row's name, then its hours.
                    Modifier.clearAndSetSemantics {
                        contentDescription = ForecastRows.spokenName(each)
                        stateDescription = ForecastRows.spoken(each, hours)
                    },
                ) {
                    for (value in each.cells) {
                        Box(Modifier.size(cell, row), contentAlignment = Alignment.Center) { Cell(value) }
                    }
                }
            }
        }
    }
}

// The table's measures, in the units of text. A column is wide enough for
// a wind with its gusts ("12/48") and for a height ("1,600") to stand apart
// from their neighbours.
private val NAMES_WIDTH = 92.sp
private val CELL_WIDTH = 46.sp
private val ROW_HEIGHT = 42.sp
private val HEAD_HEIGHT = 22.sp
private val ARROW_SIDE = 17.sp
private val CLOUD_SIDE = 24.sp

/** One hour of one row. */
@Composable
private fun Cell(cell: ForecastCell) {
    when (cell) {
        is ForecastCell.Wind -> WindCell(cell.wind)
        is ForecastCell.Number -> Text(
            cell.text,
            style = MaterialTheme.typography.labelMedium.numbers,
            maxLines = 1,
            softWrap = false,
        )
        is ForecastCell.Cloud -> {
            // More sky covered, more of the square filled in.
            val share = (cell.percent.coerceIn(0.0, 100.0) / 100).toFloat()
            val side = with(LocalDensity.current) { CLOUD_SIDE.toDp() }
            Box(
                Modifier
                    .size(side)
                    .background(
                        MaterialTheme.colorScheme.onSurface.copy(alpha = 0.08f + 0.5f * share),
                        MaterialTheme.shapes.extraSmall,
                    ),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    cell.percent.roundToInt().toString(),
                    style = MaterialTheme.typography.labelSmall.numbers,
                    color = if (cell.percent > 55) MaterialTheme.colorScheme.surface else MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    softWrap = false,
                )
            }
        }
        ForecastCell.Empty -> Text(
            "–",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.outline,
        )
    }
}

/**
 * A wind: the map's dart pointing where the air moves, north at the top, in
 * the wind scale's blue, with its km/h below. A wind without a direction is
 * a dot.
 */
@Composable
private fun WindCell(wind: ForecastWind) {
    val colour = wind.band.ink(LocalDarkAppearance.current)
    val side = with(LocalDensity.current) { ARROW_SIDE.toDp() }
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Canvas(Modifier.size(side)) {
            val from = wind.fromDeg
            if (from == null) {
                drawCircle(colour, radius = size.minDimension * 0.16f)
            } else {
                rotate((from + 180).toFloat()) { drawPath(windDart(size.minDimension), colour) }
            }
        }
        Text(
            wind.label,
            style = MaterialTheme.typography.labelSmall.numbers,
            maxLines = 1,
            softWrap = false,
        )
    }
}
