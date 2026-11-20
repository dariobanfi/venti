package fly.venti.app.community

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.design.Format
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.RouteKind
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import java.io.IOException
import java.util.Collections
import java.util.Locale

/**
 * Everyone's public flights against the real core, with the server played
 * by the answers the real server's own tests keep: a first page of the
 * newest flights, which says there is more, and the best of a day.
 */
class CommunityModelTest {
    private val locale = Locale.getDefault()

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
        val asked: MutableList<String> = Collections.synchronizedList(mutableListOf<String>())

        @Volatile
        var isReachable = true

        /** What it answers a list with instead of its flights, if anything. */
        @Volatile
        var refusal: HttpAnswer? = null

        /** Whether a pilot's flights take two pages. */
        @Volatile
        var pilotHasTwoPages = false

        /** What it hands out as a flight's file. */
        @Volatile
        var file: HttpAnswer = HttpAnswer(200, TestSupport.sampleFlight.toByteArray())

        fun answer(url: String): HttpAnswer {
            asked += url
            if (!isReachable) throw IOException("no route to host")
            return when {
                url.endsWith("/igc") -> file
                // The one pilot it knows, and what is left of the list after a first page.
                "/v1/pilots/demo?after=m1" in url -> HttpAnswer(200, PILOT_LAST_PAGE.toByteArray())
                url.endsWith("/v1/pilots/demo") ->
                    if (pilotHasTwoPages) {
                        HttpAnswer(200, PILOT_FIRST_PAGE.toByteArray())
                    } else {
                        HttpAnswer(200, recorded("pilot.json"))
                    }
                "/v1/pilots/" in url -> HttpAnswer(404, """{"error": {"code": "not_found", "message": "x"}}""".toByteArray())
                "/v1/flights/public" !in url -> HttpAnswer(404, ByteArray(0))
                refusal != null -> checkNotNull(refusal)
                "sort=score" in url -> HttpAnswer(200, recorded("public_flights_best.json"))
                // The page after the first: two more flights, and nothing after them.
                "after=" in url -> HttpAnswer(200, LAST_PAGE.toByteArray())
                else -> HttpAnswer(200, recorded("public_flights.json"))
            }
        }

        companion object {
            fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

            /** The recorded first page as a last one: other flights, and no mark for a next page. */
            val LAST_PAGE: String = recorded("public_flights.json").decodeToString()
                .replace("nfmvgygyr34epnfbcekb04nwqc", "aaaaaaaaaaaaaaaaaaaaaaaaaa")
                .replace("fbyw03atd9qdt5bfe177ra1k94", "bbbbbbbbbbbbbbbbbbbbbbbbbb")
                .replace(Regex(""""next"\s*:\s*"[^"]*""""), """"next": null""")

            /** The recorded page of a pilot as the first of two: with a mark for the next. */
            val PILOT_FIRST_PAGE: String = recorded("pilot.json").decodeToString().replaceFirst("{", """{"next": "m1", """)

            /** And the second: another flight of the same pilot, and nothing after it. */
            val PILOT_LAST_PAGE: String =
                recorded("pilot.json").decodeToString().replace("5t1eshypdtvrq7rxw39a18vsrg", "cccccccccccccccccccccccccc")
        }
    }

    private fun model(server: Server, address: () -> String? = { BASE }) =
        CommunityModel(transport = server::answer).also { it.server = address }

