import AuthenticationServices
import CryptoKit
import Foundation
import SwiftUI

/// Google's iOS client the app was built with: the build setting
/// `VENTI_GOOGLE_CLIENT_ID` in `Config/Base.xcconfig`, which reaches the
/// app as `VentiGoogleClientID` in its Info.plist, and the URL scheme Google
/// sends the pilot back by. Not secrets: they ship in every copy.
struct GoogleClient: Equatable, Sendable {
    let clientID: String
    let redirectScheme: String

    /// The client in the build; `nil` for a build without one.
    static var builtIn: GoogleClient? {
        let info = Bundle.main.infoDictionary ?? [:]
        guard let id = info["VentiGoogleClientID"] as? String, !id.isEmpty,
              let scheme = info["VentiGoogleRedirectScheme"] as? String, !scheme.isEmpty
        else { return nil }
        return GoogleClient(clientID: id, redirectScheme: scheme)
    }

    /// Where Google sends the pilot back to: the reversed client ID with a
    /// path, as Google's guide for iOS clients writes it *(to verify against
    /// a sign-in on a phone)*.
    var redirectURI: String { "\(redirectScheme):/oauth2redirect" }
}

/// Google's authorization code flow with PKCE, which an iOS client goes
/// through without a secret and without Google's SDK: the page the pilot
/// signs in on, the code Google sends back, and the code exchanged for
/// the ID token the server checks. What the server makes of the token is
/// the core's.
enum GoogleAuthorization {
    static let authorizationEndpoint = URL(string: "https://accounts.google.com/o/oauth2/v2/auth")!
    static let tokenEndpoint = URL(string: "https://oauth2.googleapis.com/token")!

    /// One attempt: the page to open, and what the answer is checked with.
    struct Attempt: Equatable, Sendable {
        let url: URL
        /// The PKCE secret that only this iPhone knows; Google gets its hash.
        let verifier: String
        /// What Google sends back unchanged, so that an answer to another
        /// attempt is not taken.
        let state: String
    }

    /// A new attempt for the client, with a fresh verifier and state.
    static func attempt(for client: GoogleClient) -> Attempt {
        attempt(for: client, verifier: randomText(), state: randomText())
    }

    static func attempt(for client: GoogleClient, verifier: String, state: String) -> Attempt {
        var components = URLComponents(url: authorizationEndpoint, resolvingAgainstBaseURL: false)!
        components.queryItems = [
            URLQueryItem(name: "client_id", value: client.clientID),
            URLQueryItem(name: "redirect_uri", value: client.redirectURI),
            URLQueryItem(name: "response_type", value: "code"),
            URLQueryItem(name: "scope", value: "openid email profile"),
            URLQueryItem(name: "code_challenge", value: challenge(for: verifier)),
            URLQueryItem(name: "code_challenge_method", value: "S256"),
            URLQueryItem(name: "state", value: state),
            URLQueryItem(name: "prompt", value: "select_account"),
        ]
        return Attempt(url: components.url!, verifier: verifier, state: state)
    }

    /// PKCE's S256: the verifier's SHA-256, in base64url without padding.
    static func challenge(for verifier: String) -> String {
        base64URL(Data(SHA256.hash(data: Data(verifier.utf8))))
    }

    /// The code in the address Google sent the pilot back by, if it
    /// answers this attempt; `nil` for a refusal or another attempt's.
    static func code(in callback: URL, for attempt: Attempt) -> String? {
        let items = URLComponents(url: callback, resolvingAgainstBaseURL: false)?.queryItems ?? []
        func value(_ name: String) -> String? { items.first { $0.name == name }?.value }
        guard value("state") == attempt.state, let code = value("code"), !code.isEmpty else { return nil }
        return code
    }

    /// The request that exchanges the code for the tokens.
    static func tokenRequest(client: GoogleClient, code: String, verifier: String) -> URLRequest {
        var request = URLRequest(url: tokenEndpoint)
        request.httpMethod = "POST"
        request.timeoutInterval = 30
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        let fields = [
            ("client_id", client.clientID),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", client.redirectURI),
        ]
        request.httpBody = Data(
            fields.map { "\($0.0)=\(formEncoded($0.1))" }.joined(separator: "&").utf8
        )
        return request
    }

    /// The ID token in Google's answer to the exchange; `nil` where there
    /// is none, which the core then refuses in its words.
    static func idToken(status: Int, body: Data) -> String? {
        guard (200..<300).contains(status),
              let answer = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
              let token = answer["id_token"] as? String, !token.isEmpty
        else { return nil }
        return token
    }

    private static func randomText() -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        for index in bytes.indices {
            bytes[index] = UInt8.random(in: .min ... .max)
        }
        return base64URL(Data(bytes))
    }

    private static func base64URL(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private static func formEncoded(_ text: String) -> String {
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        return text.addingPercentEncoding(withAllowedCharacters: allowed) ?? text
    }
}

/// Where a view gets a Google ID token: Google's page in the system's
/// browser sheet, or, in a UI test of a Debug build, the development
/// server's made-up Google.
@MainActor
struct GoogleTokens {
    let session: WebAuthenticationSession
    let server: URL?

    /// An ID token, `""` where Google gave none (the core then says so in
    /// its words), or `nil` when the pilot closed Google's page.
    func idToken() async throws -> String? {
        #if DEBUG
        if let account = LaunchOptions.devGoogleAccount, let server {
            return try await Self.devToken(from: server, email: account.email, name: account.name)
        }
        #endif
        guard let client = GoogleClient.builtIn else { return "" }
        let attempt = GoogleAuthorization.attempt(for: client)
        let callback: URL
        do {
            callback = try await session.authenticate(
                using: attempt.url, callback: .customScheme(client.redirectScheme),
                preferredBrowserSession: nil, additionalHeaderFields: [:]
            )
        } catch let error as ASWebAuthenticationSessionError where error.code == .canceledLogin {
            return nil
        }
        guard let code = GoogleAuthorization.code(in: callback, for: attempt) else { return "" }
        let request = GoogleAuthorization.tokenRequest(client: client, code: code, verifier: attempt.verifier)
        let (body, response) = try await URLSession(configuration: .ephemeral).data(for: request)
        return GoogleAuthorization.idToken(status: (response as? HTTPURLResponse)?.statusCode ?? 0, body: body) ?? ""
    }

    #if DEBUG
    /// The development server's ID token for a made-up Google account:
    /// what a UI test signs in "with Google" by, without Google.
    static func devToken(from server: URL, email: String, name: String?) async throws -> String {
        var components = URLComponents(
            url: server.appendingPathComponent("v1/dev/google-id-token"), resolvingAgainstBaseURL: false
        )!
        components.queryItems = [URLQueryItem(name: "email", value: email)]
            + (name.map { [URLQueryItem(name: "name", value: $0)] } ?? [])
        let (body, response) = try await URLSession(configuration: .ephemeral).data(from: components.url!)
        return GoogleAuthorization.idToken(status: (response as? HTTPURLResponse)?.statusCode ?? 0, body: body) ?? ""
    }
    #endif
}
