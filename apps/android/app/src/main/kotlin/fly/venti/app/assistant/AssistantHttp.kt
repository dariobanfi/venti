package fly.venti.app.assistant

import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.core.AssistantRequest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Sends the assistant's requests with the platform's own networking. */
object AssistantHttp {
    /** A long answer takes a model a while to write. */
    private const val ANSWER_MILLISECONDS = 120_000

    /**
     * Posts the request and returns the status and the body, whatever the
     * status. Throws an `IOException` if the provider cannot be reached.
     */
    suspend fun send(request: AssistantRequest): HttpAnswer =
        Http.post(request.url, request.headers.map { it.name to it.value }, request.body, ANSWER_MILLISECONDS)
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
    suspend fun answer(request: AssistantRequest): HttpAnswer {
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
        return HttpAnswer(200, body.toString().toByteArray())
    }
}
