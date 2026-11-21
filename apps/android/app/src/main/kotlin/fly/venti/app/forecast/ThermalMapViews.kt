package fly.venti.app.forecast

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import fly.venti.app.design.PillShape
import fly.venti.app.design.numbers
import fly.venti.core.Coordinate
import fly.venti.core.ThermalHoursLayer
import fly.venti.core.ThermalStripDay
import fly.venti.app.wind.WindGlyph
import fly.venti.core.ThermalMapHoursView
import fly.venti.core.ThermalMapKey
import fly.venti.core.ThermalMapLayerChoice
import fly.venti.core.ThermalMapLevel
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fly.venti.app.R
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.wind.WindModel
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.CollectionInfo
import androidx.compose.ui.semantics.CollectionItemInfo
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collectionInfo
import androidx.compose.ui.semantics.collectionItemInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.design.FloatElevation
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.StatusChip
import fly.venti.app.design.Venti
import fly.venti.app.design.status
import fly.venti.app.today.ThermalLine
import fly.venti.core.Bounds
import fly.venti.core.ThermalLegendEntry
import fly.venti.core.ThermalMap
import fly.venti.core.ThermalMapCell
import fly.venti.core.ThermalMapDay

/**
 * The card at the map's lower edge while the forecast map shows. It leaves
 * the map to be seen: the layers, the days, the line of hours and one line
 * of words for what the map shows now are always there; the key, the model
 * with when its run began and the notes stand behind "Key and notes",
 * closed when the card opens. While the map is asked for it says so; when
 * there is none, why, with "Try again".
 */
