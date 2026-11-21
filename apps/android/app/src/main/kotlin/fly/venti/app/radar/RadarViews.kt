package fly.venti.app.radar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Venti
import fly.venti.app.forecast.KeyAndNotes
import fly.venti.app.forecast.KeyLines
import fly.venti.app.forecast.MapCard
import fly.venti.app.forecast.Problem
import fly.venti.app.forecast.Strip
import fly.venti.core.RadarView

/**
 * The card at the map's lower edge while the rain radar shows, in the
 * manner of the forecast map's: one line of time for all the radar's
 * layers as the pack's hour strip, a cell a moment with the minutes of its
 * time, the newest at the right and chosen when the card opens, and a
 * button that plays them through; under it the line of the moment on the
 * map, "Rain radar, 14:15, 4 minutes ago. Austria's analysis is of 14:00.",
 * or, when the newest is too old to be the rain of now, the core's
 * sentence in the caution manner. What each layer is, the key, the line
 * for what was not seen, the note and the credits of the layers drawn
 * stand behind "Key and notes". While the list is asked for it says so;
 * when there is none, why, with "Try again". Every word about the rain is
 * the core's.
 */
@Composable
fun RadarCard(
    radar: RadarModel,
    onChoose: (Int) -> Unit,
    onPlay: () -> Unit,
    onTryAgain: () -> Unit,
    onTryAgainPicture: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colours = Venti.colours
    var showsKey by remember { mutableStateOf(false) }
    MapCard("radar-card", isUnfolded = showsKey, modifier = modifier) {
        when (val state = radar.state) {
            RadarState.Asking -> Row(
                modifier = Modifier.padding(bottom = VentiSpace.s3).testTag("radar-asking"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                CircularProgressIndicator(Modifier.size(VentiSpace.s4), strokeWidth = 2.dp)
                Text(RadarWords.ASKING, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
            }
            is RadarState.Failed -> Problem(state.message, onTryAgain, tag = "radar-problem")
            is RadarState.There -> Column {
                val moments = state.view.moments
                if (moments.isNotEmpty()) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                        Strip(
                            written = moments.map(RadarWords::minutes),
                            spoken = { place -> RadarWords.spoken(moments[place]) },
                            at = radar.place.coerceIn(0, moments.lastIndex),
                            onChoose = onChoose,
                            name = RadarWords.FRAME,
                            tag = "radar-frames",
                            modifier = Modifier.weight(1f),
                            // The newest moment, at the right, is always written.
                            fromTheEnd = true,
                        )
                        if (moments.size > 1) {
                            IconButton(onClick = onPlay, modifier = Modifier.size(VentiSize.touch).testTag("radar-play")) {
                                Icon(
                                    painterResource(if (radar.isPlaying) R.drawable.ic_pause else R.drawable.ic_play_arrow),
                                    contentDescription = if (radar.isPlaying) RadarWords.STOP else RadarWords.PLAY,
                                    tint = colours.text,
                                )
                            }
                        }
                    }
                }
                Said(radar)
                radar.pictureProblem?.let { problem ->
                    Problem(problem, onTryAgainPicture, Modifier.padding(top = VentiSpace.s2), tag = "radar-picture-problem")
                }
                KeyAndNotes(showsKey, onChange = { showsKey = it }, tag = "radar-key-and-notes") {
                    // What the pictures are: a line for each layer, in the core's words.
                    Column(Modifier.testTag("radar-layers"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                        for (layer in state.view.layers) {
                            Text(layer.what, style = MaterialTheme.typography.bodySmall, color = colours.text)
                        }
                    }
                    Key(state.view)
                    Text(
                        state.view.note,
                        style = MaterialTheme.typography.bodySmall,
                        color = colours.textSecondary,
                        modifier = Modifier.testTag("radar-note"),
                    )
                    // Whose rain it is: the credit of each layer drawn.
                    Column(Modifier.testTag("radar-credit"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                        for (credit in radar.drawn.mapNotNull { it.credit }) {
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
}

/**
 * What the map shows now: the line of the moment whose pictures are on the
 * map, which is the chosen moment's once its pictures are there. A radar
 * picture without its time is read as now, so the line is always the
 * pictures' own. While the newest frame of the topmost layer that has
 * frames is too old, or there is none, the core's sentence stands in its
 * place, in the status colour for caution with its sign: never the colour
 * alone.
 */
@Composable
private fun Said(radar: RadarModel) {
    val colours = Venti.colours
    val caution = radar.caution
    if (caution != null) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(colours.cautionSoft, RoundedCornerShape(VentiRadius.s))
                .padding(horizontal = VentiSpace.s2, vertical = VentiSpace.s1)
                .testTag("radar-stale"),
            horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painterResource(R.drawable.venti_caution),
                contentDescription = null,
                tint = colours.caution,
                modifier = Modifier.size(VentiSize.icon - VentiSpace.s1),
            )
            Text(caution, style = MaterialTheme.typography.bodyMedium, color = colours.caution)
        }
        return
    }
    radar.shownMoment?.let { moment ->
        Text(
            moment.label,
            style = MaterialTheme.typography.titleSmall,
            color = colours.text,
            modifier = Modifier.fillMaxWidth().testTag("radar-said"),
        )
    }
}

/**
 * The key, the core's, from the most rain down: each step's words with a
 * square in the colour the pictures have it in, and last the line for what
 * no layer saw, with the veil's square.
 */
@Composable
private fun Key(view: RadarView) {
    KeyLines(
        view.key.map { it.words to RadarLayer.colour(it) } + (view.unseenWords to RadarLayer.UNSEEN),
        "radar-key",
    )
}
