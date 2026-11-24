package fly.venti.app.groups

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.account.MemorySessionStore
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.ChatPage
import fly.venti.core.CommunityDraft
import fly.venti.core.HomeDraft
import fly.venti.core.Coordinate
import fly.venti.core.messageLinks
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Groups as communities with a chat (owner decision 26), against the real
 * core and a server played by the test with the answers the server's own
 * tests keep: the groups as cards, the groups near, making one, and a
 * chat's pages, what is new and what is posted. As the iPhone's
 * `GroupChatTests`.
 */
class GroupChatTest {
    private class Sent(val method: String, val url: String, val body: ByteArray?) {
        val path: String get() = url.removePrefix(SERVER).substringBefore('?')
        val query: String get() = url.substringAfter('?', "")
    }

    private class Server {
        val sent: MutableList<Sent> = CopyOnWriteArrayList<Sent>()

        /** What the chat answers to a question of what is new. */
        var since: ByteArray = """{"messages": [], "since": "14", "more": false}""".toByteArray()

        @Suppress("UNUSED_PARAMETER")
        suspend fun answer(
            method: String,
            url: String,
            headers: List<Pair<String, String>>,
            body: ByteArray?,
            answerWithinMilliseconds: Int,
        ): HttpAnswer {
            val request = Sent(method, url, body)
            sent += request
            val path = request.path
            return when {
                method == "POST" && path == "/v1/sessions" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "GET" && path == "/v1/flights/changes" -> HttpAnswer(200, NO_CHANGES.toByteArray())
                method == "GET" && path == "/v1/groups/plans" -> HttpAnswer(200, recorded("plan_board.json"))
                method == "GET" && path == "/v1/groups" -> HttpAnswer(200, recorded("groups.json"))
                method == "GET" && path == "/v1/groups/invites" -> HttpAnswer(200, """{"invites": []}""".toByteArray())
                method == "GET" && path == "/v1/groups/find" -> HttpAnswer(200, recorded("groups_found.json"))
                method == "POST" && path == "/v1/groups" -> HttpAnswer(201, recorded("group_created.json"))
                method == "GET" && path.endsWith("/messages") && "since=" in request.query -> HttpAnswer(200, since)
                method == "GET" && path.endsWith("/messages") -> HttpAnswer(200, recorded("group_messages.json"))
                method == "POST" && path.endsWith("/messages") -> HttpAnswer(201, """{"message": ${message("17", "Hello").decodeToString()}}""".toByteArray())
                method == "PATCH" && path.endsWith("/me") ->
                    HttpAnswer(200, """{"me": {"role": "member", "invited": false, "muted": false, "unread": 0, "read_up_to": "14"}}""".toByteArray())
                method == "GET" && path.startsWith("/v1/groups/") -> HttpAnswer(200, recorded("group.json"))
                else -> HttpAnswer(404, """{"error": {"code": "not_found", "message": "x"}}""".toByteArray())
            }
        }
    }

