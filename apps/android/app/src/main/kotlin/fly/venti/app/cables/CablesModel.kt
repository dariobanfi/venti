package fly.venti.app.cables

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.core.CableLine
import fly.venti.core.CableSet
import fly.venti.core.CablesMap
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.cablesRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.abs

/**
 * The cables OpenStreetMap maps around where the pilot looks or flies:
 * power lines and ropeways, as Venti's server hands them out. Asked for
 * where the airspace is asked for, before a flight and for the map's
 * area, and kept for as long as the app runs. What they are, what they
 * say and when the cockpit warns of one are the core's.
 */
class CablesModel(
    /** Where the answer is read, so that the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
    /** Where a request runs on after the screen that caused it has gone. */
    private val scope: CoroutineScope? = null,
    /** Fetches an address and returns the server's answer. */
    private val transport: suspend (String) -> HttpAnswer = { url -> Http.get(url) },
    /** The clock, in seconds since 1970: a server that failed is left alone for a while. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
) {
    /** The server to ask, without a slash at its end. The app sets this. */
    var server: () -> String? = { null }

    /** The cables of the area last fetched, for the cockpit. `null` until an answer came. */
    var set: CableSet? by mutableStateOf(null)
        private set

    /** The same as the maps draw them: the lines, the credits and the sentence on their limits. */
    var layer: CablesMap? by mutableStateOf(null)
        private set

    /** Why the cables could not be fetched, in words. */
    var problem: String? by mutableStateOf(null)
        private set

    /** The place the set was fetched around, and from which server. */
    private var fetchedAround: Coordinate? = null
    private var fetchedFrom: String? = null
    private var isFetching = false
    private var failedAt: Long? = null

    /** The cables to draw. None before an answer. */
    val lines: List<CableLine> get() = layer?.lines.orEmpty()

    /** Whose the cables are, in a line for the map's lower edge, while any is drawn. */
    val creditLine: String?
        get() {
            val layer = layer ?: return null
            if (layer.lines.isEmpty() || layer.credits.isEmpty()) return null
            return layer.credits.joinToString(" · ") { it.text }
        }

    /** The cable of this identifier, as the layer has it. */
    fun line(id: String): CableLine? = lines.firstOrNull { it.id == id }

    /**
     * Asks the server for the cables around a place, unless the set at hand
     * was fetched near enough to it: within [REFETCH_DEGREES] of its middle,
     * well inside the box of some 65 km the core asks for.
     */
    suspend fun refresh(around: Coordinate) {
        val address = server() ?: return
        if (isFetching) return
        if (!isDue(around, address)) return
        val time = now()
        failedAt?.let { failed -> if (time - failed in 0 until RETRY_AFTER_SECONDS) return }
        isFetching = true
        try {
            val request = cablesRequest(around.lat, around.lon)
            val answer = transport(address + request.path)
            val read = withContext(background) {
                CableSet.fromAnswer(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body)
            }
            if (server() != address) return
            set = read
            layer = read.layer()
            fetchedAround = around
            fetchedFrom = address
            failedAt = null
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            failedAt = time
            problem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "Venti could not be reached for cables. ${error.pilotMessage}"
            }
        } finally {
            isFetching = false
        }
    }

    /** The same, without waiting for it. */
    fun refreshSoon(around: Coordinate) {
        scope?.launch { refresh(around) }
    }

    private fun isDue(around: Coordinate, address: String): Boolean {
        val fetched = fetchedAround ?: return true
        if (fetchedFrom != address) return true
        return abs(fetched.lat - around.lat) > REFETCH_DEGREES || abs(fetched.lon - around.lon) > REFETCH_DEGREES
    }

    companion object {
        /** How far from the middle of the set the place may be before it is fetched anew, in degrees: some 15 to 20 km. */
        const val REFETCH_DEGREES = 0.15

        /** After a request that failed, the server is left alone for this long. */
        const val RETRY_AFTER_SECONDS = 5 * 60L
    }
}
