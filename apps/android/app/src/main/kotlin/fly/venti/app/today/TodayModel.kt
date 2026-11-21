package fly.venti.app.today

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.app.sites.KeptSitesModel
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.SiteOutlook
import fly.venti.core.SiteView
import fly.venti.core.SitesMap
import fly.venti.core.forecastPath
import fly.venti.core.siteOutlook
import fly.venti.core.sitesMap
import fly.venti.core.sitesPath
import fly.venti.core.todayArea
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
import kotlin.math.sin
import kotlin.math.sqrt

/** A take-off of the Today tab and what the forecast says of it. */
data class TodayPlace(
    val site: SiteView,
    /** Whether it is one of the pilot's own, as the core says. */
    val isKept: Boolean = false,
    /** How far it is from the place asked about, metres, as the core measured it. */
    val distanceM: Double? = null,
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

    data class Failed(val message: String) : TodayState
}

/**
 * Today: the pilot's own take-offs and those around a place, and what the
 * forecast says about flying from each.
 *
 * Which take-offs, how many and in which order is the core's
 * (`todayArea`, `todayTakeoffs`, `todayOrder`), and so is what to ask the
 * server, how to read its answers and what a forecast says about a
 * take-off (`siteOutlook`). This class asks for the sites of the area the
 * core names, hands the answer to the core, asks for the forecast of each
 * take-off it lists, and keeps what came back.
 */
