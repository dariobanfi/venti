package fly.venti.app.forecast

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.design.Format
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.ThermalMap
import fly.venti.core.ThermalMapCell
import fly.venti.core.ThermalMapDay
import fly.venti.core.thermalMap
import fly.venti.core.thermalMapPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.util.TimeZone

/** What the card of the forecast map has to show. */
sealed interface ThermalMapState {
    /** The map is being asked for, and none was fetched before. */
    data object Asking : ThermalMapState

    /** A map, and the day of it the pilot looks at. */
    data class There(val map: ThermalMap, val day: ThermalMapDay) : ThermalMapState

    /** No map to show, and why, in words for the pilot. */
    data class Failed(val message: String) : ThermalMapState
}

/**
 * The kind of day, square by square, for the map tab's "Forecast" layer,
 * from the Venti server the pilot has set.
 *
 * What to ask the server and how to read its answer are the core's
 * (`thermalMapPath`, `thermalMap`): which day is today, the colour step of
 * each square and every word on the card come from there. This class asks,
 * keeps the newest map of this run of the app, and remembers which day the
 * pilot looks at. A map that was fetched stays when a later asking fails:
 * the card shows its run's time, so an old map is never shown as new.
 */
class ThermalMapModel(
    private val transport: WindTransport = { url -> Http.get(url) },
    /** This device's clock, in milliseconds since 1970: only for how old an answer is. */
    private val deviceClock: () -> Long = System::currentTimeMillis,
    /** Where the core reads an answer, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
    /** How far the clock here is ahead of UTC at a moment, seconds: the map's days are this clock's days. */
    private val utcOffsetSeconds: (Long) -> Int = { unixS -> TimeZone.getDefault().getOffset(unixS * 1000) / 1000 },
) {
    /** The newest map read in this run of the app, from the server set now. */
    var map: ThermalMap? by mutableStateOf(null)
        private set

    /** Whether the server is being asked. */
    var isAsking: Boolean by mutableStateOf(false)
        private set

    /** Why the last asking brought no map, in words for the pilot; `null` after one that did. */
    var problem: String? by mutableStateOf(null)
        private set

    /** The day the pilot chose, by its date; `null` for the first the map has. */
    var chosenDate: String? by mutableStateOf(null)
        private set

    /** The server to ask, without a slash at its end. The app sets this to the one in the settings. */
    var server: () -> String? = { null }

    /** The time by the server's clock, seconds since 1970: it decides which day is today. The app sets this. */
    var clock: suspend () -> Long = { deviceClock() / 1000 }

    /** Which server the map came from, and when by this device's clock. */
    private class Fetched(val server: String, val at: Long)

    private var fetched: Fetched? = null

    /** Which server was last asked, and when by this device's clock, whatever it answered. */
    private var tried: Fetched? = null

    /** One asking at a time: the layer's own and the pilot's "Try again". */
    private val asking = Mutex()

    /** The day shown: the one chosen, or the first the map has. */
    val day: ThermalMapDay?
        get() = map?.days?.let { days -> days.firstOrNull { it.date == chosenDate } ?: days.firstOrNull() }

    /** What the card shows. */
    val state: ThermalMapState
        get() {
            val map = map
            val day = day
            return when {
                map != null && day != null -> ThermalMapState.There(map, day)
                isAsking -> ThermalMapState.Asking
                problem != null -> ThermalMapState.Failed(checkNotNull(problem))
                map != null -> ThermalMapState.Failed(ThermalMapWords.NO_DAYS)
                else -> ThermalMapState.Asking
            }
        }

    /** Whose forecast the map is, in one line for the map's lower edge; `null` without a map. */
    val creditLine: String?
        get() = map?.credits?.takeIf { it.isNotEmpty() }?.let { "Forecast: ${Format.list(it.map { credit -> credit.provider })}" }

    /** Shows another day of the map. */
    fun choose(date: String) {
        chosenDate = date
    }

    /**
     * Whether the server is to be asked now: there is no map from it yet,
     * or the one there is is older than half an hour. After an asking that
     * failed, not before a few minutes have passed; "Try again" asks at once.
     */
    fun isDue(): Boolean {
        val server = server() ?: return false
        val now = deviceClock()
        // A server not asked yet is asked at once; one that failed, after a pause.
        val tried = tried?.takeIf { it.server == server } ?: return true
        if (now - tried.at < RETRY_MILLISECONDS && tried.at != fetched?.at) return false
        val fetched = fetched?.takeIf { it.server == server } ?: return true
        return now - fetched.at >= KEEP_MILLISECONDS
    }

    /**
     * Asks the server whenever it is due, for as long as the layer is on
     * and the map in view: when it is switched on, and again once the map
     * is half an hour old.
     */
    suspend fun keepCurrent() {
        while (true) {
            if (isDue()) load()
            delay(CHECK_MILLISECONDS)
        }
    }

    /** Asks the server for the map now. */
    suspend fun load() = asking.withLock {
        val server = server()
        if (server == null) {
            forget()
            return@withLock
        }
        // Another server's map is not this one's.
        if (fetched?.let { it.server != server } == true) {
            forget()
        }
        isAsking = true
        val at = deviceClock()
        tried = Fetched(server, at)
        try {
            val answer = transport(server + thermalMapPath())
            val nowUnixS = clock()
            val read = withContext(background) {
                thermalMap(
                    answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(),
                    answer.body,
                    utcOffsetSeconds(nowUnixS),
                    nowUnixS,
                )
            }
            // The pilot may have changed the server while this was under way.
            if (server != this.server()) return@withLock
            map = read
            fetched = Fetched(server, at)
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != this.server()) return@withLock
            problem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "${ThermalMapWords.UNREACHABLE} ${error.pilotMessage}"
            }
        } finally {
            isAsking = false
        }
    }

    private fun forget() {
        map = null
        fetched = null
        tried = null
        problem = null
        chosenDate = null
    }

    companion object {
        /** How long a map is shown before the server is asked again. */
        const val KEEP_MILLISECONDS = 30 * 60_000L

        /** How long after an asking that failed the layer asks again by itself. */
        const val RETRY_MILLISECONDS = 5 * 60_000L

        /** How often the layer looks whether the map is due. */
        const val CHECK_MILLISECONDS = 60_000L

        /** Whether a day has a square in a part of the map. */
        fun hasSquaresIn(day: ThermalMapDay, region: Bounds): Boolean =
            day.cells.any { cell ->
                cell.south < region.north && cell.north > region.south && cell.west < region.east && cell.east > region.west
            }
    }
}