    private suspend fun signedIn(server: Server): GroupsModel {
        val logbook = LogbookModel.open(TestSupport.newFolder("chat"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        val groups = logbook.makeGroups(account)
        account.signIn(SERVER, "demo", "a password of the test")
        server.sent.clear()
        return groups
    }

    @Test
    fun theGroupsAreCardsAndTheGroupsNearAreFoundByPlaceOrByName() = runBlocking {
        val server = Server()
        val groups = signedIn(server)

        groups.loadChats()
        val card = checkNotNull(groups.chats).single()
        assertEquals("Mustergruppe", card.name)
        assertEquals(listOf("/v1/groups", "/v1/groups/invites"), server.sent.map { it.path })
        assertNull(groups.chatsProblem)

        // Near a place: the place goes with the question, and the core says how far.
        server.sent.clear()
        val near = groups.find(Coordinate(48.137, 11.575), "")
        assertEquals("Munich International Paragliders", near.first().group.name)
        assertTrue(checkNotNull(near.first().distance).isNotBlank())
        assertTrue(server.sent.single().query, server.sent.single().query.startsWith("lat=48.1370&lon=11.5750"))
        // By name or place: everywhere, without a place.
        server.sent.clear()
        groups.find(null, "Rosenheim")
        assertEquals("q=Rosenheim", server.sent.single().query)

        // A group made: the answer is read as made, and the group is asked for after.
        server.sent.clear()
        val made = groups.create(
            CommunityDraft("Zweite Mustergruppe", "", HomeDraft(Coordinate(47.7, 12.1), 30.0, "Inn valley")),
            isPublic = true,
            picture = null,
        )
        assertEquals("Mustergruppe", made.group.name)
        val posted = server.sent.first()
        assertEquals("POST" to "/v1/groups", posted.method to posted.path)
        val said = Json.parseToJsonElement(checkNotNull(posted.body).decodeToString()).jsonObject
        assertEquals("true", said.getValue("public").jsonPrimitive.content)
        assertEquals("Inn valley", said.getValue("home").jsonObject.getValue("place").jsonPrimitive.content)
        assertEquals("GET" to "/v1/groups/0123456789abcdefghjkmnpqrs", server.sent[1].let { it.method to it.path })
    }

    @Test
    fun aChatOpensOnItsNewestPageAsksWhatIsNewAndPosts() = runBlocking {
        val server = Server()
        val groups = signedIn(server)
        val chat = groups.chat(GROUP)

        chat.loadNewest()
        assertTrue(chat.hasLoaded)
        // The oldest first, as the screen shows them; the flight's link a card.
        assertEquals(listOf("13", "14"), chat.log.messages.map { it.cursor })
        assertEquals("flight", chat.log.messages.first().cards.single().kind)
        assertEquals("14", chat.log.since)

        // What is new is asked with the last answer's `since`, and laid in by its place.
        server.sent.clear()
        server.since = """{"messages": [${message("16", "Later").decodeToString()}, ${message("15", "Earlier").decodeToString()}], "since": "16", "more": false}"""
            .toByteArray()
        chat.loadNew()
        assertEquals("since=14", server.sent.single().query)
        assertEquals(listOf("13", "14", "15", "16"), chat.log.messages.map { it.cursor })
        assertEquals("16", chat.log.since)

        // Seen: marked read up to it, and only forward.
        server.sent.clear()
        chat.saw(chat.log.messages[2])
        assertEquals("PATCH", server.sent.single().method)
        server.sent.clear()
        chat.saw(chat.log.messages[1])
        assertTrue(server.sent.isEmpty())

        // A message posted: the core finds Venti's link in its text and sends it as a link.
        chat.post("Look https://venti.fly/flights/fbyw03atd9qdt5bfe177ra1k94", emptyList())
        val body = Json.parseToJsonElement(checkNotNull(server.sent.single().body).decodeToString()).jsonObject
        assertEquals("flight", body.getValue("links").jsonArray.single().jsonObject.getValue("kind").jsonPrimitive.content)
        assertEquals(listOf("13", "14", "15", "16", "17"), chat.log.messages.map { it.cursor })
        // Posting moves the pilot's own mark: their own message asks nothing.
        server.sent.clear()
        chat.saw(chat.log.messages.last())
        assertTrue(server.sent.isEmpty())
    }

    @Test
    fun theLogKeepsEachMessageOnceAsLastReadInTheOrderOfTheChat() {
        val core = fly.venti.core.Core(TestSupport.newFolder("log").path)
        fun page(vararg messages: Pair<String, String>, since: String = "9", older: String? = null): ChatPage {
            val json = """{"messages": [${messages.joinToString(",") { (cursor, text) -> message(cursor, text).decodeToString() }}],
                "since": "$since", "more": false${older?.let { ", \"older\": \"$it\"" } ?: ""}}"""
            return core.chatPage(200u, json.toByteArray())
        }
        var log = ChatLog().takeNewest(page("5" to "five", "4" to "four", older = "4"))
        assertEquals(listOf("4", "5"), log.messages.map { it.cursor })
        assertEquals("4", log.older)
        log = log.takeOlder(page("3" to "three", "2" to "two"))
        assertEquals(listOf("2", "3", "4", "5"), log.messages.map { it.cursor })
        assertNull(log.older)
        // A message changed since: shown as it is now, in its place.
        log = log.takeNew(page("3" to "three, edited", since = "12"))
        assertEquals(4, log.messages.size)
        assertEquals("three, edited", log.messages[1].text)
        assertEquals("12", log.since)
        // The cursor is a number, not text: 10 comes after 9.
        log = log.takeNew(page("10" to "ten", "9" to "nine"))
        assertEquals(listOf("2", "3", "4", "5", "9", "10"), log.messages.map { it.cursor })
    }

    @Test
    fun theComposerSaysHowManyLinksBecomeCards() {
        assertNull(GroupWords.links(messageLinks("No link here").size))
        assertEquals("The link shows as a card.", GroupWords.links(messageLinks("https://venti.fly/pilots/mara").size))
        assertEquals(
            "2 links show as cards.",
            GroupWords.links(messageLinks("https://venti.fly/pilots/mara and https://venti.fly/pilots/jonas").size),
        )
        assertEquals("Report the picture", GroupWords.reportPicture(0, 1))
        assertEquals("Report picture 2", GroupWords.reportPicture(1, 3))
        assertEquals("30 km around Inn valley", GroupWords.home(30.0, "Inn valley"))
    }

    private companion object {
        const val SERVER = "https://venti.example.org"
        const val GROUP = "0000000000000000000000g002"
        const val NO_CHANGES = """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"""

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

        fun message(cursor: String, text: String): ByteArray = """{
            "id": "0000000000000000000g002m${cursor.padStart(2, '0')}",
            "group": "$GROUP",
            "cursor": "$cursor",
            "author": {"handle": "demo", "display_name": "Demo Pilot"},
            "kind": "text",
            "text": "$text",
            "created_at": "2026-11-18T18:00:00Z"
        }""".toByteArray()
    }
}
