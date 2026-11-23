import VentiCore
import XCTest

@testable import Venti

/// The count of what is new for the Community tab's badge, against the
/// real core, with the server played by its golden answer
/// (`crates/server/tests/golden/unread.json`: four notifications and one
/// alarm's line).
@MainActor
final class UnreadModelTests: XCTestCase {
    private final class Noted {
        var requests: [ServerRequest] = []
        var signedIn = true
        var answer = #"{"notifications": 4, "alarms": 1, "total": 5}"#
        var fails = false
    }

    private func model(_ noted: Noted) -> UnreadModel {
        UnreadModel(
            send: { request in
                noted.requests.append(request)
                if noted.fails {
                    throw URLError(.notConnectedToInternet)
                }
                return AccountModel.Answer(status: 200, body: Data(noted.answer.utf8), retryAfter: nil)
            },
            isSignedIn: { noted.signedIn }
        )
    }

    private func body(of request: ServerRequest) throws -> [String: String] {
        let data = try XCTUnwrap(request.body)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: String])
    }

    func testTheBadgeIsTheCoresCountAndItsWords() async {
        let noted = Noted()
        let unread = model(noted)
        XCTAssertNil(unread.badge)

        await unread.read()
        XCTAssertEqual(noted.requests.map(\.method), ["GET"])
        XCTAssertEqual(noted.requests.first?.path, "/v1/account/unread")
        XCTAssertEqual(noted.requests.first?.needsToken, true)
        XCTAssertEqual(unread.badge, "5")
        XCTAssertEqual(unread.label, "5 new")

        // Nothing new: no badge at all.
        noted.answer = #"{"notifications": 0, "alarms": 0, "total": 0}"#
        await unread.read()
        XCTAssertNil(unread.badge)
        XCTAssertNil(unread.label)
    }

    func testOpeningTheNotificationsMovesOnlyTheirMark() async throws {
        let noted = Noted()
        let unread = model(noted)
        await unread.read()

        noted.answer = #"{"notifications": 0, "alarms": 1, "total": 1}"#
        await unread.sawNotifications(upTo: "1795026000000000")
        let seen = try XCTUnwrap(noted.requests.last)
        XCTAssertEqual(seen.method, "PUT")
        XCTAssertEqual(seen.path, "/v1/account/unread")
        // The alarms' mark is named with the cursor that moves nothing.
        XCTAssertEqual(
            try body(of: seen),
            ["notifications_cursor": "1795026000000000", "alarms_cursor": UnreadModel.notShown]
        )
        // The answer is the count after it: no second request.
        XCTAssertEqual(noted.requests.count, 2)
        XCTAssertEqual(unread.badge, "1")
    }

    func testOpeningTheAlertsMovesTheirMarkToTheNewest() async throws {
        let noted = Noted()
        let unread = model(noted)
        noted.answer = #"{"notifications": 4, "alarms": 0, "total": 4}"#
        await unread.sawAlarms()
        let seen = try XCTUnwrap(noted.requests.last)
        XCTAssertEqual(seen.method, "PUT")
        // Left out: up to the newest.
        XCTAssertEqual(try body(of: seen), ["notifications_cursor": UnreadModel.notShown])
        XCTAssertEqual(unread.badge, "4")
    }

    func testSignedOutThereIsNoBadgeAndNothingIsAsked() async {
        let noted = Noted()
        let unread = model(noted)
        await unread.read()
        XCTAssertEqual(unread.badge, "5")

        noted.signedIn = false
        await unread.read()
        await unread.sawNotifications(upTo: "1")
        XCTAssertNil(unread.badge)
        XCTAssertEqual(noted.requests.count, 1)
    }

    func testAFailedReadKeepsTheLastCount() async {
        let noted = Noted()
        let unread = model(noted)
        await unread.read()
        noted.fails = true
        await unread.read()
        XCTAssertEqual(unread.badge, "5")
        // A mark that could not be moved is read again, which fails too.
        await unread.sawAlarms()
        XCTAssertEqual(noted.requests.map(\.method), ["GET", "GET", "PUT", "GET"])
        XCTAssertEqual(unread.badge, "5")
    }
}
