package fly.venti.app.assistant

import com.sun.net.httpserver.HttpServer
import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.AssistantRequest
import fly.venti.core.HttpHeader
import fly.venti.core.assistantModels
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.net.InetSocketAddress
import java.util.Collections

/**
 * The assistant's conversation model against the real core, with the
 * provider played by a script.
 */
class AssistantChatTest {
    private val scope = CoroutineScope(Dispatchers.Default)

    @After
    fun stop() {
        scope.cancel()
    }

    private fun makeLogbook(): LogbookModel = runBlocking {
        LogbookModel.open(TestSupport.newFolder("assistant"), TestSupport.files).also { it.addSample() }
    }

    private fun makeChat(
        logbook: LogbookModel = makeLogbook(),
        keys: KeyStore = FixedKey("test-key"),
        preferences: MemoryPreferences = MemoryPreferences(),
        transport: AssistantTransport = ScriptedProvider::answer,
    ) = AssistantChat(logbook::makeAssistant, keys, preferences, scope, transport)

    @Test
    fun aQuestionIsAnsweredThroughTheLogbookTools() = runBlocking {
        val chat = makeChat()
        assertTrue(chat.hasKey)
        assertTrue(chat.entries.isEmpty())

        chat.ask("  How was 2026?  ").join()

        assertNull(chat.problem)
        assertFalse(chat.isWorking)
        assertEquals(2, chat.entries.size)
        assertTrue(chat.entries[0].fromPilot)
        assertEquals("How was 2026?", chat.entries[0].text)
        assertFalse(chat.entries[1].fromPilot)
        assertEquals(ScriptedProvider.REPLY, chat.entries[1].text)
        assertEquals(listOf("your list of flights"), chat.entries[1].lookedUp)

        chat.startAgain()
        assertTrue(chat.entries.isEmpty())
    }

    @Test
    fun theKeyGoesToTheProviderInAHeaderAndTheLogbookInTheBody() = runBlocking {
        val seen = Collections.synchronizedList(mutableListOf<AssistantRequest>())
        val chat = makeChat(
            transport = { request ->
                seen += request
                ScriptedProvider.answer(request)
            },
        )

        chat.ask("How was 2026?").join()

        assertEquals(2, seen.size)
        for (request in seen) {
            assertEquals("https://api.anthropic.com/v1/messages", request.url)
            assertTrue(request.headers.any { it.name == "x-api-key" && it.value == "test-key" })
            assertFalse(request.body.contains("test-key"))
        }
        // The second request carries what the tool found in the logbook.
        assertTrue(seen[1].body.contains("Sample Wing"))
    }

    @Test
    fun aProviderThatCannotBeReachedLeavesTheConversationAsItWas() = runBlocking {
        val chat = makeChat(transport = { throw IOException("No connection.") })

        chat.ask("How was 2026?").join()

        assertTrue(checkNotNull(chat.problem).startsWith("The provider could not be reached."))
        assertTrue(chat.entries.isEmpty())
        assertFalse(chat.isWorking)

        // The next question starts clean.
        chat.problem = null
        val working = makeChat()
        working.ask("How was 2026?").join()
        assertEquals(2, working.entries.size)
    }

    @Test
    fun aProvidersRefusalIsToldInTheCoresWords() = runBlocking {
        val chat = makeChat(
            transport = {
                HttpAnswer(
                    401,
                    """{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"""
                        .toByteArray(),
                )
            },
        )

        chat.ask("How was 2026?").join()

        assertNotNull(chat.problem)
        assertTrue(chat.entries.isEmpty())
        assertFalse(chat.isWorking)
    }

    @Test
    fun withoutAKeyNothingIsAsked() = runBlocking {
        var requests = 0
        val chat = makeChat(
            keys = FixedKey(null),
            transport = {
                requests += 1
                HttpAnswer(500, ByteArray(0))
            },
        )
        assertFalse(chat.hasKey)

        chat.ask("How was 2026?").join()

        assertEquals(0, requests)
        assertTrue(chat.entries.isEmpty())
    }

