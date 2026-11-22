package fly.venti.app.task

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.pilotMessage
import fly.venti.app.terrain.TerrainModel
import fly.venti.app.wind.WindTransport
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.RouteAlong
import fly.venti.core.RouteForecast
import fly.venti.core.RoutePlace
import fly.venti.core.RouteSetting
import fly.venti.core.routeAlong
import fly.venti.core.routeWeatherPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId

/**
 * The weather along a route drawn on the map: the server's forecast at
 * its places for a day (`routeWeatherPath`, `RouteForecast`), and the
 * ground under it, for the core to say when the pilot is where, what the
 * forecast has there then and the height to leave each place at
 * (`routeAlong`).
 *
 * What to ask, every rule and every word are the core's. This class asks
 * once for each route and day (the answer holds all of the day's hours,
 * so a later start reads the same answer), brings the ground once for
 * each route, closes what it lets go, and drops an answer that comes late
 * for a route or a day no longer drawn.
 */
class RouteWeatherModel(
    private val transport: WindTransport = { url -> Http.get(url) },
    /** The ground under an area; `null` for none, and the heights then say it is not loaded. */
    private val ground: (suspend (Bounds) -> TerrainModel.Ground)? = null,
    /** Where the core reads an answer, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The server to ask, without a slash at its end; `null` for none. */
    var server: () -> String? = { null }

    /** The forecast along the route and day last asked for; `null` while there is none. */
    var forecast: RouteForecast? by mutableStateOf(null)
        private set

    /** The ground under the route last shown; `null` while there is none. */
    var heights: TerrainModel.Ground? by mutableStateOf(null)
        private set

    /** Whether the server is being asked. */
    var isAsking: Boolean by mutableStateOf(false)
        private set

    /** Why the last asking brought no forecast, in words for the pilot; `null` after one that did. */
    var problem: String? by mutableStateOf(null)
        private set

    /** What the forecast held is for, and what was asked last: a late answer for something else is dropped. */
    private var heldFor: String? = null
    private var wanted: String? = null

    /** Which places the ground held is under. */
    private var groundFor: List<Coordinate>? = null

    /**
     * Brings the forecast along these places for a day (`date` as the
     * thermal map names it) and the ground under them: from what is held,
     * else from the server and the terrain.
     */
    suspend fun show(places: List<Coordinate>, date: String) = coroutineScope {
        if (places.size < 2) {
            drop()
            return@coroutineScope
        }
        launch { bringGround(places) }
        val server = server()
        if (server == null) {
            drop()
            problem = RouteWeatherWords.NO_SERVER
            return@coroutineScope
        }
        val path = try {
            routeWeatherPath(placesOf(places), date)
        } catch (error: CoreException) {
            drop()
            problem = error.pilotMessage
            return@coroutineScope
        }
        wanted = path
        if (path == heldFor && forecast != null) return@coroutineScope
        // Another route or day: the forecast of the one before is not this one's.
        letGoOfForecast()
        isAsking = true
        problem = null
        try {
            val answer = transport(server + path)
            val read = withContext(background) {
                RouteForecast.read(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body)
            }
            if (wanted != path) {
                read.close()
                return@coroutineScope
            }
            forecast = read
            heldFor = path
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (wanted != path) return@coroutineScope
            problem = if (error is CoreException) error.pilotMessage else "${RouteWeatherWords.UNREACHABLE} ${error.pilotMessage}"
        } finally {
            if (wanted == path) isAsking = false
        }
    }

    private suspend fun bringGround(places: List<Coordinate>) {
        val ground = ground ?: return
        if (places == groundFor) return
        // Taken as held only once it is: a load cut short by another route is asked again.
        val brought = try {
            ground(RouteWeatherWords.around(places))
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            return
        }
        groundFor = places
        heights?.close()
        heights = brought
    }

    /**
     * The route with the weather along it, as the core says it: with the
     * forecast and the ground held, or without them where there are none.
     */
    fun along(places: List<Coordinate>, startUnixS: Long, speedKmh: Int, glideRatio: Double, zone: ZoneId = ZoneId.systemDefault()): RouteAlong =
        routeAlong(
            RouteSetting(placesOf(places), startUnixS, speedKmh.toDouble(), glideRatio),
            forecast,
            heights?.terrain,
            zone.rules.getOffset(Instant.ofEpochSecond(startUnixS)).totalSeconds,
        )

    /** Forgets the forecast, so that "Try again" asks anew. */
    fun forgetForecast() = letGoOfForecast()

    private fun letGoOfForecast() {
        val old = forecast
        forecast = null
        heldFor = null
        old?.close()
    }

    private fun drop() {
        wanted = null
        letGoOfForecast()
        isAsking = false
        problem = null
    }

    /** Lets everything go: the planner is closed. */
    fun close() {
        drop()
        groundFor = null
        heights?.close()
        heights = null
    }

    private fun placesOf(places: List<Coordinate>) = places.map { RoutePlace(it.lat, it.lon) }
}

