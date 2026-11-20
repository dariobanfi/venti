package fly.venti.app.airspace

import fly.venti.app.HttpAnswer
import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.app.flight.FlightAirspaceWords
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.logbook.PickedFile
import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.settings.Preferences
import fly.venti.core.AirspaceCover
import fly.venti.core.AirspaceFile
import fly.venti.core.AirspaceImport
import fly.venti.core.AirspaceOrigin
import fly.venti.core.AirspacePolicy
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.maplibre.geojson.Polygon
import java.io.File
import java.io.IOException
import java.util.Locale

/**
 * The pilot's airspace against the real core, which keeps it, reads it and
 * answers for it: the pilot's files, and what a Venti server gives.
 */
class AirspaceModelTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    /** 4 July 2026, 10:30 UTC: the day the files are brought in. */
    private val today = 1_783_161_000L

    private fun makeModel(): AirspaceModel =
        LogbookModel.open(TestSupport.newFolder("airspace"), TestSupport.files).makeAirspace(now = { today })

    /** The made-up control zone the UI tests start with, as an OpenAir file. */
    private val zone = PickedFile("zones.txt") { LaunchOptions.SAMPLE_AIRSPACE_FILE.toByteArray() }

    /** The valley the zone lies in. */
    private val valley = Bounds(south = 47.6, west = 11.6, north = 47.8, east = 11.9)

    @Test
    fun aFilesAirspaceIsKeptAndFoundOnTheMap() = runBlocking {
        val airspace = makeModel()
        assertTrue(airspace.files.isEmpty())
        assertNull(airspace.set)
        assertTrue(airspace.shapes(valley).isEmpty())

        val notice = airspace.importFiles(listOf(zone))

        assertEquals("Airspace", notice.title)
        assertEquals("zones.txt: 1 airspace read.", notice.text)
        val file = airspace.files.single()
        // The file is known by its name without the ending.
        assertEquals("zones", file.name)
        assertEquals("1 airspace · brought in Sat, 4 Jul 2026", AirspaceModel.detail(file))
        assertNull(AirspaceModel.leftOutNote(file))
        assertNotNull(airspace.set)

        val shape = airspace.shapes(valley).single()
        assertEquals("SAMPLE ZONE (NOT REAL)", shape.name)
        // A control zone is closed to a paraglider.
        assertEquals(AirspacePolicy.AVOID, shape.policy)
        assertTrue(shape.outline.size >= 8)
        // Elsewhere there is nothing.
        assertTrue(airspace.shapes(Bounds(south = 45.0, west = 6.0, north = 45.5, east = 6.5)).isEmpty())

        airspace.delete(file)
        assertTrue(airspace.files.isEmpty())
        assertNull(airspace.set)
    }

    @Test
    fun aFileThatHoldsNoAirspaceIsNamedWithTheReason() = runBlocking {
        val airspace = makeModel()

        val notice = airspace.importFiles(
            listOf(
                PickedFile("holiday.jpg") { byteArrayOf(-1, -40, -1, -32, 0, 16) },
                PickedFile("gone.txt") { throw IOException("The file could not be opened.") },
                zone,
            ),
        )

        val lines = notice.text.split("\n\n")
        assertEquals(3, lines.size)
        assertTrue(lines[0], lines[0].startsWith("holiday.jpg: "))
        assertEquals("gone.txt: The file could not be opened.", lines[1])
        assertEquals("zones.txt: 1 airspace read.", lines[2])
        // Only the file that was read is kept.
        assertEquals(listOf("zones"), airspace.files.map { it.name })
    }

    @Test
    fun whatAFileLeftOutIsSaidPlainly() {
        fun result(airspaces: UInt, leftOut: UInt = 0u, assumed: UInt = 0u, problems: List<String> = emptyList()) =
            AirspaceImport(airspaces = airspaces, leftOut = leftOut, assumed = assumed, problems = problems)

        assertEquals("de.txt: 1,204 airspaces read.", AirspaceModel.summary(result(1204u), "de.txt"))
        assertEquals(
            "de.txt: 12 airspaces read. 1 airspace could not be read and is left out: " +
                "you will not be warned about it.",
            AirspaceModel.summary(result(12u, leftOut = 1u), "de.txt"),
        )
        assertEquals(
            "de.txt: 12 airspaces read. 3 airspaces could not be read and are left out: " +
                "you will not be warned about them. In 2 places something could not be read and the " +
                "cautious reading was taken.\nLine 40: no upper limit",
            AirspaceModel.summary(result(12u, leftOut = 3u, assumed = 2u, problems = listOf("Line 40: no upper limit")), "de.txt"),
        )
        assertEquals(
            "at.txt: 1 airspace read. In 1 place something could not be read and the cautious reading was taken.",
            AirspaceModel.summary(result(1u, assumed = 1u), "at.txt"),
        )

        fun file(leftOut: UInt) =
            AirspaceFile(storedName = "x", name = "x.txt", importedUnixS = null, airspaces = 5u, leftOut = leftOut)
        assertEquals("5 airspaces", AirspaceModel.detail(file(0u)))
        assertNull(AirspaceModel.leftOutNote(file(0u)))
        assertEquals(
            "1 airspace in this file could not be read. You will not be warned about it.",
            AirspaceModel.leftOutNote(file(1u)),
        )
        assertEquals(
            "4 airspaces in this file could not be read. You will not be warned about them.",
            AirspaceModel.leftOutNote(file(4u)),
        )
    }

    /** 19 November 2026, 12:00 UTC: when the server is asked. */
    private val asked = 1_795_089_600L

    /**
     * What a Venti server answers when asked for airspace: one made-up
     * zone over the valley of the sample flight, complete for the Alps,
     * written by its provider the day before.
     */
    private val serverAnswer = """
        {"layers": [{
            "source": "openaip", "fetched_at": "2026-11-18T19:05:00Z", "file_date": "2026-11-18T12:00:00Z",
            "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}], "countries": ["DE", "AT"],
            "airspaces": [{
                "id": "openaip:sample", "name": "SERVER ZONE (NOT REAL)", "class": "D", "airspace_type": "ctr",
                "country": "DE",
                "lower": {"reference": "ground", "metres": 0.0},
                "upper": {"reference": "mean-sea-level", "value": 9000.0, "unit": "feet", "metres": 2743.2},
                "geometry": {"exterior": [[11.70, 47.68], [11.76, 47.68], [11.76, 47.72], [11.70, 47.72], [11.70, 47.68]]},
                "activity": {"by_notam": false, "on_request": false, "on_demand": false, "special_agreement": false},
                "validity": {"file_date": "2026-11-18T12:00:00Z"}
            }]
        }],
        "attributions": [{
            "source": "openaip", "provider": "openAIP", "licence": "CC BY-NC 4.0",
            "text": "Data used comes from openAIP.", "url": "https://www.openaip.net",
            "notice": "OpenAIP data is not certified."
        }]}
    """.trimIndent()

    private val tegernsee = Coordinate(lat = 47.7, lon = 11.75)

    /** A model that asks a server which answers with `answer`, and the addresses it was asked for. */
    private fun makeModelWithServer(
        folder: File = TestSupport.newFolder("airspace"),
        clock: () -> Long = { asked },
        remembered: Preferences? = null,
        answer: () -> HttpAnswer,
    ): Pair<AirspaceModel, MutableList<String>> {
        val addresses = mutableListOf<String>()
        val airspace = LogbookModel.open(folder, TestSupport.files).makeAirspace(
            now = clock,
            remembered = remembered,
            transport = { url ->
                addresses += url
                answer()
            },
        )
        return airspace to addresses
    }

    @Test
    fun airspaceIsFetchedFromTheServerOnceKeptAndUsed() = runBlocking {
        val folder = TestSupport.newFolder("airspace")
        val (airspace, addresses) = makeModelWithServer(folder) { HttpAnswer(200, serverAnswer.toByteArray()) }

        // Without a server nothing is asked.
        airspace.refresh(tegernsee)
        assertTrue(addresses.isEmpty())
        assertTrue(airspace.isEmpty)
        assertEquals("None", airspace.inALine)
        assertNull(airspace.mapCredit)

        airspace.server = { "http://127.0.0.1:8080" }
        airspace.refresh(tegernsee)
        // Two degrees of latitude and three of longitude to each side.
        assertEquals(listOf("http://127.0.0.1:8080/v1/airspace?bbox=8.75,45.7,14.75,49.7"), addresses)
        assertNull(airspace.serverProblem)
        assertEquals("openAIP, 1 airspace", airspace.inALine)
        val layer = airspace.kept.single()
        assertEquals("openAIP", layer.provider)
        assertTrue(layer.saysWhatItCovers)
        assertEquals("1 airspace · as of Wed, 18 Nov 2026 · fetched Thu, 19 Nov 2026", AirspaceModel.detail(layer))
        assertEquals(
            "The airspace of Germany and Austria. Of other countries none is kept, also where the area " +
                "fetched reaches into them.",
            AirspaceModel.countriesNote(layer),
        )
        assertEquals("OpenAIP data is not certified.", layer.credit?.notice)
        assertEquals(listOf("openAIP"), airspace.credits.map { it.provider })
        assertEquals("Airspace: openAIP, not certified", airspace.mapCredit)
        assertEquals(listOf("SERVER ZONE (NOT REAL)"), airspace.shapes(valley).map { it.name })

        // What is kept is fresh: a second look around sends nothing.
        airspace.refresh(Coordinate(lat = 47.5, lon = 11.9))
        assertEquals(1, addresses.size)

        // With a file of the pilot's beside it, the line names both.
        airspace.importFiles(listOf(zone))
        assertEquals("openAIP, 1 airspace · 1 file", airspace.inALine)
        assertEquals(2, airspace.shapes(valley).size)
        airspace.delete(airspace.files.single())

        // The cockpit warns from it, and says where it is from.
        val session = FlightSession(folder.path)
        session.useAirspace(airspace.set)
        val replay = Replay(TestSupport.sampleFlight)
        while (session.airspaceWarnings().isEmpty() && replay.advance(session, 1.0)) {
            // Until the glider comes near the zone.
        }
        val warning = session.airspaceWarnings().first()
        assertEquals("SERVER ZONE (NOT REAL)", warning.name)
        assertEquals(AirspaceOrigin.SERVER, warning.origin)
        assertEquals("From openAIP, by way of Venti, as of Wed, 18 Nov 2026.", AirspaceWords.origin(warning))

        // Switched off, what was kept is gone and nothing is asked.
        airspace.stopUsingServer()
        assertFalse(airspace.usesServer)
        assertTrue(airspace.isEmpty)
        assertNull(airspace.set)
        airspace.refresh(tegernsee)
        assertEquals(1, addresses.size)

        // Switched on again, it is fetched the next time the app looks somewhere.
        airspace.useServer()
        airspace.refresh(tegernsee)
        assertEquals(2, addresses.size)
        assertEquals("openAIP, 1 airspace", airspace.inALine)
    }

    @Test
    fun aRealAnswerOfTheServerIsKeptWithItsProvidersWords() = runBlocking {
        // The answer the server's own tests pin: two airspaces at Hochfilzen.
        val recorded = TestSupport.recordedAnswers.resolve("airspace.json").readBytes()
        val (airspace, _) = makeModelWithServer { HttpAnswer(200, recorded) }
        airspace.server = { "https://venti.example" }

        airspace.refresh(Coordinate(lat = 47.5, lon = 12.65))

        assertNull(airspace.serverProblem)
        val layer = airspace.kept.single()
        assertEquals("openAIP", layer.provider)
        assertEquals(2u, layer.airspaces)
        assertEquals("openAIP, 2 airspaces", airspace.inALine)
        assertTrue(checkNotNull(layer.credit?.notice).startsWith("OpenAIP data is not certified"))
        assertTrue(checkNotNull(layer.credit).text.startsWith("Data used comes from openAIP"))
        assertTrue(checkNotNull(AirspaceModel.countriesNote(layer)).startsWith("The airspace of Germany, Austria, "))
        val hochfilzen = Bounds(south = 47.4, west = 12.5, north = 47.6, east = 12.8)
        assertEquals(2, airspace.shapes(hochfilzen).size)
    }

    @Test
    fun aServerThatCannotAnswerIsSaidAndLeftAloneForAWhile() = runBlocking {
        var time = asked
        val (airspace, addresses) = makeModelWithServer(clock = { time }) { HttpAnswer(503, ByteArray(0)) }
        airspace.server = { "http://127.0.0.1:8080" }

        airspace.refresh(tegernsee)
        assertEquals("The server is busy or being restarted. Try again in a moment.", airspace.serverProblem)
        assertTrue(airspace.isEmpty)

        // A minute later the server is left alone; after five it is asked again.
        time += 60
        airspace.refresh(tegernsee)
        assertEquals(1, addresses.size)
        time += AirspaceModel.RETRY_AFTER_SECONDS
        airspace.refresh(tegernsee)
        assertEquals(2, addresses.size)
    }

    @Test
    fun aServerThatCannotBeReachedIsSaidInWords() = runBlocking {
        val (airspace, _) = makeModelWithServer { throw IOException("No route to host") }
        airspace.server = { "http://127.0.0.1:8080" }

        airspace.refresh(tegernsee)

        assertEquals("Venti could not be reached for airspace. No route to host", airspace.serverProblem)
        assertTrue(airspace.isEmpty)
    }

    @Test
    fun theChoiceNotToUseTheServerIsRemembered() = runBlocking {
        val remembered = MemoryPreferences()
        val folder = TestSupport.newFolder("airspace")
        val (airspace, addresses) = makeModelWithServer(folder, remembered = remembered) {
            HttpAnswer(200, serverAnswer.toByteArray())
        }
        airspace.server = { "http://127.0.0.1:8080" }
        assertTrue(airspace.usesServer)
        airspace.refresh(tegernsee)
        assertEquals(1, airspace.kept.size)

        airspace.stopUsingServer()
        assertTrue(remembered.airspaceWithoutServer)

        // The next time the app starts, nothing is kept and nothing is asked.
        val (later, _) = makeModelWithServer(folder, remembered = remembered) { HttpAnswer(200, serverAnswer.toByteArray()) }
        later.server = { "http://127.0.0.1:8080" }
        assertFalse(later.usesServer)
        assertTrue(later.kept.isEmpty())
        later.refresh(tegernsee)
        assertEquals(1, addresses.size)

        later.useServer()
        assertFalse(remembered.airspaceWithoutServer)
    }

    @Test
    fun aFinishedFlightIsCheckedAgainstTheAirspaceTheAppHas() = runBlocking {
        val logbook = LogbookModel.open(TestSupport.newFolder("airspace"), TestSupport.files)
        val flight = logbook.addSample().added.first()

        // Without airspace nothing is checked, and the list says what to do.
        val unchecked = logbook.airspace(flight)
        assertEquals(AirspaceCover.NONE, unchecked.cover)
        assertTrue(unchecked.crossings.isEmpty())
        assertEquals(FlightAirspaceWords.NOT_CHECKED, FlightAirspaceWords.summary(unchecked))
        assertTrue(FlightAirspaceWords.caveat(AirspaceCover.NONE).startsWith("Add an airspace file"))

        // With the sample zone: the flight clips it, twelve minutes in.
        logbook.makeAirspace(now = { today }).importFiles(listOf(zone))
        val checked = logbook.airspace(flight)
        assertEquals(AirspaceCover.WHOLE, checked.cover)
        assertNull(FlightAirspaceWords.summary(checked))
        val crossing = checked.crossings.single()
        assertEquals("SAMPLE ZONE (NOT REAL)", FlightAirspaceWords.name(crossing))
        assertEquals("Class D control zone · GND to 9000 ft AMSL", FlightAirspaceWords.kind(crossing))
        assertEquals(AirspacePolicy.AVOID, crossing.policy)
        assertNull(crossing.uncertain)
        assertTrue("$crossing", crossing.entry.seconds.toLong() > 600)
        assertTrue("$crossing", crossing.seconds.toLong() in 20..90)
        assertTrue(FlightAirspaceWords.duration(crossing), FlightAirspaceWords.duration(crossing).endsWith(" min"))
        assertTrue(FlightAirspaceWords.moment(crossing, 0).endsWith(" m"))
        assertTrue(
            FlightAirspaceWords.caveat(AirspaceCover.WHOLE).contains("not the airspace of the day of the flight"),
        )
        assertTrue(FlightAirspaceWords.caveat(AirspaceCover.PARTLY).startsWith("Part of this flight lies where"))
        assertEquals(
            FlightAirspaceWords.NONE_ENTERED,
            FlightAirspaceWords.summary(checked.copy(crossings = emptyList())),
        )
    }

    @Test
    fun theServersAirspaceIsExplainedInWords() {
        assertEquals("Germany and Austria", AirspaceModel.countryNames(listOf("DE", "AT"), Locale.UK))
        assertEquals("Germany", AirspaceModel.countryNames(listOf("DE"), Locale.UK))
        // A code that is none stays as it is.
        assertEquals("Germany and not-a-country", AirspaceModel.countryNames(listOf("DE", "not-a-country"), Locale.UK))

        val off = AirspaceModel.serverNote(usesServer = false, hasServer = true, hasKept = false)
        assertEquals("Switched off: only your own files are used.", off)
        val noServer = AirspaceModel.serverNote(usesServer = true, hasServer = false, hasKept = false)
        assertTrue(noServer, noServer.startsWith("With Venti connected in the settings"))
        val notYet = AirspaceModel.serverNote(usesServer = true, hasServer = true, hasKept = false)
        assertTrue(notYet, notYet.startsWith("Fetched the next time the map is open"))
        val kept = AirspaceModel.serverNote(usesServer = true, hasServer = true, hasKept = true)
        assertTrue(kept, kept.startsWith("Kept on this phone for about 200 km"))
    }

    @Test
    fun theMapIsToldWhatToDrawForAnAirspace() = runBlocking {
        val airspace = makeModel()
        airspace.importFiles(listOf(zone))
        val shape = airspace.shapes(valley).single()

        val feature = AirspaceLayers.feature(shape)

        val rings = (feature.geometry() as Polygon).coordinates()
        assertEquals(1 + shape.holes.size, rings.size)
        // A closed ring: it ends where it began.
        assertEquals(rings[0].first(), rings[0].last())
        assertEquals(2, feature.getNumberProperty(AirspaceLayers.POLICY).toInt())
        val label = feature.getStringProperty(AirspaceLayers.LABEL)
        assertEquals("SAMPLE ZONE (NOT REAL)\n${shape.lower} \u2013 ${shape.upper}", label)
        assertEquals("${shape.lower} \u2013 ${shape.upper}", AirspaceLayers.label(shape.copy(name = "")))

        // What matters most is drawn last, on top.
        assertTrue(AirspaceLayers.rank(AirspacePolicy.FREE) < AirspaceLayers.rank(AirspacePolicy.CAUTION))
        assertTrue(AirspaceLayers.rank(AirspacePolicy.CAUTION) < AirspaceLayers.rank(AirspacePolicy.AVOID))
        assertEquals(3, AirspacePolicy.entries.map(AirspaceLayers::rgb).toSet().size)
    }
}
