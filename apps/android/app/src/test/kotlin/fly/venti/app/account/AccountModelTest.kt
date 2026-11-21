package fly.venti.app.account

import fly.venti.app.HttpAnswer
import fly.venti.app.Session
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.FlightVisibility
import fly.venti.core.ReportReason
import fly.venti.core.ReportSubject
import fly.venti.core.ServerRequest
import fly.venti.core.SiteKind
import fly.venti.core.sitesMap
import fly.venti.core.syncSummaryInWords
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * The account model against the real core, with the server played by the
 * answers the real server's own tests keep for signing in and for an
 * upload, and by short ones in the same format.
 */
class AccountModelTest {
    /** One request as the model sent it, and how long it would wait for the answer. */
    private class Sent(
        val method: String,
        val url: String,
        val headers: Map<String, String>,
        val body: ByteArray?,
        val answerWithinMilliseconds: Int,
    ) {
        val path: String get() = url.removePrefix(SERVER).substringBefore('?')
    }

    /** A server that signs anyone in, has no flights and takes every upload. */
    private class Server {
        val sent: MutableList<Sent> = CopyOnWriteArrayList<Sent>()

        /** Answers in place of the usual ones, where a test wants a server in a particular mood. */
        var instead: suspend (Sent) -> HttpAnswer? = { null }

        suspend fun answer(
            method: String,
            url: String,
            headers: List<Pair<String, String>>,
            body: ByteArray?,
            answerWithinMilliseconds: Int,
        ): HttpAnswer {
            val request = Sent(method, url, headers.toMap(), body, answerWithinMilliseconds)
            sent += request
            instead(request)?.let { return it }
            return when {
                method == "POST" && request.path == "/v1/sessions" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "POST" && request.path == "/v1/accounts" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "GET" && request.path == "/v1/flights/changes" -> HttpAnswer(200, NO_CHANGES.toByteArray())
                method == "POST" && request.path == "/v1/flights" -> HttpAnswer(201, recorded("upload.json"))
                method == "GET" && request.path == "$KEPT_SITES/changes" -> HttpAnswer(200, NO_KEPT_CHANGES.toByteArray())
                method == "PUT" && request.path.startsWith("$KEPT_SITES/") -> HttpAnswer(200, keptAsSent(request))
                method == "PATCH" -> HttpAnswer(
                    200,
                    recorded("upload.json").decodeToString().replace("\"private\"", "\"unlisted\"").toByteArray(),
                )
                method == "DELETE" -> HttpAnswer(204, ByteArray(0))
                else -> refusal(404, "not_found")
            }
        }

        fun methods(): List<String> = sent.map { it.method }
    }

    /** A clock the test moves by hand, in milliseconds since 1970. */
    private class Clock(var now: Long = 1_795_000_000_000) {
        fun pass(seconds: Int) {
            now += seconds * 1000L
        }
    }

    private fun logbook(): LogbookModel = LogbookModel.open(TestSupport.newFolder("account"), TestSupport.files)

    private fun model(
        logbook: LogbookModel,
        server: Server,
        session: SessionStore = MemorySessionStore(),
        clock: Clock = Clock(),
    ): AccountModel = logbook.makeAccount(session, device = "Sample Phone", transport = server::answer, clock = { clock.now })

    /** Whether the core remembers an account in this logbook's folder. */
    private fun knowsAnAccount(logbook: LogbookModel): Boolean =
        logbook.makeAccount(MemorySessionStore(), device = null).account != null

