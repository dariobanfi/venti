package fly.venti.app.map

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.LayerChip
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.forecast.KeyLines
import fly.venti.app.forecast.LevelMenu
import fly.venti.app.forecast.MapCard
import fly.venti.app.forecast.Problem
import fly.venti.app.forecast.Strip
import fly.venti.app.forecast.ThermalHoursLayers
import fly.venti.app.forecast.ThermalHoursState
import fly.venti.app.forecast.ThermalMapLayer
import fly.venti.app.forecast.ThermalMapModel
import fly.venti.app.forecast.ThermalMapWords
import fly.venti.app.radar.RadarLayer
import fly.venti.app.radar.RadarModel
import fly.venti.app.radar.RadarWords
import fly.venti.core.Bounds
import fly.venti.core.ThermalHoursLayer
import fly.venti.core.ThermalMapScale

/**
 * The one small panel at the map's foot for what it shows (the owner,
 * 2026-11-22: "we don't need all of this text, just a legend"; owner
 * decision 19), as the web's `WhatPanel.svelte`: one line that says what is
 * shown and when ("Rain, 14:15, measured"; "Wind at the ground, 13:00,
 * forecast, ICON-D2"), the one line of time from what was measured to what
 * is forecast, and the key as a compact strip. The wind's height and its
 * gusts are two small controls of the wind's forecast. The whole key and
 * the notes stand behind the "i", folded until the pilot asks; the credits
 * are the map's, behind its own (i).
 *
 * What the pilot has to be told about the data as it stands is never
 * folded away: old rain that is not the rain of now, a part of the map with
 * nothing forecast, and what could not be had, with "Try again".
 */
