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