    @Test
    fun signingInKeepsTheTokenAndSendsTheLogbook() = runBlocking {
        val logbook = logbook()
        val flight = logbook.addSample().added.single()
        val (server, session) = Server() to MemorySessionStore()
        val account = model(logbook, server, session)
        assertFalse(account.isSignedIn)
        assertEquals("Its file is removed from this phone. This cannot be undone.", logbook.deletionWarning)

        account.signIn(SERVER, "demo", "a password of the test")

        assertNull(account.problem)
        assertTrue(account.isSignedIn)
        assertEquals("demo", account.account?.handle)
        assertEquals("Demo Pilot", account.account?.displayName)
        assertEquals(TOKEN, session.read())
        // Signing in is followed by a sync, which sent the one flight.
        assertEquals(1u, account.lastSync?.sent)
        assertEquals("1 sent", syncSummaryInWords(checkNotNull(account.lastSync)))
        assertEquals(0u, account.account?.flightsToSend)
        // The logbook was read again, and says that a sync did it.
        assertTrue(logbook.syncs > 0)
        assertEquals(1u, account.account?.flightsOnServer)
        assertFalse(account.isWorking)

        // And after the logbook the pilot's own take-offs are asked for,
        // and then the pilot's alarms.
        assertEquals(listOf("POST", "GET", "POST", "GET", "GET"), server.methods())
        assertEquals("$KEPT_SITES/changes", server.sent[3].path)
        assertEquals("Bearer $TOKEN", server.sent[3].headers["Authorization"])
        assertEquals("/v1/account/alerts", server.sent[4].path)
        assertTrue(server.sent.all { it.url.startsWith("$SERVER/v1/") })
        // The password goes with the sign-in, with what to call this
        // phone, and the token does not; after that the token goes with
        // every request.
        val signIn = server.sent[0]
        assertNull(signIn.headers["Authorization"])
        assertEquals("application/json", signIn.headers["Content-Type"])
        val said = Json.parseToJsonElement(checkNotNull(signIn.body).decodeToString()).jsonObject
        assertEquals("demo", said.getValue("handle").jsonPrimitive.content)
        assertEquals("Sample Phone", said.getValue("device").jsonPrimitive.content)
        assertEquals("Bearer $TOKEN", server.sent[1].headers["Authorization"])
        assertEquals("Bearer $TOKEN", server.sent[2].headers["Authorization"])
        assertEquals("application/octet-stream", server.sent[2].headers["Content-Type"])
        assertArrayEquals(TestSupport.sampleFlight.toByteArray(), server.sent[2].body)

        // With an account, deleting a flight reaches further than this phone.
        assertTrue(logbook.deletionWarning.contains("from Venti and from your other devices"))

        // Sharing changes who may see the flight and gives its link.
        assertEquals(FlightVisibility.PRIVATE, account.sharing(flight)?.visibility)
        val shared = account.share(flight, FlightVisibility.UNLISTED)
        assertEquals(FlightVisibility.UNLISTED, shared?.visibility)
        assertEquals("$SERVER/f/wkvydb8am8wmdcxzds9vkrw5kr", shared?.link)
        assertEquals("PATCH", server.sent.last().method)
        assertEquals("/v1/flights/wkvydb8am8wmdcxzds9vkrw5kr", server.sent.last().path)
        assertEquals(FlightVisibility.UNLISTED, account.sharing(flight)?.visibility)

        // Signing out tells the server and forgets the token; the flight stays.
        account.signOut()
        assertNull(account.account)
        assertNull(session.read())
        assertNull(account.lastSync)
        assertEquals(1u, logbook.view.totals.flights)
        assertEquals("DELETE", server.sent.last().method)
        assertEquals("/v1/sessions/3kq9x2m7r4t8v1w6y0a5b3c2d4", server.sent.last().path)
        assertEquals("Bearer $TOKEN", server.sent.last().headers["Authorization"])
        assertNull(account.sharing(flight))
    }

    @Test
    fun aNewAccountIsMadeWithWhatThePilotEnteredAndWhatTheCoreRefusesIsNotSent() = runBlocking {
        val server = Server()
        val account = model(logbook(), server)

        // The core knows the server's rules, and says them before anything is sent.
        account.createAccount(SERVER, "demo", "Demo Pilot", "too short", invite = "")
        assertTrue(checkNotNull(account.problem), checkNotNull(account.problem).contains("15"))
        assertTrue(server.sent.isEmpty())
        assertFalse(account.isSignedIn)

        account.problem = null
        account.createAccount(SERVER, "demo", "Demo Pilot", "four words in a row", invite = " welcome ")
        assertNull(account.problem)
        assertTrue(account.isSignedIn)
        val made = server.sent.first()
        assertEquals("POST", made.method)
        assertEquals("/v1/accounts", made.path)
        val said = Json.parseToJsonElement(checkNotNull(made.body).decodeToString()).jsonObject
        assertEquals("Demo Pilot", said.getValue("display_name").jsonPrimitive.content)
        assertEquals("welcome", said.getValue("invite").jsonPrimitive.content)
    }