@Composable
fun ThermalMapCard(
    state: ThermalMapState,
    /** The part of the world in view, to say when the map has nothing there. */
    region: Bounds?,
    onChoose: (String) -> Unit,
    onTryAgain: () -> Unit,
    modifier: Modifier = Modifier,
    /** The map by the hour, where the server has hours; `null` without. */
    hours: ThermalHoursModel? = null,
    onLayer: (ThermalHoursLayer) -> Unit = {},
    onLevel: (Int?) -> Unit = {},
    onTryAgainHours: () -> Unit = {},
) {
    val colours = Venti.colours
    // The key and the notes: unfolded by the pilot, folded when the card opens.
    var showsKey by remember { mutableStateOf(false) }
    MapCard("forecast-map-card", isUnfolded = showsKey, modifier = modifier) {
        when (state) {
            ThermalMapState.Asking -> Row(
                modifier = Modifier.padding(bottom = VentiSpace.s3).testTag("forecast-map-asking"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                CircularProgressIndicator(Modifier.size(VentiSpace.s4), strokeWidth = 2.dp)
                Text(ThermalMapWords.ASKING, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
            }
            is ThermalMapState.Failed -> Problem(state.message, onTryAgain)
            is ThermalMapState.There -> {
                val days = state.map.days
                val layers = state.map.layers
                val chosen = hours?.let { model -> layers.firstOrNull { it.layer == model.layer } }
                val hoursState = hours?.state
                val view = (hoursState as? ThermalHoursState.There)?.view
                val byTheHour = hours?.isOn == true && chosen != null
                if (hours != null && layers.isNotEmpty()) {
                    LayerRow(layers, hours.layer, onLayer)
                }
                if (days.size > 1) {
                    SegmentedPills(
                        days.map { it.name },
                        days.indexOf(state.day).coerceAtLeast(0),
                        Modifier.testTag("forecast-days"),
                        // Four days: each as wide as its name, so that "Tomorrow" stays whole.
                        fitToWords = days.size > 3,
                        onChoose = { index -> onChoose(days[index].date) },
                    )
                }
                // The hours, the words and "Key and notes" stand close:
                // each has the height a finger needs around it.
                Column {
                    if (hours != null && chosen != null) {
                        when (hoursState) {
                            is ThermalHoursState.There -> HourStrip(
                                hoursState.view,
                                withDay = hours.layer == ThermalHoursLayer.THERMALS,
                                chosen = hours.hour,
                                onChoose = { hours.hour = it },
                            )
                            ThermalHoursState.Asking -> Row(
                                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("forecast-hours-asking"),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                            ) {
                                CircularProgressIndicator(Modifier.size(VentiSpace.s4), strokeWidth = 2.dp)
                                Text(ThermalMapWords.ASKING_HOURS, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                            }
                            is ThermalHoursState.Failed -> Column(Modifier.testTag("forecast-hours-problem")) {
                                Text(hoursState.message, style = MaterialTheme.typography.bodyMedium, color = colours.text)
                                TextButton(onClick = onTryAgainHours, modifier = Modifier.heightIn(min = VentiSize.touch)) {
                                    Text(ThermalMapWords.TRY_AGAIN)
                                }
                            }
                            else -> Unit
                        }
                        // The layer and the hour in words and, with the
                        // wind, its height at their right: it stays in
                        // its place while its hours are asked for.
                        val said = view?.let { ThermalMapWords.said(it, hours.hour, state.day) }
                        val levels = state.map.levels.takeIf { chosen.hasLevels && it.isNotEmpty() }
                        if (said != null || levels != null) {
                            Said(said, levels, hours.heightM, onLevel)
                        }
                    }
                    if (region != null && !ThermalMapModel.hasSquaresIn(state.day, region)) {
                        Problem(ThermalMapWords.NONE_HERE, onTryAgain, Modifier.padding(top = VentiSpace.s2))
                    }
                    KeyAndNotes(showsKey, onChange = { showsKey = it }) {
                        if (byTheHour && chosen != null) {
                            HourKey(chosen.legend)
                        } else {
                            Key(state.map.legend)
                        }
                        Text(
                            if (byTheHour && view != null) {
                                "${view.model}, run of ${fly.venti.app.design.Format.dayAndTime(view.runUnixS)}"
                            } else {
                                ThermalMapWords.run(state.day)
                            },
                            style = MaterialTheme.typography.bodySmall,
                            color = colours.textSecondary,
                            modifier = Modifier.testTag("forecast-map-run"),
                        )
                        // The layer's note, and the hours' when the day is not whole.
                        if (byTheHour && chosen != null && chosen.note.isNotEmpty()) {
                            Text(
                                chosen.note,
                                style = MaterialTheme.typography.bodySmall,
                                color = colours.textSecondary,
                                modifier = Modifier.testTag("forecast-layer-note"),
                            )
                        }
                        if (byTheHour && view != null && !view.whole && view.note.isNotEmpty()) {
                            Text(view.note, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                        }
                        if (state.day.note.isNotEmpty()) {
                            Text(
                                state.day.note,
                                style = MaterialTheme.typography.bodySmall,
                                color = colours.textSecondary,
                                modifier = Modifier.testTag("forecast-map-day-note"),
                            )
                        }
                    }
                }
            }
        }
    }
}

/**
 * A card at the map's lower edge, the forecast map's and the rain radar's:
 * the pack's surface with its hairline and the float shadow. What is too
 * much at a large text size scrolls inside the card rather than pushing it
 * over the chips; with its key and notes unfolded the card may take more.
 */
@Composable
internal fun MapCard(
    tag: String,
    isUnfolded: Boolean,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colours = Venti.colours
    val share = if (isUnfolded) OPEN_HEIGHT_SHARE else MAX_HEIGHT_SHARE
    val tallest = with(LocalDensity.current) { (LocalWindowInfo.current.containerSize.height * share).toDp() }
    Surface(
        modifier = modifier.testTag(tag),
        shape = MaterialTheme.shapes.medium,
        color = colours.surface,
        border = BorderStroke(1.dp, colours.border),
        shadowElevation = FloatElevation,
    ) {
        Column(
            modifier = Modifier
                .heightIn(max = tallest)
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s3, end = VentiSpace.s3, top = VentiSpace.s3),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            content = content,
        )
    }
}

/**
 * The layers to choose from, as the pack's chips of which one is on: a row
 * that scrolls sideways across the card where they do not fit.
 */
@Composable
private fun LayerRow(
    layers: List<ThermalMapLayerChoice>,
    chosen: ThermalHoursLayer,
    onLayer: (ThermalHoursLayer) -> Unit,
) {
    val colours = Venti.colours
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .selectableGroup()
            .testTag("forecast-layers"),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (choice in layers) {
            val isOn = choice.layer == chosen
            Box(
                modifier = Modifier
                    .heightIn(min = VentiSize.touch)
                    .selectable(selected = isOn, role = Role.Tab, onClick = { onLayer(choice.layer) }),
                contentAlignment = Alignment.Center,
            ) {
                Surface(
                    shape = PillShape,
                    color = if (isOn) colours.primary else colours.surfaceRaised,
                    contentColor = if (isOn) colours.onPrimary else colours.text,
                ) {
                    Text(
                        choice.name,
                        style = MaterialTheme.typography.labelLarge,
                        maxLines = 1,
                        modifier = Modifier.padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s2),
                    )
                }
            }
        }
    }
}

/**
 * What the map shows now, in the core's words: "Wind at 2000 m, 13:00".
 * With the wind, the menu of its height stands at the right of the words,
 * also while the hours of a height are asked for and there are no words.
 */
@Composable
private fun Said(words: String?, levels: List<ThermalMapLevel>?, heightM: Int?, onLevel: (Int?) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        // Long words at a large text size take a second line beside the menu.
        Text(
            words.orEmpty(),
            style = MaterialTheme.typography.titleSmall,
            color = Venti.colours.text,
            modifier = Modifier.weight(1f).testTag("forecast-said"),
        )
        if (levels != null) {
            LevelMenu(levels, heightM, onLevel)
        }
    }
}

