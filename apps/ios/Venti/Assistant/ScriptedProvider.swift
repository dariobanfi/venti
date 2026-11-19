import Foundation
import VentiCore

/// A stand-in for the model provider, for tests: it answers the
/// assistant's requests from a script, in the provider's own format.
///
/// The first request of a question is answered with a call to the
/// flight-list tool, so that the real tool loop runs against the real
/// logbook; the request that carries the tool's result is answered with a
/// sentence. Nothing in the app uses this unless it is started with
/// `--scripted-assistant`.
enum ScriptedProvider {
    static let reply = "Your logbook has one flight in 2026: 29 minutes on 4 July, with two thermals. The first was the stronger, at **2.4 m/s** in a westerly of about 11 km/h."

    @Sendable
    static func answer(_ request: AssistantRequest) async throws -> (Int, Data) {
        let hasToolResult = request.body.contains("\"tool_result\"")
        let content: [[String: Any]] =
            if hasToolResult {
                [["type": "text", "text": reply]]
            } else {
                [["type": "tool_use", "id": "scripted_1", "name": "list_flights", "input": ["year": 2026]]]
            }
        let body: [String: Any] = [
            "type": "message",
            "role": "assistant",
            "content": content,
            "stop_reason": hasToolResult ? "end_turn" : "tool_use",
        ]
        return (200, try JSONSerialization.data(withJSONObject: body))
    }
}
