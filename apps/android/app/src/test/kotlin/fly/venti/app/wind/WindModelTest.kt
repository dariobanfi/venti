package fly.venti.app.wind

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.design.Format
import fly.venti.core.Bounds
import fly.venti.core.serverStatus
import fly.venti.core.serverStatusPath
import fly.venti.core.stationHistoryPath
import fly.venti.core.windMapPath
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.IOException
import java.util.Locale

/**
 * The wind on the map against the real core, with the server played by the
 * answers the real server's own tests keep: what `venti-server` sends for
 * its health, for the stations in a box and for one station's history.
 */
class WindModelTest {
    private val locale = Locale.getDefault()

    /** A scope in which nothing starts: `show` then only notes the region. */
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

        @Volatile
        var refusesBoxes = false

        /** The server's clock when the answers were recorded, in seconds since 1970. */
        val clock: Long = serverStatus(200.toUShort(), recorded("health.json")).timeUnixS

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReachable) throw IOException("no route to host")
            val path = url.removePrefix(BASE)
            return when {
                path == serverStatusPath() -> HttpAnswer(200, recorded("health.json"))
                path.startsWith("/v1/stations?") && refusesBoxes ->
                    HttpAnswer(400, recorded("error_missing_bbox.json"))
                path.startsWith("/v1/stations?") -> HttpAnswer(200, recorded("stations.json"))
                path.startsWith("/v1/stations/") -> HttpAnswer(200, recorded("observations.json"))
                else -> HttpAnswer(404, ByteArray(0))
            }
        }

        fun stationRequests(): List<String> = asked.filter { "/v1/stations?" in it }

        companion object {
            fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()
        }
    }

    /** A clock the test moves by hand, in milliseconds since 1970. */
    private class Clock(var now: Long) {
        fun pass(minutes: Int) {
            now += minutes * 60_000L
        }
    }

    private val alps = Bounds(south = 47.0, west = 10.8, north = 47.8, east = 12.0)

    private fun model(
        server: Server,
        clock: Clock = Clock(server.clock * 1000),
        scope: CoroutineScope = still,
        address: String = "wind.example.org/",
    ) = WindModel(address, scope, transport = server::answer, clock = { clock.now })

    @Test
    fun theMapGetsItsStationsAndWhoseDataTheyAre() = runBlocking {
        val server = Server()
        val wind = model(server)
        wind.show(alps)
        wind.load()

        assertNull(wind.problem)
        assertNull(wind.notice)
        assertFalse(wind.isTruncated)
        val names = wind.stations.map { it.name }
        assertTrue("Zugspitze" in names)
        // A name the provider writes in capitals is shown in ordinary case.
        assertTrue("Patscherkofel" in names)
        val providers = listOf("Aviation Weather Center", "Deutscher Wetterdienst", "GeoSphere Austria", "Autonome Provinz Bozen - Südtirol")
        assertEquals(providers, wind.credits.map { it.provider })
        assertEquals(
            "Wind: Aviation Weather Center, Deutscher Wetterdienst, GeoSphere Austria and Autonome Provinz Bozen - Südtirol",
            wind.creditLine,
        )

        // First the server's clock, then the stations of the box in view
        // and a quarter more on every side.
        val area = alps.grown(0.25)
        assertEquals(
            listOf(BASE + serverStatusPath(), BASE + windMapPath(area.south, area.west, area.north, area.east)),
            server.asked.toList(),
        )
    }

    @Test
    fun agesAreMeasuredByTheServersClockNotThePhones() = runBlocking {
        val server = Server()
        // A phone whose clock is three hours ahead.
        val clock = Clock((server.clock + 3 * 3600) * 1000)
        val wind = model(server, clock)
        assertEquals(server.clock + 3 * 3600, wind.now)

        wind.show(alps)
        wind.load()
        assertEquals(server.clock, wind.now)
        // By the phone's clock every reading would be too old to draw.
        assertTrue(wind.stations.any { !it.isOld })

        // The phone's clock runs on, and the server's with it.
        clock.pass(minutes = 2)
        assertEquals(server.clock + 120, wind.now)
    }

    @Test
    fun withoutAServerNothingIsAsked() = runBlocking {
        val server = Server()
        val wind = model(server, address = "  ")
        assertEquals(WindServer.None, wind.server)
        wind.show(alps)
        wind.load()
        assertTrue(server.asked.isEmpty())
        assertTrue(wind.stations.isEmpty())
        assertNull(wind.notice)
        assertNull(wind.creditLine)
    }

    @Test
    fun theServerIsAskedOnceTheMapRestsAndNotForWhatIsAlreadyThere() = runBlocking {
        val server = Server()
        val clock = Clock(server.clock * 1000)
        // The model waits in this test's own scope, as it does in the
        // app's: everything happens on one thread.
        val wind = model(server, clock, scope = this)

        // A drag reports many regions; only where it comes to rest counts.
        wind.show(alps)
        wind.show(alps.copy(west = 10.9, east = 12.1))
        wind.show(alps)
        waitUntil { wind.stations.isNotEmpty() }
        assertEquals(1, server.stationRequests().size)

        // A small move stays inside what was fetched: nothing is asked.
        wind.show(alps.copy(west = 10.85, east = 12.05))
        delay(2 * WindModel.REST_MILLISECONDS)
        assertEquals(1, server.stationRequests().size)

        // A move beyond it asks again.
        wind.show(Bounds(south = 46.0, west = 12.5, north = 46.8, east = 13.7))
        waitUntil { server.stationRequests().size == 2 }

        // And so does the same place once the answer is a minute old.
        clock.pass(minutes = 1)
        wind.show(Bounds(south = 46.0, west = 12.5, north = 46.8, east = 13.7))
        waitUntil { server.stationRequests().size == 3 }
    }

    @Test
    fun arrowsOutliveAnUnreachableServerForAQuarterOfAnHourOnly() = runBlocking {
        val server = Server()
        val clock = Clock(server.clock * 1000)
        val wind = model(server, clock)
        wind.show(alps)
        wind.load()
        val before = wind.stations
        assertTrue(before.isNotEmpty())

        server.isReachable = false
        clock.pass(minutes = 5)
        wind.load()
        assertTrue(checkNotNull(wind.problem).startsWith("Venti could not be reached for the wind."))
        assertEquals(wind.problem, wind.notice)
        assertEquals(before, wind.stations)

        clock.pass(minutes = 11)
        wind.load()
        assertTrue(wind.stations.isEmpty())
        assertTrue(wind.credits.isEmpty())
        assertNotNull(wind.problem)

        // When it answers again the stations are back and the problem is gone.
        server.isReachable = true
        wind.load()
        assertEquals(before.map { it.id }, wind.stations.map { it.id })
        assertNull(wind.problem)
    }

    @Test
    fun whatTheServerRefusesIsSaidInTheCoresWords() = runBlocking {
        val server = Server()
        server.refusesBoxes = true
        val wind = model(server)
        wind.show(alps)
        wind.load()
        assertTrue(wind.stations.isEmpty())
        val problem = checkNotNull(wind.problem)
        assertTrue(problem.isNotBlank())
        assertFalse(problem.startsWith("Venti could not be reached for the wind."))
    }

    @Test
    fun aStationsLastDayIsReadForItsCharts() = runBlocking {
        val server = Server()
        val wind = model(server)
        wind.show(alps)
        wind.load()
        val summit = wind.stations.first { it.name == "Zugspitze" }

        val history = wind.history(summit)
        assertEquals(BASE + stationHistoryPath(summit.id), server.asked.last())
        assertEquals(summit.id, history.station.id)
        assertTrue(history.readings.size >= 2)
        assertEquals("Deutscher Wetterdienst", history.credits.first().provider)

        val samples = WindSample.of(history)
        assertEquals(history.readings.size, samples.size)
        val first = history.readings.first()
        assertEquals(checkNotNull(first.speedMs) * 3.6, checkNotNull(samples.first().wind), 1e-9)
        val newest = samples.maxBy { it.time }
        assertEquals(newest, WindSample.nearest(newest.time + 5, samples))
        assertEquals(samples.minBy { it.time }, WindSample.nearest(0, samples))

        Format.uses24HourClock = true
        assertEquals("Wind and gusts in km/h, and where the wind came from.", WindSample.caption(null))
        val words = WindSample.caption(newest)
        assertTrue(words, " km/h" in words && "gusts " in words && "from " in words)
    }

    @Test
    fun addressesAreReadTheWayPeopleWriteThem() {
        assertEquals(WindServer.None, WindServer.read(""))
        assertEquals(WindServer.None, WindServer.read("   "))
        assertEquals(WindServer.Usable("https://wind.example.org"), WindServer.read(" wind.example.org "))
        assertEquals(WindServer.Usable("https://wind.example.org"), WindServer.read("https://wind.example.org/"))
        assertEquals(WindServer.Usable("https://wind.example.org:8443"), WindServer.read("wind.example.org:8443//"))
        assertEquals(WindServer.Unusable, WindServer.read("not an address"))
        assertEquals(WindServer.Unusable, WindServer.read("ftp://wind.example.org"))
        assertEquals(WindServer.Unusable, WindServer.read("https://"))
    }

    @Test
    fun withoutEncryptionOnlyAServerOnThePilotsOwnNetworkIsAsked() {
        // At home: by address, by a name the router gives, or on this device.
        for (home in listOf(
            "http://192.168.1.20:8080/",
            "http://10.0.2.2:8080",
            "http://172.16.4.1",
            "http://169.254.10.1",
            "http://venti.local:8080",
            "http://raspberrypi:8080",
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "http://[fd12:3456::1]:8080",
        )) {
            assertEquals(home, WindServer.Usable(home.trimEnd('/')), WindServer.read(home))
        }
        // Anywhere else an address has to be https.
        for (elsewhere in listOf(
            "http://wind.example.org",
            "http://8.8.8.8",
            "http://172.32.0.1",
            "http://192.168.1.20.example.org",
            "http://localhost.example.org",
            "http://[2001:db8::1]",
        )) {
            assertEquals(elsewhere, WindServer.NeedsEncryption, WindServer.read(elsewhere))
        }

        // An address the app was built with that it may not ask is no server to it.
        val wind = model(Server(), address = "http://wind.example.org")
        assertEquals(WindServer.NeedsEncryption, wind.server)
    }

    private suspend fun waitUntil(isTrue: () -> Boolean) {
        var waited = 0
        while (!isTrue() && waited < 5000) {
            delay(20)
            waited += 20
        }
        assertTrue("waited five seconds", isTrue())
    }

    private companion object {
        const val BASE = "https://wind.example.org"
    }
}
