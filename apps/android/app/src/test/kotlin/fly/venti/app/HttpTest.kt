package fly.venti.app

import com.sun.net.httpserver.HttpServer
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import java.io.IOException
import java.net.InetSocketAddress
import java.util.concurrent.atomic.AtomicInteger

/**
 * The app's requests to servers: which addresses it asks at all, and what
 * it hands back. A small server on this machine plays the other end.
 */
class HttpTest {
    private lateinit var server: HttpServer
    private val elsewhereAsked = AtomicInteger()

    @Before
    fun startAServer() {
        server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/ok") { exchange ->
            val accepts = exchange.requestHeaders.getFirst("Accept").orEmpty()
            val body = """{"accepts":"$accepts"}""".toByteArray()
            exchange.sendResponseHeaders(200, body.size.toLong())
            exchange.responseBody.use { it.write(body) }
        }
        server.createContext("/refused") { exchange ->
            val body = """{"error":"no"}""".toByteArray()
            exchange.sendResponseHeaders(400, body.size.toLong())
            exchange.responseBody.use { it.write(body) }
        }
        server.createContext("/moved") { exchange ->
            exchange.responseHeaders.add("Location", "$base/elsewhere")
            exchange.sendResponseHeaders(302, -1)
            exchange.close()
        }
        server.createContext("/elsewhere") { exchange ->
            elsewhereAsked.incrementAndGet()
            exchange.sendResponseHeaders(200, -1)
            exchange.close()
        }
        server.start()
    }

    @After
    fun stopTheServer() {
        server.stop(0)
    }

    private val base: String get() = "http://127.0.0.1:${server.address.port}"

    @Test
    fun anAnswerComesBackWithItsStatusWhateverItIs() = runBlocking {
        val good = Http.get("$base/ok")
        assertEquals(200, good.status)
        assertEquals("""{"accepts":"application/json"}""", good.body.decodeToString())

        // An error's body explains it; the core reads that too.
        val refused = Http.get("$base/refused")
        assertEquals(400, refused.status)
        assertEquals("""{"error":"no"}""", refused.body.decodeToString())
    }

    @Test
    fun anAddressIsAskedAsWrittenAndRedirectsAreNotFollowed() = runBlocking {
        val moved = Http.get("$base/moved")
        assertEquals(302, moved.status)
        assertEquals(0, elsewhereAsked.get())
    }

    @Test
    fun withoutEncryptionNothingLeavesThePilotsOwnNetwork() = runBlocking {
        try {
            Http.get("http://wind.example.org/v1/health")
            fail("the request was sent")
        } catch (error: IOException) {
            assertEquals("Without encryption the app only reaches a server on your own network.", error.message)
        }
    }

    @Test
    fun whichAddressesMayBeAsked() {
        assertTrue(Http.mayBeAsked("https://wind.example.org/v1/health"))
        assertTrue(Http.mayBeAsked("https://8.8.8.8/"))
        assertTrue(Http.mayBeAsked("http://192.168.0.7:8080/v1/health"))
        assertTrue(Http.mayBeAsked("http://10.0.2.2:8080/v1/health"))
        assertTrue(Http.mayBeAsked("http://venti.local/v1/health"))
        assertTrue(Http.mayBeAsked("HTTP://LOCALHOST:8080/"))
        assertFalse(Http.mayBeAsked("http://wind.example.org/v1/health"))
        assertFalse(Http.mayBeAsked("http://8.8.8.8/"))
        assertFalse(Http.mayBeAsked("ftp://192.168.0.7/"))
        assertFalse(Http.mayBeAsked("file:///etc/hosts"))
        assertFalse(Http.mayBeAsked("not an address"))
        assertFalse(Http.mayBeAsked("https:///v1/health"))
    }

    @Test
    fun whatCountsAsThePilotsOwnNetwork() {
        for (own in listOf(
            "localhost", "127.0.0.1", "::1", "10.1.2.3", "172.16.0.1", "172.31.255.254", "192.168.178.20",
            "169.254.3.4", "fe80::1", "fd00::1", "fc00::1", "raspberrypi", "venti.local", "Venti.Local.",
        )) {
            assertTrue(own, Http.isOnOwnNetwork(own))
        }
        for (other in listOf(
            "", "example.org", "wind.example.org", "8.8.8.8", "172.15.0.1", "172.32.0.1", "192.169.0.1",
            "11.0.0.1", "2001:db8::1", "localhost.example.org", "10.0.0.1.example.org",
        )) {
            assertFalse(other, Http.isOnOwnNetwork(other))
        }
    }
}
