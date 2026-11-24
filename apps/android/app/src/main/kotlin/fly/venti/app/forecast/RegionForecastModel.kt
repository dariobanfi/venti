package fly.venti.app.forecast

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import app.venti.design.VentiLight
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.ForecastRegions
import fly.venti.core.RegionForecast
import fly.venti.core.RegionLevel
import fly.venti.core.ThermalMap
import fly.venti.core.ThermalMapDay
import fly.venti.core.regionForecastPath
import fly.venti.core.regionsPath
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.MultiPolygon

/**
 * The thermal forecast by region (owner decision 27, the web's
 * `regions.svelte.ts`, the iPhone's `RegionForecastModel`): the regions'
 * outlines, asked for once (`GET /v1/regions`), and each day asked for as
 * the pilot goes through the thermals' days (`GET /v1/regions/forecast`).
 * Every level, word and credit is the core's (`ForecastRegions`,
 * `RegionForecast`); what is drawn of them is `RegionLayers`. A day that
 * cannot be had is not kept, and the map then shows the grid alone,
 * without a word.
 */
class RegionForecastModel(
    private val transport: WindTransport,
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The regions, once read. */
    var regions: ForecastRegions? by mutableStateOf(null)
        private set

    /** The days read for the map of the days at hand, by date. */
    var days: Map<String, RegionForecast> by mutableStateOf(emptyMap())
        private set

    /** Why the last asking brought nothing: the grid then stands alone and nothing is said on the map. */
    var problem: String? by mutableStateOf(null)
        private set

    /** The regions' outlines as polygons, by the region's identifier. */
    private var polygons: Map<String, List<RegionShapes.Polygon>> = emptyMap()
    private var askedServer: String? = null
    private var keptFor: ThermalMap? = null
    private val asking = mutableSetOf<String>()

    /** The squares of a day's grid under the regions named, worked out once for a day and a set of regions. */
    private val under = mutableMapOf<String, Set<Int>>()

    /**
     * Asks for a day of the map at hand, and for the regions where they are
     * not at hand or the day names another set of them. Nothing is asked
     * twice at once.
     */
    suspend fun load(map: ThermalMap, date: String, server: String, now: Long, offset: Int) {
        if (askedServer != server) {
            regions = null
            polygons = emptyMap()
            askedServer = server
            forgetDays(map)
        }
        if (map !== keptFor) forgetDays(map)
        if (days[date] != null || date in asking) return
        asking += date
        try {
            val answer = transport(server + regionForecastPath(date))
            val day = withContext(background) { RegionForecast.read(status(answer.status), answer.body, offset, now) }
            if (regions?.version() != day.view().regionsVersion) {
                val outlines = transport(server + regionsPath())
                val read = withContext(background) {
                    val read = ForecastRegions.read(status(outlines.status), outlines.body, language)
                    read to RegionShapes.polygons(read.outlines())
                }
                if (server != askedServer) return
                polygons = read.second
                under.clear()
                regions = read.first
            }
            if (server != askedServer || map !== keptFor) return
            days = days + (date to day)
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = error.pilotMessage
        } finally {
            asking -= date
        }
    }

    private fun forgetDays(map: ThermalMap?) {
        keptFor = map
        days = emptyMap()
        under.clear()
    }

    /** Lets every day go: the layer is switched off, or the server changed. */
    fun forget() = forgetDays(null)

    /**
     * The squares of a day's grid, by their place among its cells, whose
     * middle lies in one of the regions named: those the regions' fill
     * stands in place of.
     */
    fun cellsUnder(ids: Set<String>, day: ThermalMapDay): Set<Int> {
        if (ids.isEmpty()) return emptySet()
        val key = "${day.date} ${day.cells.size} ${ids.sorted().joinToString(",")}"
        return under.getOrPut(key) {
            RegionShapes.cellsUnder(
                day.cells.map { RegionShapes.Box(south = it.south, west = it.west, north = it.north, east = it.east) },
                ids.flatMap { polygons[it].orEmpty() },
            )
        }
    }

    private fun status(status: Int): UShort = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** The language of the regions' names: the phone's, where the regions have names in it, else English (the core decides). */
        val language: String get() = Locale.getDefault().language.ifEmpty { "en" }
    }
}

