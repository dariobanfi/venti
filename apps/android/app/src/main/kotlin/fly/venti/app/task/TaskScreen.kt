package fly.venti.app.task

import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.SharedFiles
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.app.logbook.FlightFiles
import fly.venti.app.waypoints.WaypointRow
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.core.Coordinate
import fly.venti.core.TaskRole
import fly.venti.core.TaskTurnpointView
import fly.venti.core.TaskView
import fly.venti.core.WaypointView
import kotlinx.coroutines.launch
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min

/**
 * The pilot's task: what it is, where it came from and how to hand it on.
 *
 * A task comes in as a file or as text copied from a message, in the format
 * XCTrack and other instruments exchange, and goes out as a file.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TaskScreen(tasks: TaskModel, waypoints: WaypointsModel, onBack: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var isShowingMenu by remember { mutableStateOf(false) }
    var isConfirmingRemoval by remember { mutableStateOf(false) }
    var isPlanningRoute by remember { mutableStateOf(false) }
    val task = tasks.task

    // The times of a task are those of today.
    LaunchedEffect(Unit) { tasks.reload() }

    // Task files have no type Android knows, so any file can be picked; the
    // core says whether it holds a task.
    val chooseFile = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) {
            scope.launch { tasks.keep(FlightFiles.source(context, uri, what = "a task")) }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Task") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                    }
                },
                actions = {
                    if (task != null) {
                        // Handing the task on, and taking it away.
                        Box {
                            IconButton(onClick = { isShowingMenu = true }) {
                                Icon(painterResource(R.drawable.ic_more_vert), contentDescription = "More")
                            }
                            DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
                                DropdownMenuItem(
                                    text = { Text("Share as a file") },
                                    leadingIcon = { Icon(painterResource(R.drawable.ic_share), contentDescription = null) },
                                    onClick = {
                                        isShowingMenu = false
                                        scope.launch {
                                            runCatching { tasks.exportFile(SharedFiles.folder(context)) }.getOrNull()
                                                ?.let { SharedFiles.share(context, it, "Share as a file") }
                                        }
                                    },
                                )
                                DropdownMenuItem(
                                    text = { Text("Remove this task") },
                                    leadingIcon = { Icon(painterResource(R.drawable.ic_delete), contentDescription = null) },
                                    onClick = {
                                        isShowingMenu = false
                                        isConfirmingRemoval = true
                                    },
                                )
                            }
                        }
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .testTag("task-screen")
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 24.dp),
        ) {
            if (task != null) {
                Group {
                    row { TaskSketch(task, Modifier.fillMaxWidth().height(200.dp).padding(12.dp)) }
                }
                SectionFooter(TaskWords.summary(task))

                SectionHeader(if (task.isRoute) "Waypoints" else "Turnpoints")
                Group {
                    for (turnpoint in task.turnpoints) {
                        row { TaskTurnpointRow(turnpoint) }
                    }
                }

                val times = TaskWords.times(task)
                if (times.isNotEmpty()) {
                    SectionHeader("Times")
                    Group {
                        for ((name, value) in times) {
                            row { ValueRow(name, value) }
                        }
                    }
                }
            }

            SectionHeader(if (task == null) "Bring a task in" else "Replace it with another")
            Group {
                row {
                    LinkRow(
                        "Paste a task",
                        R.drawable.ic_chevron_right,
                        Modifier.testTag("task-paste"),
                        leading = R.drawable.ic_content_paste,
                    ) {
                        val copied = copiedText(context)
                        if (copied == null) {
                            tasks.problem = "There is no text to paste. Copy a task from a message first."
                        } else {
                            tasks.keep(copied)
                        }
                    }
                }
                row {
                    LinkRow("Open a task file", R.drawable.ic_chevron_right, leading = R.drawable.ic_folder_open) {
                        chooseFile.launch(arrayOf("*/*"))
                    }
                }
                row {
                    LinkRow(
                        "Plan a route from your waypoints",
                        R.drawable.ic_chevron_right,
                        Modifier.testTag("task-plan-route"),
                        leading = R.drawable.ic_route,
                    ) {
                        isPlanningRoute = true
                    }
                }
            }
            tasks.problem?.let { problem ->
                Row(
                    Modifier.padding(start = 4.dp, end = 4.dp, top = 8.dp).testTag("task-problem"),
                    verticalAlignment = Alignment.Top,
                ) {
                    Icon(
                        painterResource(R.drawable.ic_warning),
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.error,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(problem, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
            }
            SectionFooter(
                "Copy a task from a message, or open a file ending in .xctsk: the format XCTrack, Flyskyhy and " +
                    "other instruments exchange. The cockpit then leads from turnpoint to turnpoint, counts down " +
                    "to the start and measures the way to goal. A route of your own waypoints is flown from one " +
                    "to the next.",
            )
        }
    }

    if (isPlanningRoute) {
        RoutePlanner(tasks, waypoints, onDismiss = { isPlanningRoute = false })
    }

    if (isConfirmingRemoval) {
        AlertDialog(
            onDismissRequest = { isConfirmingRemoval = false },
            title = { Text("Remove this task?") },
            text = { Text("The cockpit goes back to navigating to the take-off or to a place you choose.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingRemoval = false
                        tasks.forget()
                    },
                ) {
                    Text("Remove", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { isConfirmingRemoval = false }) { Text("Cancel") } },
        )
    }
}