    @Test
    fun aKeystoreThatRefusesTheSessionIsSaidAndNoSessionIsLeftBehind() = runBlocking {
        val logbook = logbook()
        val server = Server()
        // A keystore that keeps nothing.
        val forgetful = object : SessionStore {
            override fun read(): String? = null

            override fun save(token: String): Boolean = false

            override fun remove() = Unit
        }
        val account = model(logbook, server, forgetful)

        account.signIn(SERVER, "demo", "a password of the test")

        assertFalse(account.isSignedIn)
        assertNull(account.account)
        assertFalse(knowsAnAccount(logbook))
        assertEquals(
            "This phone could not keep the session in its keystore, so you are not signed in. " +
                "Restart the phone and try again.",
            account.problem,
        )
        // The server was told to forget the session, with the session's own
        // token, and nothing else was asked of it.
        assertEquals(listOf("POST", "DELETE"), server.methods())
        assertEquals("/v1/sessions/3kq9x2m7r4t8v1w6y0a5b3c2d4", server.sent[1].path)
        assertEquals("Bearer $TOKEN", server.sent[1].headers["Authorization"])
    }

    @Test
    fun aWrongPasswordIsSaidAndNothingIsKept() = runBlocking {
        val server = Server()
        server.instead = { refusal(401, "invalid_credentials") }
        val session = MemorySessionStore()
        val account = model(logbook(), server, session)

        account.signIn(SERVER, "demo", "wrong")

        assertEquals("The handle or the password is wrong.", account.problem)
        assertFalse(account.isSignedIn)
        assertNull(session.read())
    }

    @Test
    fun aServerThatCannotBeReachedIsSaidOnlyWhenThePilotAsked() = runBlocking {
        val server = Server()
        val account = model(logbook(), server)
        account.signIn(SERVER, "demo", "a password of the test")
        assertNull(account.problem)

        server.instead = { throw IOException("no route to host") }
        // A sync nobody asked for is quiet: being offline after a flight is no news.
        account.sync(force = true)
        assertNull(account.problem)
        assertTrue(account.isSignedIn)

        account.sync(force = true, quietly = false)
        assertTrue(checkNotNull(account.problem).startsWith("Venti could not be reached."))
        assertTrue(account.isSignedIn)
        assertFalse(account.isWorking)
    }

    @Test
    fun anEndedSessionAsksThePilotToSignInAgain() = runBlocking {
        val server = Server()
        val session = MemorySessionStore()
        val account = model(logbook(), server, session)
        account.signIn(SERVER, "demo", "a password of the test")
        assertTrue(account.isSignedIn)

        server.instead = { request ->
            if (request.path == "/v1/flights/changes") refusal(401, "unauthorized") else null
        }
        // Said also after a sync the pilot did not ask for: nothing syncs until it is mended.
        account.sync(force = true)

        assertFalse(account.isSignedIn)
        assertTrue(account.needsSignIn)
        assertEquals(true, account.account?.needsSignIn)
        assertNull(session.read())
        assertEquals("This device is no longer signed in. Sign in again.", account.problem)

        // And nothing more is asked of the server until the pilot has signed in again.
        val asked = server.sent.size
        account.sync(force = true)
        assertEquals(asked, server.sent.size)
    }

    @Test
    fun aSyncNobodyAskedForWaitsAMinuteAndOneAskedForWhileAnotherRunsFollowsIt() = runBlocking {
        val server = Server()
        val clock = Clock()
        val account = model(logbook(), server, clock = clock)
        account.signIn(SERVER, "demo", "a password of the test")
        fun syncs() = server.sent.count { it.path == "/v1/flights/changes" }
        assertEquals(1, syncs())

        // The app comes to the front again and again; the server is asked once a minute.
        clock.pass(seconds = 20)
        account.sync()
        assertEquals(1, syncs())
        clock.pass(seconds = 41)
        account.sync()
        assertEquals(2, syncs())

        // The logbook changes while a sync runs, as in an import of many
        // flights: what the present run does not take, the next one does.
        var changedOnce = false
        server.instead = { request ->
            if (request.path == "/v1/flights/changes" && !changedOnce) {
                changedOnce = true
                account.sync(force = true)
            }
            null
        }
        account.sync(force = true)
        assertEquals(4, syncs())
        assertFalse(account.isWorking)
    }