@Composable
fun WhatPanel(
    what: MapWhat,
    /** What it is called: "Rain", the core's. */
    name: String,
    cells: List<LineCell>,
    /** The cell the line stands on. */
    at: Int,
    onChoose: (Int) -> Unit,
    /** Whether the line stands on what was measured. */
    measured: Boolean,
    radar: RadarModel?,
    thermalMap: ThermalMapModel?,
    /** The wind's gusts in place of its mean, at the ground. */
    gusts: Boolean,
    onGusts: (Boolean) -> Unit,
    onLevel: (Int?) -> Unit,
    /** The part of the world in view, to say when the forecast has nothing there. */
    region: Bounds?,
    onPlay: () -> Unit,
    onTryAgainRadar: () -> Unit,
    onTryAgainPicture: () -> Unit,
    onTryAgainMap: () -> Unit,
    onTryAgainHours: () -> Unit,
    modifier: Modifier = Modifier,
    /** Whether the hour's wind colours the take-offs' fans, which then have their key under the layer's. */
    showsFansKey: Boolean = false,
) {
    val colours = Venti.colours
    val words = MapWords.core
    var showsAll by remember { mutableStateOf(false) }
    val map = thermalMap?.map
    val day = thermalMap?.day
    val hours = thermalMap?.hours
    val view = (hours?.state as? ThermalHoursState.There)?.view
    val hour = hours?.hour
    val choice = map?.layers?.firstOrNull { it.layer == hours?.layer }
    val rain = measured && what == MapWhat.RAIN
    val radarView = radar?.view

    // What is shown and when, in one line.
    val line = when {
        rain -> radar?.shownMoment?.let { moment -> moment.line.ifEmpty { moment.label } }
            ?: RadarWords.ASKING.takeIf { radar?.isAsking == true || radar?.moment != null }
        measured -> "$name, ${words.now.lowercase()}, ${words.measured}"
        view != null && hour != null && view.hours.getOrNull(hour) != null && day != null -> {
            val label = view.hours[hour].label
            val whenShown = if (day.today) label else "${day.name} $label"
            "${view.title}, $whenShown, ${words.forecastWord}, ${view.model}"
        }
        day != null && (what == MapWhat.THERMALS || map?.layers.isNullOrEmpty()) ->
            "$name, ${day.name}, ${words.forecastWord}, ${day.model}"
        hours?.state == ThermalHoursState.Asking -> ThermalMapWords.ASKING_HOURS
        else -> null
    }

    // The key of what is shown, the least first, and the veil apart.
    val key: Key = when {
        rain && radarView != null -> Key(
            steps = radarView.key.reversed().map { Step(it.name.ifEmpty { it.words }, it.words, RadarLayer.colour(it)) },
            veil = Step(radarView.unseenWords, radarView.unseenWords, RadarLayer.UNSEEN),
        )
        measured -> Key(emptyList(), null)
        choice != null && hour != null -> {
            val lines = choice.legend.reversed()
            val veil = lines.firstOrNull { it.scale == ThermalMapScale.VEIL }
            Key(
                steps = lines.filter { it.scale != ThermalMapScale.VEIL }.map { Step(it.words, full(it.words, it.amount), ThermalHoursLayers.colour(it)) },
                veil = veil?.let { Step(it.words, full(it.words, it.amount), ThermalHoursLayers.colour(it)) },
            )
        }
        map != null && what == MapWhat.THERMALS -> {
            val lines = map.legend.reversed()
            val veil = lines.firstOrNull { it.scaleStep == null }
            Key(
                steps = lines.filter { it.scaleStep != null }.map { Step(it.words, it.words, ThermalMapLayer.colour(it.scaleStep?.toInt())) },
                veil = veil?.let { Step(it.words, it.words, ThermalMapLayer.colour(ThermalMapLayer.NOT_FLYABLE)) },
            )
        }
        else -> Key(emptyList(), null)
    }

    MapCard("what-panel", isUnfolded = showsAll, modifier = modifier) {
        Column(Modifier.padding(bottom = VentiSpace.s2), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
            // The line in words, and the round controls at its right.
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                Text(
                    line.orEmpty(),
                    style = MaterialTheme.typography.titleSmall.numbers,
                    color = colours.text,
                    modifier = Modifier.weight(1f).testTag("what-line"),
                )
                if (rain && (radar?.moments?.size ?: 0) > 1) {
                    IconButton(onClick = onPlay, modifier = Modifier.size(VentiSize.touch).testTag("radar-play")) {
                        Icon(
                            painterResource(if (radar?.isPlaying == true) R.drawable.ic_pause else R.drawable.ic_play_arrow),
                            contentDescription = if (radar?.isPlaying == true) RadarWords.STOP else RadarWords.PLAY,
                            tint = colours.textSecondary,
                        )
                    }
                }
                IconButton(
                    onClick = { showsAll = !showsAll },
                    modifier = Modifier
                        .size(VentiSize.touch)
                        .testTag("what-info")
                        .semantics { stateDescription = if (showsAll) ThermalMapWords.EXPANDED else ThermalMapWords.COLLAPSED },
                ) {
                    Icon(
                        painterResource(R.drawable.ic_info),
                        contentDescription = ThermalMapWords.KEY_AND_NOTES,
                        tint = if (showsAll) MaterialTheme.colorScheme.primary else colours.textSecondary,
                    )
                }
            }

            if (cells.isNotEmpty()) {
                Strip(
                    written = cells.map { it.short },
                    spoken = { place -> cells[place].label },
                    at = at.coerceIn(0, cells.lastIndex),
                    onChoose = onChoose,
                    name = TIME,
                    tag = "what-time",
                    over = cells.map { it.over },
                    measured = cells.map { it.measured },
                    startsDay = cells.map { it.startsDay },
                )
            }

            // The wind's forecast: its height, and its gusts in place of its mean.
            val gustsName = map?.layers?.firstOrNull { it.layer == ThermalHoursLayer.GUSTS }?.name
            if (what == MapWhat.WIND && !measured && gustsName != null) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    val levels = map.levels.takeIf { !gusts && choice?.hasLevels == true && it.isNotEmpty() }
                    if (levels != null) LevelMenu(levels, hours?.heightM, onLevel)
                    LayerChip(gustsName, R.drawable.venti_wind, gusts, onChange = onGusts, Modifier.testTag("what-gusts"), floats = false)
                }
            }

            if (key.steps.isNotEmpty()) CompactKey(key)
            if (showsFansKey) FansKey()

            // What the pilot has to be told about the data as it stands: never folded away.
            if (rain) radar?.caution?.let { Stale(it) }
            if (rain && radar?.pictureProblem != null) {
                Problem(checkNotNull(radar.pictureProblem), onTryAgainPicture, tag = "radar-picture-problem")
            }
            if (what == MapWhat.RAIN && radar?.problem != null && !radar.isAsking && radarView == null) {
                Problem(checkNotNull(radar.problem), onTryAgainRadar, tag = "radar-problem")
            }
            if (!measured && thermalMap != null) {
                val problem = thermalMap.problem
                val hoursState = hours?.state
                when {
                    map == null && problem != null && !thermalMap.isAsking -> Problem(problem, onTryAgainMap)
                    hoursState is ThermalHoursState.Failed -> Problem(hoursState.message, onTryAgainHours, tag = "forecast-hours-problem")
                    map == null -> Asking(ThermalMapWords.ASKING, "forecast-map-asking")
                    hoursState == ThermalHoursState.Asking && view == null -> Asking(ThermalMapWords.ASKING_HOURS, "forecast-hours-asking")
                    day != null && region != null && !ThermalMapModel.hasSquaresIn(day, region) ->
                        Text(ThermalMapWords.NONE_HERE, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary, modifier = Modifier.testTag("forecast-none-here"))
                }
            }
            if (what == MapWhat.RAIN && cells.none { it.measured } && radar?.isAsking == true) {
                Asking(RadarWords.ASKING, "radar-asking")
            }

            // Behind the "i": the whole key and the notes, without the project's own caveats.
            if (showsAll) {
                Column(Modifier.padding(top = VentiSpace.s1), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    val lines = key.steps + listOfNotNull(key.veil)
                    if (lines.isNotEmpty()) {
                        KeyLines(lines.reversed().map { it.full to it.colour }, if (rain) "radar-key" else "forecast-key")
                    }
                    for ((note, tag) in notes(what, measured, radar, thermalMap)) {
                        Text(note, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary, modifier = Modifier.testTag(tag))
                    }
                    if (rain) {
                        // Whose rain it is: the credit of each layer drawn.
                        Column(Modifier.testTag("radar-credit"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                            for (credit in radar?.drawn.orEmpty().mapNotNull { it.credit }) {
                                Text("${credit.text}. ${credit.licence}.", style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                            }
                        }
                    }
                }
            }
        }
    }
}

