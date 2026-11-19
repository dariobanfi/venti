package fly.venti.app.assistant

import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.core.AssistantRequest
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
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
 * sentence. A question about the wind on the Zugspitze is answered with a
 * call to the wind tool instead, and then with a sentence made from what
 * the tool found. Nothing in the app uses this unless it is started with
 * the "scripted-assistant" switch, which only a debug build listens to.
 */
object ScriptedProvider {
    const val REPLY = "Your logbook has one flight in 2026: 29 minutes on 4 July, with two thermals. " +
        "The first was the stronger, at **2.4 m/s** in a westerly of about 11 km/h."

    @Suppress("RedundantSuspendModifier")
    suspend fun answer(request: AssistantRequest): HttpAnswer {
        val hasToolResult = request.body.contains("\"tool_result\"")
        val asksForWind = request.body.contains("wind on the Zugspitze")
        val content: JsonObject = when {
            hasToolResult -> buildJsonObject {
                put("type", "text")
                put("text", if (asksForWind) windReply(request.body) else REPLY)
            }
            asksForWind -> buildJsonObject {
                put("type", "tool_use")
                put("id", "scripted_1")
                put("name", "wind_stations")
                put(
                    "input",
                    buildJsonObject {
                        put("latitude", 47.42)
                        put("longitude", 10.99)
                        put("radius_km", 30)
                    },
                )
            }
            else -> buildJsonObject {
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

    /**
     * A sentence about the nearest station in the wind tool's result, which
     * is the last thing in the request.
     */
    private fun windReply(request: String): String {
        val unreadable = "The wind tool's result could not be read."
        val result = try {
            val messages = (Json.parseToJsonElement(request) as? JsonObject)?.get("messages") as? JsonArray
            val blocks = (messages?.lastOrNull() as? JsonObject)?.get("content") as? JsonArray
            val text = ((blocks?.firstOrNull() as? JsonObject)?.get("content") as? JsonPrimitive)?.content
            text?.let { Json.parseToJsonElement(it) as? JsonObject }
        } catch (_: SerializationException) {
            null
        } ?: return unreadable

        (result["error"] as? JsonPrimitive)?.let { return "The wind could not be looked up. ${it.content}" }
        val station = (result["stations"] as? JsonArray)?.firstOrNull() as? JsonObject
        val name = (station?.get("name") as? JsonPrimitive)?.content
        val wind = (station?.get("wind_km_h") as? JsonPrimitive)?.doubleOrNull
        if (station == null || name == null || wind == null) return "No station near the Zugspitze has a reading."

        val parts = mutableListOf("$name: **${wind.toInt()} km/h**")
        (station["wind_from"] as? JsonPrimitive)?.let { parts += "from ${it.content}" }
        (station["gusts_km_h"] as? JsonPrimitive)?.doubleOrNull?.let { parts += "gusts ${it.toInt()}" }
        (station["age_min"] as? JsonPrimitive)?.intOrNull?.let { parts += "measured $it minutes ago" }
        val sources = (result["sources"] as? JsonArray).orEmpty()
            .mapNotNull { (it as? JsonPrimitive)?.content }
            .joinToString("; ")
        return parts.joinToString(", ") + ". " + sources
    }
}
