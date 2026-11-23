package fly.venti.app.wind

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.design.Format
import fly.venti.app.pilotMessage
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.ForecastView
import fly.venti.core.StationHistory
import fly.venti.core.WindMap
import fly.venti.core.WindStation
import fly.venti.core.forecastPath
import fly.venti.core.Registration
import fly.venti.core.ServerStatus
import fly.venti.core.serverStatus
import fly.venti.core.serverStatusPath
import fly.venti.core.StationDay
import fly.venti.core.StationDays
import fly.venti.core.stationDay
import fly.venti.core.stationDayPath
import fly.venti.core.stationHistory
import fly.venti.core.stationHistoryPath
import fly.venti.core.stationMeasuredWords
import fly.venti.core.windMap
import fly.venti.core.windMapPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.IOException
import java.net.URI
import java.net.URISyntaxException
import java.util.TimeZone
import kotlin.math.roundToLong
import fly.venti.core.forecast as readForecast

/** Fetches an address and returns the server's answer. */
typealias WindTransport = suspend (String) -> HttpAnswer

/** What the pilot wrote as the server's address, read. */
sealed interface WindServer {
    /** Nothing was written: the map has no wind. */
    data object None : WindServer

    /** Not an address. */
    data object Unusable : WindServer

    /**
     * An address without encryption that is not on the pilot's own network.
     * The app does not send requests there.
     */
    data object NeedsEncryption : WindServer

    /** A server the app can ask; `url` has no slash at the end. */
    data class Usable(val url: String) : WindServer

    companion object {
        /**
         * Reads an address as people write them: with or without
         * `https://`, with or without a slash at the end. A server on the
         * pilot's own network may be given with `http://`.
         */
        fun read(address: String): WindServer {
            var text = address.trim()
            if (text.isEmpty()) return None
            if ("://" !in text) {
                text = "https://$text"
            }
            text = text.trimEnd('/')
            val url = try {
                URI(text)
            } catch (_: URISyntaxException) {
                return Unusable
            }
            val scheme = url.scheme?.lowercase()
            if ((scheme != "https" && scheme != "http") || url.host.isNullOrEmpty()) return Unusable
            return if (Http.mayBeAsked(text)) Usable(text) else NeedsEncryption
        }
    }
}

/**
 * Live wind for the map, from Venti.
 *
 * What to ask the server and how to read its answers is the core's. This
 * class sends the requests, keeps the newest answer for the part of the map
 * in view, and asks again every minute while the map is on screen.
 */