    @Test
    fun theNewestFlightsAreListedAndMoreFollowsWhenTheEndComesIntoView() = runBlocking {
        val server = Server()
        val community = model(server)
        assertTrue(community.flights.isEmpty())

        community.loadIfNeeded()
        assertEquals(listOf("$BASE/v1/flights/public"), server.asked.toList())
        assertNull(community.problem)
        assertEquals(PublicFlightsOrder.NEWEST, community.order)
        assertEquals(listOf("Jonas Beispiel", "Mara Muster"), community.flights.map { it.pilot })
        assertTrue(community.hasMore)
        assertFalse(community.isLoading)

        // A row: who flew, the day and the glider, how far and how long.
        val row = community.flights.first()
        assertEquals("${Format.day(row.startUnixS)} · Sample Wing", CommunityWords.detail(row))
        assertEquals(RouteKind.FREE_FLIGHT, row.score?.kind)
        assertEquals("12.0 km", CommunityWords.distance(row))
        assertEquals("0:29 h", CommunityWords.airtime(row))
        assertNull(CommunityWords.triangle(row))
        assertEquals("${CommunityWords.detail(row)}, 12.0 km, 0:29 h in the air", CommunityWords.spoken(row))
        assertEquals(row, community.flight(row.id))
        assertNull(community.flight("no such flight"))
        // A triangle is named, and a flight too short to score is listed by the length of its track.
        val triangle = row.copy(score = checkNotNull(row.score).copy(kind = RouteKind.FAI_TRIANGLE))
        assertEquals("FAI triangle", CommunityWords.triangle(triangle))
        assertEquals("Triangle", CommunityWords.triangle(row.copy(score = triangle.score?.copy(kind = RouteKind.FREE_TRIANGLE))))
        assertEquals("3.4 km", CommunityWords.distance(row.copy(score = null, trackLengthM = 3_400.0)))
        assertEquals(Format.day(row.startUnixS), CommunityWords.detail(row.copy(glider = null)))

        // Looked at again, nothing is asked: the list is there.
        community.loadIfNeeded()
        assertEquals(1, server.asked.size)

        // The last row has come into view: the next page is asked for with
        // the mark the first one gave, and its flights follow.
        community.loadMore()
        assertEquals(2, server.asked.size)
        assertTrue(server.asked.last(), server.asked.last().startsWith("$BASE/v1/flights/public?after=1786881900000000_"))
        assertEquals(4, community.flights.size)
        assertEquals(listOf("Jonas Beispiel", "Mara Muster"), community.flights.take(2).map { it.pilot })
        assertFalse(community.hasMore)
        // There is no more: the end of the list asks for nothing.
        community.loadMore()
        assertEquals(2, server.asked.size)

        // Pulled down, the list begins again from its top.
        community.refresh()
        assertEquals("$BASE/v1/flights/public", server.asked.last())
        assertEquals(2, community.flights.size)
        assertTrue(community.hasMore)
    }

    @Test
    fun theBestFlightsAreAnotherListFromItsTop() = runBlocking {
        val server = Server()
        val community = model(server)
        community.loadIfNeeded()

        community.show(PublicFlightsOrder.BEST)
        assertEquals("$BASE/v1/flights/public?sort=score", server.asked.last())
        assertEquals(PublicFlightsOrder.BEST, community.order)
        assertEquals(listOf("Demo Pilot", "Mara Muster"), community.flights.map { it.pilot })
        val points = community.flights.map { checkNotNull(it.score).points }
        assertEquals(points.sortedDescending(), points)
        assertFalse(community.hasMore)

        // The order that is shown is not asked for again.
        val asked = server.asked.size
        community.show(PublicFlightsOrder.BEST)
        community.loadIfNeeded()
        assertEquals(asked, server.asked.size)

        community.show(PublicFlightsOrder.NEWEST)
        assertEquals("$BASE/v1/flights/public", server.asked.last())
        assertEquals("Jonas Beispiel", community.flights.first().pilot)

        assertEquals(listOf("Newest", "Best"), PublicFlightsOrder.entries.map(CommunityWords::title))
    }

    @Test
    fun withoutAServerThereIsNoListAndAServerThatDoesNotAnswerIsSaid() = runBlocking {
        val server = Server()
        var address: String? = null
        val community = model(server) { address }

        community.loadIfNeeded()
        assertTrue(server.asked.isEmpty())
        assertTrue(community.flights.isEmpty())
        assertNull(community.problem)

        // No route to the server: said, with nothing listed.
        address = BASE
        server.isReachable = false
        community.loadIfNeeded()
        assertTrue(community.problem, checkNotNull(community.problem).startsWith("The server could not be reached for other pilots' flights."))
        assertTrue(community.flights.isEmpty())
        assertFalse(community.isLoading)

        // A server that refuses says why in the core's words.
        server.isReachable = true
        server.refusal = HttpAnswer(503, ByteArray(0))
        community.loadIfNeeded()
        assertTrue(checkNotNull(community.problem).isNotEmpty())
        assertFalse(checkNotNull(community.problem).startsWith("The server could not be reached"))

        // Asked again once it answers, the list is there and nothing is said.
        server.refusal = null
        community.loadIfNeeded()
        assertEquals(2, community.flights.size)
        assertNull(community.problem)

        // What is listed stays when a later page does not come.
        server.isReachable = false
        community.loadMore()
        assertEquals(2, community.flights.size)
        assertTrue(checkNotNull(community.problem).startsWith("The server could not be reached"))

        // The pilot takes the server away: its flights go with it.
        address = null
        community.loadIfNeeded()
        assertTrue(community.flights.isEmpty())
        assertNull(community.problem)
    }

