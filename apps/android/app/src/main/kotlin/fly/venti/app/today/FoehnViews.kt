package fly.venti.app.today

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.R
import fly.venti.app.design.Axis
import fly.venti.app.design.Common
import fly.venti.app.design.Format
import fly.venti.app.design.StatusChip
import fly.venti.app.design.StatusKind
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.design.text
import fly.venti.core.FoehnLevel
import fly.venti.core.FoehnLine
import fly.venti.core.FoehnLimit
import fly.venti.core.FoehnRow
import fly.venti.core.FoehnView
import kotlin.math.abs
import kotlin.math.max

// Foehn on Today: the core's one line, there only when the core gives one,
// and the view behind it. Every word is the core's (`foehn` in the core);
// this only lays them out and draws a pair's last 24 hours.

/** The colour a level of foehn is drawn in: the caution's for "possible", the danger's for "likely", else the text's. */
@Composable
private fun inkOf(level: FoehnLevel) = when (level) {
    FoehnLevel.POSSIBLE -> Venti.colours.caution
    FoehnLevel.LIKELY -> Venti.colours.danger
    FoehnLevel.NONE -> Venti.colours.text
    FoehnLevel.UNKNOWN -> Venti.colours.textSecondary
}

/**
 * The core's line on Today, in the manner of the wind near by: the pack's
 * warning sign and the words in the colour of the level. A tap opens the view.
 */
@Composable
fun FoehnTodayLine(line: FoehnLine, onOpen: () -> Unit) {
    val ink = inkOf(line.level)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("today-foehn")
            .heightIn(min = VentiSize.touch)
            .clickable(role = Role.Button, onClick = onOpen),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(R.drawable.venti_caution),
            contentDescription = null,
            tint = ink,
            modifier = Modifier.size(VentiSpace.s4),
        )
        Text(line.text, style = MaterialTheme.typography.labelLarge, color = ink, modifier = Modifier.weight(1f))
        Icon(
            painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = Venti.colours.textSecondary,
            modifier = Modifier.size(VentiSize.icon),
        )
    }
}

