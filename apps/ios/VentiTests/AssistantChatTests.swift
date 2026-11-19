import VentiCore
import XCTest

@testable import Venti

/// The assistant's conversation model against the real core, with the
/// provider played by a script.
@MainActor
final class AssistantChatTests: XCTestCase {
    private func makeLogbook() async throws -> LogbookModel {
        let logbook = try LogbookModel.open(
            folder: FileManager.default.temporaryDirectory
                .appendingPathComponent("venti-assistant-test-\(UUID().uuidString)", isDirectory: true)
        )
        _ = await logbook.addSample()
        return logbook
    }

    func testAQuestionIsAnsweredThroughTheLogbookTools() async throws {
        let logbook = try await makeLogbook()
        let chat = AssistantChat(
            makeAssistant: { model in logbook.makeAssistant(model: model) },
            transport: ScriptedProvider.answer,
            readKey: { "test-key" }
        )
        XCTAssertTrue(chat.hasKey)
        XCTAssertTrue(chat.entries.isEmpty)

        await chat.ask("  How was 2026?  ")

        XCTAssertNil(chat.problem)
        XCTAssertFalse(chat.isWorking)
        XCTAssertEqual(chat.entries.count, 2)
        XCTAssertTrue(chat.entries[0].fromPilot)
        XCTAssertEqual(chat.entries[0].text, "How was 2026?")
        XCTAssertFalse(chat.entries[1].fromPilot)
        XCTAssertEqual(chat.entries[1].text, ScriptedProvider.reply)
        XCTAssertEqual(chat.entries[1].lookedUp, ["your list of flights"])

        chat.startAgain()
        XCTAssertTrue(chat.entries.isEmpty)
    }

    func testTheAssistantCanRunAtAnotherService() async throws {
        /// The other service's key, kept in memory instead of the Keychain.
        final class Kept {
            var key: String?
        }

        let logbook = try await makeLogbook()
        let seen = Seen()
        let kept = Kept()
        let chat = AssistantChat(
            makeAssistant: { model in logbook.makeAssistant(model: model) },
            makeAssistantAt: { address, model in try logbook.makeAssistant(at: address, model: model) },
            transport: { request in
                await seen.add(request)
                let reply = #"{"choices": [{"finish_reason": "stop", "message": {"role": "assistant", "content": "Two flights."}}]}"#
                return (200, Data(reply.utf8))
            },
            readKey: { nil },
            otherKey: AssistantChat.OtherKey(
                read: { kept.key },
                save: { key in
                    kept.key = key
                    return true
                },
                remove: { kept.key = nil }
            ),
            remembersService: false
        )
        XCTAssertFalse(chat.hasKey)
        XCTAssertEqual(chat.service, .anthropic)

        // An address a key must not be sent to is refused in the core's
        // words, and nothing is kept.
        XCTAssertFalse(chat.useOtherService(address: "http://api.example.org/v1", model: "some-model", key: "sk-other"))
        XCTAssertTrue(chat.problem?.contains("encrypted connection") ?? false, chat.problem ?? "")
        XCTAssertNil(kept.key)
        chat.problem = nil
        // So is a service without a key typed or kept.
        XCTAssertFalse(chat.useOtherService(address: "api.example.org/v1", model: "some-model", key: " "))
        XCTAssertEqual(chat.service, .anthropic)
        chat.problem = nil

        XCTAssertTrue(chat.useOtherService(address: "api.example.org/v1/", model: " some-model ", key: " sk-other "))
        XCTAssertEqual(chat.service, .other)
        XCTAssertEqual(chat.otherAddress, "https://api.example.org/v1")
        XCTAssertEqual(chat.otherModel, "some-model")
        XCTAssertEqual(kept.key, "sk-other")
        XCTAssertTrue(chat.hasKey)
        XCTAssertEqual(AssistantScreen.host(of: chat.otherAddress), "api.example.org")

        // A question goes to that service, in its format, with its key.
        await chat.ask("How many flights?")
        XCTAssertNil(chat.problem)
        XCTAssertEqual(chat.entries.last?.text, "Two flights.")
        let requests = await seen.requests
        XCTAssertEqual(requests.count, 1)
        XCTAssertEqual(requests.first?.url, "https://api.example.org/v1/chat/completions")
        XCTAssertTrue(
            requests.first?.headers.contains(HttpHeader(name: "authorization", value: "Bearer sk-other")) ?? false
        )
        XCTAssertFalse(requests.first?.body.contains("sk-other") ?? true)

        // Back to Anthropic, where there is no key; the other service's
        // stays kept for changing back, until it is removed.
        chat.useAnthropic()
        XCTAssertEqual(chat.service, .anthropic)
        XCTAssertFalse(chat.hasKey)
        XCTAssertTrue(chat.hasOtherKey)
        XCTAssertTrue(chat.useOtherService(address: chat.otherAddress, model: chat.otherModel, key: ""))
        XCTAssertEqual(chat.service, .other)
        chat.removeKey()
        XCTAssertNil(kept.key)
        XCTAssertEqual(chat.service, .anthropic)
    }

