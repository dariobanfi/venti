package fly.venti.app.radar

import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.core.radarPath
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.zip.CRC32
import java.util.zip.DeflaterOutputStream
import javax.imageio.ImageIO
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The rain radar against the real core, with the server played by the
 * answer Venti's own tests keep for `GET /v1/radar`, by a clock that
 * stands at 13:19 UTC of 2026-11-21: two layers, Germany's radars with
 * frames of 13:10 and 13:15 and, under them, Austria's analysis with
 * frames of 12:45 and 13:00. A small picture of steps is made here. The
 * pilot's clock is an hour ahead of UTC, as in winter in the Alps.
 */
class RadarModelTest {
    /** The server, and a list of what it was asked. */
    private class Server {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var list = HttpAnswer(200, RECORDED.toByteArray())

        @Volatile
        var picture = HttpAnswer(200, picture())

        /** The pictures of frames that have another than that one, by their paths. */
        val pictureOf = ConcurrentHashMap<String, ByteArray>()

        @Volatile
        var isReached = true

        /** What was asked, without the server's address. */
        val paths: List<String> get() = asked.map { it.removePrefix(BASE) }

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReached) throw IOException("no route to the server")
            if (!url.endsWith(".png")) return list
            return pictureOf[url.removePrefix(BASE)]?.let { HttpAnswer(200, it) } ?: picture
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
    fun theLayerOpensOnTheNewestMomentWithTheCoresWords() = runBlocking {
        val server = Server()
        val radar = model(server)
        assertEquals(RadarState.Asking, radar.state)
        radar.load()

        assertEquals(listOf(radarPath()), server.paths)
        val state = radar.state as RadarState.There
        // The layers in the order to draw them: the analysis, and the radars over it.
        assertEquals(listOf(ANALYSIS, RADARS), state.view.layers.map { it.source })
        assertEquals(
            listOf("Austria's analysis of radar and gauges, every 15 minutes", "Germany's radars, every 5 minutes"),
            state.view.layers.map { it.what },
        )
        // One line of time: the radars' moments, the newest looked at.
        assertEquals(listOf("14:10", "14:15"), radar.moments.map { it.clock })
        assertEquals(1, radar.place)
        assertNull(radar.chosenTime)
        // A clock behind the server's counts from the server's: the newest moment is four minutes old.
        assertEquals("Rain radar, 14:15, 4 minutes ago. Austria's analysis is of 14:00.", radar.moment?.label)
        assertNull(radar.caution)
        // What TalkBack says of a moment, and the panel's one line of it.
        assertEquals(radar.moment?.label, RadarWords.spoken(radar.moments.last()))
        assertEquals("Rain, 14:15, measured", radar.moment?.line)
        // The key from the most rain down, the line for what was not seen, the note.
        assertEquals(6, state.view.key.size)
        assertTrue(state.view.key.first().words.startsWith("Cloudburst"))
        assertEquals("Not seen by the radar", state.view.unseenWords)
        assertTrue(state.view.note.isNotEmpty())
        // A credit for each layer the moment draws, the lowest first.
        assertEquals(
            listOf(
                "Data source: GeoSphere Austria - https://data.hub.geosphere.at",
                "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben",
            ),
            radar.creditLines,
        )
    }

