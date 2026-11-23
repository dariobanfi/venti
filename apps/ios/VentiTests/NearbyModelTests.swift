import UserNotifications
import VentiCore
import XCTest

@testable import Venti

/// What pilots nearby do, against the real core, with the server played by
/// its own golden answers (`crates/server/tests/golden/help_*.json`,
/// `nearby_settings.json`; docs/api.md, "Push to the phone apps" and "Help
/// after landing").
@MainActor
final class NearbyModelTests: XCTestCase {
    /// What was sent.
    private final class Sent {
        var requests: [ServerRequest] = []
        var paths: [String] { requests.map { "\($0.method) \($0.path)" } }
    }

    /// The moment of the server's examples: 18 November 2026, 19:10 UTC.
    private static let nowMs: Int64 = 1_795_029_000_000

    private func model(
        _ sent: Sent, signedIn: Bool = true, settings: String = Answers.settings, changed: String? = nil
    ) -> NearbyModel {
        NearbyModel(
            send: { request in
                sent.requests.append(request)
                let (status, body): (UInt16, String) =
                    switch (request.method, request.path) {
                    case ("GET", "/v1/account/nearby"): (200, settings)
                    case ("PUT", "/v1/account/nearby"): (200, changed ?? settings)
                    case ("PUT", "/v1/account/position"), ("DELETE", "/v1/account/position"): (204, "")
                    case ("POST", "/v1/help"): (201, Answers.asked)
                    case ("GET", "/v1/help"): (200, Answers.lists)
                    case ("DELETE", _): (204, "")
                    case ("PUT", _): (200, Answers.canHelp)
                    case ("GET", _): (200, Answers.seen)
                    default: (404, #"{"error": {"code": "not_found", "message": "x"}}"#)
                    }
                return AccountModel.Answer(status: status, body: Data(body.utf8), retryAfter: nil)
            },
            isSignedIn: { signedIn },
            nowMs: { Self.nowMs }
        )
    }

    func testTheSettingsAreReadChangedAndTheirPositionForgotten() async throws {
        let sent = Sent()
        let nearby = model(sent)
        XCTAssertNil(nearby.choice)
        await nearby.loadSettings()
        let choice = try XCTUnwrap(nearby.choice)
        XCTAssertTrue(nearby.mayBeAskedForHelp)
        XCTAssertEqual(choice.openPlansRadiusKm, 30)
        XCTAssertEqual(choice.openPlansNear?.lat ?? 0, 47.6, accuracy: 1e-9)

        // While help is on, the position is told.
        await nearby.tellPosition(at: Coordinate(lat: 47.27, lon: 11.39))
        XCTAssertEqual(sent.paths.last, "PUT /v1/account/position")

        // Switched off: the settings whole, then the position forgotten.
        let off = Answers.settings.replacingOccurrences(
            of: "\"may_be_asked_for_help\": true", with: "\"may_be_asked_for_help\": false"
        )
        let changing = model(sent, changed: off)
        await changing.loadSettings()
        var wanted = choice
        wanted.mayBeAskedForHelp = false
        let refused = await changing.change(wanted)
        XCTAssertNil(refused)
        XCTAssertFalse(changing.mayBeAskedForHelp)
        XCTAssertEqual(Array(sent.paths.suffix(2)), ["PUT /v1/account/nearby", "DELETE /v1/account/position"])

        // Off: no position is told.
        let count = sent.requests.count
        await changing.tellPosition(at: Coordinate(lat: 47.27, lon: 11.39))
        XCTAssertEqual(sent.requests.count, count)

        // A distance the server does not take is the core's refusal, in
        // its words, and nothing is sent.
        wanted.openPlansRadiusKm = 101
        let words = await changing.change(wanted)
        XCTAssertEqual(words, "Choose a distance from 1 to 100 km.")
        XCTAssertEqual(sent.requests.count, count)
    }

    func testAPilotAsksIsAnsweredAndCloses() async throws {
        let sent = Sent()
        let nearby = model(sent)
        let mine = try await nearby.ask(at: Coordinate(lat: 47.2731, lon: 11.3962), note: "  ")
        XCTAssertTrue(mine.isMine)
        XCTAssertTrue(mine.stands)
        XCTAssertEqual(mine.line, "Asked 2 pilots nearby")
        XCTAssertEqual(nearby.mine?.id, mine.id)
        XCTAssertEqual(sent.paths, ["POST /v1/help"])
        XCTAssertTrue(sent.requests.allSatisfy(\.needsToken))

        try await nearby.close(mine.id)
        XCTAssertEqual(Array(sent.paths.suffix(2)), ["DELETE /v1/help/\(mine.id)", "GET /v1/help"])

        // A note longer than the server takes: the core's words.
        do {
            _ = try await nearby.ask(at: Coordinate(lat: 47.27, lon: 11.39), note: String(repeating: "x", count: 281))
            XCTFail("A note of 281 characters was sent")
        } catch {
            XCTAssertEqual(NearbyModel.words(for: error), "The note has room for 280 characters.")
        }
    }

    func testAPilotWhoWasAskedSaysTheyCanHelpAndSeesWhere() async throws {
        let sent = Sent()
        let nearby = model(sent)
        await nearby.loadHelp()
        XCTAssertNil(nearby.helpProblem)
        let asked = try XCTUnwrap(nearby.asked.first)
        XCTAssertEqual(asked.line, "About 2 km from you")
        XCTAssertNil(asked.position)
        XCTAssertNil(nearby.mine)

        let answered = try await nearby.answer(asked.id, canHelp: true)
        XCTAssertEqual(answered.canHelp, true)
        XCTAssertEqual(answered.line, "You said you can help")
        XCTAssertNotNil(answered.position)
        XCTAssertEqual(nearby.asked.first?.canHelp, true)
        XCTAssertEqual(sent.paths.last, "PUT /v1/help/\(asked.id)/answer")
        XCTAssertNotNil(HelpScreen.directions(to: try XCTUnwrap(answered.position)))
    }

    func testSignedOutNothingIsAskedOrHeld() async {
        let sent = Sent()
        let nearby = model(sent, signedIn: false)
        await nearby.loadSettings()
        await nearby.loadHelp()
        await nearby.tellPosition(at: Coordinate(lat: 47.27, lon: 11.39))
        XCTAssertTrue(sent.requests.isEmpty)
        XCTAssertNil(nearby.choice)
        XCTAssertNil(nearby.help)
    }

    func testAPushedMessageOpensWhatTheCoreNames() {
        AlertNotices.taps.opens = nil
        AlertNotices.taps.opensAlerts = false
        let id = "4bq6x0vd5cwm2b9r8y1hzk3tns"
        let pushed = AlertNotices.pushed(["aps": ["alert": ["title": "x"]], "kind": "help_request", "id": id, "sent_at": "x"])
        XCTAssertEqual(pushed?.kind, "help_request")
        AlertNotices.tapped(kind: "help_request", id: id, action: UNNotificationDefaultActionIdentifier)
        XCTAssertEqual(AlertNotices.taps.opens, .help(id: id))
        AlertNotices.tapped(kind: "alert", id: "a", action: UNNotificationDefaultActionIdentifier)
        XCTAssertTrue(AlertNotices.taps.opensAlerts)
        // A kind this version does not know, and a dismissal, open nothing.
        AlertNotices.taps.opens = nil
        AlertNotices.tapped(kind: "something_new", id: id, action: UNNotificationDefaultActionIdentifier)
        AlertNotices.tapped(kind: "open_plan", id: id, action: UNNotificationDismissActionIdentifier)
        XCTAssertNil(AlertNotices.taps.opens)
        XCTAssertNil(AlertNotices.pushed(["aps": [:]]))
        AlertNotices.taps.opensAlerts = false
    }

    func testATokenIsSentAsLowerCaseHexadecimal() {
        XCTAssertEqual(PhonePush.hex(Data([0x00, 0xab, 0x10, 0xff])), "00ab10ff")
        XCTAssertTrue(PhonePush.sandbox)
    }

    private enum Answers {
        static let settings = """
            {"open_plans_near": {"position": {"lat": 47.6, "lon": 11.7}, "radius_km": 30},
             "may_be_asked_for_help": true}
            """
        static let asked = """
            {"request": {"id": "4bq6x0vd5cwm2b9r8y1hzk3tns", "pilot": {"handle": "anna", "display_name": "Pilot anna"},
             "note": "Landed by the Inn at Völs, anyone driving to Innsbruck?",
             "created_at": "2026-11-18T19:05:00Z", "ends_at": "2026-11-18T23:05:00Z", "mine": true,
             "position": {"lat": 47.2731, "lon": 11.3962}, "asked": 2}}
            """
        static let seenRequest = """
            {"id": "5cr7y1we6dxn3c0s9z2jal4uvp", "pilot": {"handle": "anna", "display_name": "Pilot anna"},
             "note": "Landed by the Inn at Völs, anyone driving to Innsbruck?",
             "created_at": "2026-11-18T19:05:00Z", "ends_at": "2026-11-18T23:05:00Z", "mine": false,
             "distance_km": 2, "can_help": false}
            """
        static let seen = "{\"request\": \(seenRequest)}"
        static let lists = "{\"mine\": [], \"asked\": [\(seenRequest)]}"
        static let canHelp = seen
            .replacingOccurrences(of: "\"can_help\": false", with: "\"can_help\": true, \"position\": {\"lat\": 47.2731, \"lon\": 11.3962}")
    }
}
