package fly.venti.app.airspace

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.design.Format
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.PickedFile
import fly.venti.app.pilotMessage
import fly.venti.core.AirspaceFile
import fly.venti.core.AirspaceImport
import fly.venti.core.AirspaceSet
import fly.venti.core.AirspaceShape
import fly.venti.core.Bounds
import fly.venti.core.Core
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The pilot's airspace as the screens see it: the files it came in, and
 * what they hold.
 *
 * The core keeps the files, reads them and answers every question about
 * them; this class asks it and keeps the result for the maps and the
 * cockpit.
 */
class AirspaceModel(
    private val core: Core,
    /** Where files are read and parsed, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
    /** This device's clock, in seconds since 1970: the day a file is brought in. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
) {
    /** The files the pilot brought in. */
    var files: List<AirspaceFile> by mutableStateOf(emptyList())
        private set

    /** Everything those files hold, read into one index. Absent without a file. */
    var set: AirspaceSet? by mutableStateOf(null)
        private set

    /**
     * Set when there are files but their airspace could not be put
     * together: the cockpit would be silent, and the pilot must know.
     */
    var problem: String? by mutableStateOf(null)
        private set

    init {
        reload()
    }

    /** The airspaces that reach into a part of the map. */
    fun shapes(area: Bounds): List<AirspaceShape> = set?.inArea(area).orEmpty()

    /**
     * Brings in the airspace files the pilot picked, and says for each what
     * was read and what was not.
     */
    suspend fun importFiles(picked: List<PickedFile>): ImportNotice {
        val lines = withContext(background) {
            picked.map { file ->
                try {
                    summary(core.importAirspace(file.name, file.read(), now()), file.name)
                } catch (error: Exception) {
                    "${file.name}: ${error.pilotMessage}"
                }
            }
        }
        reload()
        return ImportNotice("Airspace", lines.joinToString("\n\n"))
    }

    /** Brings in a file from its text. A test's is given this way. */
    fun importText(name: String, text: String) {
        runCatching { core.importAirspace(name, text.toByteArray(), now()) }
        reload()
    }

    fun delete(file: AirspaceFile) {
        runCatching { core.deleteAirspaceFile(file.storedName) }
        reload()
    }

    private fun reload() {
        files = runCatching { core.airspaceFiles() }.getOrDefault(emptyList())
        problem = null
        set = if (files.isEmpty()) {
            null
        } else {
            try {
                core.airspace()
            } catch (error: Exception) {
                problem = "Your airspace files could not be loaded, so nothing is drawn or warned about. " +
                    error.pilotMessage
                null
            }
        }
    }

    companion object {
        /**
         * What reading a file came to, in words. What was left out is said
         * plainly: the pilot will not be warned about it.
         */
        fun summary(result: AirspaceImport, file: String): String {
            var text = "$file: ${count(result.airspaces)} read."
            if (result.leftOut > 0u) {
                val one = result.leftOut == 1u
                text += " ${count(result.leftOut)} could not be read and ${if (one) "is" else "are"} left out: " +
                    "you will not be warned about ${if (one) "it" else "them"}."
            }
            if (result.assumed > 0u) {
                val places = if (result.assumed == 1u) "1 place" else "${Format.count(result.assumed.toLong())} places"
                text += " In $places something could not be read and the cautious reading was taken."
            }
            if (result.problems.isNotEmpty()) {
                text += "\n" + result.problems.joinToString("\n")
            }
            return text
        }

        /** "1 airspace", "1,204 airspaces". */
        fun count(airspaces: UInt): String =
            if (airspaces == 1u) "1 airspace" else "${Format.count(airspaces.toLong())} airspaces"

        /** What a file's row says under its name: how much it holds, and since when. */
        fun detail(file: AirspaceFile): String {
            val imported = file.importedUnixS ?: return count(file.airspaces)
            return "${count(file.airspaces)} · brought in ${Format.day(imported)}"
        }

        /** What a file's row says in red when part of it could not be read, or `null`. */
        fun leftOutNote(file: AirspaceFile): String? = when (file.leftOut) {
            0u -> null
            1u -> "1 airspace in this file could not be read. You will not be warned about it."
            else ->
                "${Format.count(file.leftOut.toLong())} airspaces in this file could not be read. " +
                    "You will not be warned about them."
        }
    }
}
