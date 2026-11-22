package fly.venti.app.live

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.wind.WindTransport
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.FlownThermals
import fly.venti.core.LiveClimbs
import fly.venti.core.cockpitClimbsPath
import fly.venti.core.flownThermals
import fly.venti.core.flownThermalsPath
import fly.venti.core.liveClimbs
import fly.venti.core.liveClimbsPath
import fly.venti.core.liveSites
import fly.venti.core.liveSitesPath
import fly.venti.core.siteAirWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * What only a server that sees many gliders can say: where pilots climb at
 * this moment, where the public flights' thermals have been, and how many
 * pilots are in the air at a site. None of it names a pilot.
 *
 * What to ask and how to read the answer is the core's; this class sends
 * the requests and keeps the last answers. Both of the map's layers are
 * off until the pilot switches them on, and nothing is asked while they
 * are off.
 */
class AirModel(
    /** Fetches an address; nothing is kept, a climb from a cache is a wrong one. */
    private val transport: WindTransport = { url -> Http.get(url, ANSWER_WITHIN_MILLISECONDS) },
    /** Where the core reads an answer, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The server to ask, without a slash at its end; `null` for none. */
    var server: () -> String? = { null }

    /** Whether the layer "Climbs now" is on. */
    var climbsOn: Boolean by mutableStateOf(false)

    /** Whether the layer of where thermals have been is on. */
    var flownOn: Boolean by mutableStateOf(false)

    /** The climbs of the last minutes in the part of the map last asked for; `null` while none was read. */
    var climbs: LiveClimbs? by mutableStateOf(null)
        private set

    /** Whether the map shows too much for the climbs to be asked for. */
    var climbsTooWide: Boolean by mutableStateOf(false)
        private set

    /** Where thermals have been in the part of the map last asked for; `null` while none was read. */
    var flown: FlownThermals? by mutableStateOf(null)
        private set

    /** Whether the map shows too much for them: then the core's `flownTooWideWords` stand over it. */
    var flownTooWide: Boolean by mutableStateOf(false)
        private set

    /**
     * Asks for the climbs in the part of the map in view. An answer that
     * does not come, or a server from before the climbs, leaves none:
     * a climb is of this moment, and an old one is not shown as one.
     */
    suspend fun loadClimbs(region: Bounds?) {
        val server = server()
        val path = region?.let { liveClimbsPath(it.south, it.west, it.north, it.east) }
        climbsTooWide = region != null && path == null
        if (!climbsOn || server == null || path == null) {
            climbs = null
            return
        }
        climbs = ask(server + path) { status, body -> liveClimbs(status, body) }
    }

    /**
     * Asks where thermals have been in the part of the map in view, in
     * every month of the year and at any hour: the layer is not empty in
     * winter, and the core's note says so.
     */
    suspend fun loadFlown(region: Bounds?) {
        val server = server()
        val path = region?.let {
            flownThermalsPath(it.south, it.west, it.north, it.east, null, null, null)
        }
        flownTooWide = region != null && path == null
        if (!flownOn || server == null || path == null) {
            flown = null
            return
        }
        // What was there stays while the server does not answer: the past does not change by the minute.
        ask(server + path) { status, body -> flownThermals(status, body) }?.let { flown = it }
    }

    /**
     * The line a site's page says of who flies there now, in the core's
     * words; empty when nobody does, when there is no server, and when it
     * does not answer.
     */
    suspend fun siteAir(siteId: String, siteName: String, position: Coordinate): String {
        val server = server() ?: return ""
        val path = liveSitesPath(
            position.lat - AROUND_A_SITE_DEGREES,
            position.lon - AROUND_A_SITE_DEGREES,
            position.lat + AROUND_A_SITE_DEGREES,
            position.lon + AROUND_A_SITE_DEGREES,
        ) ?: return ""
        val air = ask(server + path) { status, body -> liveSites(status, body) } ?: return ""
        return siteAirWords(air, siteId, siteName)
    }

    /** The climbs around a glider, for the cockpit; `null` when they cannot be read. */
    suspend fun climbsAround(lat: Double, lon: Double): LiveClimbs? {
        val server = server() ?: return null
        val path = cockpitClimbsPath(lat, lon) ?: return null
        return ask(server + path) { status, body -> liveClimbs(status, body) }
    }

    private suspend fun <T> ask(url: String, read: (UShort, ByteArray) -> T): T? = try {
        val answer = transport(url)
        withContext(background) { read(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body) }
    } catch (error: CancellationException) {
        throw error
    } catch (_: Exception) {
        null
    }

    companion object {
        /** How often the climbs are asked for again while their layer is on, as the core asks. */
        const val CLIMBS_EVERY_MILLISECONDS = 30_000L

        /** How often the cockpit asks for the climbs around the glider. */
        const val COCKPIT_EVERY_MILLISECONDS = 60_000L

        /** How long the map rests before it is asked about. */
        const val REST_MILLISECONDS = 400L

        /** The box asked about around a site: a few kilometres to each side, well inside what the server answers for. */
        const val AROUND_A_SITE_DEGREES = 0.05

        private const val ANSWER_WITHIN_MILLISECONDS = 10_000
    }
}

/** What is in the air, for a site's page, wherever it is opened; `null` where there is none to ask. */
val LocalAir = androidx.compose.runtime.staticCompositionLocalOf<AirModel?> { null }
