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
import java.time.LocalDate
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

        /** What the server says of the account: the development mode's demo pilot, a partner, until a test says otherwise. */
        @Volatile
        var account: ByteArray = recorded("account.json")

        @Volatile
        var accountStatus = 200

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
                method == "GET" && request.path == "/v1/account" -> HttpAnswer(accountStatus, account)
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

    private fun model(server: Server, logbook: LogbookModel, account: AccountModel): SiteEditModel =
        logbook.makeSiteEdit(account, read = server::look).apply {
            this.server = { SERVER }
            takesSites = { true }
        }

    /** A phone that knows no account, with its model. */
    private fun signedOut(server: Server): Pair<AccountModel, SiteEditModel> {
        val logbook = LogbookModel.open(TestSupport.newFolder("site-edit"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        return account to model(server, logbook, account)
    }

    private suspend fun signedIn(server: Server): SiteEditModel = signedInWithItsAccount(server).second

    /** Signed in as the development mode's demo pilot, whose sign-in says that it is a partner. */
    private suspend fun signedInWithItsAccount(server: Server): Pair<AccountModel, SiteEditModel> {
        val (account, edit) = signedOut(server)
        account.signIn(SERVER, "demo", "a password of the test")
        server.sent.clear()
        return account to edit
    }

    @Test
    fun aNewSiteGoesWithTheAccountsTokenAndComesBackInTheCoresWords() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        assertTrue(edit.hasSession)
        server.sites = { HttpAnswer(201, recorded("site_written.json")) }

        // The demo pilot is a partner: the form's words are the partners' tier's, as the core said them with the entry.
        val adding = checkNotNull(edit.adding())
        val entry = SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87), adding)
        assertTrue(entry.isNew)
        assertEquals(adding.title to adding.sendWords, entry.words.title to entry.words.send)
        assertEquals(edit.form.givingAsPartner, entry.words.giving)
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
        val refused = edit.send(SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87), checkNotNull(edit.adding())).draft, "")
        assertTrue(refused is SiteEditModel.Sent.Refused)
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun aSiteInTheWayIsNamedWithTheOfferToOpenIt() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        server.sites = { HttpAnswer(409, recorded("site_too_close.json")) }

        val entry = SiteFormDraft.new(Coordinate(lat = 47.3, lon = 10.9), checkNotNull(edit.adding()))
        entry.draft = entry.draft.copy(name = "Testkogel (not real)", uses = listOf(SiteKind.TAKEOFF))
        val refused = edit.send(entry.draft, "") as SiteEditModel.Sent.Refused

        // The site in the way is a partner's, and the core says so and names it.
        assertTrue(refused.words, refused.words.contains("stands at this place already: Musterkogel West."))
        assertTrue(refused.words, refused.words.contains("partner"))
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
        // The form's words are the core's for the tier of the site corrected: a partner's.
        assertEquals("Correct this site" to "Save for everyone", entry.words.title to entry.words.send)
        assertEquals(edit.form.givingAsPartner to page.site.byPilots?.note, entry.words.giving to entry.words.note)
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

        // The form goes on from the site as it stands: the core carries
        // the pilot's own entry onto it and says the other pilot's change.
        assertTrue(entry.rebase(now))
        assertEquals("Musterkogel Westgrat", entry.draft.name)
        assertEquals(1871.0, entry.draft.elevationM)
        assertEquals("1871", entry.height)
        assertEquals(3u, entry.draft.revision)
        assertEquals("its name on the signpost", entry.comment)
        assertEquals(listOf("Height: 1862 m → 1871 m"), entry.meanwhile)
        // Nothing both changed: no sentence stands at a field.
        assertTrue(entry.clashes.isEmpty())
        assertEquals(
            "Someone else changed this site while you were editing it. Your entries are kept, and what you left " +
                "alone is as the site stands now.",
            entry.meanwhileWords,
        )
        assertEquals(now, entry.site)
    }

    @Test
    fun aHeightThePilotTypedStaysWhenBothChangedItAndTheCoreNamesTheField() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        server.sites = { HttpAnswer(200, recorded("site.json")) }
        val entry = SiteFormDraft.of(edit.page(ID).site)
        entry.typeHeight("1865")
        // The site stands at 1871 m meanwhile.
        server.sites = { HttpAnswer(200, recorded("site_written.json")) }
        assertTrue(entry.rebase(edit.page(ID).site))
        assertEquals("1865", entry.height)
        assertEquals(1865.0, entry.draft.elevationM)
        val words = checkNotNull(entry.meanwhileWords)
        assertTrue(words, words.contains("You both changed: Height."))
        // And the core's sentence for the field, which the form shows where a fault's stands.
        val clash = entry.clashes.single()
        assertEquals(SiteField.HEIGHT, clash.field)
        assertTrue(clash.words, clash.words.startsWith("Height: you entered 1865 m, it now stands at 1871 m."))
        // A refusal for another reason carries nothing: the form stays as it is.
        server.sites = { refusal(403, "sites_closed") }
        val refused = edit.send(entry.draft, "") as SiteEditModel.Sent.Refused
        assertNull(refused.now)
    }

    @Test
    fun whatAPilotMayDoWithASiteIsTheCoresAnswerForTheAccount() = runBlocking {
        val server = Server()
        val (account, edit) = signedInWithItsAccount(server)
        server.sites = { request ->
            HttpAnswer(200, recorded(if (request.path.endsWith("/confirmations")) "site_confirmations.json" else "site.json"))
        }
        val partners = edit.page(ID).site
        assertEquals("From a partner", partners.byPilots?.label)
        val listed = edit.confirmations(partners)
        assertEquals("2 pilots took off here, last on 16 November 2026", listed.words)
        // The list holds them all: nothing is said under it.
        assertEquals("", listed.notWholeWords)
        assertTrue(listed.lines.first().words, listed.lines.first().words.startsWith("Mara Muster (@mara) on 16 Nov 2026"))
        assertTrue(server.sent.all { it.headers["Authorization"] == null })

        // The demo pilot is a partner: everything is offered on a partner's
        // site, each with the core's words; the phone's entry for adding is
        // the partners', which the map does not show.
        val asPartner = checkNotNull(edit.offers(partners, listed))
        assertEquals(
            listOf("Correct this site", "Remove this site", "I took off here and it was good", "Report this site", "History"),
            listOf(asPartner.correct, asPartner.remove, asPartner.confirm, asPartner.report, asPartner.history).map { it.words },
        )
        assertFalse(asPartner.hasConfirmed || asPartner.withdraw.isOffered)
        assertEquals("", asPartner.note)
        assertTrue(checkNotNull(edit.adding()).let { it.mayAdd && it.asPartner })

        // The owner takes the trust back: the next read of the account says
        // so, and the same site offers an ordinary pilot no correcting.
        server.account = """{"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z"}}"""
            .toByteArray()
        server.sent.clear()
        account.refresh()
        val read = server.sent.single()
        assertEquals("GET" to "/v1/account", read.method to read.path)
        assertTrue(checkNotNull(read.headers["Authorization"]).startsWith("Bearer "))
        assertEquals(false, account.account?.mayEditPartnerSites)
        val asPilot = checkNotNull(edit.offers(partners, listed))
        assertFalse(asPilot.correct.isOffered || asPilot.remove.isOffered)
        assertTrue(asPilot.confirm.isOffered && asPilot.report.isOffered && asPilot.history.isOffered)
        assertEquals(
            "A partner of Venti added this site, and only Venti's partners change it. If something about it is wrong, report it.",
            asPilot.note,
        )
        val adding = checkNotNull(edit.adding())
        assertTrue(adding.mayAdd && !adding.asPartner)
        assertEquals("Add a place pilots take off from", adding.entryWords)
        assertEquals("A place pilots take off from", adding.title)
        assertEquals("Add as a place from pilots", adding.sendWords)
        // Where the server takes nothing from pilots, an ordinary pilot adds nothing, and the core says why.
        edit.takesSites = { false }
        val closed = checkNotNull(edit.adding())
        assertEquals("This server takes no places from pilots yet.", closed.whyNot)
        assertEquals("", closed.entryWords)
        assertFalse(checkNotNull(edit.offers(partners, listed)).confirm.isOffered)
        edit.takesSites = { true }

        // A site of a source is its source's record: nothing is offered, and the core says so.
        server.sites = { HttpAnswer(200, recorded("site_of_a_source.json")) }
        val sources = checkNotNull(edit.offers(edit.page("osm-free-flying:way/1080986173").site))
        assertFalse(listOf(sources.correct, sources.remove, sources.confirm, sources.report, sources.history).any { it.isOffered })
        assertTrue(sources.note, sources.note.startsWith("This site is its source's record"))

        // The server no longer knows the session: that is no error, and the page says to sign in again.
        server.accountStatus = 401
        account.refresh()
        assertTrue(account.needsSignIn)
        assertFalse(edit.hasSession)
        val ended = checkNotNull(edit.offers(partners, listed))
        assertEquals(edit.says.signedInAgain, ended.note)
        assertFalse(ended.report.isOffered || ended.confirm.isOffered)
    }

    @Test
    fun signedOutASitesPageSaysWhatSigningInIsForAndOffersItsHistory() = runBlocking {
        val server = Server()
        val (account, edit) = signedOut(server)
        assertFalse(edit.hasSession)
        server.sites = { HttpAnswer(200, recorded("site.json")) }
        val offers = checkNotNull(edit.offers(edit.page(ID).site))
        assertEquals("Sign in to correct this site, to say that you took off here and to report it.", offers.note)
        assertEquals(edit.says.signedOut, offers.note)
        assertTrue(offers.history.isOffered)
        assertFalse(listOf(offers.correct, offers.remove, offers.confirm, offers.withdraw, offers.report).any { it.isOffered })
        // The entry for adding is the core's sentence, and it leads to the account.
        val adding = checkNotNull(edit.adding())
        assertFalse(adding.mayAdd)
        assertTrue(adding.leadsToSignIn)
        assertEquals("Sign in to add a place pilots take off from.", adding.entryWords)
        // Signed out there is no account to read, and nothing is asked.
        server.sent.clear()
        account.refresh()
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun aPilotSaysTheyTookOffHereAndTakesItBack() = runBlocking {
        val server = Server()
        val edit = signedIn(server)
        val today = LocalDate.of(2026, 11, 18)
        val form = edit.confirmForm(today)
        assertEquals("I took off here and it was good", form.title)
        assertEquals(listOf("Today", "Yesterday", "16 Nov 2026"), form.days.take(3).map { it.words })
        assertEquals("2026-11-17", form.days[1].day)
        assertEquals("12 / 200", SiteEditWords.room(12, form.noteAtMost))

        server.sites = { request ->
            when (request.method) {
                "PUT" -> HttpAnswer(200, recorded("site.json"))
                "DELETE" -> HttpAnswer(204, ByteArray(0))
                else -> HttpAnswer(200, recorded("site_confirmations.json"))
            }
        }
        val written = edit.confirm(ID, form.days[1].day, "West wind, easy launch.", flight = null, today = today)
        val sent = server.sent.single()
        assertEquals("PUT" to "/v1/sites/$ID/confirmation", sent.method to sent.path)
        assertTrue(checkNotNull(sent.headers["Authorization"]).startsWith("Bearer "))
        assertEquals("You confirmed this site for 17 Nov 2026.", written.words)
        assertEquals(ID, written.site.id)
        // No flight of the logbook began there: the form shows no choice of one.
        assertTrue(edit.confirmFlights(written.site, form.days[1].day).isEmpty())

        // The list holds another pilot's and not this one's: the core offers no taking back.
        val listed = edit.confirmations(written.site)
        assertFalse(checkNotNull(edit.offers(written.site, listed)).hasConfirmed)
        // With this pilot's own in it, the core says so and offers both ways on.
        val mine = listed.copy(lines = listed.lines.map { it.copy(by = it.by.copy(handle = "demo")) }.take(1))
        val offers = checkNotNull(edit.offers(written.site, mine))
        assertTrue(offers.hasConfirmed)
        assertEquals("You confirmed this site for 16 Nov 2026.", offers.confirmedWords)
        assertEquals("Change the day" to "Take my confirmation back", offers.confirm.words to offers.withdraw.words)

        server.sent.clear()
        assertEquals("Your confirmation of this site is taken back.", edit.withdraw(ID))
        assertEquals("DELETE" to "/v1/sites/$ID/confirmation", server.sent.single().let { it.method to it.path })

        // A day the server does not take, and a day the core does not send.
        server.sites = { refusal(400, "invalid_confirmation") }
        val refused = runCatching { edit.confirm(ID, form.days[0].day, "", null, today) }.exceptionOrNull() as Exception
        assertTrue(edit.words(refused).startsWith("The server does not take this confirmation."))
        server.sent.clear()
        val tooEarly = runCatching { edit.confirm(ID, "2026-01-01", "", null, today) }.exceptionOrNull() as Exception
        assertTrue(edit.words(tooEarly), edit.words(tooEarly).startsWith("Choose today or one of the"))
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun aHistoryIsReadWithoutAnAccountAndNothingIsSentWithoutOne() = runBlocking {
        val server = Server()
        val (_, edit) = signedOut(server)
        assertFalse(edit.hasSession)
        server.sites = { HttpAnswer(200, recorded("site_history.json")) }

        val history = edit.history("venti:0000000000000000000000s002")
        assertEquals("GET", server.sent.single().method)
        assertNull(server.sent.single().headers["Authorization"])
        assertEquals(3, history.revisions.size)
        assertTrue(history.revisions.all { it.words.isNotEmpty() })
        assertTrue(history.revisions[1].changes.isNotEmpty())
        // The ways back, in the core's words: the site stands, so nothing brings it back.
        assertEquals(listOf("Put back to this revision", "Put back to this revision", ""), history.revisions.map { it.putBackWords })
        assertNull(history.bringBackRevision)
        // The newest revision is the one the site stands at.
        assertEquals(history.revisions.last().number, history.revision)
        // What the history offers is the core's answer for it, read from the
        // history alone: signed out nothing is put back or reported, and it says why.
        val offers = checkNotNull(edit.historyOffers(history))
        assertFalse(listOf(offers.putBack, offers.undo, offers.bringBack, offers.reportRevision).any { it.isOffered })
        assertTrue(offers.note, offers.note.startsWith("Sign in to put a revision back"))

        // Signed out, a draft that could be sent is not: there is no token to send it with.
        server.sent.clear()
        val entry = SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87), checkNotNull(edit.adding()))
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
            "Removed from the map. Its history stays, and any pilot who may correct the site can bring it back.",
            edit.remove(ID, "added twice"),
        )
        val written = edit.restore(ID, 2u, "it is a take-off of its own")
        assertEquals(listOf("DELETE", "POST"), server.sent.map { it.method })
        assertTrue(server.sent.all { it.path.startsWith("/v1/sites/") && it.headers["Authorization"] != null })
        assertTrue(written.words.isNotEmpty())

        // What the server refuses is said in the core's words.
        server.sites = { refusal(403, "sites_closed") }
        val problem = runCatching { edit.remove(ID, "") }.exceptionOrNull() as Exception
        assertTrue(edit.words(problem), edit.words(problem).startsWith("This server takes no places from pilots"))
    }

    @Test
    fun aHeightIsTakenAsItIsTypedOnlyWhileItCanBeANumber() {
        val adding = checkNotNull(signedOut(Server()).second.adding())
        val entry = SiteFormDraft.new(Coordinate(lat = 47.27, lon = 10.87), adding)
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
        val edit = signedOut(server).second
        val form = edit.form
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
        // Under the core's words, as every word of a site's page is.
        assertEquals(edit.says.report to edit.says.reportRevision, site.title to Reported.siteRevision(ID, 2u).title)
        assertEquals("Report this site", site.title)
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
