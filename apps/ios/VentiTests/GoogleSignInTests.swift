import VentiCore
import XCTest

@testable import Venti

/// "Continue with Google": Google's authorization code flow with PKCE as
/// the app builds it, and the account model through the handle step,
/// linking and unlinking, against the real core with canned answers.
@MainActor
final class GoogleSignInTests: XCTestCase {
    private let client = GoogleClient(
        clientID: "490070141586-pknvec3oddmk79fjbe0238lg8r259p12.apps.googleusercontent.com",
        redirectScheme: "com.googleusercontent.apps.490070141586-pknvec3oddmk79fjbe0238lg8r259p12"
    )

    // MARK: Google's side

    func testTheBuildNamesGooglesIOSClient() throws {
        let built = try XCTUnwrap(GoogleClient.builtIn)
        XCTAssertEqual(built, client)
    }

    func testTheChallengeIsTheVerifiersSHA256InBase64URL() {
        // Worked out apart from the app: Python's hashlib and base64.
        XCTAssertEqual(
            GoogleAuthorization.challenge(for: "dBjftJeZ4CVP-mJ92K1qnTvdQ9g6GtbVlObZ44wkLc0"),
            "MepheJyMC3cnGeK1vmlcMbU2zMif0V4Pc0Mb8r72LCo"
        )
    }

    func testThePageAsksForACodeWithPKCE() throws {
        let attempt = GoogleAuthorization.attempt(for: client, verifier: "verifier", state: "state-1")
        let components = try XCTUnwrap(URLComponents(url: attempt.url, resolvingAgainstBaseURL: false))
        XCTAssertEqual(components.host, "accounts.google.com")
        XCTAssertEqual(components.path, "/o/oauth2/v2/auth")
        let query = Dictionary(uniqueKeysWithValues: (components.queryItems ?? []).map { ($0.name, $0.value ?? "") })
        XCTAssertEqual(query["client_id"], client.clientID)
        XCTAssertEqual(query["redirect_uri"], client.redirectScheme + ":/oauth2redirect")
        XCTAssertEqual(query["response_type"], "code")
        XCTAssertEqual(query["scope"], "openid email profile")
        XCTAssertEqual(query["code_challenge_method"], "S256")
        XCTAssertEqual(query["code_challenge"], GoogleAuthorization.challenge(for: "verifier"))
        XCTAssertEqual(query["state"], "state-1")
        // A fresh attempt has a verifier and a state of its own.
        let one = GoogleAuthorization.attempt(for: client)
        let two = GoogleAuthorization.attempt(for: client)
        XCTAssertNotEqual(one.verifier, two.verifier)
        XCTAssertNotEqual(one.state, two.state)
        XCTAssertGreaterThanOrEqual(one.verifier.count, 43)
    }

    func testOnlyTheAnswerToThisAttemptGivesACode() throws {
        let attempt = GoogleAuthorization.attempt(for: client, verifier: "v", state: "mine")
        let scheme = client.redirectScheme
        XCTAssertEqual(
            GoogleAuthorization.code(in: try XCTUnwrap(URL(string: "\(scheme):/oauth2redirect?state=mine&code=4/abc")), for: attempt),
            "4/abc"
        )
        XCTAssertNil(GoogleAuthorization.code(in: try XCTUnwrap(URL(string: "\(scheme):/oauth2redirect?state=other&code=4/abc")), for: attempt))
        XCTAssertNil(GoogleAuthorization.code(in: try XCTUnwrap(URL(string: "\(scheme):/oauth2redirect?state=mine&error=access_denied")), for: attempt))
    }

    func testTheCodeIsExchangedWithoutASecret() throws {
        let request = GoogleAuthorization.tokenRequest(client: client, code: "4/a+b", verifier: "the-verifier")
        XCTAssertEqual(request.url?.absoluteString, "https://oauth2.googleapis.com/token")
        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/x-www-form-urlencoded")
        let body = try XCTUnwrap(request.httpBody.flatMap { String(data: $0, encoding: .utf8) })
        XCTAssertTrue(body.contains("code=4%2Fa%2Bb"), body)
        XCTAssertTrue(body.contains("code_verifier=the-verifier"), body)
        XCTAssertTrue(body.contains("grant_type=authorization_code"), body)
        XCTAssertTrue(body.contains("client_id=\(client.clientID)"), body)
        XCTAssertFalse(body.contains("client_secret"), body)
    }