/** The text the pilot copied last, if the clipboard holds any. */
private fun copiedText(context: Context): String? {
    val clip = context.getSystemService(ClipboardManager::class.java)?.primaryClip ?: return null
    if (clip.itemCount == 0) return null
    return clip.getItemAt(0).coerceToText(context)?.toString()?.takeIf { it.isNotBlank() }
}

/**
 * One turnpoint in a line: its number, name, what it is, and how far the
 * shortest way has come when it touches it.
 */
@Composable
fun TaskTurnpointRow(
    turnpoint: TaskTurnpointView,
    /** Whether the flight has reached it; `null` outside a flight. */
    isReached: Boolean? = null,
    isNext: Boolean = false,
) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("task-turnpoint")
            .heightIn(min = 60.dp)
            .padding(horizontal = 16.dp, vertical = 8.dp)
            // One element for a screen reader.
            .clearAndSetSemantics {
                contentDescription = "${turnpoint.number}, ${turnpoint.title}, ${turnpoint.detail}"
                stateDescription = TaskWords.spoken(turnpoint, isReached, isNext)
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(28.dp)
                .background(
                    if (isNext) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHighest,
                    CircleShape,
                ),
            contentAlignment = Alignment.Center,
        ) {
            val ink = if (isNext) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
            if (isReached == true) {
                Icon(painterResource(R.drawable.ic_check), contentDescription = null, modifier = Modifier.size(18.dp), tint = ink)
            } else {
                Text(turnpoint.number.toString(), style = MaterialTheme.typography.labelLarge.numbers, color = ink)
            }
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                turnpoint.title,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = if (isNext) FontWeight.SemiBold else FontWeight.Normal,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(turnpoint.detail, style = MaterialTheme.typography.bodyMedium.numbers, color = quiet, maxLines = 1)
        }
        if (turnpoint.number > 1u) {
            Spacer(Modifier.width(8.dp))
            Column(horizontalAlignment = Alignment.End) {
                Text("${Format.kilometres(turnpoint.alongM)} km", style = MaterialTheme.typography.bodyLarge.numbers)
                Text("+${Format.kilometres(turnpoint.legM)}", style = MaterialTheme.typography.bodyMedium.numbers, color = quiet)
            }
        }
    }
}

/**
 * The task drawn to scale without a map: its cylinders, its goal and the
 * shortest way through them, north at the top.
 */
@Composable
fun TaskSketch(task: TaskView, modifier: Modifier = Modifier) {
    val line = MaterialTheme.colorScheme.onSurface
    val way = MaterialTheme.colorScheme.primary
    Canvas(modifier.testTag("task-sketch").semantics { contentDescription = "A sketch of the task" }) {
        val frame = SketchFrame(task, size.width.toDouble(), size.height.toDouble(), margin = 8.dp.toPx().toDouble())
        fun path(places: List<Coordinate>) = Path().apply {
            places.forEachIndexed { index, place ->
                val (x, y) = frame.point(place)
                if (index == 0) moveTo(x.toFloat(), y.toFloat()) else lineTo(x.toFloat(), y.toFloat())
            }
        }
        // The shortest way, under the cylinders it touches.
        drawPath(path(task.way), way, style = Stroke(2.dp.toPx(), join = StrokeJoin.Round))
        for (turnpoint in task.turnpoints) {
            if (turnpoint.outline.size >= 3) {
                val ring = path(turnpoint.outline).apply { close() }
                drawPath(ring, line.copy(alpha = if (turnpoint.role == TaskRole.GOAL) 0.16f else 0.05f))
                // The start is the one cylinder with a clock on it.
                val dashes = if (turnpoint.role == TaskRole.START) {
                    PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 3.dp.toPx()))
                } else {
                    null
                }
                drawPath(ring, line, style = Stroke(1.5.dp.toPx(), pathEffect = dashes))
            } else {
                val (x, y) = frame.point(turnpoint.position)
                drawCircle(line, 3.5.dp.toPx(), Offset(x.toFloat(), y.toFloat()))
            }
        }
    }
}

