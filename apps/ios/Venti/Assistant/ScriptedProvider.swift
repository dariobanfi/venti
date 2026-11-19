import Foundation
import VentiCore

/// A stand-in for the model provider, for tests: it answers the
/// assistant's requests from a script, in the provider's own format.
///
/// The first request of a question is answered with a call to the
/// flight-list tool, so that the real tool loop runs against the real
/// logbook; the request that carries the tool's result is answered with a
/// sentence. A question about the wind on the Zugspitze is answered with a
/// call to the wind tool instead, and then with a sentence made from what
/// the tool found. Nothing in the app uses this unless it is started with
/// `--scripted-assistant`.
enum ScriptedProvider {
    static let reply = "Your logbook has one flight in 2026: 29 minutes on 4 July, with two thermals. The first was the stronger, at **2.4 m/s** in a westerly of about 11 km/h."

    @Sendable
    static func answer(_ request: AssistantRequest) async throws -> (Int, Data) {
        let hasToolResult = request.body.contains("\"tool_result\"")
        let asksForWind = request.body.contains("wind on the Zugspitze")
        let content: [[String: Any]] =
            switch (hasToolResult, asksForWind) {
            case (true, true):
                [["type": "text", "text": windReply(to: request.body)]]
            case (true, false):
                [["type": "text", "text": reply]]
            case (false, true):
                [[
                    "type": "tool_use", "id": "scripted_1", "name": "wind_stations",
                    "input": ["latitude": 47.42, "longitude": 10.99, "radius_km": 30],
                ]]
            case (false, false):
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

    /// A sentence about the nearest station in the wind tool's result,
    /// which is the last thing in the request.
    private static func windReply(to request: String) -> String {
        guard let root = try? JSONSerialization.jsonObject(with: Data(request.utf8)) as? [String: Any],
              let messages = root["messages"] as? [[String: Any]],
              let blocks = messages.last?["content"] as? [[String: Any]],
              let text = blocks.first?["content"] as? String,
              let result = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        else {
            return "The wind tool's result could not be read."
        }
        if let error = result["error"] as? String {
            return "The wind could not be looked up. \(error)"
        }
        guard let station = (result["stations"] as? [[String: Any]])?.first,
              let name = station["name"] as? String,
              let wind = station["wind_km_h"] as? Double
        else {
            return "No station near the Zugspitze has a reading."
        }
        var parts = ["\(name): **\(Int(wind)) km/h**"]
        if let from = station["wind_from"] as? String {
            parts.append("from \(from)")
        }
        if let gusts = station["gusts_km_h"] as? Double {
            parts.append("gusts \(Int(gusts))")
        }
        if let age = station["age_min"] as? Int {
            parts.append("measured \(age) minutes ago")
        }
        let sources = (result["sources"] as? [String])?.joined(separator: "; ") ?? ""
        return parts.joined(separator: ", ") + ". " + sources
    }
}
