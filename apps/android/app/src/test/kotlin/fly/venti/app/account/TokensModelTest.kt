package fly.venti.app.account

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * "Programs with access" against the real core, with the server played by
 * the answers the real server's own tests keep for a token made and for
 * the list of them.
 */
class TokensModelTest {
    private class Sent(val method: String, val url: String, val headers: Map<String, String>, val body: ByteArray?) {
        val path: String get() = url.removePrefix(SERVER)
    }

    /** A server that signs anyone in, has the demo pilot's one token and makes and withdraws any. */
    private class Server {
        val sent: MutableList<Sent> = CopyOnWriteArrayList<Sent>()

        /** Answers in place of the usual ones, where a test wants a server in a particular mood. */
        var instead: (Sent) -> HttpAnswer? = { null }

        @Suppress("UNUSED_PARAMETER", "RedundantSuspendModifier")
        suspend fun answer(
            method: String,
            url: String,
            headers: List<Pair<String, String>>,
            body: ByteArray?,
            answerWithinMilliseconds: Int,
        ): HttpAnswer {
            val request = Sent(method, url, headers.toMap(), body)
            sent += request
            instead(request)?.let { return it }
            return when {
                method == "POST" && request.path == "/v1/sessions" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "GET" && request.path == TOKENS -> HttpAnswer(200, recorded("tokens.json"))
                method == "POST" && request.path == TOKENS -> HttpAnswer(201, recorded("token_created.json"))
                method == "DELETE" && request.path.startsWith("$TOKENS/") -> HttpAnswer(204, ByteArray(0))
                method == "GET" && request.path.startsWith("/v1/flights/changes") ->
                    HttpAnswer(200, """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}""".toByteArray())
                method == "GET" && request.path.startsWith("/v1/account/kept-sites/changes") ->
                    HttpAnswer(200, """{"kept": [], "forgotten": [], "cursor": "1", "more": false, "reset": false}""".toByteArray())
                else -> refusal(404, "not_found")
            }
        }

        fun aboutTokens(): List<Sent> = sent.filter { it.path.startsWith(TOKENS) }
    }

