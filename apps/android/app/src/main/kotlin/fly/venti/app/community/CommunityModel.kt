package fly.venti.app.community

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.CoreException
import fly.venti.core.FlightAnalysis
import fly.venti.core.FlightView
import fly.venti.core.PublicFlight
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.flightAnalysisFromFile
import fly.venti.core.flightFromFile
import fly.venti.core.publicFlights
import fly.venti.core.publicFlightsPath
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

    /** What to ask the next page with. */
    private var next: String? = null

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
                else -> "The server could not be reached for other pilots' flights. ${error.pilotMessage}"
            }
        } finally {
            isLoading = false
        }
    }

    /** One of the list's flights, by its address on the server. */
    fun flight(id: String): PublicFlight? = flights.firstOrNull { it.id == id }

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
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** How long an answer is waited for: a long flight's file is a few megabytes. */
        private const val ANSWER_WITHIN_MILLISECONDS = 30_000
    }
}
