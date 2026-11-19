package fly.venti.app.sites

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.design.Format
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.app.wind.contains
import fly.venti.app.wind.grown
import fly.venti.core.Bounds
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.SiteView
import fly.venti.core.sitesMap
import fly.venti.core.sitesPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The flying sites for the map, from the Venti server the pilot has set:
 * take-offs, landings, winch sites and training slopes.
 *
 * What to ask the server and how to read its answer is the core's. This
 * class sends the request and keeps the answer for the part of the map in
 * view. Sites change by the day, not by the minute, so an answer is kept
 * for an hour.
 */
class SitesModel(
    /** Where requests wait for the map to come to rest. */
    private val scope: CoroutineScope,
    private val transport: WindTransport = { url -> Http.get(url) },
    /** This device's clock, in milliseconds since 1970. */
    private val clock: () -> Long = System::currentTimeMillis,
    /** Where the core reads an answer, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The sites around the part of the map in view. */
    var sites: List<SiteView> by mutableStateOf(emptyList())
        private set

    /** Whose data they are. The licences ask for this next to the data. */
    var credits: List<Credit> by mutableStateOf(emptyList())
        private set

    /**
     * Whether the server holds everything its sources have for the area.
     * If not, a map without sites says nothing.
     */
    var isWhole: Boolean by mutableStateOf(false)
        private set

    /** The date of the data, in seconds since 1970, if the server gives one. */
    var asOf: Long? by mutableStateOf(null)
        private set

    /** Why the sites are missing, in words for the pilot. */
    var problem: String? by mutableStateOf(null)
        private set

    /** The server to ask, without a slash at its end. The app sets this to the one in the settings. */
    var server: () -> String? = { null }

    /** The area the sites were asked for, from which server, and when. */
    private class Answered(val area: Bounds, val server: String, val at: Long)

    private var answered: Answered? = null
    private var waiting: Job? = null

    /**
     * Whose sites are on the map, in one line. The licence asks for the
     * credit next to the data; the full wording is on each site's page.
     */
    val creditLine: String?
        get() = if (credits.isEmpty() || sites.isEmpty()) null else "Sites: ${Format.list(credits.map { it.provider })}"

    /** How old the data is, as the server's API asks a client to say: "As of Wed, 18 Nov 2026." */
    val asOfInWords: String? get() = asOf?.let { "As of ${Format.day(it)}." }

    /**
     * The map says which part of it is in view. The server is asked unless
     * that part is covered by an answer still in use, or the map shows too
     * much for sites to be drawn.
     */
    fun show(region: Bounds) {
        val server = server()
        if (server == null) {
            forget()
            return
        }
        // Another server's sites are not this one's.
        if (answered?.let { it.server != server } == true) {
            forget()
        }
        if (!showsSites(region)) return
        val answered = answered
        if (answered != null && answered.area.contains(region) && clock() - answered.at < KEEP_MILLISECONDS) {
            return
        }
        waiting?.cancel()
        waiting = scope.launch {
            // The map reports every pause of a drag; wait until it rests.
            delay(REST_MILLISECONDS)
            // More than is in view, so that a drag needs no new request.
            load(region.grown(0.5), server)
        }
    }

    /** Asks the server for the sites of an area. */
    suspend fun load(area: Bounds, server: String) {
        try {
            val answer = transport(server + sitesPath(area.south, area.west, area.north, area.east))
            val status = answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
            val map = withContext(background) {
                sitesMap(area.south, area.west, area.north, area.east, status, answer.body)
            }
            // The pilot may have changed the server while this was under way.
            if (server != this.server()) return
            sites = map.sites
            credits = map.credits
            isWhole = map.isWhole
            asOf = map.asOfUnixS
            answered = Answered(area, server, clock())
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != this.server()) return
            problem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "The server could not be reached for flying sites. ${error.pilotMessage}"
            }
        }
    }

    private fun forget() {
        waiting?.cancel()
        sites = emptyList()
        credits = emptyList()
        isWhole = false
        asOf = null
        answered = null
        problem = null
    }

    companion object {
        /** How long an answer is used before the server is asked again. */
        const val KEEP_MILLISECONDS = 60 * 60_000L

        /**
         * A map that shows more than this many degrees of longitude draws
         * no sites: at that scale they are a carpet of dots. It is about
         * the width of a phone's map at zoom 9.
         */
        const val WIDEST_MAP = 0.8

        /** How long the map must rest before the server is asked. */
        const val REST_MILLISECONDS = 400L

        /** Whether a map that shows this much is one sites are asked for. */
        fun showsSites(region: Bounds): Boolean = region.east - region.west <= WIDEST_MAP
    }
}
