package fly.venti.app.fly

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.app.task.TaskTurnpointRow
import fly.venti.core.TaskPhase
import fly.venti.core.TaskStatusView
import fly.venti.core.TaskView
import kotlin.math.abs
import fly.venti.app.design.Common

/**
 * The line in the cockpit that says where the flight stands in its task:
 * the countdown to the start, then the clock, and all the while how far it
 * still is to goal. The turnpoint to fly to is in the line below it, like
 * any other place. A tap shows the whole task.
 */
@Composable
fun TaskStrip(status: TaskStatusView, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("task-strip")
            .clickable(role = Role.Button, onClickLabel = "Show the turnpoints", onClick = onOpen)
            .padding(horizontal = 16.dp, vertical = 9.dp)
            // One element for a screen reader: what it is, then where the flight stands.
            .clearAndSetSemantics {
                contentDescription = "Task"
                stateDescription = TaskStripWords.spoken(status)
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(R.drawable.ic_sports_score), contentDescription = null, modifier = Modifier.size(28.dp))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                TaskStripWords.headline(status),
                style = MaterialTheme.typography.titleMedium.numbers,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                TaskStripWords.detail(status),
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = quiet,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (status.phase != TaskPhase.IN_GOAL) {
            Spacer(Modifier.width(8.dp))
            Column(horizontalAlignment = Alignment.End) {
                Text(
                    "${Format.kilometres(status.toGoalM)} km",
                    style = MaterialTheme.typography.titleMedium.numbers,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                )
                Text(
                    TaskStripWords.goalDetail(status),
                    style = MaterialTheme.typography.bodySmall.numbers,
                    color = quiet,
                    maxLines = 1,
                )
            }
        }
        Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = quiet)
    }
}

/** What the task's line in the cockpit says. Which phase a flight is in, and every number, is the core's. */
object TaskStripWords {
    /** Where the flight stands: "Start in 4:32", "On task 0:41:12". */
    fun headline(status: TaskStatusView): String = when (status.phase) {
        TaskPhase.BEFORE_START -> {
            val opens = status.gate?.opensInS
            if (opens != null && opens > 0) "Start in ${countdown(opens)}" else "Start open"
        }
        TaskPhase.ON_TASK -> status.clockS?.let { "On task ${Format.stopwatch(maxOf(it, 0))}" } ?: "On task"
        TaskPhase.SPEED_SECTION_DONE ->
            status.clockS?.let { "Speed section ${Format.stopwatch(maxOf(it, 0))}" } ?: "Speed section done"
        TaskPhase.IN_GOAL -> "In goal"
    }

    /** What goes with it: the edge of the start and the speed it asks for, or which turnpoint is next. */
    fun detail(status: TaskStatusView): String = when (status.phase) {
        TaskPhase.BEFORE_START -> {
            val gate = status.gate
            if (gate == null) {
                "Cross the start's edge"
            } else {
                val speed = gate.speedToGateMs
                val asks = when {
                    speed == null -> "cross it to start"
                    // Faster than a glider flies means: too far away to be
                    // there when the gate opens.
                    speed > 25 -> "too far for the gate"
                    else -> "needs ${Format.kilometresPerHour(speed)} km/h"
                }
                "Edge ${Format.kilometres(gate.distanceM)} km · $asks"
            }
        }
        TaskPhase.ON_TASK, TaskPhase.SPEED_SECTION_DONE -> {
            val parts = mutableListOf<String>()
            status.next?.let { parts += "${it.number} of ${status.reached.size}" }
            // The end of the speed section, where it is not the goal: the
            // distance to goal is on the right already.
            status.toEndOfSpeedM?.let { left ->
                if (abs(left - status.toGoalM) > 50) parts += "ESS in ${Format.kilometres(left)} km"
            }
            if (parts.isEmpty()) "On the way to goal" else parts.joinToString(" · ")
        }
        TaskPhase.IN_GOAL ->
            status.clockS?.let { "Speed section ${Format.stopwatch(maxOf(it, 0))}" } ?: "The task is flown"
    }

    /** Under the distance to goal: the glide the rest of the task needs. */
    fun goalDetail(status: TaskStatusView): String =
        status.glideToGoal?.let { "to goal · needs ${Format.glideRatio(it)}" } ?: "to goal"

    /** Minutes and seconds, or hours too when it is long. */
    fun countdown(seconds: Long): String =
        if (seconds >= 3600) Format.stopwatch(seconds) else Format.minutesSeconds(seconds)

    /** The whole line as a screen reader says it. */
    fun spoken(status: TaskStatusView): String {
        val line = "${headline(status)}, ${detail(status)}"
        return if (status.phase == TaskPhase.IN_GOAL) line else "$line, ${Format.kilometres(status.toGoalM)} kilometres to goal"
    }
}

/**
 * The task during a flight: every turnpoint with whether it is reached, and
 * the two buttons a pilot needs when the task and the flight have come
 * apart.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TaskProgressSheet(
    task: TaskView?,
    status: TaskStatusView?,
    /** Moves the task on to its next turnpoint, or back to the one before. */
    onStep: (forward: Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Task", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text(Common.words.done) }
        }
        if (task == null || status == null) {
            Text(
                "No task is being flown.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 24.dp),
            )
            return@ModalBottomSheet
        }
        LazyColumn(
            modifier = Modifier.testTag("task-progress"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp),
        ) {
            item {
                SectionHeader(TaskStripWords.headline(status), Modifier.testTag("task-progress-headline"))
                Group {
                    for (turnpoint in task.turnpoints) {
                        val index = turnpoint.number.toInt() - 1
                        row {
                            TaskTurnpointRow(
                                turnpoint,
                                isReached = status.reached.getOrNull(index) ?: false,
                                isNext = status.next?.number == turnpoint.number,
                            )
                        }
                    }
                }
                SectionFooter(
                    "A turnpoint counts when you cross the edge of its cylinder, from either side. The app " +
                        "counts it only once a position lies beyond the edge: scoring allows a little more, and " +
                        "you should not need it.",
                )
            }
            status.gate?.let { gate ->
                item {
                    SectionHeader("Start")
                    Group {
                        gate.gateUnixS?.let { time ->
                            val opens = gate.opensInS
                            row { ValueRow(if (opens != null && opens > 0) "Gate opens" else "Gate opened", Format.time(time)) }
                        }
                        gate.nextGateUnixS?.let { next -> row { ValueRow("Next gate", Format.time(next)) } }
                        row { ValueRow("Edge of the start", "${Format.kilometres(gate.distanceM)} km") }
                        gate.arrivesInS?.let { arrives ->
                            row { ValueRow("At this speed", "there in ${TaskStripWords.countdown(arrives.toLong())}") }
                        }
                    }
                }
            }
            item {
                Spacer(Modifier.size(24.dp))
                Group {
                    row {
                        ActionRow(
                            "Skip to the next turnpoint",
                            MaterialTheme.colorScheme.primary,
                            Modifier.testTag("task-skip"),
                            enabled = status.next != null,
                        ) {
                            onStep(true)
                        }
                    }
                    row {
                        ActionRow("Back to the one before", MaterialTheme.colorScheme.primary, Modifier.testTag("task-back")) {
                            onStep(false)
                        }
                    }
                }
                SectionFooter(
                    "Stepping back to the start forgets the start you took: for going back to take a later gate.",
                )
            }
        }
    }
}