/** A step of a key: its few words beside the strip, its whole words behind the "i", its colour on the map. */
private class Step(val short: String, val full: String, val colour: Color)

private class Key(val steps: List<Step>, val veil: Step?)

/** "Heavy rain, 4 mm an hour and more": a class and its amount, where it has one. */
private fun full(words: String, amount: String): String = if (amount.isEmpty()) words else "$words, $amount"

/**
 * The key as the pack's scale strip (`.v-scale`): the steps side by side,
 * the least first, its two ends named; the veil apart beside it. TalkBack
 * reads it as one, every step's words.
 */
@Composable
private fun CompactKey(key: Key) {
    val colours = Venti.colours
    val spoken = (key.steps + listOfNotNull(key.veil)).joinToString(", ") { it.full }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("what-key")
            .clearAndSetSemantics { contentDescription = spoken },
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1 / 2)) {
                for (step in key.steps) {
                    Box(Modifier.weight(1f).height(VentiSpace.s3).background(step.colour, RectangleShape).border(1.dp, colours.borderStrong, RectangleShape))
                }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text(key.steps.first().short, style = MaterialTheme.typography.labelSmall, color = colours.textSecondary, maxLines = 1)
                if (key.steps.size > 1) {
                    Text(key.steps.last().short, style = MaterialTheme.typography.labelSmall, color = colours.textSecondary, maxLines = 1)
                }
            }
        }
        key.veil?.let { veil ->
            Column(Modifier.width(VentiSize.touch * 2), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                Box(Modifier.width(VentiSpace.s6).height(VentiSpace.s3).background(veil.colour, RectangleShape).border(1.dp, colours.borderStrong, RectangleShape))
                Text(veil.short, style = MaterialTheme.typography.labelSmall, color = colours.textSecondary, maxLines = 2)
            }
        }
    }
}

