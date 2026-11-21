package fly.venti.app.live

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.wind.grown
import fly.venti.core.Bounds
import fly.venti.core.liveMapPath
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.maplibre.geojson.Point
import java.io.IOException
import java.util.Locale

/**
 * Who is in the air against the real core, with the server played by the
 * answers the real server's own tests keep: two gliders over the Tegernsee
 * valley, one flying and one landed, and the last positions of the first.
 */
class LiveModelTest {
    private val locale = Locale.getDefault()

    /** A scope in which nothing starts: `show` then asks nobody. */
    private val still = CoroutineScope(Job().apply { cancel() })

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    /** The recorded server, and a list of what it was asked. */
    private class Server {
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var isReachable = true

        /** Whether the track that is followed is gone from the server. */
        @Volatile
        var hasLostTheTrack = false

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReachable) throw IOException("no route to host")
            return when {
                "/v1/live?" in url -> HttpAnswer(200, recorded("live.json"))
                "/v1/live/$FLYING" in url ->
                    if (hasLostTheTrack) {
                        HttpAnswer(404, """{"error": {"code": "live_track_not_found", "message": "x"}}""".toByteArray())
                    } else {
                        HttpAnswer(200, recorded("live_track.json"))
                    }
                else -> HttpAnswer(404, ByteArray(0))
            }
        }

        fun maps(): List<String> = asked.toList().filter { "/v1/live?" in it }

        fun tracks(): List<String> = asked.toList().filter { "/v1/live/" in it }

        companion object {
            fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()
        }
    }

    /** A clock the test moves by hand, in milliseconds since 1970. */
    private class Clock(var now: Long = 1_795_000_000_000) {
        fun pass(seconds: Int) {
            now += seconds * 1000L
        }
    }

    /** The valley the recorded gliders are over, as a phone's map shows it. */
    private val valley = Bounds(south = 47.6, west = 11.65, north = 47.75, east = 11.9)

    private fun model(
        server: Server,
        clock: Clock = Clock(),
        scope: CoroutineScope = still,
        address: () -> String? = { BASE },
    ) = LiveModel(
        scope,
        transport = server::answer,
        clock = { clock.now },
        // Sooner than every quarter minute, so that a test need not wait that long.
        askEveryMilliseconds = ASK_EVERY,
        followEveryMilliseconds = ASK_EVERY,
    ).also { it.server = address }

    private suspend fun waitUntil(condition: () -> Boolean) = withTimeout(30_000) {
        while (!condition()) delay(10)
    }

    @Test
    fun theMapGetsItsGlidersAndTheirAgesRunOn() = runBlocking {
        val server = Server()
        val clock = Clock()
        val live = model(server, clock)
        live.load(valley, BASE)

        assertEquals(listOf(BASE + liveMapPath(valley.south, valley.west, valley.north, valley.east)), server.asked.toList())
        assertEquals(2, live.gliders.size)
        assertFalse(live.isTruncated)
        val flying = live.gliders[0]
        val landed = live.gliders[1]
        assertEquals("Demo Pilot · 1596 m", flying.label)
        assertEquals(flying, live.glider(FLYING))
        assertNull(live.glider("33333333333333333333333333"))

        // Four seconds old by the server's own clock when it answered.
        assertEquals(4.0, live.age(flying), 0.001)
        assertEquals("Now", live.ageInWords(flying))
        assertEquals("Landed 9 min ago", live.ageInWords(landed))
        assertEquals("1596 m · −1.0 m/s · Now", GliderWords.detail(flying, live.ageInWords(flying)))
        // A glider on the ground does not climb.
        assertEquals("786 m · Landed 9 min ago", GliderWords.detail(landed, live.ageInWords(landed)))
        assertEquals("@demo", GliderWords.title(flying))
        assertEquals("Glider", GliderWords.title(null))

        // An answer that does not come leaves the gliders where they were:
        // their ages count on, on this phone's clock, which is what is
        // known about them.
        server.isReachable = false
        clock.pass(seconds = 40)
        live.load(valley, BASE)
        assertEquals(2, live.gliders.size)
        assertEquals(44.0, live.age(flying), 0.001)
        assertEquals("44 s ago", live.ageInWords(flying))
        // Three minutes without a word is no landing: the core never says so.
        clock.pass(seconds = 180)
        assertEquals("No position for 4 min", live.ageInWords(flying))

        // The next answer that comes sets the ages right again.
        server.isReachable = true
        live.load(valley, BASE)
        assertEquals("Now", live.ageInWords(live.gliders[0]))

        assertEquals(GliderWords.CAUTION, GliderWords.underTheList(isTruncated = false))
        assertTrue(GliderWords.underTheList(isTruncated = true).startsWith("There are more than Venti lists at once."))
    }

    @Test
    fun aMapOfHalfTheAlpsOrWithoutAServerAsksForNobody() = runBlocking {
        val server = Server()
        val wide = model(server, scope = this)
        wide.show(Bounds(south = 45.0, west = 6.0, north = 48.5, east = 13.5))
        delay(2 * LiveModel.REST_MILLISECONDS)
        assertTrue(server.asked.isEmpty())

        var address: String? = BASE
        val live = model(server, scope = this, address = { address })
        live.show(valley)
        waitUntil { live.gliders.isNotEmpty() }
        // The pilot takes the server away, or zooms out to a continent: the gliders go.
        address = null
        live.show(valley)
        assertTrue(live.gliders.isEmpty())
        address = BASE
        live.show(valley)
        waitUntil { live.gliders.isNotEmpty() }
        live.show(Bounds(south = 40.0, west = 0.0, north = 50.0, east = 20.0))
        assertTrue(live.gliders.isEmpty())
    }

    @Test
    fun theServerIsAskedOnceTheMapRestsAndAgainEveryQuarterMinuteWhileItIsLookedAt() = runBlocking {
        val server = Server()
        val live = model(server, scope = this)

        // A drag reports many regions; only where it comes to rest counts.
        live.show(valley)
        live.show(valley.copy(west = 11.7, east = 11.95))
        live.show(valley)
        waitUntil { live.gliders.isNotEmpty() }
        // A quarter as much again on every side was asked for.
        val area = valley.grown(0.25)
        assertEquals(listOf(BASE + liveMapPath(area.south, area.west, area.north, area.east)), server.maps())

        // A small drag inside that asks nothing: the next round brings it up to date.
        live.show(valley.copy(west = 11.62, east = 11.87))
        delay(2 * LiveModel.REST_MILLISECONDS)
        assertEquals(1, server.maps().size)

        // While the map is on screen the same area is asked for again, and
        // no longer once it has left the screen.
        val looking = launch { live.keepCurrent() }
        waitUntil { server.maps().size >= 3 }
        assertEquals(1, server.maps().distinct().size)
        looking.cancelAndJoin()
        val asked = server.maps().size
        delay(3 * ASK_EVERY)
        assertEquals(asked, server.maps().size)
        // The app's own rounds are a quarter of a minute apart, and a
        // followed flight's ten seconds: what the server asks for.
        assertEquals(15_000L, LiveModel.ASK_EVERY_MILLISECONDS)
        assertEquals(10_000L, LiveModel.FOLLOW_EVERY_MILLISECONDS)
        assertEquals(6.0, LiveModel.WIDEST_MAP, 0.0)
    }

    @Test
    fun theFlightOfTheGliderWhosePageIsOpenIsFollowedUntilThePageCloses() = runBlocking {
        val server = Server()
        val clock = Clock()
        val live = model(server, clock)
        live.load(valley, BASE)
        assertNull(live.followed)

        val page = launch { live.follow(FLYING) }
        waitUntil { live.followed != null }
        val flight = checkNotNull(live.followed)
        // The whole track is asked for first: the server's example holds its last three positions.
        assertEquals("$BASE/v1/live/$FLYING", server.tracks().first())
        assertEquals(3, flight.track.size)
        assertEquals(2L, flight.durationS)
        assertEquals(1598, flight.highestM)
        // The page shows the glider as the flight's answer has it, with that answer's age.
        assertEquals(flight.glider, live.glider(FLYING))
        clock.pass(seconds = 30)
        assertEquals("34 s ago", live.ageInWords(flight.glider))

        // While the page is open the flight is asked for again and again,
        // from the second time on only for what arrived since.
        waitUntil { server.tracks().size >= 3 }
        assertTrue(server.tracks().drop(1).all { it == "$BASE/v1/live/$FLYING?since=1201" })
        // The same positions twice are held once.
        assertEquals(3, live.followed?.track?.size)

        // The page closes: nothing more is asked, and the line leaves the map.
        page.cancelAndJoin()
        assertNull(live.followed)
        val asked = server.tracks().size
        delay(3 * ASK_EVERY)
        assertEquals(asked, server.tracks().size)
        assertNotNull(live.glider(FLYING))
    }

    @Test
    fun aFlightThatIsGoneFromTheServerIsSaidAndNotAskedForAgain() = runBlocking {
        val server = Server()
        server.hasLostTheTrack = true
        val live = model(server)

        // It ends by itself: the pilot stopped sharing, or two days have passed.
        withTimeout(30_000) { live.follow(FLYING) }
        assertNull(live.followed)
        assertTrue(live.followProblem, checkNotNull(live.followProblem).contains("no longer shown"))
        assertEquals(1, server.tracks().size)

        // What is no identifier of a track asks nobody.
        withTimeout(30_000) { live.follow("not a track") }
        assertEquals(1, server.tracks().size)
    }

    @Test
    fun aGliderIsDrawnTurnedTheWayItFliesAndOneOnTheGroundAsADisc() = runBlocking {
        val live = model(Server())
        live.load(valley, BASE)
        val (flying, landed) = live.gliders

        val wing = GliderFeature.of(flying)
        assertEquals(FLYING, wing.getStringProperty(GliderFeature.ID))
        assertEquals(LiveSymbols.FLYING, wing.getStringProperty(GliderFeature.PICTURE))
        assertEquals(20.0, wing.getNumberProperty(GliderFeature.HEADING).toDouble(), 0.0)
        assertEquals("Demo Pilot · 1596 m", wing.getStringProperty(GliderFeature.LABEL))
        val place = wing.geometry() as Point
        assertEquals(flying.position.lat, place.latitude(), 1e-9)
        assertEquals(flying.position.lon, place.longitude(), 1e-9)

        // On the ground it points nowhere.
        val disc = GliderFeature.of(landed)
        assertEquals(LiveSymbols.LANDED, disc.getStringProperty(GliderFeature.PICTURE))
        assertEquals(0.0, disc.getNumberProperty(GliderFeature.HEADING).toDouble(), 0.0)

        // Nothing heard of it for a while, and not known to have landed:
        // still a wing, held back in grey.
        assertEquals(LiveSymbols.QUIET, LiveSymbols.name(flying.copy(quiet = true)))
        assertEquals(LiveSymbols.LANDED, LiveSymbols.name(landed.copy(quiet = true)))
    }

    private companion object {
        const val BASE = "https://venti.example.org"

        /** The identifier of the recorded glider that is in the air. */
        const val FLYING = "11111111111111111111111111"

        /** How often a model of these tests asks again, milliseconds. */
        const val ASK_EVERY = 100L
    }
}
