import VentiCore
import XCTest

@testable import Venti

/// Groups and plans against the real core, with the server played by
/// canned answers in the server's own format.
@MainActor
final class GroupsTests: XCTestCase {
    /// What was sent.
    private actor Sent {
        var requests: [URLRequest] = []
        func add(_ request: URLRequest) { requests.append(request) }
    }

    private final class TokenBox: @unchecked Sendable {
        var token: String?
    }

    private func makeCore() throws -> Core {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-groups-test-\(UUID().uuidString)", isDirectory: true)
        return try Core(dataFolder: folder.path)
    }

    /// A server that signs the demo pilot in and has one group with one
    /// plan, which it hands out whatever is asked about groups.
    private func server(noting sent: Sent) -> AccountModel.Transport {
        { request in
            await sent.add(request)
            let path = request.url?.path ?? ""
            switch (request.httpMethod ?? "", path) {
            case ("POST", "/v1/sessions"):
                return (201, Data(GroupAnswers.signIn.utf8), nil)
            case ("GET", "/v1/flights/changes"):
                return (200, Data(#"{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"#.utf8), nil)
            case ("GET", "/v1/groups/plans"):
                return (200, Data(GroupAnswers.board.utf8), nil)
            case ("POST", "/v1/groups/join"):
                return (403, Data(#"{"error": {"code": "invalid_group_invite", "message": "x"}}"#.utf8), nil)
            case (_, let path) where path.hasSuffix("/in") || path.hasSuffix("/plans"):
                return (200, Data(GroupAnswers.plan.utf8), nil)
            default:
                return (404, Data(#"{"error": {"code": "group_not_found", "message": "x"}}"#.utf8), nil)
            }
        }
    }

    func testTheBoardIsReadAndAPlanIsPostedWithTheAccountsToken() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        let groups = GroupsModel(core: core, account: account)

        // Signed out, nothing is asked and there is no board.
        await groups.load()
        XCTAssertNil(groups.board)
        let before = await sent.requests
        XCTAssertTrue(before.isEmpty)

        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo",
            password: "demo-pilot-password"
        )
        await groups.load()

        let board = try XCTUnwrap(groups.board)
        XCTAssertEqual(board.groups.map(\.name), ["Mustergruppe"])
        XCTAssertEqual(board.groups.first?.isKeptByMe, true)
        let plan = try XCTUnwrap(board.days.first?.plans.first)
        XCTAssertEqual(plan.place, "Musterberg, Startplatz Ost")
        XCTAssertEqual(plan.inWords, "You and Mara Muster")
        XCTAssertTrue(plan.amIn)
        XCTAssertEqual(groups.plan(plan.id)?.id, plan.id)

        // The board is asked for from the pilot's own today on.
        let boards = await sent.requests.filter { $0.url?.path == "/v1/groups/plans" }
        let asked = boards.last
        XCTAssertEqual(asked?.url?.query, "from=\(GroupsModel.day(Date()))")
        XCTAssertEqual(asked?.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")

        // A plan is posted to its group, cleaned as the server cleans it.
        try await groups.post(
            PlanDraft(day: "2026-11-21", place: " Wallberg ", position: nil, meetAt: "09:30", note: ""),
            to: plan.groupId
        )
        let posts = await sent.requests.filter { $0.httpMethod == "POST" && $0.url?.path.hasSuffix("/plans") == true }
        let posted = try XCTUnwrap(posts.last)
        XCTAssertEqual(posted.url?.path, "/v1/groups/44444444444444444444444444/plans")
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: try XCTUnwrap(posted.httpBody)) as? [String: Any])
        XCTAssertEqual(body["place"] as? String, "Wallberg")
        XCTAssertEqual(body["meet_at"] as? String, "09:30")
        XCTAssertNil(body["note"])

        // The pilot's word on a plan is a request without a body.
        try await groups.setIn(plan, false)
        let words = await sent.requests.filter { $0.url?.path.hasSuffix("/in") == true }
        let out = try XCTUnwrap(words.last)
        XCTAssertEqual(out.httpMethod, "DELETE")
        XCTAssertNil(out.httpBody)

        // A refusal is said in the core's words, and a plan that the
        // server would refuse is refused before anything is sent.
        do {
            _ = try await groups.join("0000-0000-0000-0000")
            XCTFail("A code that is none was taken")
        } catch {
            XCTAssertTrue(AccountModel.words(for: error).hasPrefix("This is not a group's invite code"))
        }
        let count = await sent.requests.count
        do {
            try await groups.post(
                PlanDraft(day: "2026-11-21", place: "  ", position: nil, meetAt: nil, note: ""), to: plan.groupId
            )
            XCTFail("A plan without a place was sent")
        } catch {
            XCTAssertEqual(AccountModel.words(for: error), "Say where: the name of the place.")
        }
        let after = await sent.requests.count
        XCTAssertEqual(after, count)
    }

    func testDaysAndTimesAreWrittenAsTheServerWritesThem() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(identifier: "Europe/Berlin"))
        let evening = try XCTUnwrap(
            calendar.date(from: DateComponents(year: 2026, month: 11, day: 5, hour: 23, minute: 40))
        )
        // The pilot's own day, not Greenwich's: at twenty to midnight in
        // Berlin it is still the fifth.
        XCTAssertEqual(GroupsModel.day(evening, calendar: calendar), "2026-11-05")
        XCTAssertEqual(GroupsModel.timeOfDay(evening, calendar: calendar), "23:40")
        let noon = try XCTUnwrap(GroupsModel.date(ofDay: "2026-11-05", calendar: calendar))
        XCTAssertEqual(GroupsModel.day(noon, calendar: calendar), "2026-11-05")
        XCTAssertNil(GroupsModel.date(ofDay: "next week", calendar: calendar))

        XCTAssertEqual(PlanWords.title(ofDay: "2026-11-05", now: evening, calendar: calendar), "Today")
        XCTAssertEqual(PlanWords.title(ofDay: "2026-11-06", now: evening, calendar: calendar), "Tomorrow")
        XCTAssertTrue(PlanWords.title(ofDay: "2026-11-08", now: evening, calendar: calendar).contains("8"))
    }
}

private enum GroupAnswers {
    static let signIn = """
        {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z"},
         "session": {"id": "session0000000000000000000a", "created_at": "2026-11-18T19:05:00Z",
                     "last_used_at": "2026-11-18T19:05:00Z", "current": true},
         "token": "venti_test_token"}
        """

    static let planBody = """
        {"id": "66666666666666666666666666", "group": "44444444444444444444444444", "day": "2026-11-18",
         "place": "Musterberg, Startplatz Ost", "meet_at": "10:30",
         "author": {"handle": "mara", "display_name": "Mara Muster"},
         "created_at": "2026-11-17T17:05:00Z", "changed_at": "2026-11-17T17:05:00Z",
         "in": [{"handle": "mara", "display_name": "Mara Muster"}, {"handle": "demo", "display_name": "Demo Pilot"}]}
        """

    static let plan = "{\"plan\": \(planBody)}"

    static let board = """
        {"groups": [{"id": "44444444444444444444444444", "name": "Mustergruppe", "created_at": "2026-10-19T19:05:00Z",
                     "keeper": {"handle": "demo", "display_name": "Demo Pilot"}, "member_count": 3}],
         "plans": [\(planBody)], "truncated": false}
        """
}