/** The notes of what is shown, behind the "i", each with a tag for the tests. */
private fun notes(what: MapWhat, measured: Boolean, radar: RadarModel?, thermalMap: ThermalMapModel?): List<Pair<String, String>> {
    val radarView = radar?.view
    if (measured && what == MapWhat.RAIN && radarView != null) {
        return listOfNotNull(
            radar.shownMoment?.label?.let { it to "radar-label" },
            radarView.layers.joinToString("\n") { it.what }.takeIf { it.isNotEmpty() }?.let { it to "radar-layers" },
            withoutCaveats(radarView.note).takeIf { it.isNotEmpty() }?.let { it to "radar-note" },
        )
    }
    if (measured) return emptyList()
    val day = thermalMap?.day ?: return emptyList()
    val hours = thermalMap.hours
    val view = (hours.state as? ThermalHoursState.There)?.view
    val choice = thermalMap.map?.layers?.firstOrNull { it.layer == hours.layer }
    if (view != null && hours.hour != null) {
        return listOfNotNull(
            "${view.model}, run of ${Format.dayAndTime(view.runUnixS)}" to "forecast-map-run",
            choice?.note?.takeIf { it.isNotEmpty() }?.let { it to "forecast-layer-note" },
            day.note.takeIf { it.isNotEmpty() }?.let { it to "forecast-map-day-note" },
            view.note.takeIf { !view.whole && it.isNotEmpty() }?.let { it to "forecast-hours-note" },
        )
    }
    return listOfNotNull(
        ThermalMapWords.run(day) to "forecast-map-run",
        day.note.takeIf { it.isNotEmpty() }?.let { it to "forecast-map-day-note" },
    )
}

/**
 * A text of the core's without the sentences that are the project's own
 * caveat ("… is still to verify …"), which a screen never shows (owner
 * decision 19): those stand in the documents.
 */
internal fun withoutCaveats(text: String): String =
    text.split(Regex("(?<=\\.)\\s+")).filterNot { it.contains("to verify", ignoreCase = true) }.joinToString(" ")

/** The core's sentence when the rain shown is not the rain of now: the caution's sign, word and colour. */
@Composable
private fun Stale(words: String) {
    val colours = Venti.colours
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(colours.cautionSoft, RoundedCornerShape(VentiRadius.s))
            .padding(horizontal = VentiSpace.s2, vertical = VentiSpace.s1)
            .testTag("radar-stale"),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(R.drawable.venti_caution), contentDescription = null, tint = colours.caution, modifier = Modifier.size(VentiSize.icon - VentiSpace.s1))
        Text(words, style = MaterialTheme.typography.bodyMedium, color = colours.caution)
    }
}

/** While something is asked for: the pack's spinner and the words. */
@Composable
private fun Asking(words: String, tag: String) {
    Row(
        modifier = Modifier.heightIn(min = VentiSize.touch).testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        CircularProgressIndicator(Modifier.size(VentiSpace.s4), strokeWidth = 2.dp)
        Text(words, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
    }
}

/** What TalkBack calls the line of time. */
private const val TIME = "Time"