class TodayModel(
    /** The pilot's own take-offs, which the core keeps on the phone and lists first. */
    private val kept: KeptSitesModel,
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

    /**
     * The take-offs as the core lists them: the pilot's own first, then
     * those around the place. When Venti gave no answer they are the
     * pilot's own alone, and [state] says what failed.
     */
    var places: List<TodayPlace> by mutableStateOf(emptyList())
        private set

    /** What the core calls the pilot's own take-offs: "Your take-offs". */
    var keptTitle: String by mutableStateOf("")
        private set

    /** What the core calls the others: "Within 30 km". */
    var nearTitle: String by mutableStateOf("")
        private set

    /** The core's line when the pilot has more take-offs than Today lists. Empty otherwise. */
    var keptMore: String by mutableStateOf("")
        private set

    /** Whose data the take-offs are, one credit for each source in the list. */
    var siteCredits: List<Credit> by mutableStateOf(emptyList())
        private set

    /** The date of the sites' data in the server's answer, seconds since 1970, if it gave one. */
    var sitesAsOf: Long? by mutableStateOf(null)
        private set

    /** Whose data the forecasts are. The licences ask for this next to the data. */
    var credits: List<Credit> by mutableStateOf(emptyList())
        private set

    /** The server to ask, without a slash at its end: the app's (`AppServer`), or a test's. */
    var server: () -> String? = { null }

    /**
     * The time by the server's clock, seconds since 1970: the hours that
     * are gone are gone by that clock. The app sets this.
     */
    var clock: suspend () -> Long = { deviceClock() / 1000 }

    /** How long to wait before asking again for a forecast the server is still fetching. */
    var forecastPauseMilliseconds = 5_000L

    /**
     * Where the take-offs were looked for, from which server, when by this
     * device's clock, what the server answered, and how often the pilot
     * had kept or forgotten a site by then.
     */
    private class Asked(val place: Coordinate, val server: String, val at: Long, val map: SitesMap, val changes: Int)

    private var asked: Asked? = null

    /** Whether what is shown was asked for this place, of this server, not long ago, with the take-offs the pilot keeps now. */
    fun isFresh(place: Coordinate): Boolean {
        val asked = asked ?: return false
        return isAnswerFor(asked, place) && asked.changes == kept.changes
    }

    private fun isAnswerFor(asked: Asked, place: Coordinate): Boolean =
        state == TodayState.Loaded &&
            asked.server == server() &&
            metresBetween(asked.place, place) < MOVED_M &&
            deviceClock() - asked.at < KEEP_MILLISECONDS

    /**
     * What the screen calls whenever it is drawn for a place: nothing when
     * what is shown is fresh; the core's list anew, from the answer at
     * hand, when the pilot has kept or forgotten a take-off since; else
     * the server is asked.
     */
    suspend fun show(place: Coordinate) {
        val asked = asked
        when {
            asked == null || !isAnswerFor(asked, place) -> load(place)
            asked.changes != kept.changes -> {
                val changes = kept.changes
                if (list(asked.place, asked.map, failure = null)) {
                    this.asked = Asked(asked.place, asked.server, asked.at, asked.map, changes)
                    askForecasts(places.filter { it.outlook == null }, asked.server)
                }
            }
        }
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

    /** Asks the server for the sites around a place, the core for the take-offs, and the server for the forecast of each. */
    suspend fun load(around: Coordinate) {
        val server = server()
        val changes = kept.changes
        if (places.isEmpty() || asked?.server != server) {
            places = emptyList()
            credits = emptyList()
            state = TodayState.Loading
        }
        asked = null
        if (server == null) {
            // The app always has Venti; only a test can be without one, and
            // that reads as Venti not reached. The pilot's own are listed all the same.
            list(around, near = null, failure = UNREACHABLE)
            return
        }
        val map = try {
            sitesAround(around, server)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != server()) return
            val failure = if (error is CoreException) error.pilotMessage else "$UNREACHABLE ${error.pilotMessage}"
            list(around, near = null, failure = failure)
            return
        }
        if (server != server()) return
        // The list at once, each forecast as it arrives.
        if (!list(around, map, failure = null)) return
        asked = Asked(around, server, deviceClock(), map, changes)
        askForecasts(places, server)
    }

    /**
     * Takes the take-offs from the core: those of the server's answer
     * after the pilot's own, or the pilot's own alone when Venti gave no
     * answer, with what failed. What the forecast said of a take-off that
     * was listed before stays with it. Says whether there is an answer to
     * go on from.
     */
    private suspend fun list(around: Coordinate, near: SitesMap?, failure: String?): Boolean {
        val listed = try {
            kept.todayTakeoffs(around, near)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            places = emptyList()
            state = TodayState.Failed(failure ?: error.pilotMessage)
            return false
        }
        val had = places.associateBy { it.site.id }
        places = listed.takeoffs.map { takeoff ->
            val before = had[takeoff.site.id]
            TodayPlace(
                site = takeoff.site,
                isKept = takeoff.isKept,
                distanceM = takeoff.distanceM,
                outlook = before?.outlook,
                // Without an answer from Venti no forecast is asked for either.
                isAsked = failure != null || before?.isAsked == true,
            )
        }
        keptTitle = listed.keptTitle
        nearTitle = listed.nearTitle
        keptMore = listed.keptMore
        siteCredits = listed.credits
        sitesAsOf = near?.asOfUnixS
        state = if (failure == null) TodayState.Loaded else TodayState.Failed(failure)
        return failure == null
    }

    /** Asks for the forecast of each of these take-offs, and keeps each as it arrives. */
    private suspend fun askForecasts(takeoffs: List<TodayPlace>, server: String) {
        if (takeoffs.isEmpty()) return
        val nowUnixS = clock()
        coroutineScope {
            for (takeoff in takeoffs) {
                launch {
                    val outlook = outlook(takeoff.site, server, nowUnixS)
                    if (server != server()) return@launch
                    places = places.map {
                        if (it.site.id == takeoff.site.id) it.copy(outlook = outlook ?: it.outlook, isAsked = true) else it
                    }
                    if (outlook != null && credits.isEmpty()) {
                        credits = outlook.credits
                    }
                }
            }
        }
    }

    /** The server's sites for the area the core asks Today's take-offs of. */
    private suspend fun sitesAround(place: Coordinate, server: String): SitesMap {
        val area = todayArea(place.lat, place.lon)
        val answer = transport(server + sitesPath(area.south, area.west, area.north, area.east))
        return withContext(background) {
            sitesMap(area.south, area.west, area.north, area.east, answer.status.asStatus(), answer.body)
        }
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
        /** How long an answer is shown before the server is asked again. */
        const val KEEP_MILLISECONDS = 30 * 60_000L

        /** How far the place may move before the take-offs are looked for again, metres. */
        const val MOVED_M = 5_000.0

        /** How often a forecast the server is still fetching is asked for at most. */
        const val FORECAST_TRIES = 7

        /** The status of an answer that says "not yet, ask again". */
        private const val NOT_YET = 503

        private const val EARTH_RADIUS_M = 6_371_000.0

        /** The distance over the ground between two places, metres, on a round earth: for whether the place has moved. */
        fun metresBetween(one: Coordinate, other: Coordinate): Double {
            val dLat = Math.toRadians(other.lat - one.lat)
            val dLon = Math.toRadians(other.lon - one.lon)
            val a = sin(dLat / 2) * sin(dLat / 2) +
                cos(Math.toRadians(one.lat)) * cos(Math.toRadians(other.lat)) * sin(dLon / 2) * sin(dLon / 2)
            return 2 * EARTH_RADIUS_M * asin(sqrt(a.coerceIn(0.0, 1.0)))
        }
    }
}

/** Said when Venti does not answer, before what the phone says of why. */
private const val UNREACHABLE = "Venti could not be reached for today's forecast."