/**
 * "Key and notes": one line that unfolds the key, the model's run and the
 * notes under it, and folds them away again. TalkBack says whether it is
 * expanded or collapsed. Unfolded, what it holds is brought into the
 * card's view, which scrolls inside itself where it is at its height.
 */
@Composable
internal fun KeyAndNotes(
    isOpen: Boolean,
    onChange: (Boolean) -> Unit,
    tag: String = "forecast-key-and-notes",
    content: @Composable () -> Unit,
) {
    val intoView = remember { BringIntoViewRequester() }
    LaunchedEffect(isOpen) {
        if (isOpen) intoView.bringIntoView()
    }
    Column(Modifier.bringIntoViewRequester(intoView)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = VentiSize.touch)
                .testTag(tag)
                .clickable(role = Role.Button, onClick = { onChange(!isOpen) })
                .semantics {
                    stateDescription = if (isOpen) ThermalMapWords.EXPANDED else ThermalMapWords.COLLAPSED
                    if (isOpen) {
                        collapse {
                            onChange(false)
                            true
                        }
                    } else {
                        expand {
                            onChange(true)
                            true
                        }
                    }
                },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
        ) {
            Text(ThermalMapWords.KEY_AND_NOTES, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
            Icon(
                painterResource(R.drawable.ic_arrow_drop_down),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                // The arrow points up while what it unfolded can be folded away.
                modifier = Modifier.rotate(if (isOpen) HALF_TURN else 0f),
            )
        }
        if (isOpen) {
            Column(
                modifier = Modifier.padding(bottom = VentiSpace.s3),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                content()
            }
        }
    }
}

/** The level of the wind: "Ground", "1000 m" … "4000 m", as a menu. */
@Composable
private fun LevelMenu(levels: List<ThermalMapLevel>, heightM: Int?, onLevel: (Int?) -> Unit) {
    var isOpen by remember { mutableStateOf(false) }
    val shown = levels.firstOrNull { it.heightM == heightM } ?: levels.first()
    Box {
        TextButton(
            onClick = { isOpen = true },
            modifier = Modifier.heightIn(min = VentiSize.touch).testTag("forecast-level"),
        ) {
            Text(shown.name, maxLines = 1)
            Icon(painterResource(R.drawable.ic_arrow_drop_down), contentDescription = null)
        }
        DropdownMenu(expanded = isOpen, onDismissRequest = { isOpen = false }) {
            for (level in levels) {
                DropdownMenuItem(
                    text = { Text(level.name) },
                    onClick = {
                        isOpen = false
                        onLevel(level.heightM)
                    },
                    modifier = Modifier.testTag("forecast-level-${level.heightM ?: "ground"}"),
                )
            }
        }
    }
}

/**
 * The line of hours, the pack's hour strip: a cell for each of the day's
 * hours with the clock's hour in it ("13"), the chosen cell marked; on
 * "Thermals" a first cell "Day". The words under the strip say the hour.
 */
@Composable
private fun HourStrip(view: ThermalMapHoursView, withDay: Boolean, chosen: Int?, onChoose: (Int?) -> Unit) {
    // The cells: "Day" first on "Thermals", then each hour, by its place in the day's hours.
    val cells: List<Int?> = (if (withDay) listOf(null) else emptyList<Int?>()) + view.hours.indices.map { it }
    Strip(
        written = cells.map { hour -> hour?.let { view.hours[it].clockHour.toString() } ?: ThermalMapWords.DAY },
        spoken = { place -> cells[place]?.let { view.hours[it].label } ?: ThermalMapWords.DAY },
        at = cells.indexOf(chosen).coerceAtLeast(0),
        onChoose = { place -> onChoose(cells[place]) },
        name = ThermalMapWords.HOUR,
        tag = "forecast-hours",
    )
}

