package fly.venti.app.community

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.design.Format
import fly.venti.core.pilotsWords
import fly.venti.core.Bounds
import fly.venti.core.PublicFlightsOrder
import fly.venti.core.GliderClassKind
import fly.venti.core.RouteKind
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import java.io.IOException
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
        val asked: MutableList<String> = CopyOnWriteArrayList<String>()

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
                "/v1/ranking" in url -> HttpAnswer(200, recorded("ranking.json"))
                "/v1/pilots/near?" in url -> HttpAnswer(200, recorded("pilots_near.json"))
                // The search by name: the one pilot the recording found, or nobody.
                "/v1/pilots?q=" in url ->
                    if (url.endsWith("q=zz")) HttpAnswer(200, """{"pilots": []}""".toByteArray()) else HttpAnswer(200, recorded("pilots.json"))
                url.endsWith("/v1/pilots/lena/statistics") -> HttpAnswer(200, recorded("pilot_statistics.json"))
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

    private fun model(server: Server, searchPause: Long = 0, address: () -> String? = { BASE }) =
        CommunityModel(transport = server::answer, searchPauseMilliseconds = searchPause).also { it.server = address }

    @Test
    fun pilotsAreFoundByNameOnceTheTypingHasPaused() = runBlocking {
        val server = Server()
        val community = model(server)

        // One letter is not asked for, and the core says why.
        community.findPilots("l")
        assertTrue(server.asked.isEmpty())
        assertEquals("Type at least two letters of a pilot's name or handle.", community.search.words)
        assertTrue(community.search.pilots.isEmpty())

        community.findPilots("le")
        assertEquals(listOf("$BASE/v1/pilots?q=le"), server.asked.toList())
        val found = community.search
        assertEquals("le", found.text)
        assertFalse(found.isLoading)
        assertEquals("1 pilot", found.words)
        assertEquals(listOf("Lena Probe (@lena), 3 public flights"), found.pilots.map { it.words })
        assertEquals("lena" to "Lena Probe", found.pilots.single().let { it.handle to it.name })

        // Nobody by a name: the core's line, and no pilots.
        community.findPilots("zz")
        assertTrue(community.search.pilots.isEmpty())
        assertEquals("No pilot by this name has shared a flight with everyone.", community.search.words)

        // A server that cannot be reached is said in words.
        server.isReachable = false
        community.findPilots("len")
        assertEquals(pilotsWords().unreachable, community.search.words)
        server.isReachable = true

        // The field emptied: no search, and nothing asked.
        val asked = server.asked.size
        community.findPilots(" ")
        assertEquals(CommunityModel.PilotSearch(" "), community.search)
        assertEquals(asked, server.asked.size)

        // A letter typed before the pause is over takes the question back:
        // the server is asked once, for what stands in the field.
        val slow = model(server, searchPause = 60_000)
        val typing = launch { slow.findPilots("le") }
        yield()
        assertTrue(slow.search.isLoading)
        typing.cancelAndJoin()
        assertEquals(asked, server.asked.size)
    }

    @Test
    fun thePilotsNearAPlaceAreAskedOnceForThePlace() = runBlocking {
        val server = Server()
        val community = model(server)
        val place = fly.venti.core.Coordinate(lat = 47.3, lon = 10.9)
        community.showNear(place, fromPhone = true)
        assertEquals(listOf("$BASE/v1/pilots/near?lat=47.3&lon=10.9"), server.asked.toList())
        val near = checkNotNull(community.near.pilots)
        assertTrue(community.near.fromPhone)
        assertEquals("lena", near.pilots.first().handle)
        assertEquals("LP", near.pilots.first().initials)
        assertTrue(near.words.startsWith("Within 25 km"))
        // What the core said of each pilot's picture is known for the lines that name the pilot without it.
        for (pilot in near.pilots) assertEquals(pilot.picturePath, PilotPictures.of(pilot.handle))

        // The same place is not asked again, unless the pilot asks.
        community.showNear(place, fromPhone = true)
        assertEquals(1, server.asked.size)
        community.showNear(place, fromPhone = true, again = true)
        assertEquals(2, server.asked.size)

        // A server that cannot be reached is said in words.
        server.isReachable = false
        community.showNear(fly.venti.core.Coordinate(lat = 47.0, lon = 11.0), fromPhone = false)
        assertEquals(pilotsWords().unreachable, community.near.problem)
        assertNull(community.near.pilots)
    }

    @Test
    fun aFacesInitialsAreThoseTheCoreMakes() {
        assertEquals("LP", PilotPictures.initials("Lena  Probe"))
        assertEquals("M", PilotPictures.initials("Mara"))
        assertEquals("JB", PilotPictures.initials("Jonas Beispiel Dritter"))
        assertEquals("", PilotPictures.initials("  "))
    }

    @Test
    fun theRankingOfAMonthIsAskedByTheServersNameForIt() = runBlocking {
        val server = Server()
        val community = model(server)
        // 20 November 2026.
        val now = 1_795_132_800L
        community.rank(nowUnixS = now)
        // The heading is the period the answer names (`Ranking.period.title`), and the
        // recorded answer is of August 2026 whatever is asked for.
        val RECORDED_PERIOD = "August 2026"

        // The years are those the server says have a ranking, then all
        // years; under the year shown are its months with flights, the
        // newest first, in the core's names.
        assertEquals(listOf("2026", "2025", "All years"), community.ranked.seasons.map { it.name })
        assertEquals(RECORDED_PERIOD, community.ranked.title)
        assertNull(community.ranked.month)
        assertEquals(
            listOf("September", "August", "July", "June", "May", "April"),
            community.ranked.months.map { it.name },
        )
        val may = community.ranked.months.first { it.month == "2026-05" }
        assertEquals("May 2026: 1 flight", may.words)

        community.rank("2026", may.month, nowUnixS = now)
        assertEquals("$BASE/v1/ranking?month=2026-05", server.asked.last())
        assertEquals("2026-05", community.ranked.month)
        assertEquals(RECORDED_PERIOD, community.ranked.title)
        assertEquals("2026", community.ranked.season)
        assertTrue(community.ranked.pilots.isNotEmpty())
        assertFalse(community.ranked.isLoading)

        // Asked again as it stands, it is the month again.
        community.rank(nowUnixS = now)
        assertEquals(2, server.asked.count { it.endsWith("month=2026-05") })
        assertEquals(RECORDED_PERIOD, community.ranked.title)

        // The season alone is the whole year again; another year has its own months.
        community.rank("2026", nowUnixS = now)
        assertNull(community.ranked.month)
        assertEquals(RECORDED_PERIOD, community.ranked.title)
        assertTrue(server.asked.last(), server.asked.last().startsWith("$BASE/v1/ranking?from="))
        community.rank("2025", nowUnixS = now)
        assertEquals(listOf("July"), community.ranked.months.map { it.name })
        assertEquals(RECORDED_PERIOD, community.ranked.title)

        // All years have no months to choose from.
        community.rank("All years", nowUnixS = now)
        assertEquals("$BASE/v1/ranking", server.asked.last())
        assertTrue(community.ranked.months.isEmpty())
        assertEquals(RECORDED_PERIOD, community.ranked.title)

        // A month the server cannot be asked for is said in the core's words.
        community.rank("2026", "May", nowUnixS = now)
        assertEquals("The app could not ask the server for this ranking.", community.ranked.problem)
        assertTrue(community.ranked.pilots.isEmpty())
    }

    @Test
    fun aPilotsTotalsAreTheCoresRowsSitesAndMonths() = runBlocking {
        val server = Server()
        var address: String? = BASE
        val community = model(server) { address }
        assertNull(community.pilotTotals)

        community.showPilotTotals("lena")
        assertEquals(listOf("$BASE/v1/pilots/lena/statistics"), server.asked.toList())
        val totals = checkNotNull(community.pilotTotals)
        assertFalse(totals.isLoading)
        assertNull(totals.problem)
        val statistics = checkNotNull(totals.statistics)
        assertEquals("Lena Probe", statistics.name)
        assertEquals("All years", statistics.overall.name)
        assertEquals(
            listOf("Flights" to "3", "Distance scored" to "28.2 km", "Points, best 3 flights" to "28.19"),
            statistics.overall.rows.take(3).map { it.label to it.value },
        )
        // The longest flight is named, for its row to open it.
        assertTrue(statistics.overall.rows.first { it.label == "Longest flight" }.flightId.isNotEmpty())
        assertEquals(listOf("Musterkogel West, 2 flights"), statistics.overall.sites.map { it.words })
        assertEquals(12, statistics.overall.months.size)
        assertEquals(listOf("2026" to 2026), statistics.seasons.map { it.name to it.year })
        assertTrue(statistics.note.isNotEmpty())
        assertTrue(statistics.terms.text.isNotEmpty())

        // They are there: looked at again, nothing is asked.
        community.showPilotTotals("lena")
        assertEquals(1, server.asked.size)

        // A pilot the server does not know is said in the core's words.
        community.showPilotTotals("nobody")
        assertEquals("The server knows no pilot by this handle.", community.pilotTotals?.problem)
        assertNull(community.pilotTotals?.statistics)

        // No route to the server.
        server.isReachable = false
        community.showPilotTotals("lena")
        assertEquals(pilotsWords().unreachable, community.pilotTotals?.problem)

        // Without a server nothing is asked.
        address = null
        val asked = server.asked.size
        community.showPilotTotals("lena")
        assertEquals("Venti is not connected.", community.pilotTotals?.problem)
        assertEquals(asked, server.asked.size)
    }

    @Test
    fun theRankingIsOfASeasonAndItsFlightsOpenLikeAnyPublicFlight() = runBlocking {
        val server = Server()
        val community = model(server)
        // 20 November 2026.
        val now = 1_795_132_800L
        community.rank(nowUnixS = now)

        val ranked = community.ranked
        // The seasons are the core's, the newest first; the first is shown.
        assertTrue(ranked.seasons.size > 1)
        assertEquals(ranked.seasons.first().name, ranked.season)
        assertTrue(server.asked.single(), server.asked.single().startsWith("$BASE/v1/ranking?from="))
        assertFalse(ranked.isLoading)
        assertNull(ranked.problem)
        assertEquals("Mara Muster", ranked.pilots.first().name)
        assertEquals(1u, ranked.pilots.first().place)
        assertTrue(ranked.rule.isNotEmpty())
        val flight = ranked.pilots.first().flights.first()
        assertEquals(flight, community.flight(flight.id))

        // Another season: asked for by its own span; all years by none.
        val always = ranked.seasons.last()
        community.rank(always.name, nowUnixS = now)
        assertEquals(always.name, community.ranked.season)
        assertEquals("$BASE/v1/ranking", server.asked.last())

        // A server that cannot be reached is said in words.
        server.isReachable = false
        community.rank(nowUnixS = now)
        assertEquals(pilotsWords().unreachable, community.ranked.problem)
    }

    @Test
    fun withoutVentiThereIsNoRanking() = runBlocking {
        val server = Server()
        val community = model(server, address = { null })
        community.rank()
        assertTrue(community.ranked.seasons.isEmpty())
        assertTrue(server.asked.isEmpty())
    }

    @Test
    fun whatWasFlownAroundAFlightIsAskedForByPlaceAndTime() = runBlocking {
        val server = Server()
        val community = model(server)
        val around = community.flightsAround(
            Bounds(south = 47.6, west = 11.7, north = 47.7, east = 11.9),
            fromUnixS = 1_783_000_000,
            toUnixS = 1_783_086_400,
        )
        assertEquals(2, around.size)
        assertTrue(server.asked.last().contains("from="))
        // Each opens like a flight of the lists.
        assertEquals(around.first(), community.flight(around.first().id))
        // Nothing of it is in the list of everyone's flights.
        assertTrue(community.flights.isEmpty())
    }

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
        assertEquals("${Format.day(row.startUnixS)}, Sample Wing, 12.0 km, 0:29 h in the air", CommunityWords.spoken(row))
        // The take-off's country as a flag before its name, the pilot's said first, the class before the glider.
        val said = row.copy(takeoffName = "Hafelekar", takeoffCountry = "AT", pilotCountry = "DE", gliderClass = GliderClassKind.B)
        assertEquals("${Format.day(row.startUnixS)} · \uD83C\uDDE6\uD83C\uDDF9 Hafelekar · EN B · Sample Wing", CommunityWords.detail(said))
        assertEquals(
            "Germany, ${Format.day(row.startUnixS)}, Hafelekar, Austria, EN B · Sample Wing, 12.0 km, 0:29 h in the air",
            CommunityWords.spoken(said),
        )
        assertEquals("\uD83C\uDDE6\uD83C\uDDF9", CommunityWords.place(said.copy(takeoffName = null)))
        assertEquals("EN B", CommunityWords.glider(said.copy(glider = null)))
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
        assertTrue(community.problem, checkNotNull(community.problem).startsWith("Venti could not be reached for other pilots' flights."))
        assertTrue(community.flights.isEmpty())
        assertFalse(community.isLoading)

        // A server that refuses says why in the core's words.
        server.isReachable = true
        server.refusal = HttpAnswer(503, ByteArray(0))
        community.loadIfNeeded()
        assertTrue(checkNotNull(community.problem).isNotEmpty())
        assertFalse(checkNotNull(community.problem).startsWith("Venti could not be reached"))

        // Asked again once it answers, the list is there and nothing is said.
        server.refusal = null
        community.loadIfNeeded()
        assertEquals(2, community.flights.size)
        assertNull(community.problem)

        // What is listed stays when a later page does not come.
        server.isReachable = false
        community.loadMore()
        assertEquals(2, community.flights.size)
        assertTrue(checkNotNull(community.problem).startsWith("Venti could not be reached"))

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
        assertEquals(pilotsWords().unreachable, community.pilot?.problem)
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
