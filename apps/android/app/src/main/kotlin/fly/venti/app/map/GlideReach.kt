package fly.venti.app.map

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.design.Format
import fly.venti.app.settings.Preferences
import fly.venti.app.task.RouteWeatherWords
import fly.venti.app.terrain.TerrainModel
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.GlideReach
import fly.venti.core.GlideReachSetting
import fly.venti.core.ReachEnd
import fly.venti.core.ReachRay
import fly.venti.core.Terrain
import fly.venti.core.glideReach
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.floor

/**
 * What a glide reaches from a place on the map, over the ground: the
 * core's `glideReach` for the place the pilot long-pressed, with the
 * glide, the start height and the wind the card sets.
 *
 * Every rule and every word of the answer are the core's. This class
 * brings the ground around the place, asks the core again whenever a
 * setting changes (no server is asked), and brings the ground for a wider
 * box when the core says a direction ran off what was loaded
 * (`cutShort`), up to [GlideReachWords.WIDEST_SPAN_DEG].
 */
class GlideReachModel(
    /** The ground under an area; `null` for none, and the core then says it is not loaded. */
    private val ground: (suspend (Bounds) -> TerrainModel.Ground)? = null,
    /** Where the core lays the glide, so that the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The place the glide starts from; `null` while none is shown. */
    var place: Coordinate? by mutableStateOf(null)
        private set

    /** The glider's glide ratio in still air. */
    var glide: Double by mutableStateOf(Preferences.DEFAULT_GLIDE_RATIO)

    /** The height to start at, metres above mean sea level; `null` for the ground at the place. */
    var startHeightM: Double? by mutableStateOf(null)

    /** The wind, or `null` for calm air. */
    var wind: GlideReachWind? by mutableStateOf(null)

    /** The core's answer for the place and settings last asked; `null` while there is none. */
    var answer: GlideReach? by mutableStateOf(null)
        private set

    /** Whether the ground is being brought or the core asked. */
    var isAsking: Boolean by mutableStateOf(false)
        private set

    /** The ground held, and how far round the place it reaches, degrees of latitude. */
    private var heights: TerrainModel.Ground? = null
    private var heightsFor: Bounds? = null
    private var span = GlideReachWords.FIRST_SPAN_DEG

    /** A terrain with no ground, for a model without any. */
    private var none: Terrain? = null

    /** Starts at a place, with the pilot's glide and the other settings as they come: on the ground, calm. */
    fun show(at: Coordinate, glideRatio: Double) {
        if (at != place) {
            span = GlideReachWords.FIRST_SPAN_DEG
            answer = null
        }
        place = at
        glide = glideRatio
        startHeightM = null
        wind = null
    }

    /** What the core is asked for the place and the settings as they are now; `null` without a place. */
    fun setting(): GlideReachSetting? {
        val at = place ?: return null
        return GlideReachSetting(
            lat = at.lat,
            lon = at.lon,
            glideRatio = glide,
            startHeightM = startHeightM,
            windFromDeg = wind?.fromDeg,
            windKmh = wind?.kmh?.toDouble(),
            marginM = null,
        )
    }

    /**
     * Asks the core for the place and settings as they are, bringing the
     * ground first where it is not held, and a wider box of it while a
     * direction runs off what is loaded.
     */
    suspend fun ask() {
        val asked = setting() ?: return
        val at = place ?: return
        isAsking = true
        try {
            while (true) {
                val terrain = groundAround(at) ?: emptyTerrain()
                val reach = withContext(background) { glideReach(asked, terrain) }
                if (setting() != asked) return
                answer = reach
                if (!reach.cutShort || span >= GlideReachWords.WIDEST_SPAN_DEG) return
                span *= 2
            }
        } finally {
            isAsking = false
        }
    }

    /** Takes the area away, and lets go of the ground held for it. */
    fun clear() {
        place = null
        answer = null
        startHeightM = null
        wind = null
        letGoOfGround()
    }

    /** Lets go of everything held. */
    fun close() {
        clear()
        none?.close()
        none = null
    }

    private suspend fun groundAround(at: Coordinate): Terrain? {
        val bring = ground ?: return null
        val area = GlideReachWords.around(at, span)
        if (area == heightsFor) return heights?.terrain
        val brought = try {
            bring(area)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            return heights?.terrain
        }
        heights?.close()
        heights = brought
        heightsFor = area
        return brought.terrain
    }

    private fun emptyTerrain(): Terrain = none ?: Terrain().also { none = it }

    private fun letGoOfGround() {
        heights?.close()
        heights = null
        heightsFor = null
        span = GlideReachWords.FIRST_SPAN_DEG
    }
}

