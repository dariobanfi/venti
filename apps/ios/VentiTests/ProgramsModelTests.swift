import VentiCore
import XCTest

@testable import Venti

/// "Programs with access" against the real core, with the server played
/// by its own example answers.
@MainActor
final class ProgramsModelTests: XCTestCase {
    /// What was sent.
    private actor Sent {
        var requests: [ServerRequest] = []
        func add(_ request: ServerRequest) { requests.append(request) }
    }

    /// The moment of the server's examples: 18 November 2026, 19:05 UTC.
    private static let now: Int64 = 1_795_028_700

    private static let list = #"{"tokens": [{"id": "00000000000000000000000003", "name": "Demo logger", "scopes": ["flights:read", "flights:write"], "created_at": "2026-11-18T19:05:00Z"}]}"#
    private static let none = #"{"tokens": []}"#
    private static let made = #"{"token": {"id": "3kq9x2m7r4t8v1w6y0a5b3c2d4", "name": "My logger", "scopes": ["flights:write", "live:write"], "created_at": "2026-11-18T19:05:00Z"}, "secret": "venti_pat_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01"}"#
    private static let wrongPassword = #"{"error": {"code": "wrong_password", "message": "x"}}"#

    /// A model whose server answers each method with one answer.
    private func model(
        noting sent: Sent, get: (UInt16, String) = (200, list), post: (UInt16, String) = (201, made)
    ) -> ProgramsModel {
        ProgramsModel(
            send: { request in
                await sent.add(request)
                let (status, body): (UInt16, String) = switch request.method {
                case "GET": get
                case "POST": post
                default: (204, "")
                }
                return AccountModel.Answer(status: status, body: Data(body.utf8), retryAfter: nil)
            },
            now: { Self.now }
        )
    }

