package fly.venti.app.waypoints

import androidx.annotation.DrawableRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.logbook.FlightFileSource
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.pilotMessage
import fly.venti.core.Coordinate
import fly.venti.core.Core
import fly.venti.core.TargetPlace
import fly.venti.core.WaypointKind
import fly.venti.core.WaypointView
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/**
 * The pilot's waypoints as the screens see them: places to fly to in the
 * cockpit.
 *
 * The core keeps them, reads the files they come in and works out how far
 * away each one is; this class asks it and keeps the list for the screen.
 */
class WaypointsModel(
    private val core: Core,
    /** Where files are read and parsed, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** Every waypoint, by name. */
    var all: List<WaypointView> by mutableStateOf(emptyList())
        private set

    init {
        reload()
    }

    /** The waypoints nearest to a place first, each with its distance and direction from there. */
    fun near(place: Coordinate?): List<WaypointView> = runCatching { core.waypoints(place) }.getOrDefault(emptyList())

    /**
     * Brings in the waypoints of the files the pilot picked: SeeYou files
     * (.cup) and GEO files (.wpt). Says what was added, and for each file
     * that could not be read, why.
     */
    suspend fun importFiles(sources: List<FlightFileSource>): ImportNotice {
        var added = 0u
        var known = 0u
        val problems = mutableListOf<String>()
        withContext(background) {
            for (source in sources) {
                try {
                    val result = core.importWaypoints(source.read())
                    added += result.added
                    known += result.alreadyThere
                    if (result.skippedLines > 0u) {
                        val lines = if (result.skippedLines == 1u) "1 line" else "${result.skippedLines} lines"
                        problems += "${source.name}: $lines could not be read. ${result.firstProblem.orEmpty()}".trim()
                    }
                } catch (error: Exception) {
                    problems += "${source.name}: ${error.pilotMessage}"
                }
            }
        }
        reload()
        return notice(added, known, problems)
    }

    /** Brings in waypoints from the text of a file. A test's are given this way. */
    fun importText(text: String) {
        runCatching { core.importWaypoints(text) }
        reload()
    }

    /**
     * Adds one waypoint: a flying site from the map, for example. Says
     * whether it is among the waypoints now.
     */
    fun add(name: String, position: Coordinate, elevationM: Double?): Boolean {
        val added = runCatching { core.addWaypoint(name, position, elevationM) }.isSuccess
        reload()
        return added
    }

    fun delete(id: String) {
        runCatching { core.deleteWaypoint(id) }
        reload()
    }

    fun deleteAll() {
        runCatching { core.deleteAllWaypoints() }
        reload()
    }

    /** Writes every waypoint to a file in a folder, to share it, and returns where it is. */
    suspend fun exportFile(into: File): File = withContext(background) {
        val file = core.waypointsFile()
        into.mkdirs()
        into.resolve(file.name).also { it.writeText(file.text) }
    }

    private fun reload() {
        all = runCatching { core.waypoints(null) }.getOrDefault(emptyList())
    }

    companion object {
        /** What an import did, in words for the pilot. */
        fun notice(added: UInt, known: UInt, problems: List<String>): ImportNotice {
            val title = when {
                added == 0u && known == 0u -> "No waypoints added"
                added == 0u -> "Nothing new"
                added == 1u -> "1 waypoint added"
                else -> "${Format.count(added.toLong())} waypoints added"
            }
            val lines = mutableListOf<String>()
            if (known > 0u) {
                lines += if (known == 1u) "1 was already there." else "${Format.count(known.toLong())} were already there."
            }
            lines += problems
            return ImportNotice(title, lines.joinToString("\n"))
        }
    }
}

/** The icon for a waypoint's kind. */
@get:DrawableRes
val WaypointView.icon: Int
    get() = when (kind) {
        WaypointKind.TAKEOFF -> R.drawable.venti_takeoff
        WaypointKind.LANDING -> R.drawable.venti_landing
        WaypointKind.OTHER -> R.drawable.ic_location_on
    }

/** The waypoint as a place for the cockpit to navigate to. */
val WaypointView.place: TargetPlace
    get() = TargetPlace(name = name, position = position, elevationM = elevationM)

/** Distance and direction if they are known, the height, and the short name if it says more than the name. */
val WaypointView.detail: String
    get() {
        val parts = mutableListOf<String>()
        val distance = distanceM
        val bearing = bearingDeg
        if (distance != null && bearing != null) {
            parts += "${Format.kilometres(distance)} km ${Format.compassPoint(bearing)}"
        }
        elevationM?.let { parts += "${Format.metres(it)} m" }
        if (code.isNotEmpty() && code != name) {
            parts += code
        }
        return parts.joinToString(" · ")
    }