/**
 * The pack's hour strip (`.v-hours`), the forecast map's hours and the
 * rain radar's frames: a cell for each with a few characters in it, the
 * chosen cell marked. A tap chooses a cell, a finger drawn along the strip
 * chooses cell by cell. TalkBack reads it as one value to adjust, which
 * says the cell in words. Where the cells are narrower than two digits, at
 * a narrow width, a large text size or with many cells, only every second
 * or third cell is written, counted from the first or, `fromTheEnd`, from
 * the last: none is cut.
 */
@Composable
internal fun Strip(
    /** What is written in each cell: two digits, or a short word in the first. */
    written: List<String>,
    /** A cell in words, for TalkBack. */
    spoken: (Int) -> String,
    /** The chosen cell's place. */
    at: Int,
    onChoose: (Int) -> Unit,
    /** What TalkBack calls the strip. */
    name: String,
    tag: String,
    modifier: Modifier = Modifier,
    fromTheEnd: Boolean = false,
) {
    val colours = Venti.colours
    if (written.isEmpty()) return
    val last = written.lastIndex
    val choose by rememberUpdatedState { place: Int -> onChoose(place.coerceIn(0, last)) }
    val style = MaterialTheme.typography.labelMedium.numbers
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val gap = VentiSpace.s1 / 2
    // What two digits need, with air beside them.
    val twoDigits = remember(style, density) {
        measurer.measure("00", style).size.width + with(density) { VentiSpace.s1.roundToPx() }
    }
    BoxWithConstraints(modifier.fillMaxWidth()) {
        val gapPx = with(density) { gap.roundToPx() }
        val cell = ((constraints.maxWidth - gapPx * last) / written.size).coerceAtLeast(1)
        // Every cell, every other, every third: as many as two digits are wide.
        val every = (twoDigits + gapPx + cell + gapPx - 1) / (cell + gapPx)
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag(tag)
                .clearAndSetSemantics {
                    contentDescription = name
                    stateDescription = spoken(at.coerceIn(0, last))
                    progressBarRangeInfo = ProgressBarRangeInfo(at.toFloat(), 0f..last.toFloat(), (written.size - 2).coerceAtLeast(0))
                    setProgress { value ->
                        choose(kotlin.math.round(value).toInt())
                        true
                    }
                }
                .pointerInput(written.size) {
                    fun place(x: Float) = (x / size.width * written.size).toInt().coerceIn(0, last)
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        choose(place(down.position.x))
                        drag(down.id) { change ->
                            choose(place(change.position.x))
                            change.consume()
                        }
                    }
                },
            horizontalArrangement = Arrangement.spacedBy(gap),
        ) {
            written.forEachIndexed { place, words ->
                val isChosen = place == at
                val isWritten = (if (fromTheEnd) last - place else place) % every == 0
                Column(
                    modifier = Modifier.weight(1f).heightIn(min = VentiSize.touch),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s1, Alignment.CenterVertically),
                ) {
                    Text(
                        // Every cell keeps its line, written or not, so the bars stand in one row.
                        if (isWritten) words else "",
                        style = style,
                        color = if (isChosen) colours.text else colours.textSecondary,
                        maxLines = 1,
                        softWrap = false,
                        // "Day" is wider than a narrow cell: it is written whole, over the edge.
                        modifier = Modifier.wrapContentWidth(unbounded = true),
                    )
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .height(VentiSpace.s2)
                            .background(if (isChosen) colours.primary else colours.surfaceRaised),
                    )
                }
            }
        }
    }
}

/** The key of a layer of hours: the core's lines, each with its colour on the map. */
@Composable
private fun HourKey(legend: List<ThermalMapKey>) {
    KeyLines(legend.map { it.words to ThermalHoursLayers.colour(it) }, "forecast-key")
}

