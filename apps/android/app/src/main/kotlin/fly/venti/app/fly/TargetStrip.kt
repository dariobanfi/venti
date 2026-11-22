package fly.venti.app.fly

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.numbers
import fly.venti.app.logbook.FlightFiles
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.ImportNoticeDialog
import fly.venti.app.waypoints.RowDivider
import fly.venti.app.waypoints.WAYPOINT_FILES
import fly.venti.app.waypoints.WaypointRow
import fly.venti.app.waypoints.WaypointSearch
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.waypoints.matching
import fly.venti.core.Coordinate
import fly.venti.core.ReachView
import fly.venti.core.WayToTarget
import fly.venti.core.WaypointView
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/**
 * The line between the map and the instruments that says where the cockpit
 * is navigating to: which way to turn, how far, and how high the glider is
 * expected to arrive. A tap chooses another place.
 */
@Composable
fun TargetStrip(
    /** The way to the target, if there is one to go to yet. */
    way: WayToTarget?,
    /** The direction the glider is travelling, if it is moving. */
    courseDeg: Double?,
    onChoose: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("target-strip")
            .clickable(role = Role.Button, onClick = onChoose)
            .heightIn(min = 56.dp)
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .clearAndSetSemantics {
                contentDescription = "Flying to"
                stateDescription = TargetWords.spoken(way)
                onClick(label = "Choose another place") {
                    onChoose()
                    true
                }
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (way == null) {
            Icon(painterResource(R.drawable.ic_pin_drop), contentDescription = null, Modifier.size(28.dp))
            Spacer(Modifier.width(12.dp))
            Text("Choose a place to fly to", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
        } else {
            // An arrow that points to the target as seen from the glider:
            // up means straight ahead. Standing still, up means north.
            Icon(
                painterResource(R.drawable.ic_navigation),
                contentDescription = null,
                modifier = Modifier.size(28.dp).rotate((way.bearingDeg - (courseDeg ?: 0.0)).toFloat()),
                tint = MaterialTheme.colorScheme.primary,
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    way.name,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    "${Format.kilometres(way.distanceM)} km",
                    style = MaterialTheme.typography.bodyMedium.numbers,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
            Spacer(Modifier.width(8.dp))
            Arrival(way)
        }
        Spacer(Modifier.width(8.dp))
        Icon(
            painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** The height the glider is expected to arrive at, over the target. */
@Composable
private fun Arrival(way: WayToTarget) {
    val height = way.arrivalHeightM
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    // Falling short is the one thing here worth a warning colour.
    val warning = MaterialTheme.colorScheme.error
    Column(horizontalAlignment = Alignment.End) {
        val meets = way.groundInTheWayM
        if (meets != null) {
            // The straight glide would arrive with height to spare, and
            // meets a ridge on the way: that is what counts.
            Text(
                ReachWords.GROUND_IN_THE_WAY,
                style = MaterialTheme.typography.titleSmall,
                color = warning,
                maxLines = 1,
            )
            Text(
                "in ${Format.kilometres(meets)} km",
                style = MaterialTheme.typography.bodySmall.numbers,
                color = quiet,
                maxLines = 1,
            )
        } else if (height != null) {
            val unit = MaterialTheme.typography.bodySmall
            Text(
                buildAnnotatedString {
                    append(Format.signedMetres(height))
                    withStyle(SpanStyle(fontSize = unit.fontSize, fontWeight = FontWeight.Medium, color = quiet)) {
                        append(" m")
                    }
                },
                style = MaterialTheme.typography.titleLarge.numbers,
                fontWeight = FontWeight.SemiBold,
                color = if (height < 0) warning else MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
            )
            Text(
                if (way.windKnown) "Arrival" else "Arrival in still air",
                style = MaterialTheme.typography.bodySmall,
                color = quiet,
                maxLines = 1,
            )
        } else if (way.expectedGlide == null) {
            Text(
                "Wind too strong",
                style = MaterialTheme.typography.titleSmall,
                color = warning,
                maxLines = 1,
            )
            Text("to get there", style = MaterialTheme.typography.bodySmall, color = quiet, maxLines = 1)
        }
    }
}

/** The way to the target in words, for a screen reader. */
object TargetWords {
    fun spoken(way: WayToTarget?): String {
        if (way == null) return "Nothing chosen"
        val parts = mutableListOf("${way.name}, ${Format.kilometres(way.distanceM)} kilometres")
        val height = way.arrivalHeightM
        val meets = way.groundInTheWayM
        if (meets != null) {
            parts += "the ground is in the way in ${Format.kilometres(meets)} kilometres"
        } else if (height != null) {
            parts += if (height < 0) {
                "expected to fall short by ${Format.metres(-height)} metres"
            } else {
                "expected to arrive ${Format.metres(height)} metres above it"
            }
        } else if (way.expectedGlide == null) {
            parts += "the wind is too strong to get there"
        }
        return parts.joinToString(", ")
    }
}

/**
 * Where to fly to: the take-off, or one of the pilot's waypoints, nearest
 * first. A sheet over the cockpit.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TargetPicker(
    waypoints: WaypointsModel,
    /** Where the glider is, to list the nearest places first. */
    position: Coordinate?,
    /** The place the cockpit is navigating to now; `null` for the take-off. */
    chosen: String?,
    /** How each of some places lies within the glider's reach now, in their order. */
    reach: (List<WaypointView>) -> List<ReachView>,
    /** Called with the chosen place; `null` for the take-off. */
    onChoose: (WaypointView?) -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var search by rememberSaveable { mutableStateOf("") }
    var notice by remember { mutableStateOf<ImportNotice?>(null) }
    val chooseFiles = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) {
            scope.launch {
                notice = waypoints.importFiles(uris.map { FlightFiles.source(context, it, what = "a waypoint file") })
            }
        }
    }
    // Asked for again whenever the pilot's waypoints change.
    val all = waypoints.all
    val near = remember(all, position) { waypoints.near(position) }
    val shown = remember(near, search) { near.matching(search) }
    // Asked for again with every new position, while the list is open.
    val reaches = remember(shown, position) { reach(shown) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Fly to", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text(Common.words.cancel) }
        }
        if (all.isNotEmpty()) {
            WaypointSearch(search, onChange = { search = it })
        }
        LazyColumn(contentPadding = PaddingValues(bottom = 32.dp)) {
            item {
                TakeoffRow(isChosen = chosen == null) {
                    onChoose(null)
                    onDismiss()
                }
                SectionFooter(
                    "Without a place chosen, the cockpit shows the way back to where you took off.",
                    Modifier.padding(horizontal = 16.dp),
                )
                SectionHeader("Waypoints", Modifier.padding(horizontal = 16.dp))
            }
            itemsIndexed(shown, key = { _, waypoint -> waypoint.id }) { index, waypoint ->
                WaypointRow(
                    waypoint,
                    Modifier.testTag("target-row").clickable(role = Role.Button) {
                        onChoose(waypoint)
                        onDismiss()
                    },
                    isChosen = waypoint.name == chosen,
                    reach = reaches.getOrNull(index),
                )
                RowDivider()
            }
            item {
                if (all.isEmpty()) {
                    Hint("You have no waypoints yet. $WAYPOINT_FILES")
                } else if (shown.isEmpty()) {
                    Hint("No waypoint matches “${search.trim()}”.")
                } else {
                    // What the heights beside the places are, and are not.
                    SectionFooter(
                        ReachWords.HOW_IT_IS_WORKED_OUT,
                        Modifier.padding(horizontal = 16.dp).testTag("reach-footer"),
                    )
                }
                TextButton(
                    onClick = { chooseFiles.launch(arrayOf("*/*")) },
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                ) {
                    Icon(painterResource(R.drawable.ic_file_open), contentDescription = null, Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Add waypoints from a file")
                }
            }
        }
    }

    ImportNoticeDialog(notice) { notice = null }
}

@Composable
private fun Hint(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = 20.dp, vertical = 12.dp),
    )
}

/** The first row of the list: back to where the flight began. */
@Composable
private fun TakeoffRow(isChosen: Boolean, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 60.dp)
            .padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(R.drawable.ic_home),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(20.dp))
        Text("Take-off", style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        if (isChosen) {
            Icon(
                painterResource(R.drawable.ic_check),
                contentDescription = "Chosen",
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}
