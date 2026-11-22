package fly.venti.app.community

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
import fly.venti.core.FoundPilotLine
import fly.venti.core.PilotStatistics
import fly.venti.core.PublicFlight
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.flightAnalysisFromFile
import fly.venti.core.flightFromFile
import fly.venti.core.pilotPage
import fly.venti.core.pilotFlightsPath
import fly.venti.core.pilotPagePath
import fly.venti.core.pilotsWords
import fly.venti.core.pilotStatistics
import fly.venti.core.pilotStatisticsPath
import fly.venti.core.pilotsFound
import fly.venti.core.pilotsSearchPath
import fly.venti.core.publicFlight
import fly.venti.core.publicFlightPath
import fly.venti.core.publicFlights
import fly.venti.core.publicFlightsPath
import fly.venti.core.RankedPilot
import fly.venti.core.RankingMonthChoice
import fly.venti.core.RankingSeason
import fly.venti.core.RankingYearChoice
import fly.venti.core.ranking
import fly.venti.core.rankingMonthPath
import fly.venti.core.rankingPath
import fly.venti.core.rankingSeasons
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
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
    /**
     * How long the search for pilots waits for the next letter before it
     * asks: the server counts every search against a budget of reads.
     */
    private val searchPauseMilliseconds: Long = SEARCH_PAUSE_MILLISECONDS,
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

    /** Venti's time, seconds since 1970, for the ranking's years; 0 while it is unknown. The app sets this. */
    var clock: suspend () -> Long = { System.currentTimeMillis() / 1000 }

    /**
     * One pilot's public flights, the newest first: the page reached from a
     * flight of that pilot's. Kept here, not in its screen, so that it is
     * still there when the pilot comes back from one of its flights.
     */
    data class PilotFlights(
        val handle: String,
        /** The season the list is narrowed to: its first second and the first after it. Neither for all years. */
        val fromUnixS: Long? = null,
        val toUnixS: Long? = null,
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
        /**
         * The months of that season that have a ranking, the newest first,
         * as the core names them. None for all years, and none from a
         * server that does not say.
         */
        val months: List<RankingMonthChoice> = emptyList(),
        /** The month the ranking is of, as the server writes it: "2026-05". Absent for the whole season. */
        val month: String? = null,
        /** The span the ranking is of, in the core's words: "May 2026", "2026", "All years". */
        val title: String = "",
        /** The pilots, the one with the most points first. */
        val pilots: List<RankedPilot> = emptyList(),
        /** The rule in a sentence, for under the list. */
        val rule: String = "",
        val isLoading: Boolean = false,
        /** Why it is missing, in words for the pilot. */
        val problem: String? = null,
        /** The years that have a ranking, each with its months, as the server last said them. */
        val periods: List<RankingYearChoice> = emptyList(),
    )

    /** The ranking of the season last asked for. */
    var ranked: Ranked by mutableStateOf(Ranked())
        private set

    /** What a search for pilots by name found. */
    data class PilotSearch(
        /** The text it is of, as it was typed. */
        val text: String = "",
        /** The pilots, in the core's order. */
        val pilots: List<FoundPilotLine> = emptyList(),
        /**
         * The result in a line of the core's, its line when nobody was
         * found, or why nothing was asked or answered. Empty before any.
         */
        val words: String = "",
        val isLoading: Boolean = false,
    )

    /** The search for pilots last typed. */
    var search: PilotSearch by mutableStateOf(PilotSearch())
        private set

    /** What a pilot's public flights add up to, as the core reads the server's sums. */
    data class PilotTotals(
        val handle: String,
        /** Absent until the server has answered, and when it could not be asked. */
        val statistics: PilotStatistics? = null,
        val isLoading: Boolean = false,
        /** Why they are missing, in words. */
        val problem: String? = null,
    )

    /** The totals of the pilot last looked at. */
    var pilotTotals: PilotTotals? by mutableStateOf(null)
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
     * Fetches the ranking of a season, by the name the core gives it, or
     * of one of its months, as the server writes it ("2026-05"). Without a
     * season it is the one last shown, or the newest; without a month the
     * whole season, and with neither what was last shown, asked again.
     * The years and their months are the server's once it has said them;
     * until then the years are the core's, from the time now.
     */
    suspend fun rank(
        season: String? = null,
        month: String? = if (season == null) ranked.month else null,
        nowUnixS: Long? = null,
    ) {
        val nowUnixS = nowUnixS ?: clock()
        val server = server()
        if (server == null) {
            ranked = Ranked()
            return
        }
        val wanted = season ?: ranked.season
        val seasons = seasons(ranked.periods, wanted, nowUnixS)
        val chosen = seasons.firstOrNull { it.name == wanted } ?: seasons.firstOrNull() ?: return
        // Another span's pilots are not this one's.
        val shown = if (chosen.name == ranked.season && month == ranked.month) ranked else Ranked(periods = ranked.periods)
        val months = months(ranked.periods, chosen.name)
        ranked = shown.copy(
            seasons = seasons,
            season = chosen.name,
            months = months,
            month = month,
            title = months.firstOrNull { it.month == month }?.title ?: chosen.name,
            problem = null,
            isLoading = true,
        )
        try {
            val path = if (month != null) rankingMonthPath(null, month) else rankingPath(null, chosen.fromUnixS, chosen.toUnixS)
            val answer = transport(server + path)
            val ranking = withContext(background) { ranking(answer.status16, answer.body) }
            // The pilot may have chosen another span or server while this
            // was under way: that question's answer sets the rest.
            if (server != this.server() || ranked.season != chosen.name || ranked.month != month) return
            ranking.pilots.forEach { pilot -> known += pilot.flights.associateBy { it.id } }
            // A server that does not name its years leaves those there were.
            val periods = ranking.periods.ifEmpty { ranked.periods }
            val named = months(periods, chosen.name)
            ranked = ranked.copy(
                seasons = seasons(periods, chosen.name, nowUnixS),
                months = named,
                title = ranking.period.title,
                pilots = ranking.pilots,
                rule = ranking.rule,
                isLoading = false,
                periods = periods,
            )
        } catch (error: CancellationException) {
            if (ranked.season == chosen.name && ranked.month == month) ranked = ranked.copy(isLoading = false)
            throw error
        } catch (error: Exception) {
            if (ranked.season != chosen.name || ranked.month != month) return
            ranked = ranked.copy(
                isLoading = false,
                problem = when (error) {
                    is CoreException -> error.pilotMessage
                    else -> pilotsWords().unreachable
                },
            )
        }
    }

    /**
     * The seasons a ranking is offered for: the years the server says have
     * one, as many of the newest as the row has room for, and all years;
     * the core's years from the time now while the server has not said.
     * The season that is shown stays among them.
     */
    private fun seasons(periods: List<RankingYearChoice>, shown: String, nowUnixS: Long): List<RankingSeason> {
        val named = rankingSeasons(nowUnixS)
        if (periods.isEmpty()) return named
        val years = periods.take(YEARS_OFFERED).map { RankingSeason(it.name, it.fromUnixS, it.toUnixS) }
        val all = named.filter { it.fromUnixS == null }
        val kept = named.filter { it.name == shown && years.none { year -> year.name == shown } && it.fromUnixS != null }
        return kept + years + all
    }

    /** The months of a season that have a ranking, as the server said them. */
    private fun months(periods: List<RankingYearChoice>, season: String): List<RankingMonthChoice> =
        periods.firstOrNull { it.name == season }?.months.orEmpty()

    /**
     * Looks for the pilots whose name or handle holds a text, once the
     * pilot has stopped typing for a moment: whoever calls this for the
     * next letter cancels the call for the one before. Fewer than two
     * letters are not asked for, and the core says so; no text is no
     * search.
     */
    suspend fun findPilots(text: String) {
        val server = server()
        if (server == null || text.isBlank()) {
            search = PilotSearch(text)
            return
        }
        val path = try {
            pilotsSearchPath(text)
        } catch (error: CoreException) {
            search = PilotSearch(text, words = error.pilotMessage)
            return
        }
        // What was found for the letters before stays until this is answered.
        search = search.copy(text = text, words = if (search.pilots.isEmpty()) "" else search.words, isLoading = true)
        try {
            delay(searchPauseMilliseconds)
            val answer = transport(server + path)
            val found = withContext(background) { pilotsFound(answer.status16, answer.body) }
            if (search.text != text) return
            search = PilotSearch(text, found.pilots, found.words)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (search.text != text) return
            search = PilotSearch(
                text,
                words = when (error) {
                    is CoreException -> error.pilotMessage
                    else -> pilotsWords().unreachable
                },
            )
        }
    }

    /**
     * Fetches what a pilot's public flights add up to, unless it is there
     * already.
     */
    suspend fun showPilotTotals(handle: String) {
        val shown = pilotTotals
        if (shown != null && shown.handle == handle && shown.statistics != null) return
        pilotTotals = PilotTotals(handle, isLoading = true)
        val result = try {
            val server = server() ?: throw NoLongerPublic()
            val answer = transport(server + withContext(background) { pilotStatisticsPath(handle) })
            PilotTotals(handle, withContext(background) { pilotStatistics(answer.status16, answer.body) })
        } catch (error: CancellationException) {
            // Whoever asked has left the page. The next to look at it asks again.
            if (pilotTotals?.handle == handle) pilotTotals = null
            throw error
        } catch (error: Exception) {
            PilotTotals(
                handle,
                problem = when (error) {
                    is CoreException -> error.pilotMessage
                    is NoLongerPublic -> "Venti is not connected."
                    else -> pilotsWords().unreachable
                },
            )
        }
        // Another pilot's page may have been opened while this was under way.
        if (pilotTotals?.handle == handle) pilotTotals = result
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

    /** Flights another list has shown, the feed's for one: kept so that their pages can open. */
    fun know(shown: List<PublicFlight>) {
        known += shown.associateBy { it.id }
    }

    /** A flight one of the lists has shown, by its address on the server. */
    fun flight(id: String): PublicFlight? = known[id]

    /**
     * A public flight by its address on the server, as a list shows it:
     * the one a list has shown, or asked for alone. Kept like those of a
     * list, so that its page can open. What goes wrong is thrown, to be put
     * into words by [words].
     */
    suspend fun publicFlightById(id: String): PublicFlight {
        known[id]?.let { return it }
        val server = server() ?: throw NoLongerPublic()
        val answer = transport(server + withContext(background) { publicFlightPath(id) })
        val flight = withContext(background) { publicFlight(answer.status16, answer.body) }
        known[flight.id] = flight
        return flight
    }

    /**
     * Shows a pilot's public flights from their top, unless they are there
     * already: all of them, or those of one season, by the bounds the
     * core's totals give it.
     */
    suspend fun showPilot(handle: String, fromUnixS: Long? = null, toUnixS: Long? = null) {
        val shown = pilot
        val isSame = shown != null && shown.handle == handle && shown.fromUnixS == fromUnixS && shown.toUnixS == toUnixS
        if (isSame && shown.isLoaded && shown.problem == null) return
        pilot = PilotFlights(handle, fromUnixS, toUnixS)
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
            // All years are the pilot's page; a season is the public flights' list, narrowed.
            val (flights, next) = if (shown.fromUnixS == null && shown.toUnixS == null) {
                val answer = transport(server + pilotPagePath(shown.handle, shown.next))
                withContext(background) { pilotPage(answer.status16, answer.body) }.let { it.flights to it.next }
            } else {
                val path = withContext(background) { pilotFlightsPath(shown.handle, shown.fromUnixS, shown.toUnixS, shown.next) }
                val answer = transport(server + path)
                withContext(background) { publicFlights(answer.status16, answer.body) }.let { it.flights to it.next }
            }
            known += flights.associateBy { it.id }
            shown.copy(flights = shown.flights + flights, next = next, isLoaded = true, problem = null)
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
                    else -> pilotsWords().unreachable
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
        pilotTotals = null
        search = PilotSearch()
        ranked = Ranked()
        known.clear()
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** How long an answer is waited for: a long flight's file is a few megabytes. */
        private const val ANSWER_WITHIN_MILLISECONDS = 30_000

        /** How long the search waits for the next letter. */
        const val SEARCH_PAUSE_MILLISECONDS = 350L

        /** How many years the ranking's row of seasons has room for, beside all years. */
        private const val YEARS_OFFERED = 4
    }
}
