package fly.venti.app.sites

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.wind.grown
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.SiteKind
import fly.venti.app.map.SiteFans
import fly.venti.core.SitesShown
import fly.venti.core.SiteView
import fly.venti.core.WindSpan
import fly.venti.core.WindStation
import fly.venti.core.serverStatus
import fly.venti.core.sitesPath
import fly.venti.core.sitesPathWithPilots
import fly.venti.core.stationsNear
import fly.venti.core.windMap
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.maplibre.geojson.Point
import java.io.IOException
import java.util.Locale

/**
 * The flying sites on the map against the real core, with the server played
 * by the answer the real server's own tests keep for `/v1/sites`: five
 * sites east of the Tegernsee, as OpenStreetMap had them.
 */
class SitesModelTest {
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

        @Volatile
        var refuses = false

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReachable) throw IOException("no route to host")
            return when {
                "/v1/sites?" !in url -> HttpAnswer(404, ByteArray(0))
                refuses -> HttpAnswer(400, recorded("error_missing_bbox.json"))
                // The crowdsourced layer is in an answer only when it was asked for.
                "venti-pilots" in url -> HttpAnswer(200, recorded("sites_venti_pilots.json"))
                else -> HttpAnswer(200, recorded("sites.json"))
            }
        }

        companion object {
            fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()
        }
    }

    /** A clock the test moves by hand, in milliseconds since 1970. */
    private class Clock(var now: Long = 1_795_000_000_000) {
        fun pass(minutes: Int) {
            now += minutes * 60_000L
        }
    }

    /** The valley the recorded sites lie in, as a phone's map shows it. */
    private val valley = Bounds(south = 47.6, west = 11.7, north = 47.75, east = 11.95)

    private fun model(
        server: Server,
        clock: Clock = Clock(),
        scope: CoroutineScope = still,
        address: () -> String? = { BASE },
    ) = SitesModel(scope, transport = server::answer, clock = { clock.now }).also { it.server = address }

    @Test
    fun theMapGetsItsSitesAndWhoseDataTheyAre() = runBlocking {
        val server = Server()
        val sites = model(server)
        val area = valley.grown(0.5)
        sites.load(area, BASE)

        assertNull(sites.problem)
        assertEquals(listOf(BASE + sitesPath(area.south, area.west, area.north, area.east)), server.asked.toList())
        assertEquals(5, sites.sites.size)
        // The recorded region is the server's whole first region, which holds the valley.
        assertTrue(sites.isWhole)
        assertEquals(listOf("OpenStreetMap contributors"), sites.credits.map { it.provider })
        assertEquals("Sites: OpenStreetMap contributors", sites.creditLine)
        // The copy of OpenStreetMap that answered the server was taken on 18 November 2026.
        val asOf = checkNotNull(sites.asOfInWords)
        assertTrue(asOf, asOf.startsWith("As of ") && asOf.endsWith("Nov 2026."))

        val takeoff = sites.sites.first { it.name == "Fluggelände Jägerkamp/Nagelspitz" }
        assertEquals(SiteKind.TAKEOFF, takeoff.kind)
        assertTrue(takeoff.isNamed)
        assertEquals("Take-off · 1560 m", SiteWords.detail(takeoff))
        // A site the source has no name for is named by what it is, and its row says no more.
        val bare = sites.sites.first { !it.isNamed && it.kind == SiteKind.LANDING }
        assertEquals("Landing", bare.name)
        // Nothing else is known of it; it is at least somewhere.
        assertEquals("47.6919° N, 11.8880° E", SiteWords.detail(bare))
    }

    @Test
    fun thePlacesFromPilotsAreAskedForOnlyOnceThePilotSwitchesThemOn() = runBlocking {
        val server = Server()
        val sites = model(server, scope = this)
        val area = valley.grown(0.5)
        // Off until the pilot says so: the path names no such layer, and the filter is not on.
        assertFalse(sites.filter.withPilots)
        sites.load(area, BASE)
        assertEquals(BASE + sitesPath(area.south, area.west, area.north, area.east), server.asked.single())
        assertFalse("venti-pilots" in server.asked.single())
        assertFalse(sites.filtered.isOn)
        assertEquals("", sites.filtered.fromPilotsWords)
        assertTrue(sites.sites.none { it.byPilots?.fromPilots == true })

        // Switched on, in the filter: the same area is asked for again, with
        // the layer, and the core counts the filter as on and says so in its line.
        sites.filter = sites.filter.copy(withPilots = true)
        delay(200)
        assertEquals(BASE + sitesPathWithPilots(area.south, area.west, area.north, area.east), server.asked.last())
        assertEquals(2, server.asked.size)
        assertTrue(sites.filtered.isOn)
        assertTrue(sites.filtered.filterWords, sites.filtered.filterWords.endsWith("with places from pilots"))
        val place = sites.sites.single()
        assertEquals(true, place.byPilots?.fromPilots)
        // Marked wherever it is shown: in the list's line, and under its name on the map.
        assertEquals("From pilots", place.byPilots?.label)
        assertEquals("From pilots · Take-off · 2104 m", SiteWords.detail(place))
        assertEquals("Beispieljoch\nFrom pilots", SiteFeature.label(place))
        assertEquals("It is a place from pilots: not official and not checked by anyone.", sites.filtered.fromPilotsWords)
        // An answer with the layer does not stand for a map that asks without it, and the other way round.
        sites.show(valley)
        delay(2 * SitesModel.REST_MILLISECONDS)
        assertEquals(2, server.asked.size)

        // Off again: asked again without it, and nothing of the layer stays.
        sites.filter = sites.filter.copy(withPilots = false)
        delay(200)
        assertEquals(3, server.asked.size)
        assertFalse("venti-pilots" in server.asked.last())
        assertFalse(sites.filtered.isOn)
        assertTrue(sites.sites.none { it.byPilots?.fromPilots == true })
        // A site of a source keeps its name alone on the map and in its line.
        val takeoff = sites.sites.first { it.name == "Fluggelände Jägerkamp/Nagelspitz" }
        assertEquals("Fluggelände Jägerkamp/Nagelspitz", SiteFeature.label(takeoff))
        // The same choice again asks nobody, and neither does another part of the filter.
        sites.filter = sites.filter.copy(withPilots = false)
        sites.filter = sites.filter.copy(shown = SitesShown.TAKEOFFS)
        delay(200)
        assertEquals(3, server.asked.size)
    }

    @Test
    fun aMapThatShowsACountryAsksForNoSites() = runBlocking {
        val server = Server()
        val sites = model(server, scope = this)
        assertFalse(SitesModel.showsSites(valley.copy(west = 11.0, east = 12.0)))
        sites.show(valley.copy(west = 11.0, east = 12.0))
        delay(2 * SitesModel.REST_MILLISECONDS)
        assertTrue(server.asked.isEmpty())
        assertNull(sites.creditLine)
    }

    @Test
    fun theServerIsAskedOnceTheMapRestsAndNotForWhatIsAlreadyThere() = runBlocking {
        val server = Server()
        val clock = Clock()
        val sites = model(server, clock, scope = this)

        // A drag reports many regions; only where it comes to rest counts.
        sites.show(valley)
        sites.show(valley.copy(west = 11.72, east = 11.97))
        sites.show(valley)
        waitUntil { sites.sites.isNotEmpty() }
        assertEquals(1, server.asked.size)
        // Half as much again on every side was asked for.
        val area = valley.grown(0.5)
        assertEquals(BASE + sitesPath(area.south, area.west, area.north, area.east), server.asked.single())

        // A drag inside that needs no request, and nor does the same place a little later.
        sites.show(valley.copy(west = 11.6, east = 11.85))
        clock.pass(minutes = 59)
        sites.show(valley)
        delay(2 * SitesModel.REST_MILLISECONDS)
        assertEquals(1, server.asked.size)

        // A move beyond it asks again, and so does the same place after an hour.
        sites.show(Bounds(south = 47.3, west = 12.3, north = 47.45, east = 12.55))
        waitUntil { server.asked.size == 2 }
        clock.pass(minutes = 60)
        sites.show(Bounds(south = 47.3, west = 12.3, north = 47.45, east = 12.55))
        waitUntil { server.asked.size == 3 }
    }

    @Test
    fun whatTheServerRefusesIsSaidInTheCoresWordsAndTheSitesStay() = runBlocking {
        val server = Server()
        val sites = model(server)
        val area = valley.grown(0.5)
        sites.load(area, BASE)
        val before = sites.sites

        server.refuses = true
        sites.load(area, BASE)
        val refused = checkNotNull(sites.problem)
        assertFalse(refused, refused.startsWith("Venti could not be reached"))
        assertEquals(before, sites.sites)

        server.refuses = false
        server.isReachable = false
        sites.load(area, BASE)
        assertTrue(checkNotNull(sites.problem).startsWith("Venti could not be reached for flying sites."))
        assertEquals(before, sites.sites)

        server.isReachable = true
        sites.load(area, BASE)
        assertNull(sites.problem)
    }

    @Test
    fun withoutAServerOrWithAnotherOneTheSitesAreGone() = runBlocking {
        val server = Server()
        var address: String? = BASE
        val sites = model(server, address = { address })
        sites.load(valley.grown(0.5), BASE)
        assertTrue(sites.sites.isNotEmpty())

        // Another server's sites are not this one's.
        address = "https://other.example.org"
        sites.show(valley)
        assertTrue(sites.sites.isEmpty())
        assertTrue(sites.credits.isEmpty())

        // An answer that arrives for a server the pilot has left is dropped.
        sites.load(valley.grown(0.5), BASE)
        assertTrue(sites.sites.isEmpty())

        sites.load(valley.grown(0.5), "https://other.example.org")
        assertTrue(sites.sites.isNotEmpty())
        address = null
        sites.show(valley)
        assertTrue(sites.sites.isEmpty())
        assertNull(sites.asOfInWords)
    }

    @Test
    fun theListHasTheNamedSitesFirstAndFindsOneByName() = runBlocking {
        val sites = model(Server())
        sites.load(valley.grown(0.5), BASE)

        val listed = SiteWords.listed(sites.sites, "")
        assertEquals(
            listOf("Fluggelände Jägerkamp/Nagelspitz", "Landeplatz Jägerkamp/Nagelspitz", "Landing", "Take-off", "Take-off"),
            listed.map { it.name },
        )
        assertEquals(
            listOf("Landeplatz Jägerkamp/Nagelspitz"),
            SiteWords.listed(sites.sites, "landeplatz").map { it.name },
        )

        val under = SiteWords.underTheList(sites)
        assertTrue(under, under.startsWith(SiteWords.CAUTION))
        assertTrue(under, "© OpenStreetMap contributors" in under)

        // A site's page says what a sign on a map is worth, and what its source asks to be said.
        val footnote = SiteWords.footnote(listed.first(), SiteSources().with(sites))
        assertTrue(footnote, footnote.startsWith(SiteWords.CAUTION))
        assertTrue(footnote, "Open Database License" in footnote)
        assertTrue(footnote, "© OpenStreetMap contributors" in footnote)
        assertTrue(footnote, "As of " in footnote)
    }

    @Test
    fun theMapWritesOnlyANameTheSourceGives() = runBlocking {
        val sites = model(Server())
        sites.load(valley.grown(0.5), BASE)

        val named = SiteFeature.of(sites.sites.first { it.isNamed && it.kind == SiteKind.LANDING })
        assertEquals("site-landing", named.getStringProperty(SiteFeature.PICTURE))
        assertEquals("Landeplatz Jägerkamp/Nagelspitz", named.getStringProperty(SiteFeature.LABEL))

        val bare = sites.sites.first { !it.isNamed && it.kind == SiteKind.TAKEOFF }
        val feature = SiteFeature.of(bare)
        assertEquals("site-takeoff", feature.getStringProperty(SiteFeature.PICTURE))
        assertEquals("", feature.getStringProperty(SiteFeature.LABEL))
        assertEquals(bare.id, feature.getStringProperty(SiteFeature.ID))
        val place = feature.geometry() as Point
        assertEquals(bare.position.lat, place.latitude(), 1e-9)
        assertEquals(bare.position.lon, place.longitude(), 1e-9)

        // Every kind has a picture of its own name.
        assertEquals(SiteKind.entries.size, SiteKind.entries.map { it.picture }.toSet().size)
    }

    /** A made-up take-off and landing, as the core would hand it out. */
    private fun site(position: Coordinate, elevationM: Double?, wind: List<WindSpan>, windInWords: String) = SiteView(
        id = "test:1",
        name = "Sample take-off (not real)",
        isNamed = true,
        kind = SiteKind.TAKEOFF,
        uses = listOf(SiteKind.TAKEOFF, SiteKind.LANDING),
        kindInWords = "Take-off and landing",
        position = position,
        elevationM = elevationM,
        wind = wind,
        windInWords = windInWords,
        official = true,
        discouraged = false,
        paragliding = null,
        hangGliding = null,
        source = "test",
        url = null,
        website = null,
        description = null,
        fee = null,
        feeInWords = "",
        guestGuidelines = null,
        guestsInWords = "",
        noFlightTime = null,
        noFlightInWords = "",
        outline = emptyList(),
    )

    @Test
    fun aTakeoffsFanCoversItsDirectionsAndIsColouredByTheCore() {
        val south = site(Coordinate(lat = 47.0, lon = 11.0), 1200.0, listOf(WindSpan(fromDeg = 180.0, toDeg = 225.0)), "S to SW")
        assertTrue(SiteFans.hasFan(south))
        // A landing's pale pin is its sign; a site without directions has no fan.
        assertFalse(SiteFans.hasFan(south.copy(kind = SiteKind.LANDING)))
        assertFalse(SiteFans.hasFan(south.copy(wind = emptyList())))
        // Half a point of the compass more on each side.
        assertEquals(listOf(168.75 to 236.25), SiteFans.wedges(south.wind))
        // A span over north goes on past 360; all round stays one circle.
        assertEquals(listOf(326.25 to 416.25), SiteFans.wedges(listOf(WindSpan(fromDeg = 337.5, toDeg = 45.0))))
        assertEquals(listOf(-11.25 to 348.75), SiteFans.wedges(listOf(WindSpan(fromDeg = 0.0, toDeg = 360.0))))
        // Whether the wind fits is the core's; no wind at the place is neither.
        assertEquals(SiteFans.Fit.Fits, SiteFans.fit(south, windFrom = 200.0))
        assertEquals(SiteFans.Fit.DoesNotFit, SiteFans.fit(south, windFrom = 20.0))
        assertEquals(SiteFans.Fit.Unknown, SiteFans.fit(south, windFrom = null))
        // One picture for the same spans in the same state.
        assertEquals("site-fan-fits-180-225", SiteFans.name(south.wind, SiteFans.Fit.Fits))
        assertEquals("site-fan-flight-180-225", SiteFans.name(south.wind, SiteFans.Fit.InFlight))
    }

    @Test
    fun theHoursArrowsSayWhereTheWindComesFromSquareBySquare() {
        val arrows = """{"type":"FeatureCollection","features":[""" +
            """{"type":"Feature","geometry":{"type":"Point","coordinates":[11.0,47.0]},"properties":{"cell":0,"symbol":"calm","band":"light","heading":0,"label":"1/4"}},""" +
            """{"type":"Feature","geometry":{"type":"Point","coordinates":[11.1,47.0]},"properties":{"cell":1,"symbol":"arrow","band":"light","heading":170,"label":"9"}},""" +
            """{"type":"Feature","geometry":{"type":"Point","coordinates":[11.2,47.0]},"properties":{"cell":2,"symbol":"arrow","band":"moderate","heading":270,"label":"15"}}]}"""
        // Blowing to 170 is from 350; a calm square has no direction.
        assertEquals(mapOf(1u to 350.0, 2u to 90.0), SiteFans.windFrom(arrows))
        assertTrue(SiteFans.windFrom("not json").isEmpty())
    }

    @Test
    fun aRowSaysTheWindASiteWorksIn() {
        val site = site(
            Coordinate(lat = 47.0, lon = 11.0),
            elevationM = 1234.4,
            wind = listOf(WindSpan(fromDeg = 180.0, toDeg = 225.0)),
            windInWords = "S to SW",
        )
        assertEquals("Take-off and landing · 1234 m · wind S to SW", SiteWords.detail(site))
    }

    @Test
    fun theWindMeasuredNearASiteSaysWhereItStandsAndWhetherItsDirectionFits() {
        // The stations the server's own tests keep, at the moment they were recorded.
        val now = serverStatus(200.toUShort(), Server.recorded("health.json")).timeUnixS
        val recorded = Server.recorded("stations.json")
        val stations = windMap(200.toUShort(), recorded, now).stations
        // A made-up take-off due east of the station in Garmisch, which
        // stands in the valley and had a light wind from the north.
        val takeoff = site(
            Coordinate(lat = 47.483, lon = 11.1478),
            elevationM = 1700.0,
            wind = listOf(WindSpan(fromDeg = 90.0, toDeg = 247.5)),
            windInWords = "E to WSW",
        )

        fun near(site: SiteView, among: List<WindStation> = stations) =
            stationsNear(among, site.position, site.elevationM, site.wind, SiteWords.STATIONS_WITHIN_M, 3u)

        // The summit of the Zugspitze, 14 km on, is within reach too; the nearest comes first.
        val valley = near(takeoff).first()
        assertEquals("Garmisch-Partenkirchen", valley.station.name)
        // The network last: two stations can share a name.
        assertEquals("6.4 km W · 980 m lower · DWD", SiteWords.whereabouts(valley))
        assertEquals("From N: not a direction the site works in", SiteWords.fit(valley))

        // The same site turned to the north: the same wind fits, and that is all that is said.
        val north = takeoff.copy(wind = listOf(WindSpan(fromDeg = 315.0, toDeg = 45.0)))
        assertEquals("From N: a direction the site works in", SiteWords.fit(near(north).first()))

        // A site that gives no height and no directions: no word on either.
        val bare = near(takeoff.copy(elevationM = null, wind = emptyList())).first()
        assertEquals("6.4 km W · DWD", SiteWords.whereabouts(bare))
        assertNull(SiteWords.fit(bare))

        // Two hours on, the reading no longer stands for the wind now.
        val later = now + 2 * 3600
        val old = near(takeoff, among = windMap(200.toUShort(), recorded, later).stations).first()
        assertNull(SiteWords.fit(old))
        assertEquals("", old.station.mark.label)
        assertEquals("3 hours ago", SiteWords.age(old.station, later))
    }

    @Test
    fun aWedgeOfTheCompassStartsAtNorthAndRunsClockwise() {
        // South to south-west: from straight down on the canvas, 45 degrees on.
        assertEquals(90f to 45f, SiteWords.wedge(WindSpan(fromDeg = 180.0, toDeg = 225.0)))
        // A single direction is the sixteenth of the compass it names.
        assertEquals(168.75f to 22.5f, SiteWords.wedge(WindSpan(fromDeg = 270.0, toDeg = 270.0)))
        // A span across north, north-west to north-east.
        assertEquals(225f to 90f, SiteWords.wedge(WindSpan(fromDeg = 315.0, toDeg = 45.0)))
        // The whole compass.
        assertEquals(-90f to 360f, SiteWords.wedge(WindSpan(fromDeg = 0.0, toDeg = 360.0)))
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