    @Test
    fun aMomentDrawsBothLayersAndTheLowerIsClearWhereTheUpperSees() = runBlocking {
        val server = Server()
        // The analysis' pictures have the values 0 to 7, row by row, and
        // so has the radars' of 13:10. The radars' of 13:15 sees dry at
        // the first two places, rain at the next two and at the seventh,
        // and nothing at the others.
        server.pictureOf[radars("1315")] = picture(byteArrayOf(0, 0, 3, 1, 7, 7, 6, 7))
        val radar = model(server)
        radar.load()
        radar.showFrame()

        // The analysis the moment takes, then the radars' frame of the moment.
        assertEquals(listOf(radarPath(), analysis("1300"), radars("1315")), server.paths)
        val shown = checkNotNull(radar.shown)
        assertEquals(listOf(0, 1), shown.pictures.map { it.layer })
        assertEquals(listOf(analysis("1300"), radars("1315")), shown.pictures.map { it.path })
        assertEquals(listOf(true, false), shown.pictures.map { it.drawsUnseen })
        // Each pinned by its layer's four edges.
        for (picture in shown.pictures) {
            assertEquals(listOf(9.4, 46.291984, 17.2, 48.5), listOf(picture.west, picture.south, picture.east, picture.north))
        }
        assertNull(radar.pictureProblem)

        // No rain is clear and each step has the colour its line of the
        // key names. What was not seen has no colour in the upper picture,
        // whose veil would hide the lower's rain.
        val state = radar.state as RadarState.There
        val steps = listOf(CLEAR) + state.view.key.sortedBy { it.step }.map { RadarLayer.colour(it).toArgb() }
        val veil = RadarLayer.UNSEEN.toArgb()
        val (lower, upper) = shown.pictures.map { pixels(it.png) }
        assertEquals(listOf(CLEAR, CLEAR, steps[3], steps[1], CLEAR, CLEAR, steps[6], CLEAR), upper)
        // The lower picture was coloured under the upper's as the server
        // sent it: clear wherever the upper sees, dry over its rain and
        // rain over its rain, so that no place is drawn twice; its own
        // rain and its veil only where the upper saw nothing.
        assertEquals(listOf(CLEAR, CLEAR, CLEAR, CLEAR, steps[4], steps[5], CLEAR, veil), lower)
        // The stand-ins the core names today: a trace in the grey of the vario scale, a cloudburst in the wind's deepest.
        assertEquals(VentiLight.vario[3].toArgb(), upper[3])
        assertEquals(VentiLight.wind[6].toArgb(), upper[6])

        // The earlier moment has the same analysis under another frame of
        // the radars: only that is fetched, and the analysis' picture is
        // another, clear where that frame sees, which is all but the last
        // place.
        radar.choose(0)
        assertEquals("14:10", radar.moment?.clock)
        radar.showFrame()
        assertEquals("Rain radar, 14:10, 9 minutes ago. Austria's analysis is of 14:00.", radar.shownMoment?.label)
        val earlier = checkNotNull(radar.shown)
        assertEquals(listOf(analysis("1300"), radars("1310")), earlier.pictures.map { it.path })
        assertEquals(radars("1310"), server.paths.last())
        assertEquals(4, server.asked.size)
        assertNotSame(shown.pictures[0].png, earlier.pictures[0].png)
        assertEquals(List(7) { CLEAR } + veil, pixels(earlier.pictures[0].png))
        assertEquals(steps + CLEAR, pixels(earlier.pictures[1].png))
        // Back on the newest, nothing is asked again, and the pictures
        // are the ones kept: the analysis' by the frame that lies over it.
        radar.choose(1)
        assertNull(radar.chosenTime)
        radar.showFrame()
        assertSame(shown.pictures[0].png, radar.shown?.pictures?.get(0)?.png)
        assertSame(shown.pictures[1].png, radar.shown?.pictures?.get(1)?.png)
        radar.choose(0)
        radar.showFrame()
        assertSame(earlier.pictures[0].png, radar.shown?.pictures?.get(0)?.png)
        assertEquals(4, server.asked.size)
        // The analysis under each of the two frames, and those two.
        assertEquals(4, radar.keptPictures)
    }

    @Test
    fun aLayerWithoutAPictureAtTheMomentIsNotDrawn() = runBlocking {
        val server = Server()
        val radar = model(server)
        // The analysis' frames are of an hour earlier: too old to lie under the radars' moments.
        server.list = HttpAnswer(200, WITH_AN_OLD_ANALYSIS.toByteArray())
        radar.load()
        radar.showFrame()

        assertEquals("Rain radar, 14:15, 4 minutes ago", radar.moment?.label)
        val shown = checkNotNull(radar.shown)
        assertEquals(listOf(1), shown.pictures.map { it.layer })
        assertEquals(listOf(radars("1315")), shown.pictures.map { it.path })
        // The radars' picture is the lowest now, so it veils what it did not see: never shown as dry.
        assertEquals(listOf(true), shown.pictures.map { it.drawsUnseen })
        assertEquals(RadarLayer.UNSEEN.toArgb(), pixels(shown.pictures[0].png)[7])
        // Only the layer drawn is credited.
        assertEquals(listOf("Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben"), radar.creditLines)
        // The analysis is behind, the radars on top are not: the card has no caution.
        assertEquals(listOf(true, false), radar.view?.layers?.map { it.stale })
        assertNull(radar.caution)
    }