    @Test
    fun deletingTheAccountNeedsThePasswordAndLeavesTheLogbook() = runBlocking {
        val logbook = logbook()
        logbook.addSample()
        val server = Server()
        val session = MemorySessionStore()
        val account = model(logbook, server, session)
        account.signIn(SERVER, "demo", "a password of the test")

        // A wrong password: the account stays, here and there.
        server.instead = { request ->
            if (request.method == "DELETE" && request.path == "/v1/account") refusal(403, "wrong_password") else null
        }
        assertFalse(account.deleteAccount("not the password"))
        assertEquals("The password is wrong.", account.problem)
        assertTrue(account.isSignedIn)
        assertEquals(TOKEN, session.read())

        account.problem = null
        server.instead = { null }
        assertTrue(account.deleteAccount("a password of the test"))
        assertNull(account.problem)
        assertNull(account.account)
        assertNull(session.read())
        assertFalse(knowsAnAccount(logbook))
        // The password went in the request's body, beside the token.
        val deletion = server.sent.last()
        assertEquals("/v1/account", deletion.path)
        assertEquals("Bearer $TOKEN", deletion.headers["Authorization"])
        val said = Json.parseToJsonElement(checkNotNull(deletion.body).decodeToString()).jsonObject
        assertEquals("a password of the test", said.getValue("password").jsonPrimitive.content)
        // The flights on this phone stay.
        assertEquals(1u, logbook.view.totals.flights)
    }

    @Test
    fun aPhoneThatKnowsTheAccountButHasNoTokenSaysSoAndAsksNothing() = runBlocking {
        val logbook = logbook()
        val first = Server()
        model(logbook, first).signIn(SERVER, "demo", "a password of the test")

        // The app's files on a new phone: the account is known, the old
        // phone's keystore did not come along.
        val server = Server()
        val account = model(logbook, server, MemorySessionStore())
        assertEquals("demo", account.account?.handle)
        assertTrue(account.needsSignIn)
        assertFalse(account.isSignedIn)
        account.sync(force = true, quietly = false)
        assertTrue(server.sent.isEmpty())

        // Signing out forgets the account without a word to the server it
        // cannot speak to, and signing in again mends it.
        account.signOut()
        assertNull(account.account)
        account.signIn(SERVER, "demo", "a password of the test")
        assertTrue(account.isSignedIn)
    }

