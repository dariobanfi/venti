package fly.venti.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.IOException
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.URI
import java.net.URL

/** What a server answered: the HTTP status and the body. */
class HttpAnswer(val status: Int, val body: ByteArray)

/**
 * The app's requests to servers, with the platform's own networking: the
 * assistant's to its provider, and the wind map's to a Venti server. What
 * to ask and how to read the answer is the core's; this sends the bytes.
 *
 * Nothing is cached and no cookies are kept.
 */
object Http {
    private const val CONNECT_MILLISECONDS = 20_000

    /** Fetches an address and returns the answer, whatever its status. */
    suspend fun get(url: String, answerWithinMilliseconds: Int = 20_000): HttpAnswer =
        request(url, answerWithinMilliseconds) { connection ->
            connection.requestMethod = "GET"
            connection.setRequestProperty("Accept", "application/json")
        }

    /** Posts a body with the given headers and returns the answer, whatever its status. */
    suspend fun post(
        url: String,
        headers: List<Pair<String, String>>,
        body: String,
        answerWithinMilliseconds: Int,
    ): HttpAnswer =
        request(url, answerWithinMilliseconds) { connection ->
            connection.requestMethod = "POST"
            connection.doOutput = true
            for ((name, value) in headers) {
                connection.setRequestProperty(name, value)
            }
            connection.outputStream.use { it.write(body.toByteArray()) }
        }

    private suspend fun request(
        url: String,
        answerWithinMilliseconds: Int,
        send: (HttpURLConnection) -> Unit,
    ): HttpAnswer = withContext(Dispatchers.IO) {
        if (!mayBeAsked(url)) {
            throw IOException("Without encryption the app only reaches a server on your own network.")
        }
        val connection = URL(url).openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = CONNECT_MILLISECONDS
            connection.readTimeout = answerWithinMilliseconds
            connection.useCaches = false
            // An address is asked as written. Following a redirect could
            // lead from the pilot's own network to an unencrypted request
            // somewhere else.
            connection.instanceFollowRedirects = false
            send(connection)
            val status = connection.responseCode
            // An error's body explains it; the core reads that too.
            val stream = if (status >= 400) connection.errorStream else connection.inputStream
            HttpAnswer(status, stream?.use { it.readBytes() } ?: ByteArray(0))
        } finally {
            connection.disconnect()
        }
    }

    /**
     * Whether the app sends a request to this address at all: over HTTPS
     * anywhere, without encryption only to the pilot's own network.
     *
     * Android can switch unencrypted HTTP on for named hosts or for all of
     * them, not for "the local network", so the manifest allows it and this
     * is where the line is drawn.
     */
    fun mayBeAsked(url: String): Boolean {
        val address = try {
            URI(url)
        } catch (_: java.net.URISyntaxException) {
            return false
        }
        val host = address.host?.removeSurrounding("[", "]").orEmpty()
        return when (address.scheme?.lowercase()) {
            "https" -> host.isNotEmpty()
            "http" -> isOnOwnNetwork(host)
            else -> false
        }
    }

    /**
     * Whether a host is on the pilot's own network, as far as its name
     * tells: this device, a name without a dot or ending in `.local`, or an
     * address from the ranges set aside for private networks.
     */
    fun isOnOwnNetwork(host: String): Boolean {
        val name = host.lowercase().trimEnd('.')
        if (name.isEmpty()) return false
        if (name == "localhost" || name.endsWith(".local")) return true
        val looksLikeAnAddress = ':' in name || name.all { it.isDigit() || it == '.' }
        if (!looksLikeAnAddress) return '.' !in name
        // An address written in numbers is read without asking any server.
        val address = try {
            InetAddress.getByName(name)
        } catch (_: java.net.UnknownHostException) {
            return false
        }
        val bytes = address.address
        // fc00::/7, the private range of the newer addresses, is one Java has no question for.
        val isUniqueLocal = bytes.size == 16 && (bytes[0].toInt() and 0xFE) == 0xFC
        return address.isLoopbackAddress || address.isSiteLocalAddress || address.isLinkLocalAddress || isUniqueLocal
    }
}
