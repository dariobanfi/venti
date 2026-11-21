package fly.venti.app.sites

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.account.AccountModel
import fly.venti.app.account.MemorySessionStore
import fly.venti.app.community.Reported
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.Coordinate
import fly.venti.core.ReportSubject
import fly.venti.core.SiteField
import fly.venti.core.SiteKind
import fly.venti.core.reportFormFor
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Adding and correcting a site that pilots make, against the real core and
 * a server played by the test with the answers the server's own tests keep.
 */
class SiteEditModelTest {
    /** One request as it was sent. */
    private class Sent(val method: String, val url: String, val headers: Map<String, String>) {
        val path: String get() = url.removePrefix(SERVER).substringBefore('?')
    }

    /** A server that signs anyone in and answers about sites as the test says. */
    private class Server {
        val sent: MutableList<Sent> = CopyOnWriteArrayList<Sent>()

        /** What the server says to a request about a site. */
        var sites: (Sent) -> HttpAnswer = { refusal(404, "site_not_found") }

        @Suppress("UNUSED_PARAMETER")
        suspend fun answer(
            method: String,
            url: String,
            headers: List<Pair<String, String>>,
            body: ByteArray?,
            answerWithinMilliseconds: Int,
        ): HttpAnswer {
            val request = Sent(method, url, headers.toMap())
            sent += request
            return when {
                method == "POST" && request.path == "/v1/sessions" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "GET" && request.path == "/v1/flights/changes" -> HttpAnswer(200, NO_CHANGES.toByteArray())
                else -> sites(request)
            }
        }

        /** A look at a site or its history, which needs no account. */
        suspend fun look(url: String): HttpAnswer {
            val request = Sent("GET", url, emptyMap())
            sent += request
            return sites(request)
        }
    }

    private fun model(server: Server, account: AccountModel): SiteEditModel =
        SiteEditModel(account, read = server::look).apply {
            this.server = { SERVER }
            takesSites = { true }
        }

    private suspend fun signedIn(server: Server): SiteEditModel {
        val logbook = LogbookModel.open(TestSupport.newFolder("site-edit"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        account.signIn(SERVER, "demo", "a password of the test")
        server.sent.clear()
        return model(server, account)
    }

    @Test
    fun aNewSiteGoesWithTheAccountsTokenAndComesBackInTheCoresWords() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        assertTrue(edit.isSignedIn)
        server.sites = { HttpAnswer(201, recorded("site_written.json")) }

        val entry = SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87))
        assertTrue(entry.isNew)
        entry.draft = entry.draft.copy(name = "Testkogel (not real)", uses = listOf(SiteKind.TAKEOFF))
        val sent = edit.send(entry.draft, "")