    @Test
    fun theKeyIsSavedTrimmedAndRemovingItEndsTheConversation() = runBlocking {
        val keys = FixedKey(null)
        val chat = makeChat(keys = keys)

        chat.saveKey("   ")
        assertFalse(chat.hasKey)
        chat.saveKey("  sk-test  ")
        assertTrue(chat.hasKey)
        assertEquals("sk-test", keys.read())

        chat.ask("How was 2026?").join()
        assertEquals(2, chat.entries.size)

        chat.removeKey()
        assertFalse(chat.hasKey)
        assertNull(keys.read())
        assertTrue(chat.entries.isEmpty())
    }

    @Test
    fun theModelIsTheCoresFirstUntilThePilotChoosesAndTheChoiceIsRemembered() = runBlocking {
        val preferences = MemoryPreferences()
        val logbook = makeLogbook()
        val models = assistantModels().map { it.id }
        val seen = Collections.synchronizedList(mutableListOf<AssistantRequest>())
        val chat = makeChat(logbook, preferences = preferences, transport = { request ->
            seen += request
            ScriptedProvider.answer(request)
        })
        assertEquals(models.first(), chat.model)

        chat.ask("How was 2026?").join()
        chat.use(models.last())

        // Another model is another conversation.
        assertTrue(chat.entries.isEmpty())
        assertEquals(models.last(), preferences.assistantModel)
        chat.ask("How was 2026?").join()
        assertTrue(seen.first().body.contains(models.first()))
        assertTrue(seen.last().body.contains(models.last()))
        assertEquals(models.last(), makeChat(logbook, preferences = preferences).model)
    }

    @Test
    fun aRequestIsPostedWithItsHeadersAndTheAnswerComesBackWhateverItsStatus() = runBlocking {
        // A provider on this machine, to see what is really sent.
        val received = Collections.synchronizedList(mutableListOf<String>())
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/v1/messages") { exchange ->
            received += exchange.requestMethod
            received += exchange.requestHeaders.getFirst("x-api-key")
            received += exchange.requestBody.readBytes().decodeToString()
            val refused = exchange.requestHeaders.getFirst("x-api-key") != "test-key"
            val answer = (if (refused) "no" else "yes").toByteArray()
            exchange.sendResponseHeaders(if (refused) 401 else 200, answer.size.toLong())
            exchange.responseBody.use { it.write(answer) }
        }
        server.start()
        try {
            val url = "http://127.0.0.1:${server.address.port}/v1/messages"

            val accepted = AssistantHttp.send(
                AssistantRequest(url, listOf(HttpHeader("x-api-key", "test-key")), """{"question":"é"}"""),
            )
            assertEquals(200, accepted.status)
            assertEquals("yes", accepted.body.decodeToString())
            assertEquals(listOf("POST", "test-key", """{"question":"é"}"""), received.toList())

            val refused = AssistantHttp.send(AssistantRequest(url, listOf(HttpHeader("x-api-key", "wrong")), "{}"))
            assertEquals(401, refused.status)
            assertEquals("no", refused.body.decodeToString())
        } finally {
            server.stop(0)
        }
    }

    @Test
    fun theLittleMarkdownOfAnAnswerIsRead() {
        fun runs(text: String) = InlineMarkdown.parse(text)
        val plain = InlineMarkdown.Run("")

        assertEquals(
            listOf(
                plain.copy(text = "The first was the stronger, at "),
                plain.copy(text = "2.4 m/s", bold = true),
                plain.copy(text = " in a "),
                plain.copy(text = "westerly", italic = true),
                plain.copy(text = "."),
            ),
            runs("The first was the stronger, at **2.4 m/s** in a *westerly*."),
        )
        assertEquals(
            listOf(
                plain.copy(text = "See "),
                plain.copy(text = "the rules", link = "https://example.org/rules"),
                plain.copy(text = " and "),
                plain.copy(text = "flight_file.igc", code = true),
            ),
            runs("See [the rules](https://example.org/rules) and `flight_file.igc`"),
        )
        // Bold may hold italics.
        assertEquals(
            listOf(plain.copy(text = "very ", bold = true), plain.copy(text = "strong", bold = true, italic = true)),
            runs("**very _strong_**"),
        )
        // What is not Markdown stays as written.
        for (text in listOf("2 * 3 * 4 = 24", "flight_file_name.igc", "* one\n* two", "a ** b", "[not a link](javascript:x)")) {
            assertEquals(text, listOf(plain.copy(text = text)), runs(text))
        }
        assertEquals("Line one\nline two", runs("Line one\nline two").joinToString("") { it.text })
        assertTrue(runs("").isEmpty())
    }
}