/** What the weather along a route says that is not the core's, and the settings' steps. */
object RouteWeatherWords {
    const val TITLE = "Weather along the route"
    const val ASKING = "Asking Venti for the weather along the route"
    const val UNREACHABLE = "Venti could not be reached for the weather along the route."
    const val NO_SERVER = "Without a Venti server there is no forecast along the route."
    const val START = "Start"
    const val SPEED = "Speed over the ground"
    const val GLIDE = "Glide"

    /** The start moves by this, minutes. */
    const val START_STEP_MINUTES = 30

    /** The speed over the ground moves by this, km/h, between these. */
    const val SPEED_STEP_KMH = 5
    val SPEEDS_KMH = 5..80
    const val SPEED_KMH = 20

    /** The glide moves by this, between these: as in the settings. */
    const val GLIDE_STEP = 0.5
    val GLIDE_RATIOS = 4.0..14.0

    /** The next full hour after a moment on the pilot's clock, as minutes of the day; the last hour of the day at the latest. */
    fun nextFullHour(nowUnixS: Long, zone: ZoneId = ZoneId.systemDefault()): Int {
        val time = Instant.ofEpochSecond(nowUnixS).atZone(zone).toLocalTime()
        return minOf(time.hour + 1, 23) * 60
    }

    /** A time of a day on the pilot's clock, Unix seconds. */
    fun startUnixS(date: String, minuteOfDay: Int, zone: ZoneId = ZoneId.systemDefault()): Long =
        LocalDate.parse(date).atTime(LocalTime.of(minuteOfDay / 60, minuteOfDay % 60)).atZone(zone).toEpochSecond()

    /** The day of a moment on the pilot's clock, as the thermal map names a day: "2026-11-22". */
    fun date(unixS: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        Instant.ofEpochSecond(unixS).atZone(zone).toLocalDate().toString()

    /** A minute of the day on the pilot's clock: "14:30". */
    fun clock(minuteOfDay: Int): String = "%02d:%02d".format(minuteOfDay / 60, minuteOfDay % 60)

    /** The start a step earlier or later, kept within the day. */
    fun stepStart(minuteOfDay: Int, steps: Int): Int =
        (minuteOfDay + steps * START_STEP_MINUTES).coerceIn(0, 24 * 60 - START_STEP_MINUTES)

    fun stepSpeed(kmh: Int, steps: Int): Int = (kmh + steps * SPEED_STEP_KMH).coerceIn(SPEEDS_KMH)

    fun stepGlide(ratio: Double, steps: Int): Double =
        (Math.round((ratio + steps * GLIDE_STEP) / GLIDE_STEP) * GLIDE_STEP).coerceIn(GLIDE_RATIOS)

    /** The area round a route's places, a little wider than they reach: the ground the lines between them cross. */
    fun around(places: List<Coordinate>): Bounds {
        val margin = 0.02
        return Bounds(
            south = places.minOf { it.lat } - margin,
            west = places.minOf { it.lon } - margin,
            north = places.maxOf { it.lat } + margin,
            east = places.maxOf { it.lon } + margin,
        )
    }
}
