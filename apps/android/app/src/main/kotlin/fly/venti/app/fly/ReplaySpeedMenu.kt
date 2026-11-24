package fly.venti.app.fly

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import fly.venti.app.R

/** What the cockpit calls the speeds of a replay. */
object ReplayWords {
    /**
     * The speeds a replay offers in the cockpit: held, the flight's own,
     * and two for getting to the part worth watching.
     */
    val SPEEDS: List<Double> = listOf(0.0, 1.0, 10.0, 20.0)

    /** A speed as the menu offers it: "Paused", "In real time", "10 times faster". */
    fun choice(speed: Double): String = when (speed) {
        0.0 -> "Paused"
        1.0 -> "In real time"
        else -> "${speed.toInt()} times faster"
    }

    /** What the line at the top says: "Replay paused", "Replay", "Replay ×20". */
    fun words(speed: Double): String = when (speed) {
        0.0 -> "Replay paused"
        1.0 -> "Replay"
        else -> "Replay ×${speed.toInt()}"
    }
}

/**
 * The word "Replay" at the top of the cockpit, as a menu: a replay is
 * watched, not flown, so it can be held at a moment and run slower or
 * faster from here.
 */
@Composable
fun ReplaySpeedMenu(speed: Double, onChoose: (Double) -> Unit, modifier: Modifier = Modifier) {
    var isOpen by remember { mutableStateOf(false) }
    val words = ReplayWords.words(speed)
    Box(modifier) {
        // A surface that can be pressed, with a touch target taller than
        // the line it stands in: Material gives it 48 dp.
        Surface(
            onClick = { isOpen = true },
            shape = CircleShape,
            color = Color.Transparent,
            modifier = Modifier
                .heightIn(min = 32.dp)
                .testTag("replay-speed")
                .clearAndSetSemantics {
                    contentDescription = "Replay speed"
                    stateDescription = words
                    role = Role.DropdownList
                    onClick(label = "Change the speed") {
                        isOpen = true
                        true
                    }
                },
        ) {
            Row(Modifier.padding(horizontal = 6.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(if (speed == 0.0) R.drawable.ic_pause else R.drawable.ic_play_arrow),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
                Spacer(Modifier.width(4.dp))
                Text(words, style = MaterialTheme.typography.labelLarge, maxLines = 1)
                Icon(
                    painterResource(R.drawable.ic_arrow_drop_down),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
        DropdownMenu(expanded = isOpen, onDismissRequest = { isOpen = false }) {
            for (choice in ReplayWords.SPEEDS) {
                DropdownMenuItem(
                    text = { Text(ReplayWords.choice(choice)) },
                    onClick = {
                        isOpen = false
                        onChoose(choice)
                    },
                    // The speed it runs at now carries a tick.
                    trailingIcon = if (choice == speed) {
                        { Icon(painterResource(R.drawable.ic_check), contentDescription = "Chosen") }
                    } else {
                        null
                    },
                )
            }
        }
    }
}