    @Test
    fun aNewFrameIsShownOnlyWhileTheNewestWas() = runBlocking {
        val server = Server()
        val radar = model(server)
        radar.load()
        radar.showFrame()

        // Five minutes on the server has a third frame of the radars: the newest was looked at, so the new one is.
        server.list = HttpAnswer(200, WITH_A_THIRD.toByteArray())
        radar.load()
        assertEquals("14:20", radar.moment?.clock)
        // Until its pictures are there the map keeps those before, under their own time.
        assertEquals("Rain radar, 14:15, 9 minutes ago. Austria's analysis is of 14:00.", radar.shownMoment?.label)
        radar.showFrame()
        assertEquals("14:20", radar.shownMoment?.clock)

        // An earlier moment the pilot chose stays chosen when the list is asked again.
        radar.choose(0)
        radar.load()
        assertEquals("14:10", radar.moment?.clock)

        // A list that no longer has it goes back to the newest, and lets its picture go.
        radar.showFrame()
        // The analysis under each of the three frames, and those three.
        assertEquals(6, radar.keptPictures)
        server.list = HttpAnswer(200, WITH_A_THIRD.replace(Regex("""\{\s*"time": "2026-11-21T13:10:00Z".*?\},""", RegexOption.DOT_MATCHES_ALL), "").toByteArray())
        radar.load()
        assertEquals(listOf("14:15", "14:20"), radar.moments.map { it.clock })
        assertNull(radar.chosenTime)
        assertEquals("14:20", radar.moment?.clock)
        // The frame that went, and the analysis as it lay under it.
        assertEquals(4, radar.keptPictures)
    }

    @Test
    fun oldRainIsSaidToBeOldByTheTopmostLayerThatHasFrames() = runBlocking {
        val server = Server()
        // Twenty-five minutes after the radars' newest frame, by the device's clock.
        val radar = model(server, now = NEWEST + 25 * 60)
        radar.load()

        // The radars are behind by their own five minutes; the analysis, forty minutes old, is not by its fifteen.
        assertEquals(listOf(false, true), radar.view?.layers?.map { it.stale })
        assertEquals("The newest radar picture is 25 minutes old. It is not the rain of now.", radar.caution)

        // A layer without frames says nothing of the rain: the analysis under it speaks for the card.
        val alone = model(server)
        server.list = HttpAnswer(200, WITHOUT_THE_RADARS.toByteArray())
        alone.load()
        assertEquals(listOf("13:45", "14:00"), alone.moments.map { it.clock })
        assertEquals("Rain analysis, 14:00, 19 minutes ago", alone.moment?.label)
        assertEquals(listOf(false, true), alone.view?.layers?.map { it.stale })
        assertNull(alone.caution)
        val later = model(server, now = NEWEST + 45 * 60)
        later.load()
        assertEquals(later.view?.layers?.first()?.staleWords, later.caution)
        assertTrue(checkNotNull(later.caution).isNotEmpty())
    }