    @Test
    fun aFlightIsOpenedFromItsFileLikeOneOfThePilotsOwn() = runBlocking {
        val server = Server()
        val community = model(server)
        community.loadIfNeeded()
        val flight = community.flights.first()

        val opened = community.open(flight)
        assertEquals("$BASE/v1/flights/${flight.id}/igc", server.asked.last())
        // The whole flight, as the flight screen shows one of the logbook's.
        assertTrue(opened.flight.track.size > 1000)
        assertEquals(2, opened.analysis?.thermals?.size)
        // It is in no logbook: it has no identifier there.
        assertEquals("", opened.flight.id)
        // The page to hand on is the server's, for this flight.
        assertEquals("/f/${flight.id}", flight.pagePath)

        // Taken back by its pilot since the list was fetched.
        server.file = HttpAnswer(404, """{"error": {"code": "not_found", "message": "x"}}""".toByteArray())
        assertEquals(
            "This flight is no longer public: its pilot took it back, or deleted it.",
            whyNot(community) { community.open(flight) },
        )
        // What a server hands out that is no flight file is not shown as one.
        server.file = HttpAnswer(200, "<html>not found</html>".toByteArray())
        val unreadable = whyNot(community) { community.open(flight) }
        assertFalse(unreadable, unreadable.startsWith("The flight could not be fetched."))
        // No network.
        server.isReachable = false
        assertTrue(whyNot(community) { community.open(flight) }.startsWith("The flight could not be fetched."))
    }

    @Test
    fun aPilotsPageListsWhatThatPilotSharesWithEveryone() = runBlocking {
        val server = Server()
        val community = model(server)
        assertNull(community.pilot)

        community.showPilot("demo")
        assertEquals(listOf("$BASE/v1/pilots/demo"), server.asked.toList())
        val pilot = checkNotNull(community.pilot)
        assertEquals("demo", pilot.handle)
        assertTrue(pilot.isLoaded)
        assertFalse(pilot.isLoading)
        assertNull(pilot.problem)
        assertNull(pilot.next)
        assertTrue(pilot.flights.isNotEmpty())
        assertTrue(pilot.flights.all { it.handle == "demo" && it.pilot == "Demo Pilot" })
        // A flight of the page can be opened, though the list of everyone's
        // flights was never looked at.
        assertEquals(pilot.flights.first(), community.flight(pilot.flights.first().id))

        // Come back to from one of its flights, the page is there: nothing is asked.
        community.showPilot("demo")
        community.morePilotFlights()
        assertEquals(1, server.asked.size)

        // A pilot the server does not know is said in the core's words.
        community.showPilot("nobody")
        assertEquals("$BASE/v1/pilots/nobody", server.asked.last())
        assertEquals("The server knows no pilot by this handle.", community.pilot?.problem)
        assertTrue(checkNotNull(community.pilot).flights.isEmpty())
        // What cannot be a handle is not asked for at all.
        val asked = server.asked.size
        community.showPilot("../accounts")
        assertEquals(asked, server.asked.size)
        assertEquals("This is not the handle of a pilot.", community.pilot?.problem)

        // No network: said, and asked again the next time the page is looked at.
        server.isReachable = false
        community.showPilot("demo")
        assertTrue(checkNotNull(community.pilot?.problem).startsWith("The server could not be reached."))
        server.isReachable = true
        community.showPilot("demo")
        assertNull(community.pilot?.problem)
        assertEquals(1, community.pilot?.flights?.size)
    }

    @Test
    fun aPilotsLaterFlightsFollowWhenTheEndOfThePageComesIntoView() = runBlocking {
        val server = Server()
        server.pilotHasTwoPages = true
        val community = model(server)

        community.showPilot("demo")
        assertEquals("m1", community.pilot?.next)
        assertEquals(1, community.pilot?.flights?.size)

        community.morePilotFlights()
        assertEquals("$BASE/v1/pilots/demo?after=m1", server.asked.last())
        val pilot = checkNotNull(community.pilot)
        assertEquals(2, pilot.flights.size)
        assertEquals(2, pilot.flights.map { it.id }.distinct().size)
        assertNull(pilot.next)
        // That was the last page.
        community.morePilotFlights()
        assertEquals(2, server.asked.size)

        // Without a server there is nothing to ask and nothing kept.
        var address: String? = BASE
        val other = model(server) { address }
        other.showPilot("demo")
        address = null
        other.loadIfNeeded()
        assertNull(other.pilot)
    }

    /** The words for what went wrong in `open`. */
    private suspend fun whyNot(community: CommunityModel, open: suspend () -> Unit): String {
        try {
            open()
        } catch (error: Exception) {
            return community.words(error)
        }
        fail("the flight was opened")
        return ""
    }

    private companion object {
        const val BASE = "https://venti.example.org"
    }
}