/**
 * A key: lines of the core's words, each with a square in its colour on
 * the map, with a hairline edge since the palest is close to the surface.
 * It wraps at a narrow width or a large text size, and TalkBack reads it
 * as a list.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun KeyLines(lines: List<Pair<String, Color>>, tag: String) {
    FlowRow(
        modifier = Modifier
            .fillMaxWidth()
            .testTag(tag)
            .semantics { collectionInfo = CollectionInfo(rowCount = lines.size, columnCount = 1) },
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
    ) {
        lines.forEachIndexed { index, (words, colour) ->
            Row(
                modifier = Modifier.clearAndSetSemantics {
                    contentDescription = words
                    collectionItemInfo = CollectionItemInfo(rowIndex = index, rowSpan = 1, columnIndex = 0, columnSpan = 1)
                },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Box(
                    Modifier
                        .size(VentiSpace.s3)
                        .background(colour, RectangleShape)
                        .border(1.dp, Venti.colours.borderStrong, RectangleShape),
                )
                Text(words, style = MaterialTheme.typography.labelMedium, color = Venti.colours.textSecondary)
            }
        }
    }
}

/** At most this share of the screen's height is the card's. */
private const val MAX_HEIGHT_SHARE = 0.4f

/**
 * And this share while "Key and notes" is unfolded: the pilot asked to read
 * them. No more, or at 360 dp with large text the card reaches the map's
 * chips; what does not fit scrolls inside the card.
 */
private const val OPEN_HEIGHT_SHARE = 0.5f

/** The arrow of "Key and notes" turned to point up, degrees. */
private const val HALF_TURN = 180f

/** Why there is no map to show, and the button that asks again. */
@Composable
internal fun Problem(message: String, onTryAgain: () -> Unit, modifier: Modifier = Modifier, tag: String = "forecast-map-problem") {
    Column(modifier.testTag(tag)) {
        Text(message, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.text)
        TextButton(onClick = onTryAgain, modifier = Modifier.heightIn(min = VentiSize.touch)) {
            Text(ThermalMapWords.TRY_AGAIN)
        }
    }
}

/**
 * The key, the core's: a swatch in the colour the map fills a square with
 * and the words for it, from the best kind of day down, then "Not flyable"
 * with the veil's swatch, its words like the others'. It wraps at a narrow width or a large text size,
 * and TalkBack reads it as a list.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Key(legend: List<ThermalLegendEntry>) {
    FlowRow(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("forecast-key")
            .semantics { collectionInfo = CollectionInfo(rowCount = legend.size, columnCount = 1) },
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
    ) {
        legend.forEachIndexed { index, entry ->
            val fill = entry.scaleStep?.toInt() ?: ThermalMapLayer.NOT_FLYABLE
            Row(
                modifier = Modifier.clearAndSetSemantics {
                    contentDescription = entry.words
                    collectionItemInfo = CollectionItemInfo(rowIndex = index, rowSpan = 1, columnIndex = 0, columnSpan = 1)
                },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Swatch(fill)
                Text(
                    entry.words,
                    style = MaterialTheme.typography.labelMedium,
                    color = Venti.colours.textSecondary,
                )
            }
        }
    }
}

/** A square in the map's colour, with a hairline edge: the palest step is close to the surface. */
@Composable
private fun Swatch(fill: Int) {
    // Square, as what it stands for is.
    Box(
        Modifier
            .size(VentiSpace.s3)
            .background(ThermalMapLayer.colour(fill), RectangleShape)
            .border(1.dp, Venti.colours.borderStrong, RectangleShape),
    )
}

