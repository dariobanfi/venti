package fly.venti.app.account

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.alerts.AlertWords
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.AlertDayKind
import fly.venti.core.googleWords
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * "Continue with Google" against the real core, with the server played by
 * the answers the real server's tests keep: the handle step of a first
 * sign-in, then a sign-in; linking and unlinking; and closing Google's
 * sheet, which is no problem.
 */
class GoogleSignInTest {
    private class Sent(val method: String, val path: String, val body: String?)

    /** A server that knows the Google account once an account was made with it. */
    private class Server {
        val sent: MutableList<Sent> = CopyOnWriteArrayList<Sent>()
        var knowsTheGoogleAccount = false

        @Suppress("UNUSED_PARAMETER", "RedundantSuspendModifier")
        suspend fun answer(
            method: String,
            url: String,
            headers: List<Pair<String, String>>,
            body: ByteArray?,
            answerWithinMilliseconds: Int,
        ): HttpAnswer {
            val path = url.removePrefix(SERVER).substringBefore('?')
            sent += Sent(method, path, body?.decodeToString())
            return when {
                method == "POST" && path == "/v1/auth/google" ->
                    if (knowsTheGoogleAccount) HttpAnswer(201, recorded("sign_in.json")) else HttpAnswer(200, recorded("google_sign_up.json"))
                method == "POST" && path == "/v1/auth/google/account" -> {
                    knowsTheGoogleAccount = true
                    HttpAnswer(201, recorded("sign_in.json"))
                }
                method == "POST" && path == "/v1/account/google" -> HttpAnswer(200, withGoogle("ui.test@example.com"))
                method == "DELETE" && path == "/v1/account/google" -> HttpAnswer(204, ByteArray(0))
                method == "GET" && path == "/v1/flights/changes" ->
                    HttpAnswer(200, """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}""".toByteArray())
                method == "GET" && path == "/v1/account/kept-sites/changes" ->
                    HttpAnswer(200, """{"kept": [], "forgotten": [], "cursor": "1", "more": false, "reset": false}""".toByteArray())
                method == "DELETE" -> HttpAnswer(204, ByteArray(0))
                else -> HttpAnswer(404, """{"error": {"code": "not_found", "message": "x"}}""".toByteArray())
            }
        }
    }

    private fun model(server: Server): AccountModel {
        val logbook = LogbookModel.open(TestSupport.newFolder("google"), TestSupport.files)
        return logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
    }

    @Test
    fun aFirstSignInGoesThroughTheHandleStepAndTheNextSignsInAtOnce() = runBlocking {
        val server = Server()
        val account = model(server)
        val google: suspend () -> String = { ID_TOKEN }

        account.signInWithGoogle(SERVER, google)

        assertNull(account.problem)
        assertFalse(account.isSignedIn)
        val (at, signUp) = checkNotNull(account.googleSignUp)
        assertEquals(SERVER, at)
        assertEquals("ui_test", signUp.suggestedHandle)
        assertEquals("UI Test", signUp.name)
        assertEquals("@ui_test is free. It was made from your Google account; change it if you like.", signUp.suggestionLine)
        val asked = server.sent.first { it.path == "/v1/auth/google" }
        assertTrue(checkNotNull(asked.body).contains(ID_TOKEN))

        account.createGoogleAccount("ui_test", "UI Test", invite = " ")

        assertNull(account.problem)
        assertTrue(account.isSignedIn)
        assertNull(account.googleSignUp)
        val created = checkNotNull(server.sent.first { it.path == "/v1/auth/google/account" }.body)
        assertTrue(created, created.contains(signUp.signup) && created.contains("ui_test"))
        // A blank invite code is not sent.
        assertFalse(created, created.contains("invite"))

        account.signOut()
        assertFalse(account.isSignedIn)

        account.signInWithGoogle(SERVER, google)
        assertNull(account.problem)
        assertNull(account.googleSignUp)
        assertTrue(account.isSignedIn)
    }

    @Test
    fun closingGooglesSheetIsNoProblem() = runBlocking {
        val server = Server()
        val account = model(server)

        account.signInWithGoogle(SERVER) { throw GoogleCancelled() }

        assertNull(account.problem)
        assertNull(account.googleSignUp)
        assertFalse(account.isWorking)
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun aRefusalOfGooglesTokenIsSaidInTheCoresWords() = runBlocking {
        val account = model(Server())

        account.signInWithGoogle(SERVER) { " " }

        assertEquals("Google gave no sign-in. Try again.", account.problem)
    }

    @Test
    fun aSignedInAccountLinksAndUnlinksGoogle() = runBlocking {
        val server = Server().apply { knowsTheGoogleAccount = true }
        val account = model(server)
        account.signInWithGoogle(SERVER) { ID_TOKEN }
        assertTrue(account.isSignedIn)
        assertNull(account.account?.googleEmail)

        account.linkGoogle { ID_TOKEN }
        assertNull(account.problem)
        assertEquals("ui.test@example.com", account.account?.googleEmail)

        account.unlinkGoogle()
        assertNull(account.problem)
        assertNull(account.account?.googleEmail)
    }

    @Test
    fun theDevelopmentServersTokenIsReadFromItsAnswer() {
        assertEquals(
            Json.parseToJsonElement(recorded("dev_google_id_token.json").decodeToString()).jsonObject.getValue("id_token").jsonPrimitive.content,
            DevelopmentGoogle.idTokenIn(recorded("dev_google_id_token.json").decodeToString()),
        )
        assertNull(DevelopmentGoogle.idTokenIn("not json"))
    }

    @Test
    fun theWordsAreTheCores() {
        assertEquals("Continue with Google", googleWords().continueWithGoogle)
        // The alarms' form takes its words from the core too.
        assertEquals("Alerts", AlertWords.core.title)
        assertEquals(listOf("Today", "Tomorrow", "In 2 days", "In 3 days"), AlertWords.core.dayNames)
        assertEquals(
            listOf(AlertDayKind.FLYABLE, AlertDayKind.LOCAL, AlertDayKind.CROSS_COUNTRY, AlertDayKind.BIG),
            AlertWords.core.dayKinds.map { it.kind },
        )
        assertNotNull(AlertWords.core.fields.firstOrNull { it.field == "rule.min_kmh" })
    }

    private companion object {
        const val SERVER = "https://venti.example.org"
        const val ID_TOKEN = "eyJ.made-up.token"

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

        /** The account as `GET /v1/account` answers it, with a Google account linked. */
        fun withGoogle(email: String): ByteArray {
            val signIn = Json.parseToJsonElement(recorded("sign_in.json").decodeToString()).jsonObject
            val account = signIn.getValue("account").jsonObject
            return """{"account": ${kotlinx.serialization.json.JsonObject(account + ("google_email" to kotlinx.serialization.json.JsonPrimitive(email)))}}"""
                .toByteArray()
        }
    }
}
