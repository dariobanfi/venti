package fly.venti.app.fly

import fly.venti.app.HttpAnswer
import fly.venti.app.R
import fly.venti.app.TestSupport
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.FlightSession
import fly.venti.core.LiveAudience
import fly.venti.core.LiveShareState
import fly.venti.core.LocationSample
import fly.venti.core.ServerRequest
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Sharing a flight while it is under way: the session model against the
 * real core, with made-up sensors, a clock the test moves and a server that
 * answers as the test tells it to.
 */
class LiveSharingTest {
    private val scope = CoroutineScope(Dispatchers.Default)

    @After
    fun stop() {
        scope.cancel()
    }

    /** Sensors that hand the test the core's session, to report positions to. */
    private class FakeSensors : FlightSensors {
        @Volatile
        var session: FlightSession? = null

        override fun start(session: FlightSession, onProblem: (String) -> Unit) {
            this.session = session
        }

        override fun stop() = Unit
    }

    /** The pilot's server as the cockpit sees it: it remembers what it was sent. */
    private class Server {
        val requests: MutableList<ServerRequest> = CopyOnWriteArrayList<ServerRequest>()

        /** What it answers, or `null` for no answer at all. */
        @Volatile
        var answer: (ServerRequest) -> HttpAnswer? = { HttpAnswer(201, "{}".toByteArray()) }

        @Suppress("RedundantSuspendModifier")
        suspend fun carry(request: ServerRequest): HttpAnswer? {
            requests += request
            return answer(request)
        }

        fun positions(): List<ServerRequest> = requests.toList().filter { it.path == "/v1/live/positions" }
    }

    /** A flight being flown by the test: a clock, and positions reported at its time. */
    private class Flown(val sensors: FakeSensors) {
        @Volatile
        var now = 1_795_000_000.0

        /** Stands still for so many seconds. */
        fun stand(seconds: Int) = repeat(seconds) { report(speed = 0.0) }

        /** Flies east at ten metres a second for so many seconds, sinking a metre a second. */
        fun fly(seconds: Int) = repeat(seconds) {
            metresEast += 10.0
            altitude -= 1.0
            report(speed = 10.0)
        }

        private var metresEast = 0.0
        private var altitude = 1_600.0

        private fun report(speed: Double) {
            now += 1.0
            checkNotNull(sensors.session).onLocation(
                LocationSample(
                    timeUnixS = now,
                    lat = 47.6642,
                    // A degree of longitude is about 75 km at this latitude.
                    lon = 11.7967 + metresEast / 75_000.0,
                    altitudeM = altitude,
                    groundSpeedMs = speed,
                    courseDeg = 90.0,
                ),
            )
        }
    }

    private class Made(val fly: FlySession, val flown: Flown, val server: Server, val preferences: MemoryPreferences)

    /** A session for a pilot who is signed in to a server, unless `signedIn` says otherwise. */
    private fun make(audience: String, signedIn: Boolean = true): Made {
        val sensors = FakeSensors()
        val flown = Flown(sensors)
        val server = Server()
        val preferences = MemoryPreferences().apply { liveAudience = audience }
        var links = 0
        val fly = FlySession(
            TestSupport.newFolder("live"),
            preferences,
            sensors,
            SilentTone(),
            scope,
            clock = { flown.now },
            // A different link each time one is made.
            random = { size ->
                links += 1
                ByteArray(size) { (it * links).toByte() }
            },
        )
        fly.server = { SERVER.takeIf { signedIn } }
        fly.carry = server::carry
        return Made(fly, flown, server, preferences)
    }

    /** Waits for something the session does in its own time, five times a second. */
    private fun waitFor(what: String, condition: () -> Boolean) {
        var waited = 0
        // Moments on a machine at rest; a minute's patience for one that is building beside the test.
        while (!condition() && waited < 60_000) {
            Thread.sleep(20)
            waited += 20
        }
        assertTrue(what, condition())
    }

    private fun body(request: ServerRequest) =
        Json.parseToJsonElement(checkNotNull(request.body).decodeToString()).jsonObject

