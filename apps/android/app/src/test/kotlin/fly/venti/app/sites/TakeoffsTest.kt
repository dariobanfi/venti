package fly.venti.app.sites

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.account.AccountModel
import fly.venti.app.account.MemorySessionStore
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.SiteKind
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * What public flights say about where pilots take off, on the phone: a
 * site's flights, the places no site names, and the offer to add one,
 * against the real core and a server played by the test with the answers
 * the server's own tests keep.
 */
class TakeoffsTest {
    /** A server that signs anyone in, as an ordinary pilot unless a test says otherwise, and answers the looks. */
    private class Server {
        val looked: MutableList<String> = CopyOnWriteArrayList<String>()

        @Volatile
        var account: ByteArray = ORDINARY.toByteArray()

        @Volatile
        var candidates: ByteArray = recorded("site_candidates.json")

        @Suppress("UNUSED_PARAMETER")
        suspend fun answer(
            method: String,
            url: String,
            headers: List<Pair<String, String>>,
            body: ByteArray?,
            answerWithinMilliseconds: Int,
        ): HttpAnswer {
            val path = url.removePrefix(SERVER).substringBefore('?')
            return when {
                method == "POST" && path == "/v1/sessions" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "GET" && path == "/v1/flights/changes" -> HttpAnswer(200, NO_CHANGES.toByteArray())
                method == "GET" && path == "/v1/account" -> HttpAnswer(200, account)
                else -> HttpAnswer(404, "{}".toByteArray())
            }
        }

        suspend fun look(url: String): HttpAnswer {
            looked += url
            val path = url.removePrefix(SERVER).substringBefore('?')
            return when {
                path == "/v1/sites/candidates" -> HttpAnswer(200, candidates)
                path == "/v1/sites/$ID/flights" -> HttpAnswer(200, recorded("site_flights.json"))
                else -> HttpAnswer(404, """{"error": {"code": "site_not_found", "message": "x"}}""".toByteArray())
            }
        }
    }

    private class Phone(val logbook: LogbookModel, val account: AccountModel, val edit: SiteEditModel)

