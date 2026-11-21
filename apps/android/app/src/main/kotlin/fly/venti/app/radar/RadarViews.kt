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
import fly.venti.core.RadarLayerView
import fly.venti.core.RadarView

/**
 * The card at the map's lower edge while the rain radar shows, in the
 * manner of the forecast map's: the frames as the pack's hour strip, a
 * cell a frame with the minutes of its time, the newest at the right and
 * chosen when the card opens, and a button that plays them through; under
 * it the line of the frame on the map, "Rain radar, 12:40, 4 minutes ago",
 * or, when the newest is too old to be the rain of now, the core's
 * sentence in the caution manner. The key, the line for what the radar did
 * not see, the note and the credit stand behind "Key and notes". While the
 * frames are asked for it says so; when there are none, why, with "Try
 * again". Every word about the rain is the core's.
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
                val frames = state.layer.frames
                if (frames.isNotEmpty()) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                        Strip(
                            written = frames.map(RadarWords::minutes),
                            spoken = { place -> RadarWords.spoken(frames[place]) },
                            at = frames.indexOf(radar.frame).coerceAtLeast(0),
                            onChoose = onChoose,
                            name = RadarWords.FRAME,
                            tag = "radar-frames",
                            modifier = Modifier.weight(1f),
                            // The newest frame, at the right, is always written.
                            fromTheEnd = true,
                        )
                        if (frames.size > 1) {
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
                Said(state.layer, radar)
                radar.pictureProblem?.let { problem ->
                    Problem(problem, onTryAgainPicture, Modifier.padding(top = VentiSpace.s2), tag = "radar-picture-problem")
                }
                KeyAndNotes(showsKey, onChange = { showsKey = it }, tag = "radar-key-and-notes") {
                    Key(state.view)
                    Text(
                        state.view.note,
                        style = MaterialTheme.typography.bodySmall,
                        color = colours.textSecondary,
                        modifier = Modifier.testTag("radar-note"),
                    )
                    state.layer.credit?.let { credit ->
                        Text(
                            "${credit.text}. ${credit.licence}.",
                            style = MaterialTheme.typography.bodySmall,
                            color = colours.textSecondary,
                            modifier = Modifier.testTag("radar-credit"),
                        )
                    }
                }
            }
        }
    }
}

/**
 * What the map shows now: the line of the frame whose picture is on the
 * map, which is the chosen frame's once its picture is there. A radar
 * picture without its time is read as now, so the line is always the
 * picture's own. While the newest frame is too old, or there is none, the
 * core's sentence stands in its place, in the status colour for caution
 * with its sign: never the colour alone.
 */
@Composable
private fun Said(layer: RadarLayerView, radar: RadarModel) {
    val colours = Venti.colours
    if (layer.stale) {
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
            Text(layer.staleWords, style = MaterialTheme.typography.bodyMedium, color = colours.caution)
        }
        return
    }
    radar.shownFrame?.let { frame ->
        Text(
            frame.label,
            style = MaterialTheme.typography.titleSmall,
            color = colours.text,
            modifier = Modifier.fillMaxWidth().testTag("radar-said"),
        )
    }
}

/**
 * The key, the core's, from the most rain down: each step's words with a
 * square in the colour the picture has it in, and last the line for what
 * the radar did not see, with the veil's square.
 */
@Composable
private fun Key(view: RadarView) {
    KeyLines(
        view.key.map { it.words to RadarLayer.colour(it) } + (view.unseenWords to RadarLayer.UNSEEN),
        "radar-key",
    )
}
