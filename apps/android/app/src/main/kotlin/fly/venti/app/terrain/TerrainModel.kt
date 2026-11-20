package fly.venti.app.terrain

import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.core.Coordinate
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
            // Asking may copy the app's region out of the app: not here.
            when (val from = withContext(background) { source() }) {
                Source.Nowhere -> Unit
                is Source.Archive -> if (archiveRead != from.file) {
                    archiveRead = from.file
                    withContext(background) {
                        runCatching { terrain.addArchive(from.file.readBytes()) }
                    }
                }
                is Source.Server -> {
                    for (tile in terrain.tilesWanted(around.lat, around.lon)) {
                        val came = withContext(background) { bring(tile, from.template) }
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

    /** What became of the wish for one tile. */
    private enum class Came { YES, REFUSED, NO_ANSWER }

    /** One tile: from the phone's store, else from the server and into the store. */
    private suspend fun bring(tile: TerrainTile, template: String): Came {
        val kept = folder?.resolve("${tile.z}-${tile.x}-${tile.y}.webp")
        if (kept != null && kept.isFile && take(tile, runCatching { kept.readBytes() }.getOrNull())) {
            return Came.YES
        }
        val answer = try {
            transport(template.replace("{z}/{x}/{y}", tile.path))
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            return Came.NO_ANSWER
        }
        if (answer.status != HTTP_OK || !take(tile, answer.body)) return Came.REFUSED
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
    private fun take(tile: TerrainTile, image: ByteArray?): Boolean =
        image != null && runCatching { terrain.addTile(tile.z, tile.x, tile.y, image) }.isSuccess

    companion object {
        private const val HTTP_OK = 200

        /** How long a tile may take to arrive. */
        const val ANSWER_WITHIN_MILLISECONDS = 20_000

        /** Fetches with the app's own networking, which keeps nothing: the tile is kept here. */
        val overHttp: TerrainTransport = { url -> Http.get(url, ANSWER_WITHIN_MILLISECONDS, accept = null) }
    }
}
