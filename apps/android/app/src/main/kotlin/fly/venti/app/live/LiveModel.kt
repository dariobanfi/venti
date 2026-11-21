package fly.venti.app.live

import fly.venti.core.Coordinate
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.app.wind.contains
import fly.venti.app.wind.grown
import fly.venti.core.Bounds
import fly.venti.core.LiveFollow
import fly.venti.core.LiveFollowView
import fly.venti.core.Credit
import fly.venti.core.LiveGlider
import fly.venti.core.LiveMap
import fly.venti.core.LiveOthers
import fly.venti.core.LiveOthersNetwork
import fly.venti.core.OtherAircraftView
import fly.venti.core.liveAgeInWords
import fly.venti.core.liveLayer
import fly.venti.core.liveMap
import fly.venti.core.liveMapPath
import fly.venti.core.liveOthers
import fly.venti.core.liveOthersPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

/**
 * Who is in the air: the gliders whose pilots show their flight to
 * everyone, from Venti, and the aircraft another network hears (the Open
 * Glider Network), for the part of the map in view. The two are kept apart,
 * as the core's `liveLayer` keeps them: never one record of both.
 *
 * What to ask the server and how to read its answer is the core's. This
 * class sends the request, and again every quarter of a minute while the
 * map is on screen. It also follows the flight of the one glider whose page
 * is open.
 */
