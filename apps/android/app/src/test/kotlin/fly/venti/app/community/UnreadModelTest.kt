package fly.venti.app.community

import fly.venti.app.HttpAnswer
import fly.venti.core.ServerRequest
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.util.concurrent.CopyOnWriteArrayList

/** The Community tab's badge: the server's marker, read and moved through the core. */
class UnreadModelTest {
    private class Server(var signedIn: Boolean = true) {
        val asked = CopyOnWriteArrayList<ServerRequest>()
        var answer: String = """{"notifications":2,"alarms":1,"total":3}"""
        var fails = false
        val model = UnreadModel(
            send = { request ->
                asked += request
                if (fails) throw IOException("offline")
                HttpAnswer(200, answer.toByteArray())
            },
            isSignedIn = { signedIn },
        )
    }

    @Test
    fun theBadgeIsTheServersCountInTheCoresWords() = runBlocking {
        val server = Server()
        assertNull(server.model.badge)
        server.model.read()
        assertEquals("GET", server.asked.single().method)
        assertEquals("3", server.model.badge)
        assertEquals("3 new", server.model.label)

        // The groups' unread messages stand beside it, and the tab has one badge.
        server.answer = """{"notifications":2,"alarms":1,"total":3,"groups":4}"""
        server.model.read()
        assertEquals("7", server.model.badge)
        assertEquals("7 new", server.model.label)

        // Nothing new: no badge.
        server.answer = """{"notifications":0,"alarms":0,"total":0}"""
        server.model.read()
        assertNull(server.model.badge)
        assertNull(server.model.label)
    }

    @Test
    fun aFailedReadKeepsTheBadgeAndSigningOutTakesItOff() = runBlocking {
        val server = Server()
        server.model.read()
        server.fails = true
        server.model.read()
        assertEquals("3", server.model.badge)
        server.signedIn = false
        server.model.read()
        assertNull(server.model.badge)
        // Signed out nothing is asked.
        assertEquals(2, server.asked.size)
    }

    @Test
    fun openingAListMovesOnlyItsOwnMark() = runBlocking {
        val server = Server()
        server.answer = """{"notifications":0,"alarms":1,"total":1}"""
        server.model.sawNotifications(upTo = "17")
        val seen = server.asked.single()
        assertEquals("PUT", seen.method)
        val body = String(checkNotNull(seen.body))
        // The other list is left out: its mark stays where it is.
        assertTrue(body, body.contains("\"notifications_cursor\":\"17\""))
        assertTrue(body, !body.contains("alarms_cursor"))
        assertEquals("1", server.model.badge)

        server.model.sawAlarms(upTo = "5")
        val alarms = String(checkNotNull(server.asked.last().body))
        assertTrue(alarms, !alarms.contains("notifications_cursor"))
        assertTrue(alarms, alarms.contains("\"alarms_cursor\":\"5\""))
    }

    @Test
    fun aBadgeStopsAt99() {
        assertNull(UnreadModel.badge(0u))
        assertEquals("99", UnreadModel.badge(99u))
        assertEquals("99+", UnreadModel.badge(100u))
    }
}
