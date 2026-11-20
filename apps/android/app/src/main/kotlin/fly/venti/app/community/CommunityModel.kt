package fly.venti.app.community

import fly.venti.core.publicFlightsNearPath
import fly.venti.core.Coordinate
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.Bounds
import fly.venti.core.CoreException
import fly.venti.core.FlightAnalysis
import fly.venti.core.FlightView
import fly.venti.core.PublicFlight
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.flightAnalysisFromFile
import fly.venti.core.flightFromFile
import fly.venti.core.pilotPage
import fly.venti.core.pilotPagePath
import fly.venti.core.publicFlights
import fly.venti.core.publicFlightsPath
import fly.venti.core.RankedPilot
import fly.venti.core.RankingSeason
import fly.venti.core.ranking
import fly.venti.core.rankingPath
import fly.venti.core.rankingSeasons
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Other pilots' flights: the flights the pilots of the Venti server made
 * public, in one list.
 *
 * What to ask the server and how to read its answer is the core's. This
 * class sends the requests, keeps the pages it was given, and fetches a
 * flight's file when the pilot opens one. Nothing of another pilot's is
 * kept on the phone.
 */
class CommunityModel(
    /**
     * Fetches an address. A flight's file is no JSON, so nothing is said
     * about what is accepted.
     */
    private val transport: WindTransport = { url -> Http.get(url, ANSWER_WITHIN_MILLISECONDS, accept = null) },
    /** Where the core reads an answer or a flight's file, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The flights listed so far, in the order shown. */
    var flights: List<PublicFlight> by mutableStateOf(emptyList())
        private set

    /** The order they are in. */
    var order: PublicFlightsOrder by mutableStateOf(PublicFlightsOrder.NEWEST)
        private set

    /** Whether a page is being fetched. */
    var isLoading: Boolean by mutableStateOf(false)
        private set

    /** Whether the server has more than is listed. */
    var hasMore: Boolean by mutableStateOf(false)
        private set

    /** Why the list is missing, in words for the pilot. */
    var problem: String? by mutableStateOf(null)
        private set

    /** The server to ask, without a slash at its end. The app sets this to the one in the settings. */
    var server: () -> String? = { null }

    /**
     * One pilot's public flights, the newest first: the page reached from a
     * flight of that pilot's. Kept here, not in its screen, so that it is
     * still there when the pilot comes back from one of its flights.
     */
    data class PilotFlights(
        val handle: String,
        val flights: List<PublicFlight> = emptyList(),
        /** What to ask the next page with. Absent when there is none. */
        val next: String? = null,
        /** Whether the server has answered for this pilot. */
        val isLoaded: Boolean = false,
        val isLoading: Boolean = false,
        /** Why the flights are missing, in words. */
        val problem: String? = null,
    )

    /** Which request for a pilot's flights is the newest: an older one's answer is not shown. */
    private var pilotRound = 0

    /** The pilot whose flights were last looked at. */
    var pilot: PilotFlights? by mutableStateOf(null)
        private set

    /** What the pilot sees of the ranking of the server's pilots. */
    data class Ranked(
        /** The seasons to choose from, the newest first. None before the ranking was first asked for. */
        val seasons: List<RankingSeason> = emptyList(),
        /** The name of the season the ranking is of. */
        val season: String = "",
        /** The pilots, the one with the most points first. */
        val pilots: List<RankedPilot> = emptyList(),
        /** The rule in a sentence, for under the list. */
        val rule: String = "",
        val isLoading: Boolean = false,
        /** Why it is missing, in words for the pilot. */
        val problem: String? = null,
    )

    /** The ranking of the season last asked for. */
    var ranked: Ranked by mutableStateOf(Ranked())
        private set

    /** What to ask the next page with. */
    private var next: String? = null

    /**
     * Every flight a list has shown, by its address on the server: the one
     * that is open is looked up here, whichever list it was opened from.
     */
    private val known = mutableMapOf<String, PublicFlight>()

    /** Which server and order the list is from. */
    private var listed: Pair<String, PublicFlightsOrder>? = null

    /**
     * Loads the list, unless it is there already for the server that is
     * set and the order chosen.
     */
    suspend fun loadIfNeeded() {
        val server = server()
        if (server == null) {
            forget()
            return
        }
        if (listed == server to order) return
        load(fromTheTop = true)
    }

    /** Shows the list in another order, from its top. */
    suspend fun show(order: PublicFlightsOrder) {
        if (order == this.order) return
        this.order = order
        flights = emptyList()
        hasMore = false
        load(fromTheTop = true)
    }

    /** Asks again from the top: the pilot pulled the list down. */
    suspend fun refresh() {
        load(fromTheTop = true)
    }

    /** The next page, asked for when the last row comes into view. */
    suspend fun loadMore() {
        if (!hasMore) return
        load(fromTheTop = false)
    }

    private suspend fun load(fromTheTop: Boolean) {
        val server = server()
        if (server == null) {
            forget()
            return
        }
        if (isLoading) return
        isLoading = true
        val order = order
        try {
            val path = publicFlightsPath(order, null, null, null, if (fromTheTop) null else next)
            val answer = transport(server + path)
            val page = withContext(background) { publicFlights(answer.status16, answer.body) }
            // The pilot may have changed the server or the order while this
            // was under way.
            if (server != this.server() || order != this.order) return
            flights = if (fromTheTop) page.flights else flights + page.flights
            known += page.flights.associateBy { it.id }
            next = page.next
            hasMore = page.next != null
            listed = server to order
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != this.server() || order != this.order) return
            problem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "Venti could not be reached for other pilots' flights. ${error.pilotMessage}"
            }
        } finally {
            isLoading = false
        }
    }

    /**
     * Fetches the ranking of a season, by the name the core gives it.
     * Without a name it is the season last shown, or the newest. The
     * seasons are the core's, from the time now.
     */
    suspend fun rank(season: String? = null, nowUnixS: Long = System.currentTimeMillis() / 1000) {
        val server = server()
        if (server == null) {
            ranked = Ranked()
            return
        }
        val seasons = rankingSeasons(nowUnixS)
        val wanted = season ?: ranked.season
        val chosen = seasons.firstOrNull { it.name == wanted } ?: seasons.firstOrNull() ?: return
        // Another season's pilots are not this one's.
        val shown = if (chosen.name == ranked.season) ranked else Ranked()
        ranked = shown.copy(seasons = seasons, season = chosen.name, problem = null, isLoading = true)
        try {
            val answer = transport(server + rankingPath(null, chosen.fromUnixS, chosen.toUnixS))
            val ranking = withContext(background) { ranking(answer.status16, answer.body) }
            // The pilot may have chosen another season or server while this
            // was under way: that question's answer sets the rest.
            if (server != this.server() || ranked.season != chosen.name) return
            ranking.pilots.forEach { pilot -> known += pilot.flights.associateBy { it.id } }
            ranked = ranked.copy(pilots = ranking.pilots, rule = ranking.rule, isLoading = false)
        } catch (error: CancellationException) {
            if (ranked.season == chosen.name) ranked = ranked.copy(isLoading = false)
            throw error
        } catch (error: Exception) {
            if (ranked.season != chosen.name) return
            ranked = ranked.copy(
                isLoading = false,
                problem = when (error) {
                    is CoreException -> error.pilotMessage
                    else -> "Venti could not be reached for the ranking. ${error.pilotMessage}"
                },
            )
        }
    }

    /**
     * The flights pilots show to everyone in an area and a span of time,
     * newest first: the first page, which is as many as a list to choose
     * from needs. Nothing of it is kept for the lists; what goes wrong is
     * thrown, to be put into words by [words].
     */
    suspend fun flightsAround(area: Bounds, fromUnixS: Long, toUnixS: Long): List<PublicFlight> {
        val server = server() ?: throw NoLongerPublic()
        val answer = transport(server + publicFlightsPath(PublicFlightsOrder.NEWEST, area, fromUnixS, toUnixS, null))
        val page = withContext(background) { publicFlights(answer.status16, answer.body) }
        known += page.flights.associateBy { it.id }
        return page.flights
    }

    /**
     * The public flights that took off near a place, the newest first: what
     * was flown from a flying site. The first page of them.
     */
    suspend fun flightsNear(place: Coordinate, withinM: Double = 1500.0): List<PublicFlight> {
        val server = server() ?: throw NoLongerPublic()
        val answer = transport(server + publicFlightsNearPath(place.lat, place.lon, withinM))
        val page = withContext(background) { publicFlights(answer.status16, answer.body) }
        known += page.flights.associateBy { it.id }
        return page.flights
    }

    /** A flight one of the lists has shown, by its address on the server. */
    fun flight(id: String): PublicFlight? = known[id]

    /**
     * Shows a pilot's public flights from their top, unless they are there
     * already.
     */
    suspend fun showPilot(handle: String) {
        val shown = pilot
        if (shown != null && shown.handle == handle && shown.isLoaded && shown.problem == null) return
        pilot = PilotFlights(handle)
        loadPilot()
    }

    /** The pilot's next page, asked for when the last row comes into view. */
    suspend fun morePilotFlights() {
        val shown = pilot ?: return
        if (shown.next == null || shown.isLoading) return
        loadPilot()
    }

    /** The first page of the pilot's flights, or the one after what is listed. */
    private suspend fun loadPilot() {
        val shown = pilot ?: return
        pilotRound += 1
        val round = pilotRound
        pilot = shown.copy(isLoading = true)
        val result = try {
            val server = server() ?: throw NoLongerPublic()
            val answer = transport(server + pilotPagePath(shown.handle, shown.next))
            val page = withContext(background) { pilotPage(answer.status16, answer.body) }
            known += page.flights.associateBy { it.id }
            shown.copy(flights = shown.flights + page.flights, next = page.next, isLoaded = true, problem = null)
        } catch (error: CancellationException) {
            // Whoever asked has left the page. The next to look at it asks again.
            if (round == pilotRound) {
                pilot = shown
            }
            throw error
        } catch (error: Exception) {
            shown.copy(
                problem = when (error) {
                    is CoreException -> error.pilotMessage
                    is NoLongerPublic -> "Venti is not connected."
                    else -> "Venti could not be reached. ${error.pilotMessage}"
                },
            )
        }
        // Another pilot's page may have been opened while this was under way.
        if (round == pilotRound) {
            pilot = result
        }
    }

    /** A flight prepared for display, and its thermals and glides if they could be worked out. */
    class Opened(val flight: FlightView, val analysis: FlightAnalysis?)

    /**
     * Fetches a flight's file and prepares it for display, like one of the
     * pilot's own. Nothing is kept. What goes wrong is thrown, to be put
     * into words by [words].
     */
    suspend fun open(flight: PublicFlight): Opened {
        val server = server() ?: throw NoLongerPublic()
        val answer = transport(server + flight.filePath)
        if (answer.status !in 200..299) throw NoLongerPublic()
        return withContext(background) {
            val text = LogbookModel.decode(answer.body)
            // Without the analysis the flight is still shown in full.
            Opened(flightFromFile(text), runCatching { flightAnalysisFromFile(text) }.getOrNull())
        }
    }

    /** The server no longer hands the flight's file out. */
    private class NoLongerPublic : Exception()

    /** Why a flight could not be opened, in words. */
    fun words(error: Exception): String = when (error) {
        is CoreException -> error.pilotMessage
        is NoLongerPublic -> "This flight is no longer public: its pilot took it back, or deleted it."
        else -> "The flight could not be fetched. ${error.pilotMessage}"
    }

    private fun forget() {
        flights = emptyList()
        next = null
        hasMore = false
        listed = null
        problem = null
        pilot = null
        ranked = Ranked()
        known.clear()
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** How long an answer is waited for: a long flight's file is a few megabytes. */
        private const val ANSWER_WITHIN_MILLISECONDS = 30_000
    }
}