    @Test
    fun aSharedFlightGoesOutFromTakeOffAndTheServerIsToldWhenItEnds() {
        val made = make(audience = "public")
        val (fly, flown, server) = Triple(made.fly, made.flown, made.server)
        assertNull(fly.live)
        assertFalse(fly.isSharingLive)

        // The pilot's kept choice is taken up as the flight starts. The
        // link is there at once, to send before take-off; nothing else is.
        fly.startFlight()
        val before = checkNotNull(fly.live)
        assertTrue(fly.isSharingLive)
        assertEquals(LiveAudience.PUBLIC, before.audience)
        assertEquals("Begins at take-off", before.words)
        assertTrue(before.link, before.link.startsWith("$SERVER/live/"))
        flown.stand(20)
        waitFor("the session to have looked at the standing glider") { fly.view?.position != null }
        assertTrue(server.requests.isEmpty())

        // In the air: what has gathered goes out with the token, and the
        // server's answer is handed back to the core.
        flown.fly(30)
        waitFor("positions to be sent") { server.positions().isNotEmpty() }
        val first = server.positions().first()
        assertEquals("POST", first.method)
        assertTrue(first.needsToken)
        assertEquals("application/json", first.contentType)
        assertEquals("public", body(first).getValue("visibility").jsonPrimitive.content)
        assertFalse(body(first).getValue("landed").jsonPrimitive.boolean)
        assertTrue(body(first).getValue("fixes").jsonArray.isNotEmpty())
        waitFor("the answer to be taken") { (fly.live?.taken ?: 0u) > 0u }
        assertEquals(LiveShareState.SHARING, fly.live?.state)
        assertTrue(fly.live?.words, checkNotNull(fly.live).words.startsWith("Sent "))

        // No network: the positions wait on the phone, and it says so.
        server.answer = { null }
        flown.fly(20)
        waitFor("the positions to wait") { fly.live?.state == LiveShareState.WAITING }
        assertTrue(fly.live?.words, checkNotNull(fly.live).words.startsWith("No network: "))
        assertTrue(checkNotNull(fly.live).waiting > 0u)

        // With the network back they go out, the newest among them.
        server.answer = { HttpAnswer(201, "{}".toByteArray()) }
        flown.fly(10)
        waitFor("the waiting positions to be sent") { fly.live?.state == LiveShareState.SHARING && fly.live?.waiting == 0u }

        // The pilot narrows who may watch: said to the server at once.
        fly.share(LiveAudience.LINK)
        assertEquals(LiveAudience.LINK, fly.live?.audience)
        flown.fly(2)
        waitFor("the narrower audience to be said") {
            server.positions().any { body(it).getValue("visibility").jsonPrimitive.content == "link" }
        }

        // The pilot ends the flight. The cockpit closes at once, and the
        // server is still told that the glider has landed.
        fly.finish()
        assertNull(fly.live)
        assertNull(fly.mode)
        waitFor("the landing to be said") { server.positions().any { body(it).getValue("landed").jsonPrimitive.boolean } }
    }

    @Test
    fun theLandingIsTriedForAMinuteAndAHalfAfterTheFlightAndNoLonger() {
        val made = make(audience = "link")
        val (fly, flown, server) = Triple(made.fly, made.flown, made.server)
        fly.startFlight()
        flown.fly(30)
        waitFor("positions to be taken") { (fly.live?.taken ?: 0u) > 0u }
        fun landings() = server.positions().count { body(it).getValue("landed").jsonPrimitive.boolean }

        // The network is away as the pilot ends the flight.
        server.answer = { null }
        fly.finish()
        waitFor("the landing to be tried") { landings() == 1 }

        // Once the wait the core asks for after a failure is over, it is tried again.
        flown.now += 30.0
        waitFor("the landing to be tried again") { landings() == 2 }

        // A minute and a half after the end nothing more is tried: the
        // track goes quiet, and the server removes it in its own time.
        flown.now += FlySession.LANDED_WITHIN_SECONDS
        Thread.sleep(2_500)
        val tried = landings()
        flown.now += 120.0
        Thread.sleep(2_500)
        assertEquals(tried, landings())
    }

    @Test
    fun whatWaitedForTheNetworkStillGoesOutAfterTheFlightHasEnded() {
        val made = make(audience = "link")
        val (fly, flown, server) = Triple(made.fly, made.flown, made.server)
        fly.startFlight()
        flown.fly(30)
        waitFor("positions to be taken") { (fly.live?.taken ?: 0u) > 0u }
        fun sent(from: Int) = server.positions().drop(from).sumOf { body(it).getValue("fixes").jsonArray.size }

        // A hundred seconds without a network: more positions wait than one request takes.
        server.answer = { null }
        flown.fly(100)
        waitFor("the positions to wait") { (fly.live?.waiting ?: 0u) >= 100u }
        val waiting = checkNotNull(fly.live).waiting.toInt()

        // The network is back as the pilot ends the flight. The landing
        // goes out with the newest positions once the wait after the
        // failure is over.
        server.answer = { HttpAnswer(201, "{}".toByteArray()) }
        val before = server.positions().size
        fly.finish()
        flown.now += 10.0
        waitFor("the landing to be said") {
            server.positions().drop(before).any { body(it).getValue("landed").jsonPrimitive.boolean }
        }
        assertTrue(sent(before) < waiting)

        // The rest of the gap is due a quarter of a minute later, and is
        // sent though the cockpit has closed.
        flown.now += 20.0
        waitFor("the rest of the gap to be sent") { sent(before) >= waiting }
    }

