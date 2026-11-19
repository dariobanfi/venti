package fly.venti.app.assistant

import fly.venti.core.AssistantRequest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.net.HttpURLConnection
import java.net.URL

/** Sends the assistant's requests with the platform's own networking. */
object AssistantHttp {
    private const val CONNECT_MILLISECONDS = 30_000

    /** A long answer takes a model a while to write. */
    private const val ANSWER_MILLISECONDS = 120_000

    /**
     * Posts the request and returns the status and the body, whatever the
     * status. Throws an `IOException` if the provider cannot be reached.
     *
     * Nothing is cached and no cookies are kept: nothing about a
     * conversation stays in the app outside the conversation itself.
     */
    suspend fun send(request: AssistantRequest): ProviderAnswer = withContext(Dispatchers.IO) {
        val connection = URL(request.url).openConnection() as HttpURLConnection
        try {
            connection.requestMethod = "POST"
            connection.connectTimeout = CONNECT_MILLISECONDS
            connection.readTimeout = ANSWER_MILLISECONDS
            connection.useCaches = false
            connection.doOutput = true
            for (header in request.headers) {
                connection.setRequestProperty(header.name, header.value)
            }
            connection.outputStream.use { it.write(request.body.toByteArray()) }

            val status = connection.responseCode
            // An error's body explains it; the core reads that too.
            val stream = if (status >= 400) connection.errorStream else connection.inputStream
            ProviderAnswer(status, stream?.use { it.readBytes() } ?: ByteArray(0))
        } finally {
            connection.disconnect()
        }
    }
}

/**
 * A stand-in for the model provider, for tests: it answers the assistant's
 * requests from a script, in the provider's own format.
 *
 * The first request of a question is answered with a call to the
 * flight-list tool, so that the real tool loop runs against the real
 * logbook; the request that carries the tool's result is answered with a
 * sentence. Nothing in the app uses this unless it is started with the
 * "scripted-assistant" switch, which only a debug build listens to.
 */
object ScriptedProvider {
    const val REPLY = "Your logbook has one flight in 2026: 29 minutes on 4 July, with two thermals. " +
        "The first was the stronger, at **2.4 m/s** in a westerly of about 11 km/h."

    @Suppress("RedundantSuspendModifier")
    suspend fun answer(request: AssistantRequest): ProviderAnswer {
        val hasToolResult = request.body.contains("\"tool_result\"")
        val content: JsonObject = if (hasToolResult) {
            buildJsonObject {
                put("type", "text")
                put("text", REPLY)
            }
        } else {
            buildJsonObject {
                put("type", "tool_use")
                put("id", "scripted_1")
                put("name", "list_flights")
                put("input", buildJsonObject { put("year", 2026) })
            }
        }
        val body = buildJsonObject {
            put("type", "message")
            put("role", "assistant")
            put("content", buildJsonArray { add(content) })
            put("stop_reason", if (hasToolResult) "end_turn" else "tool_use")
        }
        return ProviderAnswer(200, body.toString().toByteArray())
    }
}
