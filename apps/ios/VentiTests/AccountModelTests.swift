import VentiCore
import XCTest

@testable import Venti

/// The account model against the real core, with the server played by
/// canned answers in the server's own format.
@MainActor
final class AccountModelTests: XCTestCase {
    /// What the model sent.
    private actor Sent {
        var requests: [URLRequest] = []
        func add(_ request: URLRequest) { requests.append(request) }
    }

    /// The token as the Keychain would hold it.
    private final class TokenBox: @unchecked Sendable {
        var token: String?
    }

    private func makeCore() throws -> Core {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-account-test-\(UUID().uuidString)", isDirectory: true)
        return try Core(dataFolder: folder.path)
    }

    private func sampleText() throws -> String {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        return try String(contentsOf: url, encoding: .utf8)
    }

    /// A server that signs anyone in, has no flights and takes every upload.
    private func server(noting sent: Sent) -> AccountModel.Transport {
        { request in
            await sent.add(request)
            let path = request.url?.path ?? ""
            switch (request.httpMethod ?? "", path) {
            case ("POST", "/v1/sessions"):
                return (201, Data(ServerAnswers.signIn.utf8), nil)
            case ("GET", "/v1/flights/changes"):
                return (200, Data(ServerAnswers.noChanges.utf8), nil)
            case ("POST", "/v1/flights"):
                return (201, Data(ServerAnswers.upload.utf8), nil)
            case ("PATCH", _):
                return (200, Data(ServerAnswers.upload.replacingOccurrences(of: "\"private\"", with: "\"unlisted\"").utf8), nil)
            case ("DELETE", _):
                return (204, Data(), nil)
            default:
                return (404, Data(#"{"error": {"code": "not_found", "message": "x"}}"#.utf8), nil)
            }
        }
    }

    func testAFlightSharedLiveGoesToTheServerWithTheToken() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )

        // A flight shared with a link, twenty seconds into the air.
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-live-test-\(UUID().uuidString)", isDirectory: true)
        let session = FlightSession(dataFolder: folder.path)
        let shared = try session.shareLive(
            server: "https://venti.example.org", random: Data(0..<26), audience: .link
        )
        XCTAssertEqual(shared.link, "https://venti.example.org/live/0123456789abcdefghjkmnpqrs")
        XCTAssertEqual(shared.words, "Begins at take-off")
        var request: ServerRequest?
        for second in 0..<20 where request == nil {
            session.onLocation(sample: LocationSample(
                timeUnixS: 1_795_028_700 + Double(second), lat: 47.66 + Double(second) * 0.0001, lon: 11.80,
                altitudeM: 1600 - Double(second), groundSpeedMs: 11, courseDeg: 0
            ))
            request = session.liveRequest(nowUnixS: 1_795_028_700 + Double(second))
        }
        let positions = try XCTUnwrap(request)

        // Without an account there is nobody to carry it to.
        let none = await account.carry(positions)
        XCTAssertNil(none)

        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        let came = await account.carry(positions)
        let answer = try XCTUnwrap(came)
        let last = await sent.requests.last
        let carried = try XCTUnwrap(last)
        XCTAssertEqual(carried.url?.absoluteString, "https://venti.example.org/v1/live/positions")
        XCTAssertEqual(carried.httpMethod, "POST")
        XCTAssertEqual(carried.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(carried.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertEqual(carried.timeoutInterval, 20)
        XCTAssertNotNil(carried.httpBody)

        // This made-up server is one from before live tracks: the core
        // puts its answer into words and sends nothing more.
        XCTAssertEqual(answer.status, 404)
        session.liveAnswer(
            status: answer.status, body: answer.body, retryAfterS: answer.retryAfter, nowUnixS: 1_795_028_730
        )
        let stopped = try XCTUnwrap(session.liveShare(nowUnixS: 1_795_028_730))
        XCTAssertEqual(stopped.state, .stopped)
        XCTAssertEqual(stopped.words, "This server has no live tracking.")
        XCTAssertNil(session.liveRequest(nowUnixS: 1_795_028_800))

        // Signed out again, the request is answered as a server would.
        box.token = nil
        let without = await account.carry(positions)
        let refused = try XCTUnwrap(without)
        XCTAssertEqual(refused.status, 401)
    }

    func testSigningInKeepsTheTokenAndSendsTheLogbook() async throws {
        let core = try makeCore()
        let flight = try core.importFlight(igcText: try sampleText()).id
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        XCTAssertFalse(account.isSignedIn)

        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")

        XCTAssertNil(account.problem)
        XCTAssertTrue(account.isSignedIn)
        XCTAssertEqual(account.account?.handle, "demo")
        XCTAssertEqual(box.token, "venti_test_token")
        // Signing in is followed by a sync, which sent the one flight.
        XCTAssertEqual(account.lastSync?.sent, 1)
        XCTAssertEqual(account.account?.flightsToSend, 0)

        let requests = await sent.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["POST", "GET", "POST"])
        XCTAssertTrue(requests.allSatisfy { $0.url?.host() == "venti.example.org" })
        // The password goes with the sign-in and the token does not; after
        // that the token goes with every request.
        XCTAssertNil(requests[0].value(forHTTPHeaderField: "Authorization"))
        XCTAssertEqual(requests[1].value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(requests[2].value(forHTTPHeaderField: "Content-Type"), "application/octet-stream")
        XCTAssertEqual(requests[2].httpBody, Data(try sampleText().utf8))

        // Sharing changes who may see the flight and gives its link.
        XCTAssertEqual(account.sharing(of: flight)?.visibility, .private)
        let shared = await account.share(flight, with: .unlisted)
        XCTAssertEqual(shared?.visibility, .unlisted)
        XCTAssertEqual(shared?.link, "https://venti.example.org/f/wkvydb8am8wmdcxzds9vkrw5kr")

        // Signing out tells the server and forgets the token; the flight stays.
        await account.signOut()
        XCTAssertNil(account.account)
        XCTAssertNil(box.token)
        XCTAssertEqual(core.logbook().totals.flights, 1)
        let last = await sent.requests.last
        XCTAssertEqual(last?.httpMethod, "DELETE")
        XCTAssertEqual(last?.url?.path, "/v1/sessions/session0000000000000000000a")
    }

    func testAKeychainThatRefusesTheSessionIsSaidAndNoSessionIsLeftBehind() async throws {
        let core = try makeCore()
        let sent = Sent()
        // A Keychain that keeps nothing, as an app without an identity has.
        let account = AccountModel(
            core: core, transport: server(noting: sent), readToken: { nil }, saveToken: { _ in }
        )

        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")

        XCTAssertFalse(account.isSignedIn)
        XCTAssertNil(account.account)
        XCTAssertNil(try core.account())
        XCTAssertEqual(
            account.problem,
            "This iPhone could not keep the session in its Keychain, so you are not signed in. Restart the iPhone and try again."
        )
        // The server was told to forget the session, with the session's own
        // token, and nothing else was asked of it.
        let requests = await sent.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["POST", "DELETE"])
        XCTAssertEqual(requests[1].url?.path, "/v1/sessions/session0000000000000000000a")
        XCTAssertEqual(requests[1].value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
    }

    func testAWrongPasswordIsSaidAndNothingIsKept() async throws {
        let box = TokenBox()
        let account = AccountModel(
            core: try makeCore(),
            transport: { _ in
                (401, Data(#"{"error": {"code": "invalid_credentials", "message": "x"}}"#.utf8), nil)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo", password: "wrong"
        )
        XCTAssertEqual(account.problem, "The handle or the password is wrong.")
        XCTAssertFalse(account.isSignedIn)
        XCTAssertNil(box.token)
    }

    func testAnEndedSessionAsksThePilotToSignInAgain() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let good = server(noting: sent)
        let hasEnded = TokenBox()
        let account = AccountModel(
            core: core,
            transport: { request in
                if hasEnded.token != nil, request.url?.path == "/v1/flights/changes" {
                    return (401, Data(#"{"error": {"code": "unauthorized", "message": "x"}}"#.utf8), nil)
                }
                return try await good(request)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo",
            password: "demo-pilot-password"
        )
        XCTAssertTrue(account.isSignedIn)

        hasEnded.token = "yes"
        await account.sync(force: true)

        XCTAssertFalse(account.isSignedIn)
        XCTAssertEqual(account.account?.needsSignIn, true)
        XCTAssertNil(box.token)
        XCTAssertEqual(account.problem, "This device is no longer signed in. Sign in again.")
    }

    func testAReportGoesToTheServerFromTheAccountAndSaysWhatBecameOfIt() async throws {
        let (sent, box) = (Sent(), TokenBox())
        let good = server(noting: sent)
        let account = AccountModel(
            core: try makeCore(),
            transport: { request in
                if request.url?.path == "/v1/reports" {
                    await sent.add(request)
                    return (201, Data(#"{"created": true}"#.utf8), nil)
                }
                return try await good(request)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )

        // Without an account nothing is sent.
        do {
            _ = try await account.report(.pilot(handle: "mara"), reason: .spam, text: "")
            XCTFail("A report without an account was sent")
        } catch {
            XCTAssertEqual(AccountModel.words(for: error), "This iPhone is no longer signed in. Sign in again.")
        }
        let before = await sent.requests
        XCTAssertTrue(before.isEmpty)

        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo",
            password: "demo-pilot-password"
        )
        let outcome = try await account.report(
            .flight(id: "fbyw03atd9qdt5bfe177ra1k94"), reason: .notAFlight, text: " This is my track. "
        )
        XCTAssertEqual(outcome, "Sent. Whoever runs Venti will read it.")

        // Signing in may have set a sync going: only the reports count.
        let reports = await sent.requests.filter { $0.url?.path == "/v1/reports" }
        XCTAssertEqual(reports.count, 1)
        let report = try XCTUnwrap(reports.last)
        XCTAssertEqual(report.httpMethod, "POST")
        XCTAssertEqual(report.url?.absoluteString, "https://venti.example.org/v1/reports")
        XCTAssertEqual(report.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(report.value(forHTTPHeaderField: "Content-Type"), "application/json")
        let body = try XCTUnwrap(report.httpBody)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(json["reason"] as? String, "not_a_flight")
        // Cleaned as the server would clean it.
        XCTAssertEqual(json["text"] as? String, "This is my track.")
        XCTAssertEqual((json["subject"] as? [String: String])?["flight"], "fbyw03atd9qdt5bfe177ra1k94")

        // A report that has to say what is wrong, and does not, is refused
        // before anything is sent.
        do {
            _ = try await account.report(.pilot(handle: "mara"), reason: .other, text: " ")
            XCTFail("A report without words was sent")
        } catch {
            XCTAssertEqual(AccountModel.words(for: error), "Say in a few words what is wrong.")
        }
        let after = await sent.requests.filter { $0.url?.path == "/v1/reports" }
        XCTAssertEqual(after.count, 1)

        // Two addresses of one server are one server.
        XCTAssertTrue(ReportSheet.same("https://venti.example.org", "https://Venti.example.org/"))
        XCTAssertFalse(ReportSheet.same("https://venti.example.org", "https://other.example.org"))
    }
}

/// Answers of the Venti server, in its own format.
private enum ServerAnswers {
    static let signIn = """
        {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z"},
         "session": {"id": "session0000000000000000000a", "created_at": "2026-11-18T19:05:00Z",
                     "last_used_at": "2026-11-18T19:05:00Z", "current": true},
         "token": "venti_test_token"}
        """

    static let noChanges = """
        {"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}
        """

    static let upload = """
        {"flight": {"id": "wkvydb8am8wmdcxzds9vkrw5kr", "identity": "d76b1d1a62211781e7fac7e5db815546",
          "pilot": {"handle": "demo", "display_name": "Demo Pilot"}, "visibility": "private",
          "uploaded_at": "2026-11-18T19:05:00Z", "start": "2026-09-01T10:30:00Z", "airtime_s": 1744,
          "track_length_m": 18402.3, "max_altitude_m": 2117, "max_climb_ms": 2.4,
          "outline": [[47.6642, 11.7967], [47.6697, 11.78986]], "file_bytes": 64678, "analysis_version": 1},
         "created": true}
        """
}