/** The geometry of the regions, without a map: their outlines read from the core's GeoJSON, and which squares of the grid lie in them. */
object RegionShapes {
    data class Box(val south: Double, val west: Double, val north: Double, val east: Double)

    /** One polygon: its outer ring and its holes, each ring as longitude and latitude pairs, and the box round the outer ring. */
    class Polygon(val rings: List<List<Pair<Double, Double>>>, val box: Box)

    /** The polygons of each region in a GeoJSON `FeatureCollection` of `Polygon`s or `MultiPolygon`s, by the features' `id`. */
    fun polygons(geoJson: String): Map<String, List<Polygon>> {
        val features = runCatching { FeatureCollection.fromJson(geoJson).features() }.getOrNull().orEmpty()
        val found = mutableMapOf<String, MutableList<Polygon>>()
        for (feature in features) {
            val id = feature.getStringProperty("id") ?: continue
            val polygons: List<List<List<org.maplibre.geojson.Point>>> = when (val geometry = feature.geometry()) {
                is org.maplibre.geojson.Polygon -> listOf(geometry.coordinates())
                is MultiPolygon -> geometry.coordinates()
                else -> emptyList()
            }
            for (polygon in polygons) {
                val rings = polygon.map { ring -> ring.map { it.longitude() to it.latitude() } }
                val outer = rings.firstOrNull()?.takeIf { it.isNotEmpty() } ?: continue
                val box = Box(
                    south = outer.minOf { it.second },
                    west = outer.minOf { it.first },
                    north = outer.maxOf { it.second },
                    east = outer.maxOf { it.first },
                )
                found.getOrPut(id) { mutableListOf() } += Polygon(rings, box)
            }
        }
        return found
    }

    /** The places among the squares of those whose middle lies in one of the polygons. */
    fun cellsUnder(cells: List<Box>, polygons: List<Polygon>): Set<Int> {
        if (polygons.isEmpty()) return emptySet()
        return cells.indices.filterTo(mutableSetOf()) { index ->
            val cell = cells[index]
            val lon = (cell.west + cell.east) / 2
            val lat = (cell.south + cell.north) / 2
            polygons.any { polygon ->
                lon >= polygon.box.west && lon <= polygon.box.east && lat >= polygon.box.south && lat <= polygon.box.north &&
                    contains(polygon, lon, lat)
            }
        }
    }

    /** Whether a place lies in a polygon: in its outer ring and in none of its holes. */
    fun contains(polygon: Polygon, lon: Double, lat: Double): Boolean {
        val outer = polygon.rings.firstOrNull() ?: return false
        return inRing(outer, lon, lat) && polygon.rings.drop(1).none { inRing(it, lon, lat) }
    }

    /** Counts the crossings of a ray to the east. */
    private fun inRing(ring: List<Pair<Double, Double>>, lon: Double, lat: Double): Boolean {
        var inside = false
        var before = ring.lastIndex
        for (at in ring.indices) {
            val (x1, y1) = ring[at]
            val (x2, y2) = ring[before]
            if ((y1 > lat) != (y2 > lat) && lon < (x2 - x1) * (lat - y1) / (y2 - y1) + x1) inside = !inside
            before = at
        }
        return inside
    }
}

/** The level's place in the pack's `level` tokens, which run worst to best. */
val RegionLevel.tokenIndex: Int
    get() = when (this) {
        RegionLevel.NOT_FLYABLE -> 0
        RegionLevel.GLIDES_ONLY -> 1
        RegionLevel.WEAK -> 2
        RegionLevel.GOOD -> 3
        RegionLevel.VERY_GOOD -> 4
    }

/** The level's colour on the map, which is light in both appearances: the light theme's (design/DESIGN.md, "Day levels"). */
val RegionLevel.colourOnMap: Color get() = VentiLight.level[tokenIndex]

/** The opacity of the level's fill on the map. */
val RegionLevel.fillOpacityOnMap: Float get() = VentiLight.levelFillOpacity[tokenIndex]