    func testTheKeyGoesToTheProviderInAHeaderAndTheLogbookInTheBody() async throws {
        let logbook = try await makeLogbook()
        let seen = Seen()
        let chat = AssistantChat(
            makeAssistant: { model in logbook.makeAssistant(model: model) },
            transport: { request in
                await seen.add(request)
                return try await ScriptedProvider.answer(request)
            },
            readKey: { "test-key" }
        )

        await chat.ask("How was 2026?")

        let requests = await seen.requests
        XCTAssertEqual(requests.count, 2)
        for request in requests {
            XCTAssertEqual(request.url, "https://api.anthropic.com/v1/messages")
            XCTAssertTrue(request.headers.contains { $0.name == "x-api-key" && $0.value == "test-key" })
            XCTAssertFalse(request.body.contains("test-key"))
        }
        // The second request carries what the tool found in the logbook.
        XCTAssertTrue(requests[1].body.contains("Sample Wing"))
    }

    func testAProviderThatCannotBeReachedLeavesTheConversationAsItWas() async throws {
        let logbook = try await makeLogbook()
        let chat = AssistantChat(
            makeAssistant: { model in logbook.makeAssistant(model: model) },
            transport: { _ in throw URLError(.notConnectedToInternet) },
            readKey: { "test-key" }
        )

        await chat.ask("How was 2026?")

        XCTAssertNotNil(chat.problem)
        XCTAssertTrue(chat.entries.isEmpty)
        XCTAssertFalse(chat.isWorking)
    }

    func testLiveWindIsFetchedFromThePilotsServerWithoutTheKey() async throws {
        let logbook = try await makeLogbook()
        let fetched = Fetched()
        let chat = AssistantChat(
            makeAssistant: { model in logbook.makeAssistant(model: model) },
            transport: ScriptedProvider.answer,
            readKey: { "test-key" },
            fetch: { url in
                await fetched.add(url)
                return (200, Data(WindAnswers.stations.utf8))
            },
            server: { "https://wind.example.org" },
            // 18 November 2026, 19:05 UTC: 45 minutes after the reading.
            now: { 1_795_028_700 }
        )

        await chat.ask("How is the wind on the Zugspitze right now?")

        XCTAssertNil(chat.problem)
        XCTAssertEqual(chat.entries.count, 2)
        XCTAssertEqual(chat.entries[1].lookedUp, ["the wind stations around a place"])
        XCTAssertEqual(
            chat.entries[1].text,
            "Zugspitze: **54 km/h**, from W, gusts 93, measured 45 minutes ago. Quelle: Deutscher Wetterdienst (CC BY 4.0)"
        )
        // The server was asked for an area, and for nothing else.
        let urls = await fetched.urls
        XCTAssertEqual(urls.count, 1)
        XCTAssertEqual(urls.first?.host(), "wind.example.org")
        XCTAssertEqual(urls.first?.path, "/v1/stations")
        XCTAssertFalse(urls.first?.absoluteString.contains("test-key") ?? true)
    }

    func testAServerThatCannotBeReachedIsExplainedInTheReply() async throws {
        let logbook = try await makeLogbook()
        let chat = AssistantChat(
            makeAssistant: { model in logbook.makeAssistant(model: model) },
            transport: ScriptedProvider.answer,
            readKey: { "test-key" },
            fetch: { _ in throw URLError(.cannotConnectToHost) },
            server: { "https://wind.example.org" }
        )

        await chat.ask("How is the wind on the Zugspitze right now?")

        // The conversation goes on: the model is told and says so.
        XCTAssertNil(chat.problem)
        XCTAssertTrue(chat.entries.last?.text.contains("could not be looked up") ?? false)
    }

    func testWithoutAKeyNothingIsAsked() async throws {
        let logbook = try await makeLogbook()
        let chat = AssistantChat(
            makeAssistant: { model in logbook.makeAssistant(model: model) },
            transport: { _ in
                XCTFail("no request may be sent without a key")
                return (500, Data())
            },
            readKey: { nil }
        )
        XCTAssertFalse(chat.hasKey)
        await chat.ask("How was 2026?")
        XCTAssertTrue(chat.entries.isEmpty)
    }
}

/// Collects the requests a test's transport was given.
private actor Seen {
    private(set) var requests: [AssistantRequest] = []

    func add(_ request: AssistantRequest) {
        requests.append(request)
    }
}

/// Collects the addresses a test's fetch was given.
private actor Fetched {
    private(set) var urls: [URL] = []

    func add(_ url: URL) {
        urls.append(url)
    }
}

/// An answer of the Venti server, in its own format.
private enum WindAnswers {
    static let stations = """
        {"stations": [
          {"id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
           "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0,
           "latest": {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7,
                      "wind_direction_deg": 260.0, "temperature_c": -7.1}}
        ], "truncated": false, "attributions": [
          {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "licence": "CC BY 4.0",
           "text": "Quelle: Deutscher Wetterdienst", "url": "https://www.dwd.de/"}
        ]}
        """
}