    private fun account(server: Server): AccountModel =
        LogbookModel.open(TestSupport.newFolder("tokens"), TestSupport.files)
            .makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)

    private suspend fun signedIn(server: Server): TokensModel {
        val account = account(server)
        account.signIn(SERVER, "demo", "a password of the test")
        assertTrue(account.isSignedIn)
        return TokensModel(account, now = { NOW })
    }

    @Test
    fun theWordsAreTheCoresAndWhatItRefusesIsNotSent() = runBlocking {
        val server = Server()
        val tokens = signedIn(server)

        val form = tokens.form
        assertEquals("Programs with access", form.title)
        assertEquals("Give access", form.submit)
        assertEquals("No program has access to your account.", form.empty)
        assertTrue(form.scopes.any { it.scope == "flights:write" && it.title.isNotEmpty() && it.line.isNotEmpty() })

        // The name is checked before anything is sent, in the core's words.
        assertEquals(
            "Give the program a name, so that you know later what this access is for.",
            tokens.nameFault("  "),
        )
        assertEquals(
            "The name can have at most ${form.nameMaxLength} characters.",
            tokens.nameFault("x".repeat(form.nameMaxLength.toInt() + 1)),
        )
        assertNull(tokens.nameFault("My logger"))

        // So are the things it may do, and the password.
        val nothingChosen = tokens.give("My logger", emptyList(), null, PASSWORD)
        assertEquals(TokensModel.Given.Refused("Choose at least one thing the program may do."), nothingChosen)
        val noPassword = tokens.give("My logger", listOf("flights:write"), null, "")
        assertEquals(TokensModel.Given.Refused("Enter your password."), noPassword)
        assertTrue(server.aboutTokens().isEmpty())
    }

    @Test
    fun aTokenIsMadeListedAndWithdrawnWithTheSession() = runBlocking {
        val server = Server()
        val tokens = signedIn(server)

        // Made from what the pilot entered: the request is the one the server's tests keep.
        val given = tokens.give(" My logger ", listOf("live:write", "flights:write"), null, PASSWORD)
        val created = (given as TokensModel.Given.Made).created
        assertTrue(created.secret.startsWith("venti_pat_"))
        assertEquals("Copy it now: Venti does not show it again.", created.shownOnce)
        assertEquals("Copy", created.copy)
        assertEquals("I have copied it", created.done)
        assertEquals("My logger", created.token.name)
        // Whoever prints what became of the form prints no secret.
        assertFalse(created.secret in given.toString())
        val making = server.aboutTokens().single()
        assertEquals("POST" to TOKENS, making.method to making.path)
        assertEquals("Bearer $TOKEN", making.headers["Authorization"])
        assertEquals("application/json", making.headers["Content-Type"])
        assertEquals(
            Json.parseToJsonElement(recorded("token_create_request.json").decodeToString()),
            Json.parseToJsonElement(checkNotNull(making.body).decodeToString()),
        )

        // With an end: the last second of the day the pilot chose, the day the list then names.
        tokens.give("My logger", listOf("flights:write"), TokenWords.endOfDay(DAY_MILLIS), PASSWORD)
        val ending = Json.parseToJsonElement(checkNotNull(server.aboutTokens().last().body).decodeToString()).jsonObject
        assertEquals("2026-12-21T23:59:59Z", ending.getValue("expires_at").jsonPrimitive.content)
        assertEquals("21 Dec 2026", TokenWords.day(DAY_MILLIS))

        // The list, in the core's words.
        server.sent.clear()
        val listed = (tokens.list() as TokensModel.Listed.Tokens).tokens
        val asking = server.sent.single()
        assertEquals("GET" to TOKENS, asking.method to asking.path)
        assertEquals("Bearer $TOKEN", asking.headers["Authorization"])
        val demo = listed.single()
        assertEquals("Demo logger", demo.name)
        assertFalse(demo.hasEnded)
        assertEquals("Flights: read · Flights: write", demo.scopesLine)
        assertEquals("${demo.scopesLine}\nMade 18 Nov 2026\nLast used never\nEnds never", TokenWords.detail(demo))
        // One that has ended says so in place of "Ends".
        val ended = demo.copy(ends = "on 1 Nov 2026", hasEnded = true)
        assertTrue(TokenWords.detail(ended).endsWith("\nEnded on 1 Nov 2026"))

        // Withdrawing asks first in the core's words, and goes by the token's identifier.
        val words = tokens.withdrawal(demo)
        assertEquals("Withdraw the access of “Demo logger”?", words.title)
        assertEquals("Withdraw access", words.confirm)
        assertEquals("Keep access", words.cancel)
        assertNull(tokens.withdraw(demo))
        val withdrawing = server.sent.last()
        assertEquals("DELETE" to "$TOKENS/${demo.id}", withdrawing.method to withdrawing.path)
        assertEquals("Bearer $TOKEN", withdrawing.headers["Authorization"])
    }

    @Test
    fun aRefusalIsSaidInTheCoresWords() = runBlocking {
        val server = Server()
        val tokens = signedIn(server)

        server.instead = { request -> if (request.path == TOKENS && request.method == "POST") refusal(403, "wrong_password") else null }
        assertEquals(
            TokensModel.Given.Refused("The password is wrong."),
            tokens.give("My logger", listOf("flights:write"), null, "not the password"),
        )

        server.instead = { request -> if (request.method == "DELETE") refusal(403, "wrong_password") else null }
        val demo = (tokens.list() as TokensModel.Listed.Tokens).tokens.single()
        assertEquals("The password is wrong.", tokens.withdraw(demo))
        // One that was withdrawn already is withdrawn.
        server.instead = { request -> if (request.method == "DELETE") refusal(404, "token_not_found") else null }
        assertNull(tokens.withdraw(demo))

        server.instead = { throw IOException("no route to host") }
        val unreached = tokens.list() as TokensModel.Listed.Refused
        assertTrue(unreached.words, unreached.words.startsWith("Venti could not be reached."))

        // Signed out, nothing is asked of the server.
        val nobody = Server()
        val signedOut = TokensModel(account(nobody), now = { NOW })
        assertEquals(
            TokensModel.Listed.Refused("This phone is no longer signed in. Sign in again."),
            signedOut.list(),
        )
        assertTrue(nobody.sent.isEmpty())
    }

    @Test
    fun anEndIsADayThatHasNotBegun() {
        // 2026-11-21 09:05 UTC: that day has begun, the next has not.
        assertFalse(TokenWords.canEndOn(DAY_MILLIS - 30 * DAY, NOW))
        assertTrue(TokenWords.canEndOn(DAY_MILLIS - 29 * DAY, NOW))
        assertEquals(DAY_MILLIS / 1000 + 86_399, TokenWords.endOfDay(DAY_MILLIS))
    }

    private companion object {
        const val SERVER = "https://venti.example.org"
        const val TOKENS = "/v1/account/tokens"
        const val PASSWORD = "demo-pilot-password"

        /** 2026-11-21 09:05 UTC. */
        const val NOW = 1_795_251_900L

        /** Midnight UTC of 21 December 2026, as a date picker says a day. */
        const val DAY_MILLIS = 1_797_811_200_000L
        const val DAY = 86_400_000L

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

        val TOKEN: String =
            Json.parseToJsonElement(recorded("sign_in.json").decodeToString()).jsonObject.getValue("token").jsonPrimitive.content

        fun refusal(status: Int, code: String) =
            HttpAnswer(status, """{"error": {"code": "$code", "message": "x"}}""".toByteArray())
    }
}
