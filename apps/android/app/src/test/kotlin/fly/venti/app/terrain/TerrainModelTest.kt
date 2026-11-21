package fly.venti.app.terrain

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.fly.AltitudeWords
import fly.venti.app.fly.CockpitMap
import fly.venti.app.fly.FlySession
import fly.venti.app.fly.FlightSensors
import fly.venti.app.fly.SilentTone
import fly.venti.app.map.MapStyle
import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.flight.AltitudeAxis
import fly.venti.app.flight.GroundAtMoment
import fly.venti.core.Coordinate
import fly.venti.core.GroundUnderFlight
import fly.venti.core.flightFromFile
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.maplibre.geojson.LineString
import java.io.File
import java.io.IOException
import java.util.Locale

/**
 * The ground: the elevation tiles that come with the app, read by the real
 * core, and the tile server played by the test.
 */
class TerrainModelTest {
    private val locale = Locale.getDefault()
    private val scope = CoroutineScope(Dispatchers.Default)

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
        scope.cancel()
    }

    /** The region's elevation tiles, where the build gathered the app's files. */
    private val archive: File get() = TestSupport.assets.resolve("MapDemo").resolve(MapStyle.BUNDLED_TERRAIN)

    /**
     * One elevation tile as a tile server sends it, cut out of the archive
     * that comes with the app: its tiles lie in it as they are, each a
     * WebP image that says its own length.
     */
    private fun aTile(): ByteArray {
        val bytes = archive.readBytes()
        fun says(at: Int, word: String) = word.indices.all { bytes[at + it] == word[it].code.toByte() }
        val start = (0..bytes.size - 12).first { says(it, "RIFF") && says(it + 8, "WEBP") }
        val length = (0..3).sumOf { (bytes[start + 4 + it].toInt() and 0xFF) shl (8 * it) } + 8
        return bytes.copyOfRange(start, start + length)
    }

    /** A tile server that notes what it was asked and answers as the test says. */
    private class Server(var answer: (String) -> HttpAnswer) {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        val transport: TerrainTransport = { url ->
            asked += url
            answer(url)
        }
    }

    @Test
    fun theRegionThatComesWithTheAppGivesTheHeightOfTheGround() = runBlocking {
        val server = Server { HttpAnswer(500, ByteArray(0)) }
        val model = TerrainModel(source = { TerrainModel.Source.Archive(archive) }, folder = null, transport = server.transport)
        assertNull(model.terrain.elevation(LAKE.lat, LAKE.lon))

        model.load(TAKEOFF)

        // The Tegernsee lies at 725 m, and the sample flight takes off on
        // the Wallberg at 1620 m.
        assertEquals(725.0, checkNotNull(model.terrain.elevation(LAKE.lat, LAKE.lon)), 1.0)
        assertEquals(1620.0, checkNotNull(model.terrain.elevation(TAKEOFF.lat, TAKEOFF.lon)), 5.0)
        // The archive is read once, and nothing is fetched when the
        // ground comes with the app.
        val tiles = model.terrain.tileCount()
        assertTrue(tiles > 0u)
        model.load(LAKE)
        assertEquals(tiles, model.terrain.tileCount())
        assertTrue(server.asked.isEmpty())

        // A session given this ground knows how high over it the glider
        // is, and the cockpit says that under the altitude in place of
        // the height over the take-off.
        val session = FlightSession(TestSupport.newFolder("terrain").path)
        session.useTerrain(model.terrain)
        val replay = Replay(TestSupport.sampleFlight)
        replay.advance(session, 540.0)
        val view = session.view(replay.now())
        assertEquals(1155.0, checkNotNull(view.heightAboveGroundM), 40.0)
        assertNotNull(view.groundElevationM)
        assertNotNull(view.heightAboveTakeoffM)
        assertTrue(checkNotNull(AltitudeWords.under(view.heightAboveGroundM, view.heightAboveTakeoffM)).endsWith(" m over ground"))
    }

    @Test
    fun underTheAltitudeStandsTheHeightOverTheGroundWhereItIsKnown() {
        assertEquals("1155 m over ground", AltitudeWords.under(aboveGroundM = 1155.4, aboveTakeoffM = 389.0))
        // The ground is a model's: a glider it puts under the ground stands on it.
        assertEquals("0 m over ground", AltitudeWords.under(aboveGroundM = -3.0, aboveTakeoffM = 12.0))
        // Without the ground, the height over the take-off as before.
        assertEquals("+389 m over take-off", AltitudeWords.under(aboveGroundM = null, aboveTakeoffM = 389.0))
        assertNull(AltitudeWords.under(aboveGroundM = null, aboveTakeoffM = null))
    }

    @Test
    fun tilesAreAskedOfTheServerByTheirAddressAndARefusalDoesNoHarm() = runBlocking {
        val server = Server { HttpAnswer(404, ByteArray(0)) }
        val model = TerrainModel(
            source = { TerrainModel.Source.Server("https://tiles.example.org/{z}/{x}/{y}.webp") },
            folder = TestSupport.newFolder("terrain"),
            transport = server.transport,
        )

        model.load(TAKEOFF)

        // Five by five tiles of zoom 12, the one over the place first.
        assertEquals(25, server.asked.size)
        assertEquals("https://tiles.example.org/12/2182/1429.webp", server.asked.first())
        assertEquals(25, server.asked.distinct().size)
        assertEquals(0u, model.terrain.tileCount())
        assertNull(model.terrain.elevation(TAKEOFF.lat, TAKEOFF.lon))

        // What is no elevation tile is not taken for one, whatever the
        // status says.
        server.asked.clear()
        server.answer = { HttpAnswer(200, "<html>not a tile</html>".toByteArray()) }
        model.load(TAKEOFF)
        assertEquals(25, server.asked.size)
        assertEquals(0u, model.terrain.tileCount())

        // Without a network the first tile that gets no answer ends the
        // round: the others would not come either.
        server.asked.clear()
        server.answer = { throw IOException("no route to host") }
        model.load(TAKEOFF)
        assertEquals(1, server.asked.size)
        // With no source there is nothing to ask.
        val nowhere = TerrainModel(source = { TerrainModel.Source.Nowhere }, folder = null, transport = server.transport)
        nowhere.load(TAKEOFF)
        assertEquals(1, server.asked.size)
    }

    @Test
    fun aTileThatCameIsKeptOnThePhoneAndIsThereAgainWithoutANetwork() = runBlocking {
        val folder = TestSupport.newFolder("terrain")
        val tile = aTile()
        val over = "https://tiles.example.org/12/2182/1429.webp"
        // The server has the tile over the place and no other.
        val server = Server { url -> if (url == over) HttpAnswer(200, tile) else HttpAnswer(404, ByteArray(0)) }
        val source = { TerrainModel.Source.Server("https://tiles.example.org/{z}/{x}/{y}.webp") }
        val model = TerrainModel(source, folder, server.transport)

        model.load(TAKEOFF)

        assertEquals(1u, model.terrain.tileCount())
        assertNotNull(model.terrain.elevation(TAKEOFF.lat, TAKEOFF.lon))
        // Kept as it came, under its address, and nothing half-written
        // beside it.
        val kept = folder.resolve("12-2182-1429.webp")
        assertArrayEquals(tile, kept.readBytes())
        assertEquals(listOf("12-2182-1429.webp"), folder.list().orEmpty().toList())
        // What is held is not asked for again.
        server.asked.clear()
        model.load(TAKEOFF)
        assertEquals(24, server.asked.size)
        assertFalse(over in server.asked)

        // The next time the app runs, the tile is there without a network.
        val offline = Server { throw IOException("no route to host") }
        val again = TerrainModel(source, folder, offline.transport)
        again.load(TAKEOFF)
        assertEquals(1u, again.terrain.tileCount())
        assertFalse(over in offline.asked)
        // A kept file that is no tile is fetched anew and replaced.
        kept.writeText("cut short")
        val repaired = TerrainModel(source, folder, server.transport)
        repaired.load(TAKEOFF)
        assertEquals(1u, repaired.terrain.tileCount())
        assertArrayEquals(tile, kept.readBytes())
    }

    /** Sensors that report nothing: the session is fed by a replay. */
    private class NoSensors : FlightSensors {
        override fun start(session: FlightSession, onProblem: (String) -> Unit) = Unit

        override fun stop() = Unit
    }

    @Test
    fun theGroundUnderAFlightIsReadForItsProfile() = runBlocking {
        val model = TerrainModel(source = { TerrainModel.Source.Archive(archive) }, folder = null, transport = { HttpAnswer(500, ByteArray(0)) })
        val flight = flightFromFile(TestSupport.sampleFlight)

        val ground = model.ground(flight)

        // At 240 moments, from the take-off on the Wallberg at 1,620 m to
        // the valley floor.
        assertEquals(240, ground.size)
        assertEquals(1620.0, ground.first().elevationM, 10.0)
        assertTrue(ground.last().elevationM < 800)
        // The archive was read into the cockpit's terrain, once.
        assertTrue(model.terrain.tileCount() > 0u)

        // The chart makes room for the lowest ground, and the panel reads
        // the height over the ground at any moment of the track.
        assertTrue(AltitudeAxis.range(flight, ground = ground).start <= 700)
        assertEquals(700.0, AltitudeAxis.range(flight).start, 100.0)
        val middle = flight.track[flight.track.size / 2]
        assertTrue(checkNotNull(GroundAtMoment.heightAbove(middle.seconds, ground)) > 500)
        assertNull(GroundAtMoment.heightAbove(middle.seconds, emptyList()))
        // Where the ground was read at two moments far apart only, a moment
        // between them has none, and the moments themselves have theirs.
        val sparse = listOf(ground.first(), ground.last())
        assertNotNull(GroundAtMoment.heightAbove(0u, sparse))

        // Without a source there is no ground, and nothing is asked.
        val none = Server { error("Nothing is fetched without a source") }
        val nothing = TerrainModel(source = { TerrainModel.Source.Nowhere }, folder = null, transport = none.transport).ground(flight)
        assertTrue(nothing.isEmpty())
        assertTrue(none.asked.isEmpty())
    }

    @Test
    fun theGroundUnderAFlightIsAskedOfTheServerTileByTile() = runBlocking {
        val server = Server { HttpAnswer(200, aTile()) }
        val model = TerrainModel(
            source = { TerrainModel.Source.Server("https://tiles.example.org/{z}/{x}/{y}.webp") },
            folder = null,
            transport = server.transport,
        )
        val flight = flightFromFile(TestSupport.sampleFlight)

        model.ground(flight)

        // The tiles that cover the flight, each once, and none into the
        // cockpit's terrain: the flight's ground is let go again.
        assertTrue(server.asked.isNotEmpty())
        assertEquals(server.asked.size, server.asked.toSet().size)
        assertTrue(server.asked.all { it.startsWith("https://tiles.example.org/12/") })
        assertEquals(0u, model.terrain.tileCount())

        // Without a network the first tile goes unanswered and no more are asked.
        val offline = Server { throw IOException("no network") }
        val ground = TerrainModel(
            source = { TerrainModel.Source.Server("https://tiles.example.org/{z}/{x}/{y}.webp") },
            folder = null,
            transport = offline.transport,
        ).ground(flight)
        assertEquals(1, offline.asked.size)
        assertTrue(ground.isEmpty())
    }

    /** The ground read every `step` seconds from `from` on; the glider is as many metres over it as seconds have passed. */
    private fun every(step: Int, moments: Int, from: Int = 0): List<GroundUnderFlight> =
        List(moments) { index ->
            val seconds = from + index * step
            GroundUnderFlight(seconds = seconds.toUInt(), elevationM = 800.0, heightAboveM = seconds.toDouble())
        }

    @Test
    fun theHeightOverTheGroundAtAMomentIsThatOfTheNearestMomentTheGroundWasReadAt() {
        val ground = every(10, 7)
        assertEquals(0.0, GroundAtMoment.heightAbove(0u, ground))
        assertEquals(10.0, GroundAtMoment.heightAbove(14u, ground))
        assertEquals(20.0, GroundAtMoment.heightAbove(16u, ground))
        assertEquals(60.0, GroundAtMoment.heightAbove(60u, ground))
        // Half-way between two, the later one.
        assertEquals(20.0, GroundAtMoment.heightAbove(15u, ground))

        // Not where the ground was not read: further off than the moments lie apart.
        val holed = every(10, 20) + every(10, 20, 600)
        assertEquals(190.0, GroundAtMoment.heightAbove(190u, holed))
        assertNull(GroundAtMoment.heightAbove(400u, holed))
        assertEquals(600.0, GroundAtMoment.heightAbove(600u, holed))
        // Nor before the first moment or after the last.
        assertNull(GroundAtMoment.heightAbove(300u, every(10, 4)))
        assertNull(GroundAtMoment.heightAbove(0u, every(10, 4, 500)))
        // One moment read: there, and for a second around it.
        assertEquals(50.0, GroundAtMoment.heightAbove(51u, every(10, 1, 50)))
        assertNull(GroundAtMoment.heightAbove(52u, every(10, 1, 50)))
    }

    @Test
    fun theCockpitHandsTheGroundToItsSessionAndAsksForTheGroundAroundTheGlider() {
        val model = TerrainModel(source = { TerrainModel.Source.Archive(archive) }, folder = null)
        runBlocking { model.load(TAKEOFF) }
        val wanted: MutableList<Coordinate> = CopyOnWriteArrayList<Coordinate>()
        val fly = FlySession(TestSupport.newFolder("fly"), MemoryPreferences(), NoSensors(), SilentTone(), scope)
        fly.terrain = { model.terrain }
        fly.wantTerrain = { place -> wanted += place }

        // Five hundred times faster: nine minutes of flight in a second.
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        Thread.sleep(1200)

        val view = checkNotNull(fly.view)
        assertNotNull(view.heightAboveGroundM)
        assertNotNull(view.groundElevationM)
        // Asked for with the first position, and not with every refresh:
        // every ten seconds of the clock.
        assertEquals(1, wanted.size)
        // The side view of what lies ahead has the ground too, while the
        // session runs.
        assertTrue(checkNotNull(fly.airspaceAhead()).ground.isNotEmpty())
        // And the map has the edge of what a straight glide reaches: a
        // closed ring, a place every five degrees, drawn as one line.
        val range = checkNotNull(fly.range)
        assertEquals(73, range.outline.size)
        assertEquals(range.outline.first(), range.outline.last())
        assertEquals(100.0, range.reserveM, 0.0)
        assertTrue(range.nearestM < range.farthestM)
        val line = CockpitMap.rangeFeatures(range).single().geometry() as LineString
        assertEquals(73, line.coordinates().size)
        assertTrue(CockpitMap.rangeFeatures(null).isEmpty())
        fly.finish()
        assertNull(fly.airspaceAhead())
        assertNull(fly.range)
    }

    private companion object {
        /** Where the sample flight takes off, on the Wallberg. */
        val TAKEOFF = Coordinate(lat = 47.6642, lon = 11.7967)

        /** A place on the Tegernsee. */
        val LAKE = Coordinate(lat = 47.713, lon = 11.74)
    }
}
