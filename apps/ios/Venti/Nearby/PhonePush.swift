import Foundation
import Observation
import UIKit
import UserNotifications
import VentiCore

/// Push through Apple's service (owner decision 24; docs/api.md, "Push to
/// the phone apps"): permission is asked the first time something would be
/// sent (an alarm saved, a setting of pilots nearby turned on), never at
/// the start; the iPhone's token goes to Venti on sign-in and when it
/// changes, and comes off before signing out.
///
/// Nothing is sent to the iPhone until the owner's APNs key is on the
/// server; registering works either way. In a simulator Apple may give a
/// token or none: without one nothing is registered, and nothing else
/// changes.
@MainActor
@Observable
final class PhonePush {
    /// The one the app runs with; the app delegate hands it the token.
    static let shared = PhonePush()

    /// The token Apple gave this app on this iPhone, in hexadecimal.
    private(set) var token: String?

    /// Sends a request with the session's token. The app sets it.
    @ObservationIgnored var send: AlertsModel.Send?
    /// Whether a pilot is signed in. The app sets it.
    @ObservationIgnored var isSignedIn: @MainActor () -> Bool = { false }
    /// The token last registered on the account signed in, so that it is
    /// not sent twice in a run; forgotten when the account changes.
    @ObservationIgnored private var registered: String?

    /// A build for development has a token of Apple's sandbox.
    static let sandbox: Bool = {
        #if DEBUG
        true
        #else
        false
        #endif
    }()

    /// A device token as Venti takes it: hexadecimal, lower case.
    nonisolated static func hex(_ token: Data) -> String {
        token.map { String(format: "%02x", $0) }.joined()
    }

    // MARK: Permission and registering

    /// Asks whether the app may notify, if it has not asked yet, and
    /// registers with Apple once allowed.
    func askAndRegister() async {
        let centre = UNUserNotificationCenter.current()
        if await centre.notificationSettings().authorizationStatus == .notDetermined {
            _ = try? await centre.requestAuthorization(options: [.alert, .sound])
        }
        await registerIfAllowed()
    }

    /// Registers with Apple where the pilot has allowed notifications:
    /// at every start, since a token can change, and after signing in.
    func registerIfAllowed() async {
        guard !LaunchOptions.isUnderTest || LaunchOptions.isUITesting else { return }
        let status = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
        guard status == .authorized || status == .provisional || status == .ephemeral else { return }
        UIApplication.shared.registerForRemoteNotifications()
        // A token already had is sent at once: Apple answers again only
        // when it is asked again, and a sign-in needs it on the account.
        await sendToken()
    }

    /// Apple gave a token, new or the same.
    func received(_ data: Data) {
        token = Self.hex(data)
        Task { await sendToken() }
    }

    /// Sends the token to the account, once for each account and token.
    func sendToken() async {
        guard let token, isSignedIn(), let send, token != registered else { return }
        do {
            let request = try deviceRegisterRequest(platform: .apple, token: token, sandbox: Self.sandbox)
            let answer = try await send(request)
            try phoneRequestDone(status: answer.status, body: answer.body)
            registered = token
        } catch {
            // Tried again at the next start or sign-in.
        }
    }

    /// The account changed: a new sign-in registers the token anew.
    func accountChanged() async {
        registered = nil
        guard isSignedIn() else { return }
        await registerIfAllowed()
    }

    /// Takes the iPhone off the account, before signing out: it is told
    /// nothing more. Signing out removes it on Venti too.
    func removeBeforeSigningOut() async {
        defer { registered = nil }
        guard let token, isSignedIn(), let send,
              let request = try? deviceRemoveRequest(platform: .apple, token: token)
        else { return }
        _ = try? await send(request)
    }
}

/// The app's delegate, for what SwiftUI does not hand over: the token of
/// Apple's push service.
final class VentiAppDelegate: NSObject, UIApplicationDelegate {
    func application(
        _ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        MainActor.assumeIsolated {
            PhonePush.shared.received(deviceToken)
        }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: any Error) {
        // No token (a simulator, no network, no entitlement): nothing is
        // registered, and the app works as before.
    }
}