class WindModel(
    /** Venti's address: the app's server ([fly.venti.app.AppServer]), or a test's. */
    address: String,
    /** Where requests wait for the map to come to rest. */
    private val scope: CoroutineScope,
    private val transport: WindTransport = { url -> Http.get(url) },
    /** This device's clock, in milliseconds since 1970. */
    private val clock: () -> Long = System::currentTimeMillis,
    /** Where the core reads an answer, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
    /** How far this phone's clock is ahead of UTC, in seconds: a forecast's days are that clock's days. */
    private val utcOffsetSeconds: () -> Int = { TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000 },
) {
    /** The stations around the part of the map in view. */
    var stations: List<WindStation> by mutableStateOf(emptyList())
        private set

    /** Whose data they are. The licences ask for this next to the data. */
    var credits: List<Credit> by mutableStateOf(emptyList())
        private set

    /** Whether the area holds more stations than the server lists at once. */
    var isTruncated: Boolean by mutableStateOf(false)
        private set

    /** Why the wind is missing or not current, in words for the pilot. */
    var problem: String? by mutableStateOf(null)
        private set

    /**
     * The core's sentence on the networks among the stations here that have
     * sent nothing new for a while; `null` when every one of them delivers.
     */
    var lateNotice: String? by mutableStateOf(null)
        private set

    /** Whether the server takes sites from pilots, as its health answer says: no until it has said so. */
    var takesSitesFromPilots: Boolean by mutableStateOf(false)
        private set

    /** Whether the server offers signing in with Google, as its health answer says: no until it has said so. */
    var googleSignIn: Boolean by mutableStateOf(false)
        private set

    /** Who may create an account there, as its health answer says; `null` until it has said so. */
    var registration: Registration? by mutableStateOf(null)
        private set

    /** Takes in what the server's health answer says. */
    private fun take(state: ServerStatus) {
        serverClock = (state.timeUnixS - clock() / 1000.0) to clock()
        takesSitesFromPilots = state.takesSitesFromPilots
        googleSignIn = state.googleSignIn
        registration = state.registration
    }

    /**
     * Reads the server's health again: the account screen asks it when it
     * opens signed out, for whether to offer Google and an invite code.
     * Quiet: without an answer what was known stays.
     */
    suspend fun readStatus() {
        val server = url ?: return
        try {
            val answer = transport(server + serverStatusPath())
            take(withContext(background) { serverStatus(answer.status.asStatus(), answer.body) })
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // What was known stays.
        }
    }

    /** Venti's address. Empty only in a test that names no server. */
    val address: String = address

    /** The part of the map in view. */
    private var wanted: Bounds? = null

    /** The area the stations were asked for, and when the answer came. */
    private var answered: Pair<Bounds, Long>? = null

    /** The server's clock minus this device's, in seconds, and when it was read. */
    private var serverClock: Pair<Double, Long>? = null
    private var waiting: Job? = null

    /** What the address is: none, unusable, or a server to ask. */
    val server: WindServer get() = WindServer.read(address)

    private val url: String? get() = (server as? WindServer.Usable)?.url

    /** What the pilot should know about the wind on the map, if anything. */
    val notice: String?
        get() = problem ?: when {
            isTruncated -> "There are more stations here than fit on the map. Zoom in to see them all."
            else -> lateNotice
        }

    /**
     * Whose wind data is on the map, in one line. Their licences ask for
     * the credit next to the data; the full wording is on each station's
     * page.
     */
    val creditLine: String?
        get() = if (credits.isEmpty()) null else "Wind: ${Format.list(credits.map { it.provider })}"

    /**
     * The current time by the server's clock, in seconds since 1970, which
     * the ages of readings are measured against. A phone's clock can be
     * wrong.
     */
    val now: Long get() = (clock() / 1000.0 + (serverClock?.first ?: 0.0)).roundToLong()

    /**
     * The current time by the server's clock, read from the server first if
     * it has not been yet. This phone's own time when there is no server or
     * no answer.
     */
    suspend fun serverNow(): Long {
        readClockIfUnread()
        return now
    }

    /**
     * The server's clock as last read, advanced by the time passed since,
     * in seconds since 1970; read from the server first if it has not been
     * yet. 0 while there is no answer, never this phone's own time: the
     * core then says a comment's day, where an age counted by a wrong
     * phone clock would be days off.
     */
    suspend fun serverTimeOrZero(): Long {
        readClockIfUnread()
        return nowOrZero
    }

    /** The server's clock as last read, advanced; 0 while it is unread. */
    val nowOrZero: Long get() = if (serverClock == null) 0 else now

    /** Reads the server's health when its clock has not been read yet. Quiet: without an answer it stays unread. */
    private suspend fun readClockIfUnread() {
        val server = url
        if (serverClock == null && server != null) {
            try {
                val answer = transport(server + serverStatusPath())
                val state = withContext(background) { serverStatus(answer.status.asStatus(), answer.body) }
                take(state)
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                // Unread, then: asked again next time.
            }
        }
    }

    /**
     * The map says which part of it is in view. The server is asked unless
     * that part is covered by a recent answer.
     */
    fun show(region: Bounds) {
        wanted = region
        if (url == null) return
        val answered = answered
        if (answered != null && !isTruncated && answered.first.contains(region) &&
            clock() - answered.second < REFRESH_MILLISECONDS
        ) {
            return
        }
        waiting?.cancel()
        waiting = scope.launch {
            // The map reports every pause of a drag; wait until it rests.
            delay(REST_MILLISECONDS)
            load()
        }
    }

    /**
     * Keeps the wind current for as long as the caller lives: the map
     * screen runs this while it is on screen.
     */
    suspend fun keepCurrent() {
        while (true) {
            load()
            delay(REFRESH_MILLISECONDS)
        }
    }

    /** Asks the server for the stations around the part of the map in view. */
    suspend fun load() {
        val server = url ?: return
        val region = wanted ?: return
        // Somewhat more than is in view, so a small drag needs no request.
        val area = region.grown(0.25)
        try {
            val read = serverClock
            if (read == null || clock() - read.second >= REFRESH_MILLISECONDS) {
                val answer = transport(server + serverStatusPath())
                val state = withContext(background) { serverStatus(answer.status.asStatus(), answer.body) }
                take(state)
            }
            val answer = transport(server + windMapPath(area.south, area.west, area.north, area.east))
            // Against Venti's clock, or 0 while it is unread: then the core gives no age.
            val map = withContext(background) { windMap(answer.status.asStatus(), answer.body, nowOrZero) }
            // The pilot may have changed the address while this was under way.
            if (server != url) return
            stations = map.stations
            credits = map.credits
            isTruncated = map.truncated
            lateNotice = map.lateNotice
            answered = area to clock()
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != url) return
            problem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "Venti could not be reached for the wind. ${error.pilotMessage}"
            }
            // An arrow must not outlive the wind it stood for.
            val since = answered?.second
            if (since == null || clock() - since > KEEP_WITHOUT_ANSWER_MILLISECONDS) {
                stations = emptyList()
                credits = emptyList()
            }
        }
    }

    /**
     * The stations around the glider and around the place the pilot means
     * to land, for the cockpit's wind on the ground. Some eighty kilometres
     * every way from the glider: the stations of the weather services stand
     * far apart. `null` without a server or an answer. Nothing of it is kept
     * for the map.
     */
    suspend fun stationsAround(place: Coordinate, landing: Coordinate?): WindMap? {
        val server = url ?: return null
        var area = Bounds(south = place.lat - 0.75, west = place.lon - 1.1, north = place.lat + 0.75, east = place.lon + 1.1)
        if (landing != null) {
            area = Bounds(
                south = minOf(area.south, landing.lat - 0.05),
                west = minOf(area.west, landing.lon - 0.07),
                north = maxOf(area.north, landing.lat + 0.05),
                east = maxOf(area.east, landing.lon + 0.07),
            )
        }
        return try {
            // Ages are measured against the server's clock. A pilot who went
            // straight to the cockpit has not read it yet.
            if (serverClock == null) {
                val answer = transport(server + serverStatusPath())
                val state = withContext(background) { serverStatus(answer.status.asStatus(), answer.body) }
                take(state)
            }
            val answer = transport(server + windMapPath(area.south, area.west, area.north, area.east))
            withContext(background) { windMap(answer.status.asStatus(), answer.body, now) }
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            null
        }
    }

    /** What a station measured over the last 24 hours. */
    suspend fun history(station: WindStation): StationHistory {
        val server = url ?: throw IOException("Venti is not connected.")
        val answer = transport(server + stationHistoryPath(station.id))
        val nowUnixS = serverTimeOrZero()
        return withContext(background) { stationHistory(answer.status.asStatus(), answer.body, nowUnixS) }
    }

    /**
     * What a station's page says under its newest reading, in the core's
     * words: its age by Venti's running clock, or its time on the pilot's
     * clock while Venti's clock is unread.
     */
    fun measuredWords(station: WindStation): String = stationMeasuredWords(station, nowOrZero, utcOffsetSeconds())

    /** The span of days a station's page can show, today last, by this phone's clock. */
    fun stationDays(): StationDays = fly.venti.core.stationDays(now, utcOffsetSeconds())

    /** One past day of a station's readings, by its date as the core names it ("2026-11-18"). */
    suspend fun day(station: WindStation, date: String): StationDay {
        val server = url ?: throw IOException("Venti is not connected.")
        val offset = utcOffsetSeconds()
        val answer = transport(server + stationDayPath(station.id, date, offset))
        return withContext(background) { stationDay(answer.status.asStatus(), answer.body, date, offset) }
    }

    /** How long to wait before asking again for a forecast the server is still fetching. */
    var forecastPauseMilliseconds = 5_000L

    /**
     * What the weather models say about a place for today and the next
     * days. The days are those of this phone's clock.
     *
     * The first forecast of a place is one the server has to fetch, and
     * until it has it the server answers "not yet": it is asked again every
     * few seconds, for about half a minute.
     */
    suspend fun forecast(place: Coordinate): ForecastView {
        val server = url ?: throw IOException("Venti is not connected.")
        val address = server + forecastPath(place.lat, place.lon)
        var answer = transport(address)
        var tries = 1
        while (answer.status == NOT_YET && tries < FORECAST_TRIES) {
            delay(forecastPauseMilliseconds)
            answer = transport(address)
            tries += 1
        }
        val body = answer.body
        val status = answer.status.asStatus()
        return withContext(background) { readForecast(status, body, utcOffsetSeconds()) }
    }

    private fun Int.asStatus(): UShort = coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /**
         * How often the wind is asked for again. The server lets an answer
         * be reused for this long, and stations report every ten minutes.
         */
        const val REFRESH_MILLISECONDS = 60_000L

        /**
         * After this long without an answer the stations are taken off the
         * map.
         */
        const val KEEP_WITHOUT_ANSWER_MILLISECONDS = 15 * 60_000L

        /** How long the map must rest before the server is asked. */
        const val REST_MILLISECONDS = 300L

        /** How often a forecast the server is still fetching is asked for at most. */
        const val FORECAST_TRIES = 7

        /** The status of an answer that says "not yet, ask again". */
        private const val NOT_YET = 503
    }
}

/** Whether another box lies wholly inside this one. */
fun Bounds.contains(other: Bounds): Boolean =
    south <= other.south && north >= other.north && west <= other.west && east >= other.east

/** The box widened on every side by a share of its own size. */
fun Bounds.grown(share: Double): Bounds {
    val height = (north - south) * share
    val width = (east - west) * share
    return Bounds(south = south - height, west = west - width, north = north + height, east = east + width)
}