/**
 * A square of the forecast map, tapped: the pack's sheet with the day's
 * name, the status as icon, word and colour with what speaks against the
 * day beside it, the mark with the kind of day
 * and the thermals' line, the height the square was judged at, and what
 * the map is, with its credits. Every word but the height's is the core's.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThermalSquareSheet(
    map: ThermalMap,
    day: ThermalMapDay,
    square: ThermalMapCell,
    /** Where the forecast for the square's place is asked for. */
    wind: WindModel,
    onDismiss: () -> Unit,
) {
    val colours = Venti.colours
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = false)
    // The forecast for the square's place, opened in the same sheet with an arrow back.
    var showsForecast by remember(square) { mutableStateOf(false) }
    // A forecast is a table of a whole day: it takes all the room there is.
    LaunchedEffect(showsForecast) {
        if (showsForecast) sheet.expand()
    }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheet,
        containerColor = MaterialTheme.colorScheme.surface,
        modifier = Modifier.semantics { paneTitle = "Forecast for this square" },
    ) {
        // Taller than half the screen, so that the sheet opens at half
        // height with the square still in view above it.
        Box(Modifier.fillMaxHeight()) {
            if (showsForecast) {
                ForecastPage(
                    ThermalMapWords.place(square),
                    wind,
                    onBack = { showsForecast = false },
                    onDone = onDismiss,
                    // On the day the map shows, from the model it is read from.
                    opensOn = ForecastOpening(day.date, day.model),
                )
                return@Box
            }
            Column(Modifier.testTag("forecast-square")) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                ) {
                    Text(
                        day.name,
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                    TextButton(onClick = onDismiss) { Text("Done") }
                }
                // The status, and what speaks against the day beside it,
                // read by TalkBack as one: "Marginal, Gusts".
                Row(
                    modifier = Modifier
                        .padding(start = VentiSpace.s5, end = VentiSpace.s5)
                        .testTag("square-status")
                        .clearAndSetSemantics { contentDescription = ThermalMapWords.status(square) },
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                ) {
                    StatusChip(square.verdict.status, square.status)
                    if (square.why.isNotEmpty()) {
                        Text(
                            square.why,
                            style = MaterialTheme.typography.bodyMedium,
                            color = colours.textSecondary,
                            modifier = Modifier.testTag("square-why"),
                        )
                    }
                }
                Column(
                    modifier = Modifier
                        .verticalScroll(rememberScrollState())
                        .padding(start = VentiSpace.s5, end = VentiSpace.s5, top = VentiSpace.s2, bottom = VentiSpace.s6),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                ) {
                    if (square.thermalWords.isNotEmpty()) {
                        ThermalLine(square.thermalWords, square.thermalLine, ThermalMapLayer.colour(ThermalMapLayer.fill(square)))
                    }
                    square.groundM?.let { ground ->
                        Text(
                            ThermalMapWords.judgedAt(ground),
                            style = MaterialTheme.typography.bodyMedium,
                            color = colours.text,
                            modifier = Modifier.testTag("square-ground"),
                        )
                    }
                    // The forecast for the middle of the square, as a site's page has it.
                    Group {
                        row {
                            LinkRow(
                                "Forecast for this place",
                                R.drawable.ic_chevron_right,
                                Modifier.testTag("forecast-square-forecast"),
                                leading = R.drawable.venti_forecast,
                                onClick = { showsForecast = true },
                            )
                        }
                    }
                    if (day.note.isNotEmpty()) {
                        Text(day.note, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                    }
                    Text(
                        map.note,
                        style = MaterialTheme.typography.bodyMedium,
                        color = colours.textSecondary,
                        modifier = Modifier.testTag("square-note"),
                    )
                    Text(ThermalMapWords.run(day), style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                    for (credit in map.credits) {
                        Text(
                            "${credit.text}. ${credit.licence}.",
                            style = MaterialTheme.typography.bodySmall,
                            color = colours.textSecondary,
                        )
                    }
                }
            }
        }
    }
}

