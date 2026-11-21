package fly.venti.app.alerts

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.account.AccountModel
import fly.venti.app.account.MemorySessionStore
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.FiredLine
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList

/**
 * The alarms against the real core, with the server played by the answers
 * the real server's own tests keep: the list, what fired, an alarm saved.
 */
class AlertsModelTest {
    private class Sent(val method: String, val path: String, val body: ByteArray?)

    private class Server {
        val sent: MutableList<Sent> = CopyOnWriteArrayList<Sent>()

        /** Whether what fired has been fetched once: the next time nothing is new. */
        var firedOnce = false

        @Suppress("UNUSED_PARAMETER", "RedundantSuspendModifier")
        suspend fun answer(
            method: String,
            url: String,
            headers: List<Pair<String, String>>,
            body: ByteArray?,
            answerWithinMilliseconds: Int,
        ): HttpAnswer {
            val path = url.removePrefix(SERVER)
            sent += Sent(method, path, body)
            return when {
                method == "POST" && path == "/v1/sessions" -> HttpAnswer(201, recorded("sign_in.json"))
                method == "GET" && path.startsWith("/v1/flights/changes") ->
                    HttpAnswer(200, """{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}""".toByteArray())
                method == "GET" && path.startsWith("/v1/account/kept-sites/changes") ->
                    HttpAnswer(200, """{"kept": [], "forgotten": [], "cursor": "1", "more": false, "reset": false}""".toByteArray())
                method == "GET" && path.startsWith("/v1/account/alerts/fired") -> if (firedOnce) {
                    HttpAnswer(200, """{"fired": [], "cursor": "f2", "more": false}""".toByteArray())
                } else {
                    firedOnce = true
                    HttpAnswer(200, recorded("alerts_fired.json"))
                }
                method == "GET" && path == ALERTS -> HttpAnswer(200, recorded("alerts.json"))
                method == "POST" && path == ALERTS -> HttpAnswer(201, recorded("alert.json"))
                method == "DELETE" && path.startsWith("$ALERTS/") -> HttpAnswer(204, ByteArray(0))
                else -> HttpAnswer(404, """{"error": {"code": "not_found", "message": "x"}}""".toByteArray())
            }
        }
    }

    private val logbook = LogbookModel.open(TestSupport.newFolder("alerts"), TestSupport.files)

    private fun account(server: Server): AccountModel =
        logbook.makeAccount(MemorySessionStore(), device = "Sample Phone", transport = server::answer)

    @Test
    fun signingInFetchesTheAlarmsAndSaysWhatFiredOnce() = runBlocking {
        val server = Server()
        val account = account(server)
        val fetched = mutableListOf<List<FiredLine>>()
        account.onAlertsFetched = { fetched += it }
        account.signIn(SERVER, "demo", "a password of the test")
        assertTrue(account.isSignedIn)

        // The sign-in's sync fetched the list and what fired: one line, new.
        assertEquals(1, fetched.size)
        assertEquals(listOf("Patscherkofel north"), fetched[0].map { it.title })
        assertTrue(fetched[0][0].line.startsWith("Patscherkofel: 15 km/h"))

        // The phone keeps them: the list and what fired show from its copy.
        val alerts = logbook.makeAlerts(account, clock = { NOW_MILLIS })
        alerts.read()
        assertEquals("Patscherkofel north", alerts.held.first().name)
        assertEquals(1, alerts.fired.size)

        // The next run finds nothing new: no second notification.
        assertEquals(emptyList<FiredLine>(), account.syncAlerts())
    }

    @Test
    fun aDraftIsCheckedBeforeItIsSentAndSavedOnTheAccount() = runBlocking {
        val server = Server()
        val account = account(server)
        account.signIn(SERVER, "demo", "a password of the test")
        val alerts = logbook.makeAlerts(account, clock = { NOW_MILLIS })

        val draft = AlertsModel.windDraft("geosphere-tawes:11126", "Patscherkofel")
        // Nothing chosen: the core says what the alarm needs, at the directions.
        val faults = alerts.faults(draft)
        assertEquals(listOf("rule.directions"), faults.map { it.field })
        val chosen = draft.copy(fromPoint = 0u, toPoint = 2u, minKmh = 10u, maxKmh = 20u)
        assertEquals(emptyList<Any>(), alerts.faults(chosen))
        assertEquals("Wind at Patscherkofel from N to NE, 10 to 20 km/h", alerts.line(chosen))

        assertNull(alerts.save(null, chosen))
        val added = server.sent.single { it.method == "POST" && it.path == ALERTS }
        assertNotNull(added.body)
        // Saving fetched the list again.
        assertTrue(server.sent.indexOf(added) < server.sent.indexOfLast { it.method == "GET" && it.path == ALERTS })

        assertNull(alerts.remove("0000000000000000000000a001"))
        assertTrue(server.sent.any { it.method == "DELETE" && it.path == "$ALERTS/0000000000000000000000a001" })
    }

    @Test
    fun aDayDraftForATakeoffIsTodayAndTomorrow() {
        val draft = AlertsModel.dayDraft("site-1", "Brauneck", 47.66, 11.53)
        assertEquals("A cross-country day or better at Brauneck, today or tomorrow", fly.venti.core.alertLine(draft))
    }

    @Test
    fun aStepperStartsFromAnyAndGoesBackToIt() {
        assertEquals(10, AlertSteps.more(null, 5, 10))
        assertEquals(15, AlertSteps.more(10, 5, 10))
        assertEquals(5, AlertSteps.less(10, 5, 0))
        assertEquals(0, AlertSteps.less(5, 5, 0))
        assertNull(AlertSteps.less(0, 5, 0))
        assertNull(AlertSteps.less(null, 5, 0))
    }

    private companion object {
        const val SERVER = "https://venti.example.org"
        const val ALERTS = "/v1/account/alerts"

        /** 2026-11-21 09:05 UTC: three days after what fired. */
        const val NOW_MILLIS = 1_795_251_900_000L

        fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()
    }
}
