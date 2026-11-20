package fly.venti.app.live

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
import fly.venti.core.LiveGlider
import fly.venti.core.liveAgeInWords
import fly.venti.core.liveMap
import fly.venti.core.liveMapPath
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
 * everyone, from the Venti server the pilot has set, for the part of the
 * map in view.
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

    /** The flight being followed: the glider whose page is open, with its track so far. */
    var followed: LiveFollowView? by mutableStateOf(null)
        private set

    /** When the answer came that it is from. */
    var followedAt: Long by mutableStateOf(clock())
        private set

    /** Why a flight cannot be followed, in words. */
    var followProblem: String? by mutableStateOf(null)
        private set

    /** The server to ask, without a slash at its end. The app sets this to the one in the settings. */
    var server: () -> String? = { null }

    /** The area last asked for, and from which server. */
    private class Asked(val area: Bounds, val server: String)

    private var asked: Asked? = null
    private var waiting: Job? = null

    /** The map says which part of it is in view. */
    fun show(region: Bounds) {
        val server = server()
        if (server == null || region.east - region.west > WIDEST_MAP) {
            forget()
            return
        }
        // Inside what was asked for: the next round brings it up to date.
        val asked = asked
        if (asked != null && asked.server == server && asked.area.contains(region)) return
        waiting?.cancel()
        waiting = scope.launch {
            // The map reports every pause of a drag; wait until it rests.
            delay(REST_MILLISECONDS)
            // A little more than is in view, so that a small drag needs no
            // new request.
            load(region.grown(0.25), server)
        }
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
                load(asked.area, server)
            }
        }
    }

    /**
     * Asks the server who is in the air in an area. An answer that does
     * not come leaves the gliders where they were: their ages go on
     * counting, which is what is known about them.
     */
    suspend fun load(area: Bounds, server: String) {
        try {
            val answer = transport(server + liveMapPath(area.south, area.west, area.north, area.east))
            val map = withContext(background) { liveMap(answer.status16, answer.body) }
            // The pilot may have changed the server while this was under way.
            if (server != this.server()) return
            gliders = map.gliders
            isTruncated = map.truncated
            answeredAt = clock()
            asked = Asked(area, server)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // A server from before live tracks, or none in reach: nothing
            // new is known. The area is remembered, to be asked for again.
            if (asked == null && server == this.server()) {
                asked = Asked(area, server)
            }
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
    }

    private val fly.venti.app.HttpAnswer.status16: UShort
        get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
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