    @Test
    fun aServerWithoutARadarSaysSoAndOneNotReachedToo() = runBlocking {
        val server = Server()
        val radar = model(server)
        server.list = HttpAnswer(501, """{"error":{"code":"no_radar","message":"This server collects no rain radar."}}""".toByteArray())
        radar.load()
        assertEquals(RadarState.Failed("This server has no rain radar."), radar.state)
        assertEquals(emptyList<String>(), radar.creditLines)

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
    fun aPictureThatCannotBeFetchedLeavesTheOnesBeforeOnTheMap() = runBlocking {
        val server = Server()
        val radar = model(server)
        radar.load()
        radar.showFrame()
        val newest = checkNotNull(radar.shown)

        server.picture = HttpAnswer(503, ByteArray(0))
        radar.choose(0)
        radar.showFrame()
        assertSame(newest, radar.shown)
        assertEquals("14:15", radar.shownMoment?.clock)
        assertEquals("The server is busy or being restarted. Try again in a moment.", radar.pictureProblem)

        // Something that is no picture is not drawn either.
        server.picture = HttpAnswer(200, "not a picture".toByteArray())
        radar.showFrame()
        assertSame(newest, radar.shown)
        assertEquals("The radar picture could not be read.", radar.pictureProblem)

        // "Try again", with the pictures back.
        server.picture = HttpAnswer(200, picture())
        radar.showFrame()
        assertEquals("14:10", radar.shownMoment?.clock)
        assertNull(radar.pictureProblem)
    }

    @Test
    fun playingStepsThroughTheMomentsAndStopsOnTheNewest() = runBlocking {
        val server = Server()
        val radar = model(server)
        server.list = HttpAnswer(200, WITH_A_THIRD.toByteArray())
        radar.load()
        radar.choose(1)

        radar.play()
        assertFalse(radar.isPlaying)
        // Every moment's pictures were fetched, the oldest first, the analysis under all three once; the newest is looked at as the newest.
        assertEquals(
            listOf(analysis("1300"), radars("1310"), radars("1315"), radars("1320")),
            server.paths.drop(1),
        )
        assertNull(radar.chosenTime)
        assertEquals("14:20", radar.shownMoment?.clock)
        assertEquals(listOf(analysis("1300"), radars("1320")), radar.shown?.pictures?.map { it.path })
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
    fun aColourIsHandedToTheCoreAsRedGreenBlueAndHowSolid() {
        assertEquals(0x11223344u, RadarLayer.rgba(androidx.compose.ui.graphics.Color(0x44112233)))
        // Solid, since the layer as a whole is see-through.
        assertEquals(0xFFu, RadarLayer.colours.unseen and 0xFFu)
        // And no colour at all for what a picture over another did not see.
        assertEquals(0u, RadarColours.CLEAR and 0xFFu)
    }

    companion object {
        private const val BASE = "http://venti.example"

        /** The layers' sources, as the recorded answer names them: the analysis, and the radars. */
        private const val ANALYSIS = "geosphere-inca-rr"
        private const val RADARS = "dwd-radar-ry"

        /** 13:15 UTC of 2026-11-21, the time of the radars' newest recorded frame. */
        private const val NEWEST = 1_795_266_900L

        /** A device whose clock is behind the server's: the ages are counted from the server's 13:19. */
        private const val BEFORE = NEWEST - 3 * 86_400

        private val RECORDED: String by lazy { File(TestSupport.recordedAnswers, "radar.json").readText() }

        /** The path of a frame of the analysis and of one of the radars, by its time: "1300". */
        private fun analysis(time: String) = "/v1/radar/$ANALYSIS/20261121T${time}Z.png"

        private fun radars(time: String) = "/v1/radar/$RADARS/20261121T${time}Z.png"

        /** The recorded answer five minutes on: a third frame of the radars, of 13:20, by a clock at 13:24. */
        private val WITH_A_THIRD: String by lazy {
            val third = """, {"time": "2026-11-21T13:20:00Z", "interval_s": 300, """ +
                """"path": "/v1/radar/dwd-radar-ry/20261121T1320Z.png", "bytes": 4500}"""
            RECORDED
                .replace("\"time\": \"2026-11-21T13:19:00Z\"", "\"time\": \"2026-11-21T13:24:00Z\"")
                .replace(Regex("""("bytes": 4695\s*\})"""), "$1$third")
                .also { check(it.contains("1320Z") && it.contains("13:24:00Z")) { "the recorded answer has changed" } }
        }

        /** The recorded answer with the analysis' two frames an hour earlier: of 11:45 and 12:00. */
        private val WITH_AN_OLD_ANALYSIS: String by lazy {
            RECORDED
                .replace("\"time\": \"2026-11-21T12:45:00Z\"", "\"time\": \"2026-11-21T11:45:00Z\"")
                .replace("\"time\": \"2026-11-21T13:00:00Z\"", "\"time\": \"2026-11-21T12:00:00Z\"")
                .also { check(it.contains("11:45:00Z") && it.contains("12:00:00Z")) { "the recorded answer has changed" } }
        }

        /** The recorded answer with no frame of the radars: the analysis stands alone on the line of time. */
        private val WITHOUT_THE_RADARS: String by lazy {
            RECORDED
                .replace(Regex("""("source": "dwd-radar-ry".*?"frames": )\[.*?\]""", RegexOption.DOT_MATCHES_ALL), "$1[]")
                .also { check(!it.contains("1315Z.png")) { "the recorded answer has changed" } }
        }

        /** A pixel without a colour, as [pixels] reads one. */
        private const val CLEAR = 0

        /** A picture's eight pixels as Java reads them, row by row; one that is clear is [CLEAR], whatever colour lies under it. */
        private fun pixels(png: ByteArray): List<Int> {
            val read = checkNotNull(ImageIO.read(ByteArrayInputStream(png)))
            return (0 until 2).flatMap { y -> (0 until 4).map { x -> read.getRGB(x, y).let { if (it ushr 24 == 0) CLEAR else it } } }
        }

        /**
         * A picture as a server writes one: four by two pixels, row by
         * row, with the values 0 to 7 unless others are given, a palette
         * of eight greys, one byte a pixel.
         */
        fun picture(values: ByteArray = byteArrayOf(0, 1, 2, 3, 4, 5, 6, 7)): ByteArray {
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
            val rows = byteArrayOf(0) + values.copyOfRange(0, 4) + byteArrayOf(0) + values.copyOfRange(4, 8)
            chunk("IDAT", ByteArrayOutputStream().also { packed -> DeflaterOutputStream(packed).use { it.write(rows) } }.toByteArray())
            chunk("IEND", ByteArray(0))
            return out.toByteArray()
        }
    }
}