    @Test
    fun theCockpitsRequestsAreCarriedWithTheTokenAndNotWaitedForLong() = runBlocking {
        val logbook = logbook()
        val (server, session) = Server() to MemorySessionStore()
        val account = model(logbook, server, session)
        val positions = ServerRequest(
            method = "POST",
            path = "/v1/live/positions",
            body = """{"fixes": []}""".toByteArray(),
            contentType = "application/json",
            needsToken = true,
        )

        // Signed in nowhere, there is nobody to carry anything to.
        assertNull(account.carry(positions))
        assertTrue(server.sent.isEmpty())

        account.signIn(SERVER, "demo", "a password of the test")
        // A flight's file is waited for two minutes; that is the usual wait.
        assertTrue(server.sent.all { it.answerWithinMilliseconds == 120_000 })
        server.sent.clear()
        server.instead = { request ->
            if (request.path == "/v1/live/positions") HttpAnswer(201, "{}".toByteArray(), retryAfterSeconds = 7u) else null
        }

        // The answer comes back as it is, the server's wish to wait included.
        val answer = checkNotNull(account.carry(positions))
        assertEquals(201, answer.status)
        assertEquals(7u, answer.retryAfterSeconds)
        val carried = server.sent.single()
        assertEquals("POST", carried.method)
        assertEquals("$SERVER/v1/live/positions", carried.url)
        assertEquals("Bearer $TOKEN", carried.headers["Authorization"])
        assertEquals("application/json", carried.headers["Content-Type"])
        assertArrayEquals(positions.body, carried.body)
        // Twenty seconds and no longer: the next positions are due by then.
        assertEquals(20_000, carried.answerWithinMilliseconds)
        // Nothing of it is the pilot's business: it is no problem to show.
        assertNull(account.problem)

        // No network: no answer, said as that and not as a failure.
        server.instead = { throw IOException("no route to host") }
        assertNull(account.carry(positions))
        assertNull(account.problem)

        // A phone whose token is gone has nothing to send it with: as good
        // as the server's own refusal, and nothing is sent.
        server.sent.clear()
        session.remove()
        val refused = checkNotNull(account.carry(positions))
        assertEquals(401, refused.status)
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun whatWhoeverRunsTheServerTookDownOrStoppedIsSaidWhenThePilotShares() = runBlocking {
        val logbook = logbook()
        val flight = logbook.addSample().added.single()
        val server = Server()
        val account = model(logbook, server)
        account.signIn(SERVER, "demo", "a password of the test")
        assertEquals(FlightVisibility.PRIVATE, account.sharing(flight)?.visibility)

        // A flight the operator took down cannot be shared again, and the
        // refusal is said in the core's words where the pilot asked.
        server.instead = { request -> if (request.method == "PATCH") refusal(403, "flight_taken_down") else null }
        assertNull(account.share(flight, FlightVisibility.PUBLIC))
        assertEquals(
            "This flight was taken down on Venti. " +
                "It stays in your logbook, and cannot be shared from it.",
            account.problem,
        )
        assertEquals(FlightVisibility.PRIVATE, account.sharing(flight)?.visibility)

        // So for an account that was stopped from sharing with everyone.
        account.problem = null
        server.instead = { request -> if (request.method == "PATCH") refusal(403, "publishing_suspended") else null }
        assertNull(account.share(flight, FlightVisibility.PUBLIC))
        assertTrue(checkNotNull(account.problem).endsWith("A flight can still be shared with a link."))
    }

    @Test
    fun aReportGoesToTheServerFromTheAccountAndSaysWhatBecameOfIt() = runBlocking {
        val (server, session) = Server() to MemorySessionStore()
        val account = model(logbook(), server, session)
        var isNew = true
        server.instead = { request ->
            when {
                request.path != "/v1/reports" -> null
                isNew -> HttpAnswer(201, """{"created": true}""".toByteArray())
                else -> HttpAnswer(200, """{"created": false}""".toByteArray())
            }
        }
        fun reports() = server.sent.filter { it.path == "/v1/reports" }

        // Without an account nothing is sent.
        val unsigned = account.report(ReportSubject.Pilot("mara"), ReportReason.SPAM, "")
        assertNull(unsigned.sent)
        assertEquals("This phone is no longer signed in. Sign in again.", unsigned.problem)
        assertTrue(server.sent.isEmpty())

        account.signIn(SERVER, "demo", "a password of the test")
        val flight = ReportSubject.Flight("fbyw03atd9qdt5bfe177ra1k94")
        val outcome = account.report(flight, ReportReason.NOT_A_FLIGHT, " This is my track. ")
        assertEquals("Sent. Whoever runs Venti will read it.", outcome.sent)
        assertNull(outcome.problem)

        val report = reports().single()
        assertEquals("POST", report.method)
        assertEquals("$SERVER/v1/reports", report.url)
        assertEquals("Bearer $TOKEN", report.headers["Authorization"])
        assertEquals("application/json", report.headers["Content-Type"])
        assertEquals(30_000, report.answerWithinMilliseconds)
        val said = Json.parseToJsonElement(checkNotNull(report.body).decodeToString()).jsonObject
        assertEquals("not_a_flight", said.getValue("reason").jsonPrimitive.content)
        // Cleaned as the server would clean it.
        assertEquals("This is my track.", said.getValue("text").jsonPrimitive.content)
        assertEquals(
            "fbyw03atd9qdt5bfe177ra1k94",
            said.getValue("subject").jsonObject.getValue("flight").jsonPrimitive.content,
        )

        // Reported once more while the first report waits: said as that.
        isNew = false
        val again = account.report(flight, ReportReason.SPAM, "")
        assertEquals("You reported this before. Your report is still waiting to be read.", again.sent)
        assertEquals(2, reports().size)

        // A report that has to say what is wrong, and does not, is refused
        // before anything is sent.
        val wordless = account.report(ReportSubject.Pilot("mara"), ReportReason.OTHER, " ")
        assertNull(wordless.sent)
        assertEquals("Say in a few words what is wrong.", wordless.problem)
        assertEquals(2, reports().size)

        // What is gone cannot be reported, and a server out of reach is
        // said as that. Neither is a problem of the account's to show
        // anywhere but on the form.
        server.instead = { request -> if (request.path == "/v1/reports") refusal(404, "live_track_not_found") else null }
        val gone = account.report(ReportSubject.LiveTrack("11111111111111111111111111"), ReportReason.PRIVACY, "")
        assertEquals("This track is no longer there.", gone.problem)
        server.instead = { throw IOException("no route to host") }
        val unreached = account.report(ReportSubject.Pilot("mara"), ReportReason.SPAM, "")
        assertTrue(checkNotNull(unreached.problem).startsWith("Venti could not be reached."))
        assertNull(account.problem)
    }

    @Test
    fun thePilotsOwnTakeoffsFollowTheAccount() = runBlocking {
        val logbook = logbook()
        val kept = logbook.makeKeptSites()
        val server = Server()
        val account = model(logbook, server)
        // Tied as the app ties them, but for the scope: the test runs the sync a change asks for itself.
        var asked = 0
        kept.onChanged = { asked += 1 }
        account.onKeptSitesChanged = kept::synced
        val map = sitesMap(47.6, 11.7, 47.8, 12.0, 200.toUShort(), recorded("sites.json"))
        val takeoff = map.sites.first { it.isNamed && SiteKind.TAKEOFF in it.uses }
        val landing = map.sites.first { it.isNamed && SiteKind.TAKEOFF !in it.uses }
        fun aboutKept() = server.sent.filter { it.path.startsWith(KEPT_SITES) }
        suspend fun note() = checkNotNull(kept.all(null)).note

        // Signed out, a kept take-off is on this phone alone and nothing is sent.
        assertTrue(kept.keep(takeoff, map.credits, map.asOfUnixS))
        assertEquals(1, asked)
        account.syncKeptSites()
        assertTrue(server.sent.isEmpty())
        assertTrue(note(), note().startsWith("Kept on this device only"))

        // Signing in offers the account what the phone holds, with the
        // session's token, and the list then says where it lives.
        val read = kept.changes
        account.signIn(SERVER, "demo", "a password of the test")
        assertNull(account.problem)
        assertEquals(
            listOf("GET" to "$KEPT_SITES/changes", "PUT" to "$KEPT_SITES/${takeoff.id}"),
            aboutKept().map { it.method to it.path },
        )
        assertTrue(aboutKept().all { it.headers["Authorization"] == "Bearer $TOKEN" })
        val offered = Json.parseToJsonElement(checkNotNull(aboutKept().last().body).decodeToString()).jsonObject
        assertEquals(takeoff.id, offered.getValue("site").jsonObject.getValue("id").jsonPrimitive.content)
        assertTrue(note(), note().startsWith("Kept on your Venti account and on this device"))
        // What lists the take-offs was told to read them again.
        assertTrue(kept.changes > read)
        assertFalse(account.isWorking)

        // Kept while signed in: it waits for the sync its keeping asks for,
        // which is the take-offs' alone, not the logbook's.
        assertTrue(kept.keep(landing, map.credits, map.asOfUnixS))
        assertEquals(2, asked)
        assertTrue(note(), "after the next sync" in note())
        server.sent.clear()
        account.syncKeptSites()
        assertEquals(listOf("GET", "PUT"), server.methods())
        assertEquals("$KEPT_SITES/${landing.id}", server.sent.last().path)
        assertTrue(note(), note().startsWith("Kept on your Venti account and on this device"))

        // Forgotten here, forgotten there.
        assertTrue(kept.forget(landing.id))
        assertEquals(3, asked)
        server.sent.clear()
        account.syncKeptSites()
        assertEquals("DELETE" to "$KEPT_SITES/${landing.id}", server.sent.first().let { it.method to it.path })
        assertEquals(listOf(takeoff.id), kept.all(null)?.sites?.map { it.site.id })

        // A server out of reach is no news: the list says that something waits.
        server.instead = { throw IOException("no route to host") }
        assertTrue(kept.keep(landing, map.credits, map.asOfUnixS))
        account.syncKeptSites()
        assertNull(account.problem)
        assertTrue(note(), "after the next sync" in note())

        // Another phone signs in to the account: what the account keeps comes to it.
        val other = logbook()
        val there = other.makeKeptSites()
        val itsServer = Server()
        itsServer.instead = { request ->
            if (request.path == "$KEPT_SITES/changes") HttpAnswer(200, recorded("kept_site_changes.json")) else null
        }
        val itsAccount = model(other, itsServer)
        itsAccount.onKeptSitesChanged = there::synced
        assertEquals(emptyList<String>(), there.all(null)?.sites?.map { it.site.name })
        itsAccount.signIn(SERVER, "demo", "a password of the test")
        assertEquals(listOf("Fluggelände Jägerkamp/Nagelspitz"), there.all(null)?.sites?.map { it.site.name })
        assertEquals(1, there.changes)
        assertTrue(checkNotNull(there.all(null)).note.startsWith("Kept on your Venti account and on this device"))
        // Nothing was sent from a phone that held nothing.
        assertTrue(itsServer.sent.none { it.method == "PUT" })

        // Signed out, the list stays on the phone and says so.
        itsAccount.signOut()
        assertEquals(1, there.all(null)?.sites?.size)
        assertTrue(checkNotNull(there.all(null)).note.startsWith("Kept on this device only"))
    }

    @Test
    fun theThreeAnswersToWhoMaySeeAFlightHaveTheirWords() {
        assertEquals(
            listOf("Only me", "Anyone with the link", "Everyone"),
            listOf(FlightVisibility.PRIVATE, FlightVisibility.UNLISTED, FlightVisibility.PUBLIC).map { it.title },
        )
        assertEquals(listOf("private", "unlisted", "public"), FlightVisibility.entries.map { it.key })
        assertTrue(FlightVisibility.entries.all { it.explanation.endsWith(".") })
    }

    @Test
    fun aPhoneIsCalledByItsMakerAndModel() {
        assertEquals("Google Pixel 8", Session.deviceName(maker = "Google", model = "Pixel 8"))
        assertEquals("Samsung SM-S911B", Session.deviceName(maker = "samsung", model = "SM-S911B"))
        // A model that names its maker is not named twice.
        assertEquals("Fairphone 5", Session.deviceName(maker = "Fairphone", model = "Fairphone 5"))
        assertEquals("", Session.deviceName(maker = "", model = ""))
    }

    private companion object {
        const val SERVER = "https://venti.example.org"

        const val NO_CHANGES = """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"""

        /** Where the account keeps the pilot's own take-offs. */
        const val KEPT_SITES = "/v1/account/kept-sites"

        const val NO_KEPT_CHANGES = """{"kept": [], "forgotten": [], "cursor": "1", "more": false, "reset": false}"""

        /** The server's answer to keeping a site: the record it was sent, with the moment it was kept. */
        fun keptAsSent(request: Sent): ByteArray {
            val said = Json.parseToJsonElement(checkNotNull(request.body).decodeToString()).jsonObject
            val kept = JsonObject(said + ("kept_at" to JsonPrimitive("2026-11-21T09:05:00Z")))
            return JsonObject(mapOf("kept" to kept)).toString().toByteArray()
        }

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

        /** The token in the server's recorded answer to signing in: the example of its documentation. */
        val TOKEN: String =
            Json.parseToJsonElement(recorded("sign_in.json").decodeToString()).jsonObject.getValue("token").jsonPrimitive.content

        fun refusal(status: Int, code: String) =
            HttpAnswer(status, """{"error": {"code": "$code", "message": "x"}}""".toByteArray())
    }
}
