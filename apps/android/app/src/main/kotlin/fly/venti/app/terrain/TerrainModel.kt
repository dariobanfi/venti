package fly.venti.app.terrain

import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.FlightView
import fly.venti.core.GroundUnderFlight
import fly.venti.core.Terrain
import fly.venti.core.TerrainTile
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

/** Fetches one tile by its address and returns the answer, whatever its status. */
typealias TerrainTransport = suspend (url: String) -> HttpAnswer

/**
 * The ground: the map's elevation tiles around the place the pilot flies
 * at, for the cockpit's height above ground, for airspace given above
 * ground and for what a glide reaches.
 *
 * Tiles are the ones the map draws its relief from. Each is fetched once
 * and kept on the phone, so that the ground is there again without a
 * network: over the place flown at last time, and wherever a flight led
 * while there was one. The core reads the heights out of them; nothing
 * here knows what a tile holds.
 */
class TerrainModel(
    /**
     * Where the tiles come from. Asked with every load: which tiles the
     * map draws is settled when the first screen opens, after this exists.
     */
    private val source: () -> Source,
    /** The folder the tiles are kept in; `null` keeps none. */
    private val folder: File?,
    private val transport: TerrainTransport = overHttp,
    /** Where reading, decoding and writing run: they take a moment. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** Where the tiles come from. */
    sealed interface Source {
        /** A tile server: its address with `{z}/{x}/{y}` for the tile. */
        data class Server(val template: String) : Source

        /** An archive that came with the app, read once. Needs no network. */
        data class Archive(val file: File) : Source

        /** Nowhere: the cockpit then knows no ground. */
        data object Nowhere : Source
    }

    /** The heights, as the flying session uses them. */
    val terrain = Terrain()

    private val isLoading = AtomicBoolean(false)

    /** The elevation archives of the regions kept on the phone, read before anything is fetched. */
    @Volatile
    private var archives: List<File> = emptyList()

    /** Uses the elevation of the regions kept on the phone, from their files. */
    fun use(archives: List<File>) {
        this.archives = archives
        terrain.closeArchives()
        for (file in archives) {
            runCatching { terrain.openArchive(file.path) }
        }
    }

    /** The archive that was read, so that it is read once. */
    @Volatile
    private var archiveRead: File? = null

    /**
     * Brings the tiles around a place that are not there yet: from the
     * phone's store where it has them, else from the tile server. Tiles
     * far from the place are let go from memory; they stay in the store.
     * Does nothing while an earlier call is still at work.
     */
    suspend fun load(around: Coordinate) {
        if (!isLoading.compareAndSet(false, true)) return
        try {
            // What a kept region has is read from its file first; only what
            // it lacks is fetched.
            if (archives.isNotEmpty()) {
                withContext(background) { terrain.readAround(around.lat, around.lon) }
            }
            // Asking may copy the app's region out of the app: not here.
            when (val from = withContext(background) { source() }) {
                Source.Nowhere -> Unit
                is Source.Archive -> readArchive(from.file)
                is Source.Server -> {
                    for (tile in terrain.tilesWanted(around.lat, around.lon)) {
                        val came = withContext(background) { bring(tile, from.template, terrain) }
                        // No answer at all: without a network the other
                        // tiles would not come either. The next load asks.
                        if (came == Came.NO_ANSWER) break
                    }
                    terrain.keepAround(around.lat, around.lon)
                }
            }
        } finally {
            isLoading.set(false)
        }
    }

    /**
     * The ground under a flight's track, for its profile: the tiles under
     * the flight are brought, from the phone's store or the tile server,
     * read, and let go again. Empty where there is no ground to be had.
     * Which moments and what heights are the core's (`groundUnder`).
     */
    suspend fun ground(under: FlightView): List<GroundUnderFlight> =
        covering(under.bounds).use { ground ->
            withContext(background) { ground.terrain.groundUnder(under.track, PLACES_UNDER_A_FLIGHT) }
        }

    /** Heights read for an area, and whether they are a terrain of their own, which goes when they are closed. */
    class Ground(val terrain: Terrain, private val isOwn: Boolean) : AutoCloseable {
        override fun close() {
            if (isOwn) terrain.close()
        }
    }

    /**
     * The ground under an area: a flight's, or a planned route's for the
     * heights to leave its places at. What kept regions have of it is
     * read, then the tiles it lacks are brought from the phone's store or
     * the tile server. How many tiles, at which zoom, is the core's
     * (`tilesCovering`). The caller closes what it is given.
     */
    suspend fun covering(area: Bounds): Ground =
        when (val from = withContext(background) { source() }) {
            // The region that comes with the app is small: it is read
            // once, into the terrain the cockpit uses too.
            is Source.Archive -> {
                readArchive(from.file)
                if (archives.isNotEmpty()) withContext(background) { terrain.readCovering(area) }
                Ground(terrain, isOwn = false)
            }
            // A terrain of its own, which goes when its user is done:
            // the cockpit's holds the ground around the glider and no more.
            is Source.Server, Source.Nowhere -> {
                val own = Terrain()
                try {
                    for (file in archives) {
                        runCatching { own.openArchive(file.path) }
                    }
                    if (archives.isNotEmpty()) withContext(background) { own.readCovering(area) }
                    if (from is Source.Server) {
                        for (tile in own.tilesCovering(area)) {
                            // Without a network the other tiles would not come either.
                            if (withContext(background) { bring(tile, from.template, own) } == Came.NO_ANSWER) break
                        }
                    }
                    Ground(own, isOwn = true)
                } catch (error: Throwable) {
                    own.close()
                    throw error
                }
            }
        }

    /** Reads the archive that came with the app into the terrain, once. */
    private suspend fun readArchive(file: File) {
        if (archiveRead == file) return
        withContext(background) {
            runCatching { terrain.addArchive(file.readBytes()) }
        }
        // Only once it is read: a reading cut short is done again.
        archiveRead = file
    }

    /** What became of the wish for one tile. */
    private enum class Came { YES, REFUSED, NO_ANSWER }

    /** One tile: from the phone's store, else from the server and into the store. */
    private suspend fun bring(tile: TerrainTile, template: String, into: Terrain): Came {
        val kept = folder?.resolve("${tile.z}-${tile.x}-${tile.y}.webp")
        if (kept != null && kept.isFile && take(tile, runCatching { kept.readBytes() }.getOrNull(), into)) {
            return Came.YES
        }
        val answer = try {
            transport(template.replace("{z}/{x}/{y}", tile.path))
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            return Came.NO_ANSWER
        }
        if (answer.status != HTTP_OK || !take(tile, answer.body, into)) return Came.REFUSED
        if (kept != null) {
            // Written beside and moved into place: a tile cut short by the
            // app's end is not taken for a whole one the next time.
            runCatching {
                kept.parentFile?.mkdirs()
                val part = kept.resolveSibling(kept.name + ".part")
                part.writeBytes(answer.body)
                if (!part.renameTo(kept)) part.delete()
            }
        }
        return Came.YES
    }

    /** Hands a tile's bytes to the core; `false` when they are no elevation tile. */
    private fun take(tile: TerrainTile, image: ByteArray?, into: Terrain): Boolean =
        image != null && runCatching { into.addTile(tile.z, tile.x, tile.y, image) }.isSuccess

    companion object {
        private const val HTTP_OK = 200

        /** At how many moments of a flight the ground is read for its profile, as on the iPhone. */
        const val PLACES_UNDER_A_FLIGHT: UInt = 240u

        /** How long a tile may take to arrive. */
        const val ANSWER_WITHIN_MILLISECONDS = 20_000

        /** Fetches with the app's own networking, which keeps nothing: the tile is kept here. */
        val overHttp: TerrainTransport = { url -> Http.get(url, ANSWER_WITHIN_MILLISECONDS, accept = null) }
    }
}
