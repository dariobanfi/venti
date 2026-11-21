package fly.venti.app

import java.io.Closeable
import java.io.IOException
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URL
import java.util.concurrent.Executors

/**
 * A server the test plays on the device itself, in front of the stand-in
 * with a made-up day: it hands what the app asks on and the answer back
 * (every request as a GET, which is all a map without an account sends),
 * but the hours of the forecast map (`GET /v1/thermals/hours`) it holds
 * back until the test says what becomes of them. A server that answers at once
 * never shows the card waiting, and the stand-in refuses nothing the app
 * asks; this one does both, so that both can be looked at.
 *
 * The app under test runs in the test's own process and reaches it at
 * `http://127.0.0.1:<port>`, which it takes for a server on the pilot's
 * own network, as it does the emulator's `10.0.2.2`.
 */
class HoursHeldBack(private val behind: String) : Closeable {
    /** What becomes of a request for the hours. */
    enum class Hours {
        /** It waits, as with a server that is slow to answer. */
        HELD,

        /** It is answered with 503 and nothing else, as by a server that is being restarted. */
        REFUSED,

        /** It is handed on to the stand-in, as every other request is. */
        HANDED_ON,
    }

    @Volatile
    var hours = Hours.HELD

    // By its number: Android's own loopback address is the IPv6 one, which "127.0.0.1" does not reach.
    private val listening = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
    private val workers = Executors.newCachedThreadPool()

    /** Where the app finds this server, without a slash at its end. */
    val address: String = "http://127.0.0.1:${listening.localPort}"

    init {
        workers.execute {
            while (!listening.isClosed) {
                val socket = try {
                    listening.accept()
                } catch (_: IOException) {
                    break
                }
                workers.execute { runCatching { socket.use(::answer) } }
            }
        }
    }

    private fun answer(socket: Socket) {
        val asked = socket.getInputStream().bufferedReader(Charsets.ISO_8859_1)
        // "GET /v1/thermals/hours?date=… HTTP/1.1", then headers this server has no use for.
        val target = asked.readLine()?.split(" ")?.getOrNull(1) ?: return
        while (!asked.readLine().isNullOrEmpty()) Unit

        val forHours = target.substringBefore("?") == "/v1/thermals/hours"
        if (forHours) {
            while (hours == Hours.HELD && !listening.isClosed) Thread.sleep(50)
        }
        var status = 503
        var type: String? = null
        var body = ByteArray(0)
        if (!forHours || hours == Hours.HANDED_ON) {
            try {
                val connection = URL(behind + target).openConnection() as HttpURLConnection
                try {
                    connection.connectTimeout = 10_000
                    connection.readTimeout = 30_000
                    connection.setRequestProperty("Accept", "application/json")
                    status = connection.responseCode
                    type = connection.contentType
                    body = (if (status >= 400) connection.errorStream else connection.inputStream)?.use { it.readBytes() } ?: ByteArray(0)
                } finally {
                    connection.disconnect()
                }
            } catch (_: IOException) {
                status = 502
            }
        }
        val head = buildString {
            append("HTTP/1.1 $status Answered\r\n")
            if (type != null) append("Content-Type: $type\r\n")
            append("Content-Length: ${body.size}\r\n")
            append("Connection: close\r\n\r\n")
        }
        socket.getOutputStream().apply {
            write(head.toByteArray(Charsets.ISO_8859_1))
            write(body)
            flush()
        }
    }

    override fun close() {
        listening.close()
        workers.shutdownNow()
    }
}
