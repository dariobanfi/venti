package fly.venti.app.wind

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.design.Format
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.Credit
import fly.venti.core.ForecastView
import fly.venti.core.StationHistory
import fly.venti.core.WindStation
import fly.venti.core.forecastPath
import fly.venti.core.serverStatus
import fly.venti.core.serverStatusPath
import fly.venti.core.stationHistory
import fly.venti.core.stationHistoryPath
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
 * Live wind for the map, from the Venti server the pilot has set.
 *
 * What to ask the server and how to read its answers is the core's. This
 * class sends the requests, keeps the newest answer for the part of the map
 * in view, and asks again every minute while the map is on screen.
 */
class WindModel(
    /** Where the address is kept. Without it the address is not remembered: a test's is given. */
    private val remembered: Preferences?,
    address: String = remembered?.server.orEmpty(),
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

    /** The providers the server has not heard from lately. */
    var lateProviders: List<String> by mutableStateOf(emptyList())
        private set

    /** The server's address as the pilot wrote it. Empty for none. */
    var address: String by mutableStateOf(address)
        private set

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
            lateProviders.isNotEmpty() -> "No new wind data from ${Format.list(lateProviders)} for a while."
            else -> null
        }

    /**
     * Whose wind data is on the map, in one line. Their licences ask for
     * the credit next to the data; the full wording is on each station's
     * page.
     */
    val creditLine: String?
        get() = if (credits.isEmpty()) null else "Wind: ${Format.list(credits.map { it.provider })}"

    /** What the address field in the settings means, and whether what is in it is usable. */
    val addressNote: String
        get() = when (server) {
            WindServer.None ->
                "The address of a Venti server. The map shows the wind stations it collects, and the " +
                    "assistant can look them up. Without one, the map has no wind."
            WindServer.Unusable -> "This is not an address the app can use. It should look like wind.example.org."
            WindServer.NeedsEncryption ->
                "A server on the internet is asked over https only. An address that starts with " +
                    "http:// works for a server on your own network."
            is WindServer.Usable ->
                problem ?: "The map shows the wind stations this server collects, and the assistant can look them up."
        }

    /**
     * The current time by the server's clock, in seconds since 1970, which
     * the ages of readings are measured against. A phone's clock can be
     * wrong.
     */
    val now: Long get() = (clock() / 1000.0 + (serverClock?.first ?: 0.0)).roundToLong()

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
                serverClock = (state.timeUnixS - clock() / 1000.0) to clock()
                lateProviders = state.lateProviders
            }
            val answer = transport(server + windMapPath(area.south, area.west, area.north, area.east))
            val map = withContext(background) { windMap(answer.status.asStatus(), answer.body, now) }
            // The pilot may have changed the address while this was under way.
            if (server != url) return
            stations = map.stations
            credits = map.credits
            isTruncated = map.truncated
            answered = area to clock()
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != url) return
            problem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "The wind server could not be reached. ${error.pilotMessage}"
            }
            // An arrow must not outlive the wind it stood for.
            val since = answered?.second
            if (since == null || clock() - since > KEEP_WITHOUT_ANSWER_MILLISECONDS) {
                stations = emptyList()
                credits = emptyList()
            }
        }
    }

    /** What a station measured over the last 24 hours. */
    suspend fun history(station: WindStation): StationHistory {
        val server = url ?: throw IOException("No wind server is set.")
        val answer = transport(server + stationHistoryPath(station.id))
        return withContext(background) { stationHistory(answer.status.asStatus(), answer.body, now) }
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
        val server = url ?: throw IOException("No Venti server is set.")
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

    /** Changes the server, and drops everything learned from the old one. */
    fun use(address: String) {
        if (address == this.address) return
        this.address = address
        remembered?.server = address
        waiting?.cancel()
        stations = emptyList()
        credits = emptyList()
        isTruncated = false
        problem = null
        lateProviders = emptyList()
        answered = null
        serverClock = null
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