    @Test
    fun nothingIsSharedWithoutAChoiceWithoutAnAccountOrInAReplay() {
        // Nobody is the choice until the pilot makes another.
        val quiet = make(audience = "")
        quiet.fly.startFlight()
        assertNull(quiet.fly.live)
        quiet.fly.share(null)
        assertNull(quiet.fly.live)
        quiet.flown.fly(30)
        quiet.fly.finish()

        // A choice without an account to send to is not kept as if it could be.
        val signedOut = make(audience = "link", signedIn = false)
        signedOut.fly.startFlight()
        assertNull(signedOut.fly.live)
        signedOut.fly.share(LiveAudience.PUBLIC)
        assertNull(signedOut.fly.live)
        signedOut.fly.finish()

        // What is replayed is not shared, whatever the pilot chose.
        val replayed = make(audience = "public")
        replayed.fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        assertNull(replayed.fly.live)
        replayed.fly.share(LiveAudience.PUBLIC)
        assertNull(replayed.fly.live)
        Thread.sleep(600)
        assertNull(replayed.fly.live)
        replayed.fly.finish()

        for (made in listOf(quiet, signedOut, replayed)) {
            assertTrue(made.server.requests.isEmpty())
            assertNull(made.fly.problem)
        }
    }

    @Test
    fun deletingTheTrackTellsTheServerAndSharingAgainMakesANewLink() {
        val made = make(audience = "link")
        val (fly, flown, server) = Triple(made.fly, made.flown, made.server)
        fly.startFlight()
        val link = checkNotNull(fly.live).link
        flown.fly(30)
        waitFor("positions to be sent") { server.positions().isNotEmpty() }

        fly.deleteLiveTrack()
        val stopped = checkNotNull(fly.live)
        assertEquals(LiveShareState.STOPPED, stopped.state)
        assertEquals("You deleted the track.", stopped.words)
        assertFalse(fly.isSharingLive)
        waitFor("the server to be told") { server.requests.any { it.method == "DELETE" } }
        val deletion = server.requests.first { it.method == "DELETE" }
        assertTrue(deletion.path, deletion.path.startsWith("/v1/live/"))
        assertTrue(deletion.needsToken)
        assertTrue(link.endsWith(deletion.path.substringAfterLast('/')))

        // Nothing more goes out for a deleted track.
        val sent = server.positions().size
        flown.fly(30)
        Thread.sleep(600)
        assertEquals(sent, server.positions().size)

        // Sharing again begins a new track, with a link of its own.
        fly.share(LiveAudience.LINK)
        assertTrue(fly.isSharingLive)
        assertNotEquals(link, checkNotNull(fly.live).link)
        fly.finish()
    }

    @Test
    fun theChoiceOfWhoMayWatchIsKeptAndSaidInWords() {
        val preferences = MemoryPreferences()
        assertEquals(LiveChoice.Nobody, LiveChoice.stored(preferences))
        preferences.liveAudience = "link"
        assertEquals(LiveChoice.Link, LiveChoice.stored(preferences))
        preferences.liveAudience = "public"
        assertEquals(LiveChoice.Everyone, LiveChoice.stored(preferences))
        // Something a later version wrote is nobody, not a guess.
        preferences.liveAudience = "friends"
        assertEquals(LiveChoice.Nobody, LiveChoice.stored(preferences))

        assertEquals(listOf("", "link", "public"), LiveChoice.entries.map { it.key })
        assertEquals(listOf("Nobody", "People I give the link", "Everyone"), LiveChoice.entries.map { it.title })
        assertEquals(listOf(null, LiveAudience.LINK, LiveAudience.PUBLIC), LiveChoice.entries.map { it.audience })
        assertTrue(LiveChoice.entries.all { it.explanation.endsWith(".") })

        // The Fly tab's row. A choice that cannot be kept, for want of an
        // account, is not shown as if it could.
        assertEquals(listOf("Off", "With a link", "Everyone"), LiveChoice.entries.map { it.summary(isSignedIn = true) })
        assertEquals(
            listOf("Off", "Not signed in", "Not signed in"),
            LiveChoice.entries.map { it.summary(isSignedIn = false) },
        )

        // The cockpit's button: one sign while positions go out or have
        // gone, one while they wait, one when nothing is shared.
        assertEquals(R.drawable.venti_live, LiveLook.icon(LiveShareState.SHARING))
        assertEquals(R.drawable.venti_live, LiveLook.icon(LiveShareState.ENDED))
        assertEquals(R.drawable.ic_sync_problem, LiveLook.icon(LiveShareState.WAITING))
        assertEquals(R.drawable.ic_location_off, LiveLook.icon(LiveShareState.STOPPED))
        assertEquals(R.drawable.ic_location_off, LiveLook.icon(null))
    }

    @Test
    fun aLinkIsMadeOfTheSystemsRandomNumbers() {
        val first = FlySession.randomBytes(26)
        val second = FlySession.randomBytes(26)
        assertEquals(26, first.size)
        assertFalse(first.contentEquals(second))
        // Twenty-six equal bytes are no random numbers, and the core refuses them.
        assertTrue(first.distinct().size > 1)
    }

    private companion object {
        const val SERVER = "https://venti.example"
    }
}
