package fly.venti.app.maps

import android.annotation.SuppressLint
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.MapFileView
import fly.venti.core.MapRegionRow
import fly.venti.core.MapRegionView
import fly.venti.core.checkMapFile
import fly.venti.core.mapDownloadWords
import fly.venti.core.mapRegionCovering
import fly.venti.core.mapRegionFromRecord
import fly.venti.core.mapRegionRecord
import fly.venti.core.mapRegions
import fly.venti.core.mapRows
import fly.venti.core.mapsPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URI
import kotlin.coroutines.coroutineContext

/** A region kept on the phone, as a map draws from it: its two archives. */
data class KeptMap(val id: String, val version: String, val basemap: File, val terrain: File)

/**
 * Fetches a file from one of its bytes on and hands it over in pieces as
 * they arrive. Returns the HTTP status. Pieces are handed over only of an
 * answer that is the file from that byte on: status 200 for a fetch from
 * the start, 206 for one from further in.
 */
typealias MapFetch = suspend (url: String, from: Long, take: (ByteArray, Int) -> Unit) -> Int

/**
 * Maps for flying without a network: the regions Venti hands out, and the
 * ones kept on the phone.
 *
 * A region is two files, the base map and the elevation. The core reads
 * the list, checks a download against it and says which kept region covers
 * a place; this class fetches the files, keeps them, and says what a map
 * draws from. A download runs while the app runs. When it breaks off, what
 * has arrived stays, and the download goes on from there the next time.
 */