        val asked = server.sent.single()
        assertEquals("POST", asked.method)
        assertEquals("/v1/sites", asked.path)
        assertTrue(checkNotNull(asked.headers["Authorization"]).startsWith("Bearer "))
        val written = (sent as SiteEditModel.Sent.Written).written
        assertEquals("Added. The site is on the map for everyone now.", written.words)
        assertNotNull(written.site.byPilots)
    }

    @Test
    fun aDraftWithSomethingMissingIsNotSent() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        // No name and no use: the core refuses to make the request.
        val refused = edit.send(SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87)).draft, "")
        assertTrue(refused is SiteEditModel.Sent.Refused)
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun aSiteInTheWayIsNamedWithTheOfferToOpenIt() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        server.sites = { HttpAnswer(409, recorded("site_too_close.json")) }

        val entry = SiteFormDraft.new(Coordinate(lat = 47.3, lon = 10.9))
        entry.draft = entry.draft.copy(name = "Testkogel (not real)", uses = listOf(SiteKind.TAKEOFF))
        val refused = edit.send(entry.draft, "") as SiteEditModel.Sent.Refused

        assertTrue(refused.words, refused.words.startsWith("A site that pilots added stands at this place already: Musterkogel West."))
        val other = checkNotNull(refused.inTheWay)
        assertEquals("Open Musterkogel West", other.words)
        assertEquals("venti:0000000000000000000000s001", other.id)
        assertNull(refused.now)
    }

    @Test
    fun aCorrectionOfASiteThatChangedMeanwhileKeepsWhatThePilotEntered() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        // The site as it stood when the form opened, at revision 2 and 1862 m.
        server.sites = { HttpAnswer(200, recorded("site.json")) }
        val page = edit.page(ID)
        assertTrue(page.canBeCorrected)
        val entry = SiteFormDraft.of(page.site)
        assertFalse(entry.isNew)
        assertEquals("1862", entry.height)
        entry.draft = entry.draft.copy(name = "Musterkogel Westgrat")
        entry.comment = "its name on the signpost"

        // Someone else saved revision 3 meanwhile, with another height.
        server.sent.clear()
        server.sites = { request ->
            if (request.method == "PUT") refusal(409, "site_changed") else HttpAnswer(200, recorded("site_written.json"))
        }
        val refused = edit.send(entry.draft, entry.comment) as SiteEditModel.Sent.Refused
        assertEquals(listOf("PUT", "GET"), server.sent.map { it.method })
        assertTrue(refused.words, refused.words.startsWith("Someone else changed this site while you were editing it."))
        val now = checkNotNull(refused.now)

        // The form goes on from the site as it stands, with the pilot's
        // own entry in it and the other pilot's change said in words.
        entry.carryOnto(now)
        assertEquals("Musterkogel Westgrat", entry.draft.name)
        assertEquals(1871.0, entry.draft.elevationM)
        assertEquals("1871", entry.height)
        assertEquals(3u, entry.draft.revision)
        assertEquals("its name on the signpost", entry.comment)
        assertEquals(listOf("Height: 1862 m → 1871 m"), entry.meanwhile)
    }

    @Test
    fun aHistoryIsReadWithoutAnAccountAndNothingIsSentWithoutOne() = runBlocking {
        val server = Server()
        val logbook = LogbookModel.open(TestSupport.newFolder("site-edit"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        val edit = model(server, account)
        assertFalse(edit.isSignedIn)
        server.sites = { HttpAnswer(200, recorded("site_history.json")) }

        val history = edit.history("venti:0000000000000000000000s002")
        assertEquals("GET", server.sent.single().method)
        assertNull(server.sent.single().headers["Authorization"])
        assertEquals(3, history.revisions.size)
        assertTrue(history.revisions.all { it.words.isNotEmpty() })
        assertTrue(history.revisions[1].changes.isNotEmpty())
        // The newest revision is the one the site stands at.
        assertEquals(history.revisions.last().number, history.revision)

        // Signed out, a draft that could be sent is not: there is no token to send it with.
        server.sent.clear()
        val entry = SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87))
        entry.draft = entry.draft.copy(name = "Testkogel (not real)", uses = listOf(SiteKind.TAKEOFF))
        val refused = edit.send(entry.draft, "") as SiteEditModel.Sent.Refused
        assertTrue(refused.words.isNotEmpty())
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun removingAndPuttingBackGoAsRevisionsOfTheSite() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        server.sites = { request ->
            if (request.method == "DELETE") HttpAnswer(204, ByteArray(0)) else HttpAnswer(200, recorded("site_written.json"))
        }
        assertEquals(
            "Removed from the map. Its history stays, and any pilot can bring it back.",
            edit.remove(ID, "added twice"),
        )
        val written = edit.restore(ID, 2u, "it is a take-off of its own")
        assertEquals(listOf("DELETE", "POST"), server.sent.map { it.method })
        assertTrue(server.sent.all { it.path.startsWith("/v1/sites/") && it.headers["Authorization"] != null })
        assertTrue(written.words.isNotEmpty())

        // What the server refuses is said in the core's words.
        server.sites = { refusal(403, "sites_closed") }
        val problem = runCatching { edit.remove(ID, "") }.exceptionOrNull() as Exception
        assertTrue(edit.words(problem).startsWith("This server takes no sites from pilots."))
    }

    @Test
    fun aHeightIsTakenAsItIsTypedOnlyWhileItCanBeANumber() {
        val entry = SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87))
        assertEquals("", entry.height)
        entry.typeHeight("1862")
        assertEquals(1862.0, entry.draft.elevationM)
        entry.typeHeight("1862 m")
        assertEquals("1862", entry.height)
        entry.typeHeight("")
        assertNull(entry.draft.elevationM)
        assertEquals("1862.5", SiteFormDraft.heightAsTyped(1862.5))
        assertEquals("-12", SiteFormDraft.heightAsTyped(-12.0))
    }

    @Test
    fun theFormAndTheReportOfASiteAreTheCores() {
        val server = Server()
        val logbook = LogbookModel.open(TestSupport.newFolder("site-edit"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        val form = model(server, account).form
        assertEquals(SiteField.NAME, form.fields.first().field)
        assertEquals(SiteField.COMMENT, form.fields.last().field)
        assertEquals(16, form.winds.size)
        assertTrue(form.giving.contains(form.licence))
        // Every field has a name the tests find it by.
        assertEquals("no-flight-time", SiteEditWords.tag(SiteField.NO_FLIGHT_TIME))

        // A site and one revision of it go through the app's report sheet, with a site's reasons.
        val site = Reported.site(ID)
        assertEquals(ReportSubject.Site(ID), site.subject)
        assertEquals(ReportSubject.SiteRevision(ID, 2u), Reported.siteRevision(ID, 2u).subject)
        assertTrue(reportFormFor(site.subject).reasons.isNotEmpty())
    }

    private companion object {
        const val SERVER = "https://venti.example.org"

        /** The development mode's "Musterkogel West". */
        const val ID = "venti:0000000000000000000000s001"

        const val NO_CHANGES = """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"""

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

        fun refusal(status: Int, code: String) =
            HttpAnswer(status, """{"error": {"code": "$code", "message": "x"}}""".toByteArray())
    }
}