/**
 * The foehn view, over Today: each pair with its words, its level, how old
 * its reading is and its last 24 hours; then what the number means, what it
 * does not say, and whose data it is.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FoehnSheet(state: FoehnState, onTryAgain: () -> Unit, onDismiss: () -> Unit) {
    val view = (state as? FoehnState.Read)?.view
    val colours = Venti.colours
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                .testTag("foehn-sheet")
                // Clear of the navigation bar, so that the last line can be scrolled above it.
                .navigationBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    view?.title ?: TodayWords.FOEHN,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("foehn-close")) {
                    Text(Common.words.done)
                }
            }
            when (state) {
                FoehnState.Idle, FoehnState.Loading -> Box(
                    modifier = Modifier.fillMaxWidth().padding(vertical = VentiSpace.s5),
                    contentAlignment = Alignment.Center,
                ) {
                    CircularProgressIndicator()
                }
                is FoehnState.Failed -> {
                    Text(state.message, color = colours.textSecondary, modifier = Modifier.testTag("foehn-failed"))
                    Button(onClick = onTryAgain, modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch)) {
                        Text(Common.words.tryAgain)
                    }
                }
                is FoehnState.Read -> FoehnBody(state.view)
            }
        }
    }
}

@Composable
private fun FoehnBody(view: FoehnView) {
    val colours = Venti.colours
    if (view.rows.isEmpty()) {
        Text(view.empty, color = colours.textSecondary, modifier = Modifier.testTag("foehn-empty"))
    }
    view.rows.forEachIndexed { index, row ->
        if (index > 0) HorizontalDivider(color = colours.border)
        FoehnPairRow(row, view)
    }
    HorizontalDivider(color = colours.border)
    Text(view.explanation, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.testTag("foehn-explanation"))
    Text(view.caution, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.testTag("foehn-caution"))
    for (credit in view.credits) {
        Text(credit.text, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
    }
}

/** One pair of stations: its name and level, its words, when, its chart, and its note in small print. */
@Composable
private fun FoehnPairRow(row: FoehnRow, view: FoehnView) {
    val colours = Venti.colours
    Column(modifier = Modifier.fillMaxWidth().testTag("foehn-row"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
            Text(
                row.title,
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f).semantics { heading() },
            )
            FoehnBadge(row.level, row.levelLabel)
        }
        Text(row.words, style = MaterialTheme.typography.bodyMedium)
        if (row.`when`.isNotEmpty()) {
            Text(row.`when`, style = MaterialTheme.typography.bodySmall.numbers, color = colours.textSecondary)
        }
        if (row.points.isNotEmpty()) {
            Text(
                "${view.chartTitle}, ${view.unit}",
                style = VentiType.label.text,
                color = colours.textSecondary,
            )
            Text(row.aboveLabel, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
            FoehnChart(row, view)
            Text(row.belowLabel, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
        }
        if (row.note.isNotEmpty()) {
            Text(row.note, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
        }
    }
}

/**
 * The level as a badge: "possible" and "likely" as the pack's status, with
 * its sign; "no sign" plain, never as good; "not known" greyed.
 */
@Composable
private fun FoehnBadge(level: FoehnLevel, label: String) {
    when (level) {
        FoehnLevel.POSSIBLE -> StatusChip(StatusKind.Caution, label, Modifier.testTag("foehn-level"))
        FoehnLevel.LIKELY -> StatusChip(StatusKind.Danger, label, Modifier.testTag("foehn-level"))
        FoehnLevel.NONE, FoehnLevel.UNKNOWN -> Row(
            modifier = Modifier
                .testTag("foehn-level")
                .clearAndSetSemantics { contentDescription = label }
                .background(Venti.colours.surfaceRaised, RoundedCornerShape(VentiRadius.s))
                .heightIn(min = VentiSize.icon + VentiSpace.s1)
                .padding(horizontal = VentiSpace.s3),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                label,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
                color = inkOf(level),
                maxLines = 1,
            )
        }
    }
}

/**
 * A pair's last 24 hours: south less north in hPa, zero in the middle, the
 * core's limits as thin lines with their words at the end, each point in the
 * colour of its level, the hours below.
 */
@Composable
private fun FoehnChart(row: FoehnRow, view: FoehnView) {
    val colours = Venti.colours
    val measurer = rememberTextMeasurer()
    val labels = MaterialTheme.typography.labelSmall.numbers.copy(color = colours.textSecondary)
    val points = row.points.sortedBy { it.timeUnixS }
    val start = points.first().timeUnixS
    val end = max(points.last().timeUnixS, start + 1)
    val reach = max(points.maxOf { abs(it.differenceHpa) }, view.limits.maxOfOrNull { abs(it.differenceHpa) } ?: 0.0)
    // Room above the outermost line, and the same below: zero stays in the middle.
    val bound = Axis.ceiling(reach * 1.25, least = 1.0)
    val levelInk = mapOf(
        FoehnLevel.UNKNOWN to colours.textSecondary,
        FoehnLevel.NONE to colours.text,
        FoehnLevel.POSSIBLE to colours.caution,
        FoehnLevel.LIKELY to colours.danger,
    )
    val lineInk = colours.primary
    val grid = colours.border
    val zeroInk = colours.borderStrong
    Canvas(
        Modifier
            .fillMaxWidth()
            .height(VentiSpace.s8 + VentiSpace.s7)
            .testTag("foehn-chart")
            .semantics { contentDescription = "${view.chartTitle}: ${row.title}. ${row.aboveLabel}. ${row.belowLabel}." },
    ) {
        val hair = VentiSpace.s1.toPx() / 4
        val stroke = VentiSpace.s1.toPx() / 2
        val gap = VentiSpace.s1.toPx()
        val limitTexts = view.limits.map { measurer.measure(it.label, labels) }
        val zeroText = measurer.measure("0 ${view.unit}", labels)
        val trailing = (limitTexts.map { it.size.width } + zeroText.size.width).maxOrNull()?.toFloat() ?: 0f
        val timeHeight = measurer.measure("00", labels).size.height.toFloat()
        val left = 0f
        val right = size.width - trailing - gap * 2
        val top = gap
        val bottom = size.height - timeHeight - gap
        fun x(time: Long) = left + (time - start).toFloat() / (end - start) * (right - left)
        fun y(hpa: Double) = top + ((bound - hpa) / (2 * bound)).toFloat() * (bottom - top)

        // The hours, down the chart, with the time below.
        for (time in Axis.clockTicks(start, end, most = 4)) {
            val at = x(time)
            drawLine(grid, Offset(at, top), Offset(at, bottom), hair)
            val text = measurer.measure(Format.timeOnDayAxis(time), labels)
            val leftEdge = (at - text.size.width / 2f).coerceIn(left, max(left, right - text.size.width))
            drawText(text, topLeft = Offset(leftEdge, bottom + gap / 2))
        }
        // Zero, and the core's limits with their words.
        drawLine(zeroInk, Offset(left, y(0.0)), Offset(right, y(0.0)), hair * 2)
        drawText(zeroText, topLeft = Offset(right + gap, y(0.0) - zeroText.size.height / 2f))
        view.limits.forEachIndexed { index, limit: FoehnLimit ->
            val at = y(limit.differenceHpa)
            drawLine(levelInk.getValue(limit.level), Offset(left, at), Offset(right, at), hair)
            val text = limitTexts[index]
            drawText(text, topLeft = Offset(right + gap, at - text.size.height / 2f))
        }
        // The difference, and each point in the colour of its level.
        if (points.size >= 2) {
            val path = Path()
            points.forEachIndexed { index, point ->
                val at = Offset(x(point.timeUnixS), y(point.differenceHpa))
                if (index == 0) path.moveTo(at.x, at.y) else path.lineTo(at.x, at.y)
            }
            drawPath(path, lineInk, style = Stroke(stroke, cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
        for (point in points) {
            drawCircle(levelInk.getValue(point.level), stroke * 1.5f, Offset(x(point.timeUnixS), y(point.differenceHpa)))
        }
    }
}
