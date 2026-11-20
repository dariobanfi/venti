package fly.venti.app.groups

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.core.Coordinate
import fly.venti.app.account.AccountModel
import fly.venti.app.account.MemorySessionStore
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.GroupMember
import fly.venti.core.PlanDraft
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.IOException
import java.time.LocalDate
import java.util.Collections
import java.util.Locale

/**
 * Groups and their plans for a flying day, against the real core and a
 * server played by the test with the answers the server's own tests keep.
 */
class GroupsModelTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    /** One request as it was sent. */
    private class Sent(val method: String, val url: String, val headers: Map<String, String>, val body: ByteArray?) {
        val path: String get() = url.removePrefix(SERVER).substringBefore('?')
    }

    /** A server that signs anyone in as the demo pilot and answers about groups and plans as the real one does. */
    private class Server {
        val sent: MutableList<Sent> = Collections.synchronizedList(mutableListOf<Sent>())

        /** Answers in place of the usual ones, where a test wants a server in a particular mood. */
        var instead: (Sent) -> HttpAnswer? = { null }

        @Suppress("UNUSED_PARAMETER")
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
            val path = request.path
            return when {
                method == "POST" && path == "/v1/sessions" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "GET" && path == "/v1/flights/changes" -> HttpAnswer(200, NO_CHANGES.toByteArray())
                // The board: the plans of every group the pilot is in.
                method == "GET" && path == "/v1/groups/plans" -> HttpAnswer(200, recorded("plan_board.json"))
                // A plan, and the pilot's word on one.
                path.endsWith("/in") -> HttpAnswer(200, recorded("plan.json"))
                "/plans" in path && "/groups/" in path && method == "DELETE" -> HttpAnswer(204, ByteArray(0))
                "/plans" in path && "/groups/" in path -> HttpAnswer(if (method == "POST") 201 else 200, recorded("plan.json"))
                // Groups.
                method == "POST" && path == "/v1/groups" -> HttpAnswer(201, recorded("group_created.json"))
                method == "POST" && path.endsWith("/join") -> HttpAnswer(200, recorded("group_joined.json"))
                method == "POST" && path.endsWith("/invite") -> HttpAnswer(200, recorded("group_invite.json"))
                method == "DELETE" && path.startsWith("/v1/groups/") -> HttpAnswer(204, ByteArray(0))
                path.startsWith("/v1/groups/") -> HttpAnswer(200, recorded("group.json"))
                else -> refusal(404, "not_found")
            }
        }

        fun methods(): List<String> = sent.map { it.method }
    }

    /** A signed-in pilot's groups, on a day of the test's choosing. */
    private suspend fun signedIn(server: Server, today: LocalDate = TODAY): Pair<AccountModel, GroupsModel> {
        val logbook = LogbookModel.open(TestSupport.newFolder("groups"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        val groups = logbook.makeGroups(account) { today }
        account.signIn(SERVER, "demo", "a password of the test")
        server.sent.clear()
        return account to groups
    }

    @Test
    fun theBoardIsAskedForFromThePilotsOwnTodayAndNeedsAnAccount() = runBlocking {
        val server = Server()
        val logbook = LogbookModel.open(TestSupport.newFolder("groups"), TestSupport.files)
        val account = logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)
        val groups = logbook.makeGroups(account) { TODAY }

        // Signed out there is nothing to ask, and that is no problem.
        groups.load()
        assertNull(groups.board)
        assertNull(groups.problem)
        assertTrue(server.sent.isEmpty())
        assertTrue(groups.groups.isEmpty())

        account.signIn(SERVER, "demo", "a password of the test")
        server.sent.clear()
        groups.load()

        // Asked of the account's server with its token, from the pilot's
        // own today on: the phone knows the day and the server does not.
        val asked = server.sent.single()
        assertEquals("GET", asked.method)
        assertEquals("$SERVER/v1/groups/plans?from=2026-11-18", asked.url)
        assertTrue(checkNotNull(asked.headers["Authorization"]).startsWith("Bearer "))
        assertFalse(groups.isLoading)
        assertNull(groups.problem)

        // The demo pilot keeps the one group, and its one plan is the
        // pilot's own, for today, with the pilot in on it.
        val board = checkNotNull(groups.board)
        val group = board.groups.single()
        assertEquals("Mustergruppe", group.name)
        assertTrue(group.isKeptByMe)
        assertEquals("3 members · kept by you", group.summary)
        assertEquals(listOf(group), groups.groups)
        val day = board.days.single()
        assertEquals("2026-11-18", day.day)
        assertEquals("Today", PlanWords.title(day.day, TODAY))
        val plan = day.plans.single()
        assertEquals("Musterberg, Startplatz Ost", plan.place)
        assertEquals("10:30", plan.meetAt)
        assertEquals("Mustergruppe", plan.groupName)
        assertTrue(plan.isMine)
        assertTrue(plan.canDelete)
        assertTrue(plan.amIn)
        assertEquals("You, Mara Muster and Jonas Beispiel", plan.inWords)
        assertEquals(plan, groups.plan(plan.id))
        assertNull(groups.plan("no such plan"))
        // A row of the board, for someone who cannot see it.
        assertEquals("At 10:30. You, Mara Muster and Jonas Beispiel. Mustergruppe. You are in.", PlanWords.spoken(plan))

        // Today's plan says where it is: the map marks it, with its place
        // and time. Seen from the day before it is tomorrow's, still
        // marked; from two days before it is in the board only.
        val pin = groups.pins(TODAY).single()
        assertEquals(plan.id, pin.id)
        assertEquals(checkNotNull(plan.position), pin.position)
        assertEquals("Musterberg, Startplatz Ost 10:30", pin.label)
        assertEquals(1, groups.pins(TODAY.minusDays(1)).size)
        assertTrue(groups.pins(TODAY.minusDays(2)).isEmpty())
        assertTrue(groups.pins(TODAY.plusDays(1)).isEmpty())
    }

    @Test
    fun plansAtOnePlaceAreOneMarkWithTheirWordsOneUnderTheOther() {
        val takeoff = Coordinate(lat = 47.66, lon = 11.80)
        val elsewhere = Coordinate(lat = 47.70, lon = 11.90)
        val pins = PlanPin.gathered(
            listOf(
                PlanPin("today", takeoff, "Startplatz Ost 10:30"),
                PlanPin("other", elsewhere, "Landeplatz"),
                PlanPin("tomorrow", takeoff, "Startplatz Ost 09:00"),
            ),
        )
        assertEquals(
            listOf(
                PlanPin("today", takeoff, "Startplatz Ost 10:30\nStartplatz Ost 09:00"),
                PlanPin("other", elsewhere, "Landeplatz"),
            ),
            pins,
        )
    }

    @Test
    fun everyChangeGoesToTheServerAndTheBoardIsAskedForAgain() = runBlocking {
        val server = Server()
        val (_, groups) = signedIn(server)
        groups.load()
        val plan = checkNotNull(groups.board).days.single().plans.single()
        val group = plan.groupId
        server.sent.clear()

        // "I am out" takes the pilot's word back.
        groups.setIn(plan, false)
        assertEquals(listOf("DELETE", "GET"), server.methods())
        assertTrue(server.sent.first().path.endsWith("/${plan.id}/in"))
        server.sent.clear()
        groups.setIn(plan, true)
        assertEquals(listOf("PUT", "GET"), server.methods())

        // A new plan goes to its group as the form holds it.
        server.sent.clear()
        groups.post(PlanDraft(day = "2026-11-21", place = "Beispielwiese", position = null, meetAt = "09:30", note = ""), group)
        assertEquals(listOf("POST", "GET"), server.methods())
        val posted = server.sent.first()
        assertTrue(posted.path, posted.path.startsWith("/v1/groups/$group/"))
        val said = Json.parseToJsonElement(checkNotNull(posted.body).decodeToString()).jsonObject
        assertEquals("Beispielwiese", said.getValue("place").jsonPrimitive.content)
        assertEquals("2026-11-21", said.getValue("day").jsonPrimitive.content)
        assertEquals("09:30", said.getValue("meet_at").jsonPrimitive.content)

        // Changing and deleting name the plan.
        server.sent.clear()
        groups.change(plan, PlanDraft(day = plan.day, place = "Musterberg West", position = plan.position, meetAt = null, note = ""))
        assertEquals(listOf("PUT", "GET"), server.methods())
        assertTrue(server.sent.first().path.endsWith("/${plan.id}"))
        server.sent.clear()
        groups.delete(plan)
        assertEquals(listOf("DELETE", "GET"), server.methods())
        assertTrue(server.sent.first().path.endsWith("/${plan.id}"))

        // A group that is made shows its invite code this once.
        server.sent.clear()
        val made = groups.make("Zweite Mustergruppe")
        assertEquals("7k3m-9q2x-4r8t-1v6w", made.inviteCode)
        assertEquals("Zweite Mustergruppe", made.group.name)
        assertEquals("POST" to "/v1/groups", server.sent.first().let { it.method to it.path })
        assertEquals("GET", server.sent.last().method)

        // Coming into one with a code says so in the core's words.
        server.sent.clear()
        val joined = groups.join("5555-5555-5555-5555")
        assertTrue(joined.words, joined.words.startsWith("You are in ") || joined.words.startsWith("You were in "))
        assertEquals(listOf("POST", "GET"), server.methods())

        // A new code is asked for and read; the board has nothing to do with it.
        server.sent.clear()
        assertEquals("2w8x-5c9p-3h7k-6n4d", groups.newInvite(group))
        assertEquals("POST" to "/v1/groups/$group/invite", server.sent.single().let { it.method to it.path })

        // A group with its members: who keeps it, and which of them is the pilot.
        server.sent.clear()
        val detail = groups.detail(group)
        assertEquals("GET" to "/v1/groups/$group", server.sent.single().let { it.method to it.path })
        assertEquals(listOf("demo", "mara", "jonas"), detail.members.map { it.handle })
        val me = detail.members.first()
        assertTrue(me.isMe && me.isKeeper)
        assertEquals("Demo Pilot (you)", GroupWords.name(me))
        assertEquals("@demo, Keeper", GroupWords.spoken(me))
        assertEquals("Mara Muster", GroupWords.name(detail.members[1]))
        assertEquals("@mara", GroupWords.spoken(detail.members[1]))

        // What a keeper does to a group and its members.
        server.sent.clear()
        groups.rename(group, "Mustergruppe Süd")
        assertEquals(listOf("PATCH", "GET"), server.methods())
        server.sent.clear()
        groups.handOver(group, "mara")
        assertEquals(listOf("PUT", "GET"), server.methods())
        server.sent.clear()
        groups.remove("jonas", from = group)
        assertEquals(listOf("DELETE", "GET"), server.methods())
        assertTrue(server.sent.first().path.endsWith("/jonas"))
        server.sent.clear()
        groups.leave(group)
        assertEquals(listOf("DELETE", "GET"), server.methods())
        server.sent.clear()
        groups.deleteGroup(group)
        assertEquals("DELETE" to "/v1/groups/$group", server.sent.first().let { it.method to it.path })
    }

    @Test
    fun whatGoesWrongIsSaidInWordsAndWhatTheCoreRefusesIsNotSent() = runBlocking {
        val server = Server()
        val (account, groups) = signedIn(server)
        groups.load()
        assertNotNull(groups.board)
        server.sent.clear()

        // A code that lets nobody in: the server's refusal in the core's words.
        server.instead = { request -> if (request.path.endsWith("/join")) refusal(403, "invalid_group_invite") else null }
        val refused = checkNotNull(groups.attempt { groups.join("0000-0000-0000-0000") })
        assertTrue(refused, refused.startsWith("This is not a group's invite code"))
        // Nothing changed, so the board is not asked for again.
        assertEquals(listOf("POST"), server.methods())
        server.instead = { request -> if (request.method == "PATCH") refusal(403, "not_group_keeper") else null }
        assertEquals("Only the group's keeper can do this.", groups.attempt { groups.rename(GROUP, "Anders") })

        // A name of nothing but spaces is refused before anything is sent.
        server.instead = { null }
        server.sent.clear()
        assertNotNull(groups.attempt { groups.make("   ") })
        assertTrue(server.sent.isEmpty())
        // And what works says nothing.
        assertNull(groups.attempt { groups.newInvite(GROUP) })

        // Without a network the board says so and keeps what it had.
        server.instead = { throw IOException("no route to host") }
        groups.load()
        assertTrue(checkNotNull(groups.problem).startsWith("The server could not be reached."))
        assertNotNull(groups.board)
        assertFalse(groups.isLoading)
        val unreached = checkNotNull(groups.attempt { groups.leave(GROUP) })
        assertTrue(unreached.startsWith("The server could not be reached."))

        // Signed out, the board is forgotten: it was never the phone's to keep.
        server.instead = { null }
        account.signOut()
        groups.load()
        assertNull(groups.board)
        assertNull(groups.problem)
        // And the core puts no request together for nobody.
        assertEquals("This device is not signed in.", groups.attempt { groups.make("Zweite Mustergruppe") })
    }

    @Test
    fun aPlansDayAndTimeAreWrittenAsTheServerWritesThemAndSaidAsAPilotWould() {
        val friday = LocalDate.of(2026, 11, 20)
        assertEquals("2026-11-20", PlanWords.day(friday))
        assertEquals(friday, PlanWords.date("2026-11-20"))
        assertNull(PlanWords.date("20.11.2026"))
        assertNull(PlanWords.date(""))

        assertEquals("Today", PlanWords.title("2026-11-20", friday))
        assertEquals("Tomorrow", PlanWords.title("2026-11-21", friday))
        assertEquals("Sunday, 22 Nov", PlanWords.title("2026-11-22", friday))
        // A day that has passed is said by its date, and what is no day as it came.
        assertEquals("Thursday, 19 Nov", PlanWords.title("2026-11-19", friday))
        assertEquals("some day", PlanWords.title("some day", friday))

        assertEquals("09:05", PlanWords.timeOfDay(9, 5))
        assertEquals("17:30", PlanWords.timeOfDay(17, 30))
        assertEquals(9 to 5, PlanWords.time("09:05"))
        assertNull(PlanWords.time("25:00"))
        assertNull(PlanWords.time("9"))
        assertNull(PlanWords.time("nine:thirty"))
    }

    @Test
    fun everythingDoneToAGroupIsAskedAboutWithWhatItMeans() {
        val mara = GroupMember(handle = "mara", name = "Mara Muster", isKeeper = false, isMe = false)
        val all = listOf(
            GroupWords.Asking.NewCode,
            GroupWords.Asking.Leave,
            GroupWords.Asking.Delete,
            GroupWords.Asking.Remove(mara),
            GroupWords.Asking.HandOver(mara),
        )
        for (asking in all) {
            assertTrue(GroupWords.question(asking).endsWith("?"))
            assertTrue(GroupWords.consequence(asking).endsWith("."))
            assertTrue(GroupWords.confirm(asking).isNotBlank())
        }
        assertEquals("Make a new invite code?", GroupWords.question(GroupWords.Asking.NewCode))
        assertEquals("Remove Mara Muster from the group?", GroupWords.question(GroupWords.Asking.Remove(mara)))
        assertEquals("Make Mara Muster the keeper?", GroupWords.question(GroupWords.Asking.HandOver(mara)))
        // Removing a member ends the invite code, which they know.
        assertTrue(GroupWords.consequence(GroupWords.Asking.Remove(mara)).contains("invite code"))
        // What takes something away for good is set in the warning colour.
        assertEquals(
            listOf(false, true, true, true, false),
            all.map(GroupWords::isDestructive),
        )
    }

    private companion object {
        const val SERVER = "https://venti.example.org"

        /** The development mode's group that the demo pilot keeps. */
        const val GROUP = "44444444444444444444444444"

        /** The day the recorded answers were made on: their plans are for it. */
        val TODAY: LocalDate = LocalDate.of(2026, 11, 18)

        const val NO_CHANGES = """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"""

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

        fun refusal(status: Int, code: String) =
            HttpAnswer(status, """{"error": {"code": "$code", "message": "x"}}""".toByteArray())
    }
}
