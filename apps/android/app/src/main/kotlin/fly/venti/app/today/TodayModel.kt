package fly.venti.app.today

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.SiteKind
import fly.venti.core.SiteOutlook
import fly.venti.core.SiteView
import fly.venti.core.forecastPath
import fly.venti.core.siteOutlook
import fly.venti.core.sitesMap
import fly.venti.core.sitesPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.TimeZone
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt

/** A take-off of the Today tab and what the forecast says of it. */
data class TodayPlace(
    val site: SiteView,
    /** How far it is from the place asked about, metres. */
    val distanceM: Double,
    /** Absent until the server has answered, and when it could not. */
    val outlook: SiteOutlook? = null,
    /** Whether the server has been asked and the asking is over, with an answer or without one. */
    val isAsked: Boolean = false,
)

/** What the Today tab has to show. */
sealed interface TodayState {
    /** Nothing asked yet. */
    data object Idle : TodayState

    data object Loading : TodayState

    data object Loaded : TodayState

    /** No Venti to ask: its address is not set. */
    data object NoServer : TodayState

    data class Failed(val message: String) : TodayState
}

/**
 * Today: the take-offs around a place, and what the forecast says about
 * flying from each.
 *
 * What to ask the server, how to read its answers and what a forecast says
 * about a take-off are the core's (`siteOutlook`). This class asks for the
 * sites around the place and for the forecast of each of the nearest
 * take-offs, and keeps what came back.
 */