/** A wind the pilot sets for the glide: where it comes from, degrees, and its speed, km/h. */
data class GlideReachWind(val fromDeg: Double, val kmh: Int)

/** The edge of the area reached, as the map draws it: solid where the glide ends on the ground, dashed where the ground loaded ends. */
data class GlideReachEdges(val solid: List<List<Coordinate>>, val dashed: List<List<Coordinate>>)

/** Words of the card that are not the core's, and the steps of its settings. */
object GlideReachWords {
    /** The entry in what a long press on the map opens, and the card's title. */
    const val ENTRY = "What a glide reaches from here"
    const val GLIDE = "Glide"
    const val START = "Start height"
    const val ON_THE_GROUND = "On the ground"
    const val WIND = "Wind"
    const val WIND_FROM = "Wind from"
    const val WIND_SPEED = "Wind speed"
    const val CLEAR = "Clear"
    const val ASKING = "Laying the glide over the ground"

    /** How far round the place the ground is brought first, and at most, degrees of latitude. */
    const val FIRST_SPAN_DEG = 0.15
    const val WIDEST_SPAN_DEG = 0.6

    const val START_STEP_M = 50.0
    const val START_AT_MOST_M = 9000.0
    const val WIND_STEP_DEG = 45.0
    const val WIND_STEP_KMH = 5
    val WIND_SPEEDS_KMH = 5..60

    /** The wind a pilot switches on: from the west, 15 km/h, to be set from there. */
    val FIRST_WIND = GlideReachWind(fromDeg = 270.0, kmh = 15)

    fun glide(ratio: Double): String = Format.glideRatio(ratio)

    fun stepGlide(ratio: Double, steps: Int): Double = RouteWeatherWords.stepGlide(ratio, steps)

    fun start(heightM: Double?): String = heightM?.let { "${Format.metres(it)} m" } ?: ON_THE_GROUND

    /**
     * The start height a step up or down, by whole steps of 50 m from
     * the height shown or the ground's: at or under the ground it is the
     * ground again. Without the ground known a start on the ground stays.
     */
    fun stepStart(heightM: Double?, groundM: Double?, steps: Int): Double? {
        val from = heightM ?: groundM ?: return heightM
        val next = if (steps > 0) {
            floor(from / START_STEP_M) * START_STEP_M + steps * START_STEP_M
        } else {
            ceil(from / START_STEP_M) * START_STEP_M + steps * START_STEP_M
        }
        if (groundM != null && next <= groundM) return null
        return next.coerceAtMost(START_AT_MOST_M)
    }

    fun windFrom(wind: GlideReachWind): String = Format.compassPoint(wind.fromDeg)

    fun windSpeed(wind: GlideReachWind): String = "${wind.kmh} km/h"

    fun stepWindFrom(wind: GlideReachWind, steps: Int): GlideReachWind =
        wind.copy(fromDeg = ((wind.fromDeg + steps * WIND_STEP_DEG) % 360 + 360) % 360)

    fun stepWindSpeed(wind: GlideReachWind, steps: Int): GlideReachWind =
        wind.copy(kmh = (wind.kmh + steps * WIND_STEP_KMH).coerceIn(WIND_SPEEDS_KMH))

    /** The box round a place the ground is brought for: `span` degrees of latitude either way, as far east and west. */
    fun around(at: Coordinate, span: Double): Bounds {
        val wide = span / cos(Math.toRadians(at.lat)).coerceAtLeast(0.1)
        return Bounds(south = at.lat - span, west = at.lon - wide, north = at.lat + span, east = at.lon + wide)
    }

    /**
     * The edge of the area cut where it changes between ending on the
     * ground (or against the wind) and running off the ground loaded:
     * a stretch between two directions that both ran off is dashed.
     */
    fun edges(rays: List<ReachRay>): GlideReachEdges {
        if (rays.size < 3) return GlideReachEdges(emptyList(), emptyList())
        val solid = mutableListOf<List<Coordinate>>()
        val dashed = mutableListOf<List<Coordinate>>()
        var run = mutableListOf(rays[0].endAt)
        var runDashed: Boolean? = null
        for (index in rays.indices) {
            val from = rays[index]
            val to = rays[(index + 1) % rays.size]
            val isDashed = from.end == ReachEnd.NOT_LOADED && to.end == ReachEnd.NOT_LOADED
            if (runDashed != null && isDashed != runDashed) {
                (if (runDashed) dashed else solid) += run
                run = mutableListOf(from.endAt)
            }
            runDashed = isDashed
            run += to.endAt
        }
        (if (runDashed == true) dashed else solid) += run
        return GlideReachEdges(solid, dashed)
    }
}
