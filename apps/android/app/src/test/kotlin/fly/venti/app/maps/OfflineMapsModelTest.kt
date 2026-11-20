package fly.venti.app.maps

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.core.Coordinate
import fly.venti.core.MapRegionState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.io.IOException

/**
 * Maps for flying without a network, against the real core. Venti is played
 * by the list its own tests keep and the region that comes with the app,
 * which is what that list describes.
 */
class OfflineMapsModelTest {
    private val scope = CoroutineScope(Dispatchers.Default)

    @After
    fun stop() {
        scope.cancel()
    }

    private val region: File get() = TestSupport.assets.resolve("MapDemo")

    /** Plays the server: the list, and each file from a byte on. `breakAfter` bytes, then the line drops, once. */
    private class Server(private val region: File, var breakAfter: Long? = null, var corrupt: Boolean = false) {
        val asked = mutableListOf<Pair<String, Long>>()

        val transport: suspend (String) -> HttpAnswer = { url ->
            check(url.endsWith("/v1/maps")) { url }
            HttpAnswer(200, TestSupport.recordedAnswers.resolve("maps.json").readBytes())
        }

        val fetch: MapFetch = { url, from, take ->
            synchronized(asked) { asked += url to from }
            val bytes = region.resolve(url.substringAfterLast('/')).readBytes()
            if (corrupt) bytes[bytes.size / 2] = (bytes[bytes.size / 2] + 1).toByte()
            val rest = bytes.copyOfRange(from.toInt(), bytes.size)
            val cut = breakAfter
            if (cut != null && cut < rest.size) {
                breakAfter = null
                take(rest, cut.toInt())
                throw IOException("The line dropped.")
            }
            take(rest, rest.size)
            if (from > 0) 206 else 200
        }
    }

    private suspend fun OfflineMapsModel.untilDone(id: String) = withTimeout(30_000) {
        delay(50)
        while (isDownloading(id)) delay(50)
    }

    @Test
    fun aRegionIsDownloadedCheckedKeptAndDrawnFrom() = runBlocking {
        val folder = TestSupport.newFolder("maps")
        val server = Server(region, breakAfter = 1_000_000)
        val maps = OfflineMapsModel(folder, scope, server.transport, server.fetch)
        maps.server = { "http://127.0.0.1:8080" }
        var archives: List<File> = emptyList()
        maps.onKeptChanged = { archives = it }
        assertEquals("None", maps.inALine)

        maps.refresh()
        val offered = maps.rows.single()
        assertEquals("Tegernsee valley (demo)", offered.region.name)
        assertEquals(MapRegionState.OFFERED, offered.state)

        // The line drops after a megabyte: what has arrived is kept.
        maps.download(offered.region)
        maps.untilDone(offered.region.id)
        val stopped = maps.progress[offered.region.id] as OfflineMapsModel.Progress.Stopped
        assertTrue(stopped.words, stopped.words.contains("What has arrived is kept"))
        assertTrue(maps.kept.isEmpty())

        // Going on asks for the rest only.
        maps.download(offered.region)
        maps.untilDone(offered.region.id)
        assertNull(maps.progress[offered.region.id])
        assertTrue(server.asked.any { (url, from) -> url.endsWith("basemap.pmtiles") && from == 1_000_000L })
        val kept = maps.kept.single()
        assertEquals("Tegernsee valley (demo)", maps.inALine)
        assertEquals(MapRegionState.KEPT, maps.rows.single().state)
        assertTrue(maps.rows.single().words.startsWith("On this device"))
        assertEquals(listOf("terrain.pmtiles"), archives.map { it.name })
        assertTrue(archives.single().isFile)

        // Over the Tegernsee the cockpit draws from it; far away it does not.
        maps.flying(Coordinate(lat = 47.69, lon = 11.76))
        assertEquals(kept.id, checkNotNull(maps.flyingMap).id)
        assertTrue(checkNotNull(maps.flyingMap).basemap.isFile)
        maps.flying(Coordinate(lat = 46.0, lon = 8.0))
        assertNull(maps.flyingMap)
        // Maps for looking around use it only without a network.
        assertNull(maps.browsingMap())
        maps.isOnline = false
        assertNotNull(maps.browsingMap())

        // Read again from the folder, as at the next start.
        assertEquals(listOf(kept), OfflineMapsModel.regions(folder))

        maps.delete(kept)
        assertTrue(maps.kept.isEmpty())
        assertEquals(MapRegionState.OFFERED, maps.rows.single().state)
        assertTrue(archives.isEmpty())
    }

    @Test
    fun aDownloadThatIsNotVentisIsThrownAway() = runBlocking {
        val folder = TestSupport.newFolder("maps")
        val server = Server(region, corrupt = true)
        val maps = OfflineMapsModel(folder, scope, server.transport, server.fetch)
        maps.server = { "http://127.0.0.1:8080" }
        maps.refresh()
        val region = maps.rows.single().region
        maps.download(region)
        maps.untilDone(region.id)
        val stopped = maps.progress[region.id] as OfflineMapsModel.Progress.Stopped
        assertTrue(stopped.words, stopped.words.contains("not what the server described"))
        assertTrue(maps.kept.isEmpty())
        assertFalse(OfflineMapsModel.incoming(folder).resolve(region.id).resolve(region.version).resolve("basemap.pmtiles").exists())
    }

    @Test
    fun withoutAServerNothingIsOffered() = runBlocking {
        val maps = OfflineMapsModel(TestSupport.newFolder("maps"), scope)
        maps.refresh()
        assertNull(maps.offered)
        assertTrue(maps.rows.isEmpty())
    }
}