class TodayModel(
    private val transport: WindTransport = { url -> Http.get(url) },
    /** This device's clock, in milliseconds since 1970: only for how old an answer is. */
    private val deviceClock: () -> Long = System::currentTimeMillis,
    /** Where the core reads an answer, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
    /** How far the clock here is ahead of UTC at a moment, seconds: a forecast's days are this clock's days. */
    private val utcOffsetSeconds: (Long) -> Int = { unixS -> TimeZone.getDefault().getOffset(unixS * 1000) / 1000 },
) {
    var state: TodayState by mutableStateOf(TodayState.Idle)
        private set

    /** The take-offs: those with a name first, and the nearest first. */
    var places: List<TodayPlace> by mutableStateOf(emptyList())
        private set

    /** Whose data the forecasts are. The licences ask for this next to the data. */
    var credits: List<Credit> by mutableStateOf(emptyList())
        private set

    /** The server to ask, without a slash at its end. The app sets this to the one in the settings. */
    var server: () -> String? = { null }

    /**
     * The time by the server's clock, seconds since 1970: the hours that
     * are gone are gone by that clock. The app sets this.
     */
    var clock: suspend () -> Long = { deviceClock() / 1000 }

    /** How long to wait before asking again for a forecast the server is still fetching. */
    var forecastPauseMilliseconds = 5_000L

    /** Where the take-offs were looked for, from which server, and when by this device's clock. */
    private class Asked(val place: Coordinate, val server: String, val at: Long)

    private var asked: Asked? = null

    /** Whether what is shown was asked for this place, of this server, not long ago. */
    fun isFresh(place: Coordinate): Boolean {
        val asked = asked ?: return false
        return state == TodayState.Loaded &&
            asked.server == server() &&
            metresBetween(asked.place, place) < MOVED_M &&
            deviceClock() - asked.at < KEEP_MILLISECONDS
    }

    /** What was asked about single sites, for their own pages, and when by this device's clock. */
    private val askedSites = mutableMapOf<String, Pair<SiteOutlook, Long>>()

    /**
     * What the forecast says about one site, for its own page: what the
     * list already has, what was asked not long ago, or a new answer.
     * `null` without a server and when no answer could be read.
     */
    suspend fun outlook(site: SiteView): SiteOutlook? {
        places.firstOrNull { it.site.id == site.id }?.outlook?.let { return it }
        askedSites[site.id]?.let { (kept, at) -> if (deviceClock() - at < KEEP_MILLISECONDS) return kept }
        val server = server() ?: return null
        val outlook = outlook(site, server, clock()) ?: return null
        askedSites[site.id] = outlook to deviceClock()
        return outlook
    }

    /** Asks for the take-offs around a place and the forecast of each. */
    suspend fun load(around: Coordinate) {
        val server = server()
        if (server == null) {
            places = emptyList()
            credits = emptyList()
            asked = null
            state = TodayState.NoServer
            return
        }
        if (places.isEmpty() || asked?.server != server) {
            places = emptyList()
            credits = emptyList()
            state = TodayState.Loading
        }
        try {
            val found = takeoffs(around, server)
            val nowUnixS = clock()
            // The list at once, each forecast as it arrives.
            places = found
            asked = Asked(around, server, deviceClock())
            state = TodayState.Loaded
            coroutineScope {
                for (takeoff in found) {
                    launch {
                        val outlook = outlook(takeoff.site, server, nowUnixS)
                        if (server != server()) return@launch
                        places = places.map {
                            if (it.site.id == takeoff.site.id) it.copy(outlook = outlook, isAsked = true) else it
                        }
                        if (outlook != null && credits.isEmpty()) {
                            credits = outlook.credits
                        }
                    }
                }
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != server()) return
            state = TodayState.Failed(
                if (error is CoreException) {
                    error.pilotMessage
                } else {
                    "Venti could not be reached for today's forecast. ${error.pilotMessage}"
                },
            )
        }
    }

    /** The take-offs within reach of a place. */
    private suspend fun takeoffs(place: Coordinate, server: String): List<TodayPlace> {
        // A box that holds the circle of the reach.
        val north = REACH_M / METRES_PER_DEGREE
        val east = north / max(cos(Math.toRadians(place.lat)), 0.2)
        val area = Bounds(south = place.lat - north, west = place.lon - east, north = place.lat + north, east = place.lon + east)
        val answer = transport(server + sitesPath(area.south, area.west, area.north, area.east))
        val map = withContext(background) {
            sitesMap(area.south, area.west, area.north, area.east, answer.status.asStatus(), answer.body)
        }
        return nearest(place, map.sites)
    }

    /**
     * What the forecast says about a take-off. The first forecast of a
     * place is one the server has to fetch: it answers "not yet", and is
     * asked again. `null` when no answer could be read.
     */
    private suspend fun outlook(site: SiteView, server: String, nowUnixS: Long): SiteOutlook? = try {
        val address = server + forecastPath(site.position.lat, site.position.lon)
        var answer = transport(address)
        var tries = 1
        while (answer.status == NOT_YET && tries < FORECAST_TRIES) {
            delay(forecastPauseMilliseconds)
            answer = transport(address)
            tries += 1
        }
        withContext(background) {
            siteOutlook(
                answer.status.asStatus(),
                answer.body,
                utcOffsetSeconds(nowUnixS),
                site.wind,
                site.elevationM,
                nowUnixS,
            )
        }
    } catch (error: CancellationException) {
        throw error
    } catch (_: Exception) {
        null
    }

    private fun Int.asStatus(): UShort = coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** How many take-offs are shown: the nearest ones. */
        const val TAKEOFFS_SHOWN = 6

        /** How far from the place a take-off may lie, metres. */
        const val REACH_M = 30_000.0

        /** How long an answer is shown before the server is asked again. */
        const val KEEP_MILLISECONDS = 30 * 60_000L

        /** How far the place may move before the take-offs are looked for again, metres. */
        const val MOVED_M = 5_000.0

        /** How often a forecast the server is still fetching is asked for at most. */
        const val FORECAST_TRIES = 7

        /** The status of an answer that says "not yet, ask again". */
        private const val NOT_YET = 503

        private const val METRES_PER_DEGREE = 111_195.0
        private const val EARTH_RADIUS_M = 6_371_000.0

        /**
         * The take-offs among the sites within reach of a place: those
         * with a name of their own first, since "Take-off" tells a pilot
         * nothing, and among them, and among the rest, the nearest.
         */
        fun nearest(place: Coordinate, sites: List<SiteView>): List<TodayPlace> =
            sites.asSequence()
                .filter { SiteKind.TAKEOFF in it.uses && !it.discouraged }
                .map { TodayPlace(it, metresBetween(place, it.position)) }
                .filter { it.distanceM <= REACH_M }
                .sortedWith(compareBy<TodayPlace> { !it.site.isNamed }.thenBy { it.distanceM })
                .take(TAKEOFFS_SHOWN)
                .toList()

        /** The distance over the ground between two places, metres, on a round earth. */
        fun metresBetween(one: Coordinate, other: Coordinate): Double {
            val dLat = Math.toRadians(other.lat - one.lat)
            val dLon = Math.toRadians(other.lon - one.lon)
            val a = sin(dLat / 2) * sin(dLat / 2) +
                cos(Math.toRadians(one.lat)) * cos(Math.toRadians(other.lat)) * sin(dLon / 2) * sin(dLon / 2)
            return 2 * EARTH_RADIUS_M * asin(sqrt(a.coerceIn(0.0, 1.0)))
        }
    }
}
