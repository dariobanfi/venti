package fly.venti.app.radar

import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.radarPath
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.io.File
import java.io.IOException
import java.util.concurrent.CopyOnWriteArrayList
import java.util.zip.CRC32
import java.util.zip.DeflaterOutputStream
import javax.imageio.ImageIO
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The rain radar against the real core, with the server played by the
 * answer Venti's own tests keep for `GET /v1/radar` (two frames, of 11:35
 * and 11:40 UTC of 2026-11-21, by a clock that stands at 11:44) and a
 * small picture of steps made here. The pilot's clock is an hour ahead of
 * UTC, as in winter in the Alps.
 */
class RadarModelTest {
    /** The server, and a list of what it was asked. */
    private class Server {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var list = HttpAnswer(200, RECORDED.toByteArray())

        @Volatile
        var picture = HttpAnswer(200, picture())

        @Volatile
        var isReached = true

        /** What was asked, without the server's address. */
        val paths: List<String> get() = asked.map { it.removePrefix(BASE) }

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReached) throw IOException("no route to the server")
            return if (url.endsWith(".png")) picture else list
        }
    }

    private fun model(server: Server, now: Long = BEFORE) =
        RadarModel(
            transport = server::answer,
            pictures = server::answer,
            background = Dispatchers.Unconfined,
            utcOffsetSeconds = { 3600 },
            stepMilliseconds = 1,
        ).also {
            it.server = { BASE }
            it.clock = { now }
        }

    @Test
    fun theLayerOpensOnTheNewestFrameWithTheCoresWords() = runBlocking {
        val server = Server()
        val radar = model(server)
        assertEquals(RadarState.Asking, radar.state)
        radar.load()

        assertEquals(listOf(radarPath()), server.paths)
        val state = radar.state as RadarState.There
        assertEquals("dwd-radar-ry", state.layer.source)
        assertEquals(listOf("12:35", "12:40"), state.layer.frames.map { it.clock })
        // A clock behind the server's counts from the server's: the newest frame is four minutes old.
        assertEquals("Rain radar, 12:40, 4 minutes ago", radar.frame?.label)
        assertNull(radar.chosenTime)
        assertFalse(state.layer.stale)
        // What a cell of the strip holds, and what TalkBack says of it.
        assertEquals(listOf("35", "40"), state.layer.frames.map(RadarWords::minutes))
        assertEquals("12:40, 4 minutes ago", RadarWords.spoken(state.layer.frames.last()))
        // The key from the most rain down, the line for what was not seen, the note and the credit.
        assertEquals(6, state.view.key.size)
        assertTrue(state.view.key.first().words.startsWith("Cloudburst"))
        assertEquals("Not seen by the radar", state.view.unseenWords)
        assertTrue(state.view.note.isNotEmpty())
        assertEquals("Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben", radar.creditLine)
    }

    @Test
    fun aFramesPictureIsFetchedOnceAndWrittenInTheAppsColours() = runBlocking {
        val server = Server()
        val radar = model(server)
        radar.load()
        radar.showFrame()

        val newest = "/v1/radar/dwd-radar-ry/20261121T1140Z.png"
        assertEquals(listOf(radarPath(), newest), server.paths)
        val shown = checkNotNull(radar.shown)
        assertEquals(newest, shown.frame.path)
        // Pinned by the layer's four edges.
        assertEquals(listOf(9.4, 46.291984, 17.2, 48.5), listOf(shown.west, shown.south, shown.east, shown.north))
        assertNull(radar.pictureProblem)

        // The picture's pixels are the values 0 to 7, row by row: no rain
        // stays clear, each step has the colour its line of the key names,
        // and what the radar did not see has the veil's.
        val read = checkNotNull(ImageIO.read(ByteArrayInputStream(shown.png)))
        val pixels = (0 until 2).flatMap { y -> (0 until 4).map { x -> read.getRGB(x, y) } }
        assertEquals(0, pixels[0] ushr 24)
        val state = radar.state as RadarState.There
        val bySteps = state.view.key.sortedBy { it.step }
        assertEquals(bySteps.map { RadarLayer.colour(it).toArgb() }, pixels.subList(1, 7))
        assertEquals(RadarLayer.UNSEEN.toArgb(), pixels[7])
        // The stand-ins the core names today: a trace in the grey of the vario scale, a cloudburst in the wind's deepest.
        assertEquals(VentiLight.vario[3].toArgb(), pixels[1])
        assertEquals(VentiLight.wind[6].toArgb(), pixels[6])

        // The other frame is fetched when it is chosen; back on the newest, nothing is asked again.
        radar.choose(0)
        assertEquals("12:35", radar.frame?.clock)
        radar.showFrame()
        assertEquals("Rain radar, 12:35, 9 minutes ago", radar.shownFrame?.label)
        radar.choose(1)
        assertNull(radar.chosenTime)
        radar.showFrame()
        assertSame(shown.png, radar.shown?.png)
        assertEquals(3, server.asked.size)
        assertEquals(2, radar.keptPictures)
    }

    @Test
    fun aNewFrameIsShownOnlyWhileTheNewestWas() = runBlocking {
        val server = Server()
        val radar = model(server)
        radar.load()
        radar.showFrame()

        // Five minutes on the server has a third frame: the newest was looked at, so the new one is.
        server.list = HttpAnswer(200, WITH_A_THIRD.toByteArray())
        radar.load()
        assertEquals("12:45", radar.frame?.clock)
        // Until its picture is there the map keeps the one before, under its own time.
        assertEquals("12:40", radar.shownFrame?.clock)
        assertEquals("Rain radar, 12:40, 9 minutes ago", radar.shownFrame?.label)
        radar.showFrame()
        assertEquals("12:45", radar.shownFrame?.clock)

        // An earlier frame the pilot chose stays chosen when the list is asked again.
        radar.choose(0)
        radar.load()
        assertEquals("12:35", radar.frame?.clock)

        // A list that no longer has it goes back to the newest, and lets its picture go.
        radar.showFrame()
        assertEquals(3, radar.keptPictures)
        server.list = HttpAnswer(200, WITH_A_THIRD.replace(Regex("""\{\s*"time": "2026-11-21T11:35:00Z".*?\},""", RegexOption.DOT_MATCHES_ALL), "").toByteArray())
        radar.load()
        assertEquals(listOf("12:40", "12:45"), radar.layer?.frames?.map { it.clock })
        assertNull(radar.chosenTime)
        assertEquals("12:45", radar.frame?.clock)
        assertEquals(2, radar.keptPictures)
    }

    @Test
    fun oldRainIsSaidToBeOld() = runBlocking {
        val server = Server()
        // Twenty-five minutes after the newest frame, by the device's clock.
        val radar = model(server, now = NEWEST + 25 * 60)
        radar.load()

        val layer = checkNotNull(radar.layer)
        assertTrue(layer.stale)
        assertEquals("The newest radar picture is 25 minutes old. It is not the rain of now.", layer.staleWords)
    }

    @Test
    fun aServerWithoutARadarSaysSoAndOneNotReachedToo() = runBlocking {
        val server = Server()
        val radar = model(server)
        server.list = HttpAnswer(501, """{"error":{"code":"no_radar","message":"This server collects no rain radar."}}""".toByteArray())
        radar.load()
        assertEquals(RadarState.Failed("This server has no rain radar."), radar.state)
        assertNull(radar.creditLine)

        server.isReached = false
        radar.load()
        val failed = radar.state as RadarState.Failed
        assertTrue(failed.message, failed.message.startsWith(RadarWords.UNREACHABLE))

        // "Try again", with the server back.
        server.isReached = true
        server.list = HttpAnswer(200, RECORDED.toByteArray())
        radar.load()
        assertTrue(radar.state is RadarState.There)
        assertNull(radar.problem)

        // What was fetched stays when a later asking fails.
        server.isReached = false
        radar.load()
        assertTrue(radar.state is RadarState.There)
        assertNotNull(radar.problem)
    }

    @Test
    fun aPictureThatCannotBeFetchedLeavesTheOneBeforeOnTheMap() = runBlocking {
        val server = Server()
        val radar = model(server)
        radar.load()
        radar.showFrame()
        val newest = checkNotNull(radar.shown)

        server.picture = HttpAnswer(503, ByteArray(0))
        radar.choose(0)
        radar.showFrame()
        assertSame(newest, radar.shown)
        assertEquals("12:40", radar.shownFrame?.clock)
        assertEquals("The server is busy or being restarted. Try again in a moment.", radar.pictureProblem)

        // Something that is no picture is not drawn either.
        server.picture = HttpAnswer(200, "not a picture".toByteArray())
        radar.showFrame()
        assertSame(newest, radar.shown)
        assertEquals("The radar picture could not be read.", radar.pictureProblem)

        // "Try again", with the pictures back.
        server.picture = HttpAnswer(200, picture())
        radar.showFrame()
        assertEquals("12:35", radar.shownFrame?.clock)
        assertNull(radar.pictureProblem)
    }

    @Test
    fun playingStepsThroughTheFramesAndStopsOnTheNewest() = runBlocking {
        val server = Server()
        val radar = model(server)
        server.list = HttpAnswer(200, WITH_A_THIRD.toByteArray())
        radar.load()
        radar.choose(1)

        radar.play()
        assertFalse(radar.isPlaying)
        // Every frame's picture was fetched, the oldest first, and the newest is looked at as the newest.
        assertEquals(
            listOf("1135", "1140", "1145").map { "/v1/radar/dwd-radar-ry/20261121T${it}Z.png" },
            server.paths.drop(1),
        )
        assertNull(radar.chosenTime)
        assertEquals("12:45", radar.shownFrame?.clock)
    }

    @Test
    fun anotherServersRainIsNotThisOnes() = runBlocking {
        val server = Server()
        val radar = model(server)
        radar.load()
        radar.showFrame()
        assertNotNull(radar.shown)

        radar.server = { "http://another.example" }
        server.isReached = false
        radar.load()
        assertNull(radar.shown)
        assertNull(radar.view)
        assertEquals(0, radar.keptPictures)
        assertTrue(radar.state is RadarState.Failed)
    }

    @Test
    fun theLayerIsOffUntilThePilotSwitchesItOn() {
        val preferences = MemoryPreferences()
        assertFalse(preferences.mapShowsRadar)
        preferences.mapShowsRadar = true
        assertTrue(preferences.mapShowsRadar)
    }

    @Test
    fun aColourIsHandedToTheCoreAsRedGreenBlueAndHowSolid() {
        assertEquals(0x11223344u, RadarLayer.rgba(androidx.compose.ui.graphics.Color(0x44112233)))
        // Solid, since the layer as a whole is see-through.
        assertEquals(0xFFu, RadarLayer.colours.unseen and 0xFFu)
    }

    companion object {
        private const val BASE = "http://venti.example"

        /** 11:40 UTC of 2026-11-21, the newest recorded frame's time. */
        private const val NEWEST = 1_795_261_200L

        /** A device whose clock is behind the server's: the ages are counted from the server's 11:44. */
        private const val BEFORE = NEWEST - 3 * 86_400

        private val RECORDED: String by lazy { File(TestSupport.recordedAnswers, "radar.json").readText() }

        /** The recorded answer five minutes on: a third frame, of 11:45, by a clock at 11:49. */
        private val WITH_A_THIRD: String by lazy {
            val third = """, {"time": "2026-11-21T11:45:00Z", "interval_s": 300, """ +
                """"path": "/v1/radar/dwd-radar-ry/20261121T1145Z.png", "bytes": 4500}"""
            RECORDED
                .replace("\"time\": \"2026-11-21T11:44:00Z\"", "\"time\": \"2026-11-21T11:49:00Z\"")
                .replace(Regex("""("bytes": 4511\s*\})"""), "$1$third")
                .also { check(it.contains("1145Z") && it.contains("11:49:00Z")) { "the recorded answer has changed" } }
        }

        /**
         * A picture as a server writes one: four by two pixels with the
         * values 0 to 7, a palette of eight greys, one byte a pixel.
         */
        fun picture(): ByteArray {
            val out = ByteArrayOutputStream()
            out.write(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A))
            fun chunk(name: String, data: ByteArray) {
                val named = name.toByteArray(Charsets.US_ASCII) + data
                DataOutputStream(out).apply {
                    writeInt(data.size)
                    write(named)
                    writeInt(CRC32().apply { update(named) }.value.toInt())
                }
            }
            chunk("IHDR", byteArrayOf(0, 0, 0, 4, 0, 0, 0, 2, 8, 3, 0, 0, 0))
            chunk("PLTE", ByteArray(24) { (it / 3 * 30).toByte() })
            // Two rows, each a filter byte and four pixels.
            val rows = byteArrayOf(0, 0, 1, 2, 3, 0, 4, 5, 6, 7)
            chunk("IDAT", ByteArrayOutputStream().also { packed -> DeflaterOutputStream(packed).use { it.write(rows) } }.toByteArray())
            chunk("IEND", ByteArray(0))
            return out.toByteArray()
        }
    }
}