    private func json(_ request: ServerRequest) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: request.body ?? Data()) as? [String: Any])
    }

    func testTheListIsReadWithTheSession() async throws {
        let sent = Sent()
        let model = model(noting: sent)
        XCTAssertEqual(model.state, .loading)

        await model.load()

        XCTAssertEqual(model.tokens.map(\.name), ["Demo logger"])
        XCTAssertEqual(model.tokens.first?.scopes.count, 2)
        XCTAssertEqual(model.tokens.first?.hasEnded, false)
        let requests = await sent.requests
        XCTAssertEqual(requests.map(\.method), ["GET"])
        XCTAssertEqual(requests.first?.path, "/v1/account/tokens")
        XCTAssertTrue(requests.allSatisfy(\.needsToken))
    }

    func testAListThatCannotBeReadSaysWhyInTheCoresWords() async throws {
        let model = model(noting: Sent(), get: (401, #"{"error": {"code": "unauthorized", "message": "x"}}"#))
        await model.load()
        guard case .failed(let words) = model.state else {
            return XCTFail("The list was read")
        }
        XCTAssertFalse(words.isEmpty)
        XCTAssertFalse(words.contains("unauthorized"), words)
    }

    func testGivingAccessSendsTheChoiceAndHoldsTheSecretUntilItsPageIsLeft() async throws {
        let sent = Sent()
        let model = model(noting: sent)
        await model.load()

        // The scopes go in the form's order, whatever order they were ticked in.
        let isMade = await model.give(
            name: "  My logger ", scopes: ["live:write", "flights:write"], endsUnixS: nil,
            password: "demo-pilot-password"
        )

        XCTAssertTrue(isMade)
        XCTAssertNil(model.refusal)
        let created = try XCTUnwrap(model.created)
        XCTAssertTrue(created.secret.hasPrefix("venti_pat_"))
        XCTAssertEqual(created.shownOnce, "Copy it now: Venti does not show it again.")
        // The list begins with it, without being asked for again.
        XCTAssertEqual(model.tokens.map(\.name), ["My logger", "Demo logger"])
        let last = await sent.requests.last
        let request = try XCTUnwrap(last)
        XCTAssertEqual(request.method, "POST")
        XCTAssertEqual(request.path, "/v1/account/tokens")
        XCTAssertTrue(request.needsToken)
        let body = try json(request)
        XCTAssertEqual(body["name"] as? String, "My logger")
        XCTAssertEqual(body["scopes"] as? [String], ["flights:write", "live:write"])
        XCTAssertEqual(body["password"] as? String, "demo-pilot-password")
        XCTAssertNil(body["expires_at"])

        // The page is left: the secret is gone, the list stays.
        model.forgetSecret()
        XCTAssertNil(model.created)
        XCTAssertEqual(model.tokens.count, 2)
    }

    func testAnAccessThatEndsOnADayEndsWithThatDayInUtc() async throws {
        let sent = Sent()
        let model = model(noting: sent)
        // 21 December 2026 in Berlin, at noon and a minute after midnight.
        var berlin = Calendar(identifier: .gregorian)
        berlin.timeZone = try XCTUnwrap(TimeZone(identifier: "Europe/Berlin"))
        for (hour, minute) in [(12, 0), (0, 1)] {
            let day = try XCTUnwrap(berlin.date(from: DateComponents(
                year: 2026, month: 12, day: 21, hour: hour, minute: minute
            )))
            let end = try XCTUnwrap(ProgramsModel.end(of: day, in: berlin))
            XCTAssertEqual(end, 1_797_897_599)

            _ = await model.give(name: "My logger", scopes: ["flights:read"], endsUnixS: end, password: "x")
            let last = await sent.requests.last
            let body = try json(try XCTUnwrap(last))
            XCTAssertEqual(body["expires_at"] as? String, "2026-12-21T23:59:59Z")
        }
    }

    func testWhatTheFormLacksIsSaidBeforeAnythingIsSent() async throws {
        let sent = Sent()
        let model = model(noting: sent)

        XCTAssertEqual(
            model.nameFault(" "),
            "Give the program a name, so that you know later what this access is for."
        )
        XCTAssertNil(model.nameFault("My logger"))

        var isMade = await model.give(name: "My logger", scopes: [], endsUnixS: nil, password: "x")
        XCTAssertFalse(isMade)
        XCTAssertEqual(model.refusal, "Choose at least one thing the program may do.")
        isMade = await model.give(name: "My logger", scopes: ["flights:read"], endsUnixS: nil, password: "")
        XCTAssertFalse(isMade)
        XCTAssertEqual(model.refusal, "Enter your password.")
        let requests = await sent.requests
        XCTAssertTrue(requests.isEmpty)
        XCTAssertNil(model.created)
    }

    func testARefusalIsSaidInTheCoresWordsAndNoSecretIsHeld() async throws {
        let model = model(noting: Sent(), post: (403, Self.wrongPassword))
        let isMade = await model.give(
            name: "My logger", scopes: ["flights:read"], endsUnixS: nil, password: "not-the-password"
        )
        XCTAssertFalse(isMade)
        XCTAssertNil(model.created)
        let refusal = try XCTUnwrap(model.refusal)
        XCTAssertFalse(refusal.isEmpty)
        XCTAssertFalse(refusal.contains("wrong_password"), refusal)
        model.clearRefusal()
        XCTAssertNil(model.refusal)
    }

    func testWithdrawingAsksInTheCoresWordsAndReadsTheListAgain() async throws {
        let sent = Sent()
        let model = model(noting: sent)
        await model.load()
        let token = try XCTUnwrap(model.tokens.first)

        let question = tokenWithdrawal(name: token.name)
        XCTAssertEqual(question.title, "Withdraw the access of \u{201c}Demo logger\u{201d}?")
        XCTAssertEqual(question.confirm, "Withdraw access")

        await model.withdraw(token)

        XCTAssertNil(model.problem)
        let requests = await sent.requests
        XCTAssertEqual(requests.map(\.method), ["GET", "DELETE", "GET"])
        XCTAssertEqual(requests[1].path, "/v1/account/tokens/00000000000000000000000003")
        XCTAssertTrue(requests[1].needsToken)
    }

    func testAnEmptyListSaysSoInTheCoresWords() async throws {
        let model = model(noting: Sent(), get: (200, Self.none))
        await model.load()
        XCTAssertEqual(model.state, .loaded([]))
        XCTAssertEqual(model.form.empty, "No program has access to your account.")
    }
}