class LiveModel(
    /** Where requests wait for the map to come to rest. */
    private val scope: CoroutineScope,
    /**
     * Fetches an address. The app's own networking keeps no answers, and
     * none may be kept here: a position from a cache is a wrong one, and
     * its age would start again from nothing.
     */
    private val transport: WindTransport = { url -> Http.get(url, ANSWER_WITHIN_MILLISECONDS) },
    /** This device's clock, in milliseconds since 1970. */
    private val clock: () -> Long = System::currentTimeMillis,
    /** Where the core reads an answer, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
    /** How often the server is asked again who is in the air. A test asks sooner. */
    private val askEveryMilliseconds: Long = ASK_EVERY_MILLISECONDS,
    /** How often a followed flight is asked for again. */
    private val followEveryMilliseconds: Long = FOLLOW_EVERY_MILLISECONDS,
) {
    /** The gliders in the part of the map in view. */
    var gliders: List<LiveGlider> by mutableStateOf(emptyList())
        private set

    /**
     * When the answer came that they are from. A position's age runs on
     * from there, on this phone's clock.
     */
    var answeredAt: Long by mutableStateOf(clock())
        private set

    /** Whether the server has more than it lists at once. */
    var isTruncated: Boolean by mutableStateOf(false)
        private set

    /**
     * The aircraft of the other networks in the part of the map in view,
     * paragliders and hang gliders first, as the core lists them.
     */
    var others: List<OtherAircraftView> by mutableStateOf(emptyList())
        private set

    /** The credits of the networks that have an aircraft in view: with the map's credits while they are drawn. */
    var othersCredits: List<Credit> by mutableStateOf(emptyList())
        private set

    /** How many are in the air in view, in the core's words: "3 pilots of Venti, 5 others in the air". */
    var countWords: String by mutableStateOf("")
        private set

    /**
     * Whether the map shows too much for the other networks' aircraft:
     * then the core's `othersTooWideWords` stand over it.
     */
    var othersTooWide: Boolean by mutableStateOf(false)
        private set

    /** Whether the other networks are asked at all: while the map's "Pilots" layer is on. */
    var wantsOthers: () -> Boolean = { true }

    /** The other networks' last answer, which the core puts beside Venti's pilots. */
    private var othersAnswer = LiveOthers(networks = emptyList(), listedMinutes = 0u)

    /** The flight being followed: the glider whose page is open, with its track so far. */
    var followed: LiveFollowView? by mutableStateOf(null)
        private set

    /** When the answer came that it is from. */
    var followedAt: Long by mutableStateOf(clock())
        private set

    /** Why a flight cannot be followed, in words. */
    var followProblem: String? by mutableStateOf(null)
        private set

    /** The server to ask, without a slash at its end: the app's (`AppServer`), or a test's. */
    var server: () -> String? = { null }

    /**
     * The area last asked for, from which server, and whether Venti's own
     * pilots were asked for in it (not on a map of more than [WIDEST_MAP]).
     */
    private class Asked(val area: Bounds, val server: String, val venti: Boolean = true)

    private var asked: Asked? = null
    private var waiting: Job? = null

    /** The map says which part of it is in view. */
    fun show(region: Bounds) {
        val server = server()
        // Too wide for the other networks is the core's to say.
        othersTooWide = liveOthersPath(region.south, region.west, region.north, region.east) == null
        val venti = region.east - region.west <= WIDEST_MAP
        if (server == null || (!venti && (othersTooWide || !wantsOthers()))) {
            forget()
            return
        }
        // Inside what was asked for: the next round brings it up to date.
        val asked = asked
        if (asked != null && asked.server == server && asked.venti == venti && asked.area.contains(region)) return
        waiting?.cancel()
        waiting = scope.launch {
            // The map reports every pause of a drag; wait until it rests.
            delay(REST_MILLISECONDS)
            // A little more than is in view, so that a small drag needs no
            // new request; no more than the other networks answer for.
            val grown = region.grown(0.25)
            val area = if (othersTooWide || liveOthersPath(grown.south, grown.west, grown.north, grown.east) != null) grown else region
            load(area, server, venti)
        }
    }

    /**
     * Asks again at once for the area last asked for: when the "Pilots"
     * layer is switched on, so that the other networks' aircraft need not
     * wait for the next round.
     */
    suspend fun again() {
        val asked = asked ?: return
        val server = server() ?: return
        if (asked.server == server) load(asked.area, server, asked.venti)
    }

    /**
     * Asks again every quarter of a minute until the coroutine that runs
     * this is cancelled, which is when the map leaves the screen.
     */
    suspend fun keepCurrent() {
        while (coroutineContext.isActive) {
            delay(askEveryMilliseconds)
            val asked = asked
            val server = server()
            if (asked != null && server != null && asked.server == server) {
                load(asked.area, server, asked.venti)
            }
        }
    }

    /**
     * Asks the server who is in the air in an area: Venti's own pilots
     * (unless [venti] is false) and the aircraft of the other networks.
     * An answer that does not come leaves the gliders where they were:
     * their ages go on counting, which is what is known about them.
     */
    suspend fun load(area: Bounds, server: String, venti: Boolean = true) {
        var answered = false
        try {
            if (venti) {
                val answer = transport(server + liveMapPath(area.south, area.west, area.north, area.east))
                val map = withContext(background) { liveMap(answer.status16, answer.body) }
                // The pilot may have changed the server while this was under way.
                if (server != this.server()) return
                gliders = map.gliders
                isTruncated = map.truncated
                answeredAt = clock()
            } else if (gliders.isNotEmpty()) {
                gliders = emptyList()
                isTruncated = false
            }
            answered = true
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // A server from before live tracks, or none in reach: nothing
            // new is known. The area is remembered, to be asked for again.
        }
        loadOthers(area, server)
        if (server != this.server()) return
        if (answered || asked == null) asked = Asked(area, server, venti)
        layOut()
    }

    /**
     * Asks for the other networks' aircraft in an area, while they are
     * wanted. A server from before them, or an answer that does not come,
     * leaves what was there.
     */
    private suspend fun loadOthers(area: Bounds, server: String) {
        val path = liveOthersPath(area.south, area.west, area.north, area.east)
        if (path == null || !wantsOthers()) {
            othersAnswer = LiveOthers(networks = emptyList(), listedMinutes = othersAnswer.listedMinutes)
            return
        }
        try {
            val answer = transport(server + path)
            val read = withContext(background) { liveOthers(answer.status16, answer.body) }
            if (server == this.server()) othersAnswer = read
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // Nothing new is known of them.
        }
    }

    /** Venti's pilots and the other networks' aircraft side by side, as the core lays them out. */
    private fun layOut() {
        val layer = liveLayer(LiveMap(gliders = gliders, truncated = isTruncated), othersAnswer)
        val drawn = layer.others.filter(::isDrawn)
        others = drawn.flatMap { it.aircraft }
        othersCredits = layer.credits.filter { credit -> drawn.any { it.source == credit.source } }
        countWords = layer.countWords
    }

    /** An aircraft of another network, by its identifier, as the newest answer has it. */
    fun other(id: String): OtherAircraftView? = others.firstOrNull { it.id == id }

    /** The credit of the network an aircraft is heard by. */
    fun creditOf(id: String): Credit? {
        val network = othersAnswer.networks.firstOrNull { network -> network.aircraft.any { it.id == id } } ?: return null
        return network.credit
    }

    /**
     * Who is in the air around a place, for the cockpit's map: some thirty
     * kilometres every way. Nothing of it is kept for the map tab. `null`
     * without a server or an answer.
     */
    suspend fun around(place: Coordinate, reach: Double = 0.35): List<LiveGlider>? {
        val server = server() ?: return null
        return try {
            val path = liveMapPath(place.lat - reach, place.lon - reach * 1.5, place.lat + reach, place.lon + reach * 1.5)
            val answer = transport(server + path)
            withContext(background) { liveMap(answer.status16, answer.body) }.gliders
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            null
        }
    }

    /**
     * Follows one glider's flight until the coroutine that runs this is
     * cancelled, which is when its page closes: the whole track first, then
     * what arrives.
     */
    suspend fun follow(id: String) {
        followed = null
        followProblem = null
        try {
            val server = server() ?: return
            val flight = try {
                LiveFollow(id)
            } catch (_: Exception) {
                return
            }
            while (coroutineContext.isActive) {
                var more = false
                try {
                    val answer = transport(server + flight.path())
                    val view = withContext(background) { flight.receive(answer.status16, answer.body) }
                    followed = view
                    followedAt = clock()
                    followProblem = null
                    more = view.more
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    // Gone from the server: said, and not asked for again.
                    // Any other failure leaves the track as it was.
                    if (flight.isOver()) {
                        followed = null
                        followProblem = error.pilotMessage
                        return
                    }
                }
                if (!more) {
                    delay(followEveryMilliseconds)
                }
            }
        } finally {
            // The page is closed: its line leaves the map.
            followed = null
        }
    }

    /**
     * A glider as the newest answer about it has it: the one that follows
     * its flight, if one does, or else the map's.
     */
    fun glider(id: String): LiveGlider? =
        followed?.glider?.takeIf { it.id == id } ?: gliders.firstOrNull { it.id == id }

    /** How old a glider's position is now, seconds. */
    fun age(glider: LiveGlider, now: Long = clock()): Double {
        val since = if (followed?.glider == glider) followedAt else answeredAt
        return glider.ageS + ((now - since).coerceAtLeast(0) / 1000.0)
    }

    /** The same in words: "Now", "45 s ago", "Landed 9 min ago". */
    fun ageInWords(glider: LiveGlider, now: Long = clock()): String = liveAgeInWords(age(glider, now), glider.landed)

    private fun forget() {
        waiting?.cancel()
        asked = null
        if (gliders.isNotEmpty()) {
            gliders = emptyList()
            isTruncated = false
        }
        othersAnswer = LiveOthers(networks = emptyList(), listedMinutes = othersAnswer.listedMinutes)
        layOut()
    }

    private val fly.venti.app.HttpAnswer.status16: UShort
        get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /**
         * Whether a network's aircraft are drawn. The server lists only
         * what it heard within the last minutes; one that is not connected
         * to the network lists nothing, and nothing is said of it.
         */
        fun isDrawn(network: LiveOthersNetwork): Boolean = network.aircraft.isNotEmpty()

        /** How often the server is asked again. It asks for ten to fifteen seconds. */
        const val ASK_EVERY_MILLISECONDS = 15_000L

        /** How often a followed flight is asked for again. The server asks for five to ten seconds. */
        const val FOLLOW_EVERY_MILLISECONDS = 10_000L

        /**
         * A map that shows more than this many degrees of longitude asks
         * for no gliders: the Alps from end to end are a crowd, not pilots.
         */
        const val WIDEST_MAP = 6.0

        /** How long the map must rest before the server is asked. */
        const val REST_MILLISECONDS = 400L

        /** How long an answer is waited for: the next round is due soon after. */
        private const val ANSWER_WITHIN_MILLISECONDS = 10_000
    }
}