    private fun phone(server: Server): Phone {
        val logbook = LogbookModel.open(TestSupport.newFolder("takeoffs"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        val edit = logbook.makeSiteEdit(account, read = server::look).apply {
            this.server = { SERVER }
            takesSites = { true }
        }
        return Phone(logbook, account, edit)
    }

    /** Signed in, with the account as the server says it now. */
    private suspend fun Phone.signIn() {
        account.signIn(SERVER, "demo", "a password of the test")
        account.refresh()
    }

    @Test
    fun aTakeoffsFlightsComeInTheCoresWordsWithTheTwelveMonthsAndTheBest() = runBlocking {
        val server = Server()
        val phone = phone(server)
        val flights = phone.edit.flights(ID)

        assertEquals("$SERVER/v1/sites/$ID/flights", server.looked.single())
        assertTrue(flights.isTakeoff)
        assertEquals(3u, flights.flights)
        assertTrue(flights.words, flights.words.startsWith("3 public flights by 2 pilots began here"))
        assertEquals(12, flights.months.size)
        assertEquals("Jan", flights.months.first().name)
        // The busiest month is the tallest bar; each month says itself for a reader of the screen.
        assertEquals(1.0, flights.months.maxOf { it.share }, 0.0)
        assertTrue(flights.months.all { it.words.isNotEmpty() })
        assertEquals(3, flights.best.size)
        assertTrue(flights.best.first().words.startsWith("Lena Probe"))
        assertTrue(flights.terms.text.isNotEmpty())
        // The page's words are the core's.
        assertEquals("Public flights from here", phone.edit.flightsWords.title)
    }

    @Test
    fun thePlacesNoSiteNamesAreAskedForThePartOfTheMapAndAPartTooLargeIsNotAsked() = runBlocking {
        val server = Server()
        val phone = phone(server)

        val read = phone.edit.candidates(Bounds(south = 47.25, west = 10.92, north = 47.27, east = 10.95))
        assertTrue(server.looked.single(), server.looked.single().startsWith("$SERVER/v1/sites/candidates?bbox="))
        val place = read.candidates.single()
        assertEquals(47.2612, place.position.lat, 1e-9)
        assertEquals("1620 m", place.altitudeWords)
        assertTrue(place.words, place.words.startsWith("Pilots take off here"))

        server.looked.clear()
        try {
            phone.edit.candidates(Bounds(south = 46.0, west = 10.0, north = 48.5, east = 12.0))
            fail("a part of the map over two degrees is refused")
        } catch (refused: CoreException) {
            assertTrue(server.looked.isEmpty())
        }
    }

    @Test
    fun theLayerAsksNothingWhileOffAndSaysTheCoresWordsOverTheMap() = runBlocking {
        val server = Server()
        val phone = phone(server)
        val layer = CandidatesModel(CoroutineScope(Dispatchers.Unconfined), phone.edit, restMilliseconds = 0)
        val small = Bounds(south = 47.25, west = 10.92, north = 47.27, east = 10.95)

        layer.show(small)
        assertTrue(server.looked.isEmpty())
        assertTrue(layer.candidates.isEmpty())

        // Switched on, the part last shown is asked for.
        layer.switch(true)
        layer.load(small)
        assertEquals(1, layer.candidates.size)
        assertNull(layer.notice)

        // Zoomed far out: no places, and the core's words to zoom in.
        layer.load(Bounds(south = 46.0, west = 10.0, north = 48.5, east = 12.0))
        assertTrue(layer.candidates.isEmpty())
        assertEquals("Zoom in to see the places where pilots take off and no site stands.", layer.notice)

        // Off again: nothing is drawn and nothing said.
        layer.switch(false)
        assertTrue(layer.candidates.isEmpty())
        assertNull(layer.notice)
    }

    @Test
    fun aPilotWhoTookOffAtAPlaceIsOfferedItUntilTheySayNotThisOne() = runBlocking {
        val server = Server()
        val phone = phone(server)
        // A place at the sample flight's take-off, made from the server's own answer.
        server.candidates = String(recorded("site_candidates.json"))
            .replace("\"lat\": 47.2612", "\"lat\": 47.6642")
            .replace("\"lon\": 10.9312", "\"lon\": 11.7967")
            .toByteArray()
        phone.logbook.addSample()
        val area = checkNotNull(CandidateArea.around(listOf(Coordinate(lat = 47.6642, lon = 11.7967))))

        // Signed out, the core offers nothing.
        assertTrue(phone.edit.candidateOffers(phone.edit.candidates(area)).isEmpty())

        phone.signIn()
        val offer = phone.edit.candidateOffers(phone.edit.candidates(area)).single()
        assertTrue(offer.words, offer.words.startsWith("You took off here on"))
        assertTrue(offer.words, offer.words.endsWith("Add it as a place pilots take off from?"))
        assertEquals("Add this place" to "Not this one", offer.addWords to offer.dismissWords)
        // The form opens on the core's draft: at the pilot's own take-off, as a take-off, in the words of the pilots' tier.
        val adding = checkNotNull(phone.edit.adding())
        val draft = SiteFormDraft.offered(offer, adding)
        assertTrue(draft.isNew)
        assertEquals(listOf(SiteKind.TAKEOFF), draft.draft.uses)
        assertNotNull(draft.draft.elevationM)
        assertEquals(adding.title, draft.words.title)

        // "Not this one": not offered again on this phone.
        phone.edit.setAside(offer.candidate.position)
        assertTrue(phone.edit.candidateOffers(phone.edit.candidates(area)).isEmpty())
    }

    @Test
    fun aPartnerIsOfferedNothingOnThePhone() = runBlocking {
        val server = Server()
        server.account = recorded("account.json")
        val phone = phone(server)
        server.candidates = String(recorded("site_candidates.json"))
            .replace("\"lat\": 47.2612", "\"lat\": 47.6642")
            .replace("\"lon\": 10.9312", "\"lon\": 11.7967")
            .toByteArray()
        phone.logbook.addSample()
        phone.signIn()
        assertTrue(checkNotNull(phone.edit.adding()).asPartner)
        val area = checkNotNull(CandidateArea.around(listOf(Coordinate(lat = 47.6642, lon = 11.7967))))
        assertTrue(phone.edit.candidateOffers(phone.edit.candidates(area)).isEmpty())
    }

    @Test
    fun todaysPartOfTheMapIsTheBoxRoundItsTakeoffs() {
        assertNull(CandidateArea.around(emptyList()))
        val one = checkNotNull(CandidateArea.around(listOf(Coordinate(lat = 47.0, lon = 11.0))))
        assertTrue(one.north > one.south && one.east > one.west)
        val two = checkNotNull(
            CandidateArea.around(listOf(Coordinate(lat = 47.0, lon = 11.0), Coordinate(lat = 47.2, lon = 11.5))),
        )
        assertEquals(47.0 - CandidateArea.MARGIN_DEG, two.south, 1e-9)
        assertEquals(11.5 + CandidateArea.MARGIN_DEG, two.east, 1e-9)
        assertFalse(two.north < 47.2)
    }

    companion object {
        const val SERVER = "https://venti.example.org"

        /** The development mode's "Musterkogel West". */
        const val ID = "venti:0000000000000000000000s001"

        const val NO_CHANGES = """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"""

        const val ORDINARY = """{"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z"}}"""

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()
    }
}