/**
 * Words of the forecast map that are not the core's: the shell's own, for
 * when there is no map to read, and the height a square was judged at.
 */
object ThermalMapWords {
    const val ASKING = "Asking Venti for the forecast map"
    const val UNREACHABLE = "Venti could not be reached for the forecast map."
    const val NO_DAYS = "The forecast map has no days from today on."
    const val NONE_HERE = "The forecast map has no squares for this part of the map."
    const val TRY_AGAIN = "Try again"

    /** A square's status as TalkBack reads it, with what speaks against the day: "Marginal, Gusts". */
    fun status(square: ThermalMapCell): String =
        if (square.why.isEmpty()) square.status else "${square.status}, ${square.why}"

    /** The place whose forecast a square leads to: its middle, named by where it is. */
    fun place(square: ThermalMapCell): ForecastPlace {
        val middle = Coordinate(lat = (square.south + square.north) / 2, lon = (square.west + square.east) / 2)
        return ForecastPlace(Format.place(middle.lat, middle.lon), middle)
    }

    /** "Judged at the model's ground, 1240 m" */
    fun judgedAt(groundM: Double): String = "Judged at the model's ground, ${Format.metres(groundM)} m"

    /**
     * "ICON-D2, run of Sat, 21 Nov at 06:00": the model a day is read from
     * and that model's run. Each day is one model's; the days after
     * ICON-D2's are ICON-EU's.
     */
    fun run(day: ThermalMapDay): String = "${day.model}, run of ${Format.dayAndTime(day.runUnixS)}"
}