/**
 * A square of the forecast map by the hour, tapped: the square's days side
 * by side (a tap on another changes the map's day), the hour and the layer
 * in the core's words, the status on "Thermals", the arrow on "Wind" and
 * "Gusts", the ground, the forecast for the place, the notes and the
 * credits. The square is read again from the model as the day or the hour
 * changes.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThermalHourSheet(
    model: ThermalMapModel,
    place: Coordinate,
    /** The square drawn under the finger, if one was; else the core finds it from the place. */
    cell: UInt?,
    wind: WindModel,
    onDismiss: () -> Unit,
) {
    val colours = Venti.colours
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = false)
    var showsForecast by remember(place) { mutableStateOf(false) }
    LaunchedEffect(showsForecast) {
        if (showsForecast) sheet.expand()
    }
    var strip by remember(place) { mutableStateOf<List<ThermalStripDay>>(emptyList()) }
    LaunchedEffect(place, model.map) {
        strip = model.strip(place.lat, place.lon)
    }
    val hours = model.hours
    val square = hours.square(cell, place.lat, place.lon)
    val view = (hours.state as? ThermalHoursState.There)?.view
    val choice = model.map?.layers?.firstOrNull { it.layer == hours.layer }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheet,
        containerColor = MaterialTheme.colorScheme.surface,
        modifier = Modifier.semantics { paneTitle = "Forecast for this square" },
    ) {
        Box(Modifier.fillMaxHeight()) {
            if (showsForecast && square != null) {
                ForecastPage(
                    ForecastPlace(
                        fly.venti.app.design.Format.place((square.south + square.north) / 2, (square.west + square.east) / 2),
                        Coordinate(lat = (square.south + square.north) / 2, lon = (square.west + square.east) / 2),
                    ),
                    wind,
                    onBack = { showsForecast = false },
                    onDone = onDismiss,
                    opensOn = model.day?.let { ForecastOpening(it.date, it.model) },
                )
                return@Box
            }
            Column(Modifier.testTag("forecast-hour-square")) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        square?.hour ?: model.day?.name.orEmpty(),
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                    TextButton(onClick = onDismiss) { Text("Done") }
                }
                Column(
                    modifier = Modifier
                        .verticalScroll(rememberScrollState())
                        .padding(start = VentiSpace.s5, end = VentiSpace.s5, top = VentiSpace.s2, bottom = VentiSpace.s6),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                ) {
                    if (strip.isNotEmpty()) {
                        DayStrip(strip, model.day?.date, onChoose = model::choose)
                    }
                    if (square != null) {
                        Column(
                            Modifier.testTag("square-hour").clearAndSetSemantics {
                                contentDescription = square.title
                                stateDescription = square.detail
                            },
                            verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                                square.thermals?.let { StatusChip(it.verdict.status, it.status) }
                                square.wind?.let { WindGlyph(it) }
                                Text(square.title, style = MaterialTheme.typography.titleMedium, color = colours.text)
                            }
                            if (square.detail.isNotEmpty()) {
                                Text(square.detail, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                            }
                        }
                        square.groundM?.let { ground ->
                            Text(
                                ThermalMapWords.judgedAt(ground),
                                style = MaterialTheme.typography.bodyMedium,
                                color = colours.text,
                                modifier = Modifier.testTag("square-ground"),
                            )
                        }
                        Group {
                            row {
                                LinkRow(
                                    "Forecast for this place",
                                    R.drawable.ic_chevron_right,
                                    Modifier.testTag("forecast-square-forecast"),
                                    leading = R.drawable.venti_forecast,
                                    onClick = { showsForecast = true },
                                )
                            }
                        }
                    }
                    choice?.note?.takeIf { it.isNotEmpty() }?.let {
                        Text(it, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                    }
                    if (view != null && !view.whole && view.note.isNotEmpty()) {
                        Text(view.note, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                    }
                    view?.let {
                        Text(
                            "${it.model}, run of ${fly.venti.app.design.Format.dayAndTime(it.runUnixS)}",
                            style = MaterialTheme.typography.bodySmall,
                            color = colours.textSecondary,
                        )
                    }
                    for (credit in view?.credits ?: model.map?.credits.orEmpty()) {
                        Text("${credit.text}. ${credit.licence}.", style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                    }
                }
            }
        }
    }
}

/**
 * A square's days side by side: each day's name and, under it, the
 * square's mark in its step and its status, as Today's rows have them.
 * The day the map shows is marked; a tap on another shows that day.
 */
@Composable
private fun DayStrip(days: List<ThermalStripDay>, chosen: String?, onChoose: (String) -> Unit) {
    val colours = Venti.colours
    Row(
        modifier = Modifier.fillMaxWidth().selectableGroup().testTag("square-days"),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1),
    ) {
        for (day in days) {
            val isChosen = day.date == chosen
            val square = day.square
            Column(
                modifier = Modifier
                    .weight(1f)
                    .background(if (isChosen) colours.surfaceRaised else colours.surface, MaterialTheme.shapes.small)
                    .selectable(selected = isChosen, role = Role.Tab, onClick = { onChoose(day.date) })
                    .heightIn(min = VentiSize.touch)
                    .padding(VentiSpace.s1)
                    .semantics(mergeDescendants = true) {},
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Text(
                    day.name,
                    style = MaterialTheme.typography.labelMedium,
                    color = if (isChosen) colours.text else colours.textSecondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (square != null) {
                    Box(
                        Modifier
                            .size(VentiSpace.s3)
                            .background(ThermalMapLayer.colour(ThermalMapLayer.fill(square)), RectangleShape)
                            .border(1.dp, colours.borderStrong, RectangleShape),
                    )
                    Text(
                        square.status,
                        style = MaterialTheme.typography.labelSmall,
                        color = colours.textSecondary,
                        maxLines = 2,
                        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    )
                }
            }
        }
    }
}
