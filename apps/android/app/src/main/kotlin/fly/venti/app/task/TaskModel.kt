package fly.venti.app.task

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.design.Format
import fly.venti.app.logbook.FlightFileSource
import fly.venti.app.pilotMessage
import fly.venti.core.Coordinate
import fly.venti.core.Core
import fly.venti.core.TaskFile
import fly.venti.core.TaskRole
import fly.venti.core.TaskTurnpointView
import fly.venti.core.TaskView
import fly.venti.core.WaypointView
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/**
 * The pilot's task: the competition task of the day, or a route planned for
 * a cross-country flight. One is kept at a time.
 *
 * Reading a task, measuring it and following a flight through it is the
 * core's. This class brings a task in from a file or the clipboard, and
 * hands it to the cockpit and to other pilots.
 */
class TaskModel(
    private val core: Core,
    /** This device's clock, in seconds since 1970: a task's times of day are those of today. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
    /** Where files are read and written, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** The task that is kept, as screens show it. */
    var task: TaskView? by mutableStateOf(null)
        private set

    /** Why the last task could not be taken in, in words for the pilot. */
    var problem: String? by mutableStateOf(null)

    init {
        reload()
    }

    /** Reads the kept task again: its times of day are those of today. */
    fun reload() {
        task = runCatching { core.keptTask(now()) }.getOrNull()
    }

    /**
     * Keeps a task from the text of a file, of a message or of a QR code.
     * It replaces the task kept before. Returns whether the text was a
     * task; where it was not, [problem] is the core's refusal.
     */
    fun keep(text: String): Boolean = change { core.keepTask(text) }

    /** Keeps the task in a file the pilot chose, or that another app handed over. */
    suspend fun keep(file: FlightFileSource): Boolean {
        val text = try {
            withContext(background) { file.read() }
        } catch (error: Exception) {
            problem = "The file could not be read. ${error.pilotMessage}"
            return false
        }
        if (text.length > LARGEST_FILE) {
            problem = "This file is too large to be a task."
            return false
        }
        return keep(text)
    }

    /**
     * Makes a route of these waypoints, in this order, and keeps it as the
     * task. Returns whether that worked.
     */
    fun keepRoute(waypoints: List<WaypointView>): Boolean = change { core.keepRoute(waypoints.map { it.id }) }

    /** Keeps a route drawn on the map as the task: its places named "Start", "Turnpoint 1" and on to "Finish". */
    fun keepDrawnRoute(places: List<Coordinate>): Boolean = change { core.keepDrawnRoute(places) }

    /** Forgets the kept task. */
    fun forget() {
        change { core.forgetTask() }
    }

    private fun change(what: () -> Unit): Boolean {
        val worked = try {
            what()
            problem = null
            true
        } catch (error: Exception) {
            problem = error.pilotMessage
            false
        }
        reload()
        return worked
    }

    /** The kept task as a file's text: for the cockpit, and for sharing. */
    val file: TaskFile? get() = runCatching { core.keptTaskFile() }.getOrNull()

    /**
     * The kept task as the text of a QR code, for another pilot's
     * instrument to scan; `null` without a task. Compressed, the code has
     * fewer dots; some older readers want the other kind.
     */
    fun code(compressed: Boolean): String? = runCatching { core.keptTaskCode(compressed) }.getOrNull()

    /** Writes the kept task to a file in a folder, to share it, and returns where it is. */
    suspend fun exportFile(into: File): File? = withContext(background) {
        val file = file ?: return@withContext null
        into.mkdirs()
        into.resolve(file.name).also { it.writeText(file.text) }
    }

    companion object {
        /** No task file is larger than this; a larger file is something else. */
        private const val LARGEST_FILE = 256 * 1024

        /** What a task file's name ends in. */
        const val FILE_ENDING = ".xctsk"
    }
}

/** The task in a line, for the Fly tab: "5 turnpoints · 45.4 km". */
val TaskView.inALine: String
    get() {
        val places = if (isRoute) "${turnpoints.size} waypoints" else "${turnpoints.size} turnpoints"
        return "$places · ${Format.kilometres(lengthM)} km"
    }

/** What a turnpoint is called: its name, or what it is where it has none. */
val TaskTurnpointView.title: String get() = name.ifEmpty { roleInWords }

/** What it is and how large, in a line: "Start · 5 km", "Goal line · 400 m wide". */
val TaskTurnpointView.detail: String
    get() = when {
        radiusM <= 0 || role == TaskRole.LAUNCH -> roleInWords
        isLine -> "$roleInWords · ${TaskWords.length(2 * radiusM)} wide"
        else -> "$roleInWords · ${TaskWords.length(radiusM)}"
    }

/** What the task's screens say, apart from what the core has worded. */
object TaskWords {
    /** A radius the way task boards write it: "400 m", "5 km", "2.5 km". */
    fun length(metres: Double): String {
        if (metres < 1000) return "${Format.metres(metres)} m"
        val kilometres = metres / 1000
        return if (kilometres == Math.rint(kilometres)) "${Format.whole(kilometres)} km" else "${Format.oneDecimal(kilometres)} km"
    }

    /** What a turnpoint's row says after its name, for a screen reader. */
    fun spoken(turnpoint: TaskTurnpointView, isReached: Boolean?, isNext: Boolean): String {
        val parts = mutableListOf<String>()
        if (isNext) parts += "next"
        if (isReached == true) parts += "reached"
        if (turnpoint.number > 1u) parts += "${Format.kilometres(turnpoint.alongM)} kilometres along the task"
        return parts.joinToString(", ")
    }

    /** The task's lengths in a sentence. */
    fun summary(task: TaskView): String = buildString {
        append("${Format.kilometres(task.lengthM)} km along the shortest way through ")
        append(if (task.isRoute) "its waypoints" else "the edges of its cylinders")
        task.speedSectionM?.let { append(", ${Format.kilometres(it)} km of it in the speed section") }
        append(". Measured on the ${task.measuredOn}.")
        task.finishAltitudeM?.let { append(" Goal is to be crossed ${Format.metres(it)} m above its ground.") }
    }

    /** The task's times on this phone's clock, in the order of the day: each with its name. */
    fun times(task: TaskView): List<Pair<String, String>> {
        val times = mutableListOf<Pair<String, String>>()
        task.takeoffOpenUnixS?.let { open ->
            val close = task.takeoffCloseUnixS?.let { " to ${Format.time(it)}" }.orEmpty()
            times += "Launch open" to Format.time(open) + close
        }
        task.start?.let { start ->
            val gates = start.gatesUnixS.joinToString(", ") { Format.time(it) }
            times += start.inWords to if (gates.isEmpty()) "Start always open" else "Start $gates"
            start.closeUnixS?.let { times += "Start closes" to Format.time(it) }
        }
        task.deadlineUnixS?.let { times += "Task ends" to Format.time(it) }
        return times
    }
}