/** Fits a task's box into a drawing, with the same scale both ways. */
class SketchFrame(task: TaskView, wide: Double, high: Double, margin: Double) {
    private val west = task.west
    private val north = task.north

    // A degree of longitude is shorter than one of latitude.
    private val squeeze = cos(Math.toRadians((task.south + task.north) / 2))
    private val width = max((task.east - task.west) * squeeze, 1e-6)
    private val height = max(task.north - task.south, 1e-6)
    private val scale = min((wide - 2 * margin) / width, (high - 2 * margin) / height)
    private val left = (wide - width * scale) / 2
    private val top = (high - height * scale) / 2

    /** Where a place is drawn: across, and down from the top. */
    fun point(place: Coordinate): Pair<Double, Double> =
        Pair(left + (place.lon - west) * squeeze * scale, top + (north - place.lat) * scale)
}

/**
 * Makes a route of the pilot's own waypoints: tapped in the order to fly
 * them, and kept as the task. A route has no cylinders, start or goal; the
 * cockpit leads from waypoint to waypoint.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RoutePlanner(tasks: TaskModel, waypoints: WaypointsModel, onDismiss: () -> Unit) {
    // The waypoints of the route so far, in order. One may be in it twice:
    // out to a place and back is a route too.
    var route by remember { mutableStateOf(emptyList<WaypointView>()) }
    var search by remember { mutableStateOf("") }
    var problem by remember { mutableStateOf<String?>(null) }
    val wanted = search.trim()
    val shown = remember(waypoints.all, wanted) {
        waypoints.all.filter {
            wanted.isEmpty() || it.name.contains(wanted, ignoreCase = true) || it.code.contains(wanted, ignoreCase = true)
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onDismiss) { Text("Cancel") }
            Text(
                "Plan a route",
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.weight(1f).padding(horizontal = 8.dp),
            )
            TextButton(
                onClick = {
                    if (tasks.keepRoute(route)) onDismiss() else problem = tasks.problem
                },
                enabled = route.size >= 2,
                modifier = Modifier.testTag("route-keep"),
            ) {
                Text("Keep")
            }
        }
        LazyColumn(modifier = Modifier.testTag("route-planner"), contentPadding = PaddingValues(bottom = 32.dp)) {
            item {
                SectionHeader("The route", Modifier.padding(horizontal = 16.dp))
                if (route.isEmpty()) {
                    Text(
                        "Tap your waypoints below in the order you want to fly them.",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
                    )
                }
            }
            itemsIndexed(route) { index, waypoint ->
                Row(
                    modifier = Modifier.fillMaxWidth().testTag("route-stop").padding(start = 20.dp, end = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(
                        Modifier.size(28.dp).background(MaterialTheme.colorScheme.surfaceContainerHighest, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text((index + 1).toString(), style = MaterialTheme.typography.labelLarge.numbers)
                    }
                    Spacer(Modifier.width(12.dp))
                    Text(
                        waypoint.name,
                        style = MaterialTheme.typography.bodyLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = { route = route.filterIndexed { at, _ -> at != index } }) {
                        Icon(painterResource(R.drawable.ic_cancel), contentDescription = "Take ${waypoint.name} out")
                    }
                }
            }
            item {
                problem?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
                    )
                }
                SectionHeader("Your waypoints", Modifier.padding(horizontal = 16.dp))
                if (waypoints.all.isEmpty()) {
                    Text(
                        "You have no waypoints yet. Bring a waypoint file in under Settings, or keep a flying " +
                            "site from the map as a waypoint.",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
                    )
                } else {
                    OutlinedTextField(
                        value = search,
                        onValueChange = { search = it },
                        placeholder = { Text("Waypoint") },
                        leadingIcon = { Icon(painterResource(R.drawable.ic_search), contentDescription = null) },
                        singleLine = true,
                        shape = MaterialTheme.shapes.extraLarge,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                    )
                }
            }
            items(shown, key = { it.id }) { waypoint ->
                WaypointRow(
                    waypoint,
                    Modifier.testTag("route-waypoint").clickable(role = Role.Button) { route = route + waypoint },
                )
            }
        }
    }
}
