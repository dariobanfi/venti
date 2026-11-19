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