    func testTheIDTokenIsReadFromGooglesAnswer() {
        let answer = Data(#"{"access_token": "ya29", "id_token": "eyJ.token.sig", "expires_in": 3599}"#.utf8)
        XCTAssertEqual(GoogleAuthorization.idToken(status: 200, body: answer), "eyJ.token.sig")
        XCTAssertNil(GoogleAuthorization.idToken(status: 400, body: Data(#"{"error": "invalid_grant"}"#.utf8)))
        XCTAssertNil(GoogleAuthorization.idToken(status: 200, body: Data("{}".utf8)))
    }

    // MARK: The account

    /// What the model sent.
    private actor Sent {
        var requests: [URLRequest] = []
        func add(_ request: URLRequest) { requests.append(request) }
        var paths: [String] { requests.map { "\($0.httpMethod ?? "") \($0.url?.path ?? "")" } }
    }

    private final class TokenBox: @unchecked Sendable {
        var token: String?
    }

    private func makeCore() throws -> Core {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-google-test-\(UUID().uuidString)", isDirectory: true)
        return try Core(dataFolder: folder.path)
    }

    /// A server with the made-up Google: a first sign-in goes on to the
    /// handle step; once the account is made, Google signs it in.
    private func server(noting sent: Sent) -> AccountModel.Transport {
        { request in
            await sent.add(request)
            switch (request.httpMethod ?? "", request.url?.path ?? "") {
            case ("POST", "/v1/auth/google"):
                let made = await sent.paths.contains("POST /v1/auth/google/account")
                return made
                    ? (201, Data(Answers.signedIn.utf8), nil)
                    : (200, Data(Answers.needsHandle.utf8), nil)
            case ("POST", "/v1/auth/google/account"):
                return (201, Data(Answers.signedIn.utf8), nil)
            case ("POST", "/v1/account/google"):
                return (200, Data(Answers.account.utf8), nil)
            case ("DELETE", "/v1/account/google"):
                return (204, Data(), nil)
            case ("GET", "/v1/flights/changes"):
                return (200, Data(#"{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"#.utf8), nil)
            case ("GET", "/v1/account/kept-sites/changes"):
                return (200, Data(#"{"kept": [], "forgotten": [], "cursor": "1", "more": false, "reset": false}"#.utf8), nil)
            case ("DELETE", _):
                return (204, Data(), nil)
            default:
                return (404, Data(#"{"error": {"code": "not_found", "message": "x"}}"#.utf8), nil)
            }
        }
    }

    func testAFirstSignInWithGoogleGoesThroughTheHandleStep() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent), readToken: { box.token }, saveToken: { box.token = $0 }
        )
        let venti = try XCTUnwrap(URL(string: "https://venti.example.org"))

        await account.continueWithGoogle(server: venti, idToken: "made-up-id-token")
        XCTAssertNil(account.account)
        XCTAssertNil(account.problem)
        let signUp = try XCTUnwrap(account.googleSignUp)
        XCTAssertEqual(signUp.suggestedHandle, "ui_test")
        XCTAssertEqual(signUp.name, "UI Test")
        XCTAssertEqual(signUp.suggestionLine, "@ui_test is free. It was made from your Google account; change it if you like.")
        let first = await sent.requests.first
        let body = try XCTUnwrap(first?.httpBody.flatMap { String(data: $0, encoding: .utf8) })
        XCTAssertTrue(body.contains("made-up-id-token"), body)
        XCTAssertNil(first?.value(forHTTPHeaderField: "Authorization"))

        await account.createGoogleAccount(server: venti, handle: "ui_test", displayName: "UI Test", invite: " ")
        XCTAssertNil(account.problem)
        XCTAssertNil(account.googleSignUp)
        XCTAssertEqual(account.account?.handle, "ui_test")
        XCTAssertEqual(account.account?.googleEmail, "ui.test@example.com")
        XCTAssertEqual(box.token, "venti_google_token")
        let made = await sent.requests.first { $0.url?.path == "/v1/auth/google/account" }
        let madeBody = try XCTUnwrap(made?.httpBody.flatMap { String(data: $0, encoding: .utf8) })
        XCTAssertTrue(madeBody.contains("venti_signup_1"), madeBody)
        XCTAssertFalse(madeBody.contains("invite"), madeBody)

        // Signed out and in again: no handle step.
        await account.signOut()
        XCTAssertNil(account.account)
        await account.continueWithGoogle(server: venti, idToken: "made-up-id-token")
        XCTAssertNil(account.googleSignUp)
        XCTAssertEqual(account.account?.handle, "ui_test")
    }

    func testGoogleIsUnlinkedAndLinkedAgain() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent), readToken: { box.token }, saveToken: { box.token = $0 }
        )
        let venti = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.continueWithGoogle(server: venti, idToken: "t")
        await account.createGoogleAccount(server: venti, handle: "ui_test", displayName: "UI Test", invite: "")
        XCTAssertEqual(account.account?.googleEmail, "ui.test@example.com")

        await account.unlinkGoogle()
        XCTAssertNil(account.problem)
        XCTAssertNil(account.account?.googleEmail)
        XCTAssertEqual(account.account?.handle, "ui_test")

        await account.linkGoogle(idToken: "another-id-token")
        XCTAssertNil(account.problem)
        XCTAssertEqual(account.account?.googleEmail, "ui.test@example.com")
        let link = await sent.requests.first { $0.httpMethod == "POST" && $0.url?.path == "/v1/account/google" }
        XCTAssertEqual(link?.value(forHTTPHeaderField: "Authorization"), "Bearer venti_google_token")
    }

    func testNoTokenFromGoogleIsSaidInTheCoresWords() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent), readToken: { box.token }, saveToken: { box.token = $0 }
        )
        await account.continueWithGoogle(server: try XCTUnwrap(URL(string: "https://venti.example.org")), idToken: "")
        XCTAssertEqual(account.problem, "Google gave no sign-in. Try again.")
        let none = await sent.requests.isEmpty
        XCTAssertTrue(none)
    }
}

private enum Answers {
    static let needsHandle = """
        {"needs_handle": true, "signup": "venti_signup_1", "suggested_handle": "ui_test", "name": "UI Test"}
        """

    static let signedIn = """
        {"account": {"handle": "ui_test", "display_name": "UI Test", "created_at": "2026-11-18T19:05:00Z",
                     "google_email": "ui.test@example.com"},
         "session": {"id": "session0000000000000000000g", "created_at": "2026-11-18T19:05:00Z",
                     "last_used_at": "2026-11-18T19:05:00Z", "current": true},
         "token": "venti_google_token"}
        """

    static let account = """
        {"account": {"handle": "ui_test", "display_name": "UI Test", "created_at": "2026-11-18T19:05:00Z",
                     "google_email": "ui.test@example.com"}}
        """
}
