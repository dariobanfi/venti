package fly.venti.app.airspace

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.design.Format
import fly.venti.app.logbook.ImportNotice
import fly.venti.app.logbook.PickedFile
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.core.AirspaceFile
import fly.venti.core.AirspaceImport
import fly.venti.core.AirspaceOverPlace
import fly.venti.core.AirspaceSet
import fly.venti.core.AirspaceShape
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.Core
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.KeptAirspace
import fly.venti.core.serverAirspacePath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.Locale

/**
 * The pilot's airspace as the screens see it.
 *
 * It comes from two places: files the pilot brings in, and the pilot's
 * Venti server, which the app asks for the airspace around where the pilot
 * is and whose answer it keeps on the phone. The core keeps both, reads
 * them and answers every question about them; this class asks it, reads
 * files from where the pilot picked them, and sends the request to the
 * server.
 */
class AirspaceModel(
    private val core: Core,
    /** Where files are read and parsed, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
    /** This device's clock, in seconds since 1970: the day a file is brought in, and the age of what is kept. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
    /** Where the choice to use the server is kept. Without it the choice lasts as long as the app runs. */
    private val remembered: Preferences? = null,
    /** Where a request to the server runs on after the screen that caused it has gone. */
    private val scope: CoroutineScope? = null,
    /** Fetches an address and returns the server's answer. */
    private val transport: suspend (String) -> HttpAnswer = { url -> Http.get(url) },
) {
    /** The files the pilot brought in. */
    var files: List<AirspaceFile> by mutableStateOf(emptyList())
        private set

    /** The airspace kept from the server, one entry for each source. */
    var kept: List<KeptAirspace> by mutableStateOf(emptyList())
        private set

    /**
     * Every airspace of the files and of what is kept as one set, for the
     * maps and the cockpit. Changes whenever either does; absent when
     * there is neither.
     */
    var set: AirspaceSet? by mutableStateOf(null)
        private set

    /**
     * Set when there is airspace but it could not be put together: the
     * cockpit would be silent, and the pilot must know.
     */
    var problem: String? by mutableStateOf(null)
        private set

    /** Why the server's airspace could not be fetched, in words. */
    var serverProblem: String? by mutableStateOf(null)
        private set

    /** Whether the server is asked for airspace at all. The pilot's choice, remembered. */
    var usesServer: Boolean by mutableStateOf(remembered?.airspaceWithoutServer != true)
        private set

    /** The server to ask, without a slash at its end. The app sets this to the one in the settings. */
    var server: () -> String? = { null }

    private var isFetching = false

    /** When the server last failed to answer, by `now`. */
    private var failedAt: Long? = null

    init {
        reload()
    }

    /** Whether there is no airspace to draw or to warn about. */
    val isEmpty: Boolean get() = files.isEmpty() && kept.isEmpty()

    /** Whose airspace the server's is, with what their licences ask to be shown wherever it is. */
    val credits: List<Credit> get() = kept.mapNotNull { it.credit }

    /** What the cockpit will warn from, in a line for before take-off. */
    val inALine: String
        get() {
            val parts = kept.map { "${it.provider}, ${count(it.airspaces)}" } +
                listOfNotNull(
                    when (files.size) {
                        0 -> null
                        1 -> "1 file"
                        else -> "${Format.count(files.size.toLong())} files"
                    },
                )
            return if (parts.isEmpty()) "None" else parts.joinToString(" · ")
        }

    /**
     * Whose airspace is on a map, in a few words for its lower edge. What
     * the providers ask to be said is in full on the airspace screen.
     */
    val mapCredit: String?
        get() {
            val credits = credits
            if (credits.isEmpty()) return null
            val certified = if (credits.any { it.notice != null }) ", not certified" else ""
            return "Airspace: ${Format.list(credits.map { it.provider })}$certified"
        }

    /**
     * The airspaces that reach into a part of the map, without those that
     * certainly begin above `ceilingM` (the core's filter by height; `null`
     * for all).
     */
    fun shapes(area: Bounds, ceilingM: Double? = null): List<AirspaceShape> = set?.inAreaBelow(area, ceilingM).orEmpty()

    /** What lies over a place, as the core lists it; `null` without airspace data at all. */
    fun over(place: Coordinate): AirspaceOverPlace? = set?.overPlace(place)

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

    /**
     * Asks the server for the airspace around a place and keeps it, when
     * the core says it is time: nothing is kept, what is kept is a day old,
     * or the place is at the edge of it or beyond. Otherwise nothing is
     * sent. Called whenever the app learns where the pilot is looking or
     * flying.
     */
    suspend fun refresh(around: Coordinate) {
        if (!usesServer || isFetching) return
        val address = server() ?: return
        val time = now()
        // A server that failed is left alone for a while.
        failedAt?.let { failed -> if (time - failed in 0 until RETRY_AFTER_SECONDS) return }
        if (!core.keptAirspaceIsDue(address, around.lat, around.lon, time)) return
        isFetching = true
        try {
            val answer = transport(address + serverAirspacePath(around.lat, around.lon))
            // A country's airspace is a megabyte to read and to write.
            withContext(background) {
                core.keepServerAirspace(
                    address,
                    around.lat,
                    around.lon,
                    answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(),
                    answer.body,
                    time,
                )
            }
            // Switched off while the answer was under way: it is not kept.
            if (!usesServer) {
                runCatching { core.deleteKeptAirspace() }
            }
            failedAt = null
            serverProblem = null
            reload()
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            failedAt = time
            serverProblem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "Venti could not be reached for airspace. ${error.pilotMessage}"
            }
        } finally {
            isFetching = false
        }
    }

    /**
     * The same, without waiting for it: the request runs on when the screen
     * that asked has gone, and its answer is kept.
     */
    fun refreshSoon(around: Coordinate) {
        scope?.launch { refresh(around) }
    }

    /** Asks the server again from now on. Nothing is fetched until a map or a flight says where. */
    fun useServer() {
        usesServer = true
        remembered?.airspaceWithoutServer = false
    }

    /** Removes what is kept from the server and stops asking for it. */
    fun stopUsingServer() {
        usesServer = false
        remembered?.airspaceWithoutServer = true
        runCatching { core.deleteKeptAirspace() }
        serverProblem = null
        failedAt = null
        reload()
    }

    private fun reload() {
        files = runCatching { core.airspaceFiles() }.getOrDefault(emptyList())
        kept = runCatching { core.keptAirspace() }.getOrDefault(emptyList())
        problem = null
        set = if (isEmpty) {
            null
        } else {
            try {
                core.airspace()
            } catch (error: Exception) {
                problem = "Your airspace could not be loaded, so nothing is drawn or warned about. " +
                    error.pilotMessage
                null
            }
        }
    }

    companion object {
        /** After a request that failed, the server is left alone for this long. */
        const val RETRY_AFTER_SECONDS = 5 * 60L

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

        /**
         * What the row of a provider's airspace says under its name: how
         * much is kept, as of when by the provider's word, and when the
         * app fetched it.
         */
        fun detail(layer: KeptAirspace): String = listOfNotNull(
            count(layer.airspaces),
            layer.writtenUnixS?.let { "as of ${Format.day(it)}" },
            "fetched ${Format.day(layer.keptUnixS)}",
        ).joinToString(" · ")

        /**
         * Whose airspace a provider's row holds, or `null` when the server
         * does not say. The kept area reaches across borders, and the
         * cockpit cannot draw one.
         */
        fun countriesNote(layer: KeptAirspace): String? {
            if (layer.countries.isEmpty()) return null
            return "The airspace of ${countryNames(layer.countries)}. Of other countries none is kept, also " +
                "where the area fetched reaches into them."
        }

        /**
         * Countries by their names in the pilot's language: "Germany and
         * Austria". A code the phone does not know stays as it is.
         */
        fun countryNames(codes: List<String>, language: Locale = Locale.getDefault()): String =
            Format.list(
                codes.map { code ->
                    val name = runCatching { Locale.Builder().setRegion(code).build().getDisplayCountry(language) }
                    name.getOrNull()?.takeIf { it.isNotEmpty() } ?: code
                },
            )

        /** Said in red on the row of airspace whose server does not say where it is complete. */
        const val COVERS_UNKNOWN =
            "Venti does not say for which area this is complete, so the cockpit cannot take silence " +
                "for “no airspace near” and says that it is not checking."

        /** What stands under the server's airspace: what the app does with a server, or would. */
        fun serverNote(usesServer: Boolean, hasServer: Boolean, hasKept: Boolean): String = when {
            !usesServer -> "Switched off: only your own files are used."
            !hasServer ->
                "With Venti connected in the settings, the app fetches the airspace around you by itself " +
                    "and keeps it on this phone, so the cockpit warns without a network."
            !hasKept ->
                "Fetched the next time the map is open or a flight starts with a network, for about 200 km " +
                    "around you, and kept on this phone."
            else ->
                "Kept on this phone for about 200 km around where it was fetched, so the cockpit warns " +
                    "without a network. Fetched again once a day, and when you are somewhere else."
        }
    }
}