class OfflineMapsModel(
    /** Where regions are kept; `null` keeps none. */
    private val folder: File?,
    private val scope: CoroutineScope,
    private val transport: suspend (String) -> HttpAnswer = { url -> Http.get(url) },
    private val fetch: MapFetch = ::fetchOverHttp,
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** Where a download stands. */
    sealed interface Progress {
        data class Downloading(val received: Long, val total: Long) : Progress {
            val words: String get() = mapDownloadWords(received.toULong(), total.toULong())
            val share: Float get() = if (total > 0) (received.toFloat() / total).coerceIn(0f, 1f) else 0f
        }

        /** Everything has arrived and is being checked against what Venti said. */
        data object Checking : Progress

        /** It stopped, and the words say why. What arrived is kept. */
        data class Stopped(val words: String) : Progress
    }

    /** The regions kept on the phone, by name. */
    var kept: List<MapRegionView> by mutableStateOf(regions(folder))
        private set

    /** What Venti hands out. `null` until it has answered. */
    var offered: List<MapRegionView>? by mutableStateOf(null)
        private set

    var isLoading: Boolean by mutableStateOf(false)
        private set

    /** Why the list is not there, in words for the pilot. */
    var problem: String? by mutableStateOf(null)
        private set

    /** The downloads under way or stopped, by region. */
    val progress = mutableStateMapOf<String, Progress>()

    /** Whether the phone has a network, as far as it knows. */
    var isOnline: Boolean by mutableStateOf(true)

    /** What the cockpit's map is drawn from: the kept region that covers the glider, where there is one. */
    var flyingMap: KeptMap? by mutableStateOf(null)
        private set

    /** The server to ask. The app sets this. */
    var server: () -> String? = { null }

    /** Called with the elevation archives of the kept regions whenever the regions change. */
    var onKeptChanged: (List<File>) -> Unit = {}

    private val downloads = java.util.concurrent.ConcurrentHashMap<String, Job>()

    /** The list for the pilot: what the phone has first, then what else Venti hands out. */
    val rows: List<MapRegionRow> get() = mapRows(offered.orEmpty(), kept)

    /** The kept regions in a line: "None", a region's name, or how many. */
    val inALine: String
        get() = when (kept.size) {
            0 -> "None"
            1 -> kept.single().name
            else -> "${kept.size} regions"
        }

    /** The elevation archives of the kept regions. */
    val terrainArchives: List<File>
        get() = folder?.let { home -> kept.map { place(it, home).resolve(it.terrain.name) } }.orEmpty()

    /** Asks Venti which regions it hands out. */
    suspend fun refresh() {
        val server = server()
        if (server == null) {
            offered = null
            problem = null
            return
        }
        isLoading = true
        try {
            val answer = transport(server + mapsPath())
            val regions = withContext(background) { mapRegions(answer.status.coerceIn(0, 65535).toUShort(), answer.body) }
            if (server != server()) return
            offered = regions
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = if (error is CoreException) {
                error.pilotMessage
            } else {
                "Venti could not be reached for its maps. ${error.pilotMessage}"
            }
        } finally {
            isLoading = false
        }
    }

    /** The cockpit says where the glider is: its map is drawn from the kept region that covers the place. */
    fun flying(at: Coordinate) {
        val region = mapRegionCovering(kept, at.lat, at.lon)
        val drawnFrom = region?.let(::map)
        if (drawnFrom != flyingMap) flyingMap = drawnFrom
    }

    /**
     * What a map for looking around is drawn from: the network while there
     * is one, since it reaches everywhere; without one, the first kept
     * region.
     */
    fun browsingMap(): KeptMap? = if (isOnline) null else kept.firstOrNull()?.let(::map)

    /** A kept region as a map draws from it. */
    fun map(region: MapRegionView): KeptMap? {
        val home = folder ?: return null
        val place = place(region, home)
        return KeptMap(region.id, region.version, place.resolve(region.basemap.name), place.resolve(region.terrain.name))
    }

    /** Downloads a region, or goes on with a download that stopped. */
    fun download(region: MapRegionView) {
        if (downloads.containsKey(region.id)) return
        val server = server()
        val home = folder
        if (server == null || home == null) {
            progress[region.id] = Progress.Stopped("There is nothing to download from.")
            return
        }
        progress[region.id] = Progress.Downloading(0, region.bytes.toLong())
        // Started once it is noted, so that its end cannot come before.
        val job = scope.launch(start = CoroutineStart.LAZY) {
            try {
                val stopped = withContext(background) { bring(region, server, home) }
                if (stopped != null) {
                    progress[region.id] = stopped
                } else {
                    progress.remove(region.id)
                    keptChanged()
                }
            } catch (error: CancellationException) {
                // Stopped by the pilot: what arrived stays for the next time.
                progress.remove(region.id)
            } finally {
                downloads.remove(region.id)
            }
        }
        downloads[region.id] = job
        job.start()
    }

    /** Stops a download. What has arrived stays for the next time. */
    fun stop(id: String) {
        downloads[id]?.cancel()
    }

    /** Whether a region is being downloaded. */
    fun isDownloading(id: String): Boolean = downloads.containsKey(id)

    /** Removes a region from the phone, with anything of it that was under way. */
    fun delete(region: MapRegionView) {
        val home = folder ?: return
        downloads[region.id]?.cancel()
        home.resolve(region.id).deleteRecursively()
        incoming(home).resolve(region.id).deleteRecursively()
        progress.remove(region.id)
        keptChanged()
    }

    private fun keptChanged() {
        kept = regions(folder)
        // A map is not left drawing from files that are gone.
        val drawn = flyingMap
        if (drawn != null && kept.none { it.id == drawn.id && it.version == drawn.version }) flyingMap = null
        onKeptChanged(terrainArchives)
    }

    /**
     * Brings a region to the phone: fetches its two files, going on from
     * what an earlier attempt left, checks them against what Venti said,
     * and puts them in the region's place. Returns `null` when the region
     * is kept, or why it stopped.
     */
    // The free space is asked only to refuse a download early that cannot
    // fit; Android's cache that it could clear makes a download fit more
    // often, never less.
    @SuppressLint("UsableSpace")
    private suspend fun bring(region: MapRegionView, server: String, home: File): Progress.Stopped? {
        val gathering = incoming(home).resolve(region.id).resolve(region.version)
        val parts = listOf(region.basemap, region.terrain)
        try {
            gathering.mkdirs()
            // Room for what is still to come, and a little more.
            val here = parts.sumOf { gathering.resolve(it.name).length() }
            if (home.usableSpace in 1 until region.bytes.toLong() - here + ROOM_TO_SPARE) {
                return Progress.Stopped("There is not enough room on this phone: the map needs ${region.size}.")
            }
            var before = 0L
            for (file in parts) {
                val done = before
                bring(file, server, gathering.resolve(file.name)) { received ->
                    progress[region.id] = Progress.Downloading(done + received, region.bytes.toLong())
                }
                before += file.bytes.toLong()
            }
            coroutineContext.ensureActive()
            progress[region.id] = Progress.Checking
            for (file in parts) {
                val part = gathering.resolve(file.name)
                try {
                    checkMapFile(part.path, file)
                } catch (error: CoreException) {
                    // A file that is not Venti's is of no use, and going on
                    // from it would not mend it.
                    part.delete()
                    return Progress.Stopped(error.pilotMessage)
                }
            }
            gathering.resolve(RECORD).writeBytes(mapRegionRecord(region))
            // Into the region's place; an older version goes once the new one is there.
            val regionHome = home.resolve(region.id).apply { mkdirs() }
            val place = regionHome.resolve(region.version)
            place.deleteRecursively()
            if (!gathering.renameTo(place)) {
                return Progress.Stopped("The downloaded map could not be put in its place on this phone.")
            }
            regionHome.listFiles()?.filter { it.name != region.version }?.forEach { it.deleteRecursively() }
            return null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Stop) {
            return Progress.Stopped(error.words)
        } catch (error: Exception) {
            return Progress.Stopped(
                "The download stopped: ${error.pilotMessage} What has arrived is kept, and the download goes on from there.",
            )
        }
    }

    /** A download that cannot go on, with the reason for the pilot. */
    private class Stop(val words: String) : Exception(words)

    /** Brings one file into its place among a download's parts, going on from the bytes that are there already. */
    private suspend fun bring(file: MapFileView, server: String, part: File, tell: (Long) -> Unit) {
        val total = file.bytes.toLong()
        // Twice at the most: once more from the start when the server does
        // not go on from where the file stopped.
        repeat(2) {
            var have = part.length()
            if (have > total) {
                part.delete()
                have = 0
            }
            if (have == total) {
                tell(total)
                return
            }
            val status = FileOutputStream(part, true).use { out ->
                var written = have
                var told = 0L
                fetch(server + file.path, have) { bytes, count ->
                    out.write(bytes, 0, count)
                    written += count
                    // A few times a second is enough for a bar that fills.
                    val now = System.nanoTime()
                    if (now - told > 250_000_000L) {
                        told = now
                        tell(written)
                    }
                }
            }
            when {
                (status == 206 && have > 0) || (status == 200 && have == 0L) -> {
                    if (part.length() == total) {
                        tell(total)
                        return
                    }
                    throw Stop(
                        "The download stopped before the map was whole. What has arrived is kept, and the download " +
                            "goes on from there.",
                    )
                }
                // Not the file from where it stopped: from the start, then.
                status == 200 || status == 206 || status == 416 -> part.delete()
                status == 404 -> throw Stop("Venti no longer has this map. Pull down to see what it has now.")
                else -> throw Stop("Venti refused the download (status $status).")
            }
        }
        throw Stop("Venti does not hand this map out in a way that can be downloaded.")
    }

    companion object {
        /** The name of the file in a region's folder that says what the region is. */
        const val RECORD = "region.json"

        /** What is left free on the phone beyond a download, bytes. */
        private const val ROOM_TO_SPARE = 50_000_000L

        /** The folder of a kept region: its identifier, then its version. */
        fun place(region: MapRegionView, home: File): File = home.resolve(region.id).resolve(region.version)

        /** Where downloads are put together before they are whole. No region's identifier begins with a dot. */
        fun incoming(home: File): File = home.resolve(".incoming")

        /** The regions whose files are whole in a folder, by name. */
        fun regions(home: File?): List<MapRegionView> {
            val ids = home?.listFiles()?.filter { it.isDirectory && !it.name.startsWith(".") }.orEmpty()
            val found = mutableListOf<MapRegionView>()
            for (id in ids) {
                for (version in id.listFiles()?.filter { it.isDirectory }.orEmpty()) {
                    val record = version.resolve(RECORD)
                    val region = runCatching { mapRegionFromRecord(record.readBytes()) }.getOrNull() ?: continue
                    if (region.id != id.name || region.version != version.name) continue
                    if (version.resolve(region.basemap.name).length() != region.basemap.bytes.toLong()) continue
                    if (version.resolve(region.terrain.name).length() != region.terrain.bytes.toLong()) continue
                    found += region
                    // One version of a region is kept.
                    break
                }
            }
            return found.sortedBy { it.name }
        }

        /**
         * Fetches with the platform's networking, piece by piece, so that a
         * file of hundreds of megabytes never lies in memory.
         */
        suspend fun fetchOverHttp(url: String, from: Long, take: (ByteArray, Int) -> Unit): Int {
            if (!Http.mayBeAsked(url)) throw IOException("This address is not asked without encryption.")
            val connection = URI(url).toURL().openConnection() as HttpURLConnection
            try {
                // A minute without a byte ends the attempt; the whole may take hours.
                connection.connectTimeout = 20_000
                connection.readTimeout = 60_000
                connection.useCaches = false
                // The bytes as they are: a packed answer could not be gone on with.
                connection.setRequestProperty("Accept-Encoding", "identity")
                if (from > 0) connection.setRequestProperty("Range", "bytes=$from-")
                val status = connection.responseCode
                val wanted = if (from > 0) 206 else 200
                if (status != wanted) return status
                connection.inputStream.use { input ->
                    val buffer = ByteArray(64 * 1024)
                    while (true) {
                        coroutineContext.ensureActive()
                        val read = input.read(buffer)
                        if (read < 0) break
                        take(buffer, read)
                    }
                }
                return status
            } finally {
                connection.disconnect()
            }
        }
    }
}
