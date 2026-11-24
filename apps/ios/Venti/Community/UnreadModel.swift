import Foundation
import Observation
import VentiCore

/// How much is new for the pilot: the notifications and the lines of what
/// the alarms fired after the account's mark of what the pilot saw, the
/// same on every device (`docs/api.md`, `GET /v1/account/unread`). The
/// Community tab shows the core's `badge`, VoiceOver its `label`.
///
/// No count is kept on the iPhone: the server keeps the mark, and this
/// reads it when the app comes to the front, after signing in and out,
/// when a push arrives, and moves it when the pilot opened the
/// notifications or the alerts (`seen`). The words are the core's
/// (`social.rs`, `unread`).
@MainActor
@Observable
final class UnreadModel {
    /// Sends a request with the session's token and returns the answer.
    typealias Send = @MainActor (ServerRequest) async throws -> AccountModel.Answer

    /// The last count Venti gave; `nil` signed out and before the first
    /// answer. Kept as it was when a read fails: a badge is no promise.
    private(set) var unread: Unread?

    @ObservationIgnored private let send: Send
    @ObservationIgnored private let isSignedIn: @MainActor () -> Bool
    /// The cursor of the last fetch of what the alarms fired, which the
    /// alerts' list shows (the core's `alertsFiredCursor`); `nil` before
    /// the first.
    @ObservationIgnored private let alarmsCursor: @MainActor () -> String?

    init(
        send: @escaping Send, isSignedIn: @escaping @MainActor () -> Bool,
        alarmsCursor: @escaping @MainActor () -> String? = { nil }
    ) {
        self.send = send
        self.isSignedIn = isSignedIn
        self.alarmsCursor = alarmsCursor
    }

    /// With the app's account and its core.
    convenience init(account: AccountModel, core: Core) {
        self.init(
            send: { try await account.answer(to: $0) }, isSignedIn: { account.isSignedIn },
            alarmsCursor: { try? core.alertsFiredCursor() }
        )
    }

    /// What the badge says: "3", "99+"; `nil` for none. The groups' unread
    /// messages are a marker beside the notifications' (`groups`, not in
    /// `total`), and the tab has one badge: the two together.
    var badge: String? {
        guard let unread else { return nil }
        guard unread.groups > 0 else { return unread.badge }
        return Self.badge(of: unread.total + unread.groups)
    }

    /// What VoiceOver says of it: "3 new".
    var label: String? {
        guard let unread else { return nil }
        guard unread.groups > 0 else { return unread.label }
        return badge.map { "\($0) new" }
    }

    /// The core's form of a badge: the count, "99+" over 99, none for 0.
    nonisolated static func badge(of count: UInt32) -> String? {
        switch count {
        case 0: nil
        case 1...99: String(count)
        default: "99+"
        }
    }

    /// Reads the count anew; signed out there is none.
    func read() async {
        await ask(unreadRequest())
    }

    /// The pilot was shown the notifications up to `cursor`, the cursor
    /// of the last answer the list read; the alarms' mark stays where it
    /// is (`nil`).
    func sawNotifications(upTo cursor: String) async {
        await seen(notificationsCursor: cursor, alarmsCursor: nil)
    }

    /// The pilot was shown what the alarms fired, up to the cursor of the
    /// fetch the list shows; the notifications' mark stays where it is.
    /// Before the first fetch nothing was shown, and neither mark moves.
    func sawAlarms() async {
        await seen(notificationsCursor: nil, alarmsCursor: alarmsCursor())
    }

    /// Moves the mark, then reads the count the answer gives; where the
    /// mark could not be moved, reads it anew.
    private func seen(notificationsCursor: String?, alarmsCursor: String?) async {
        guard isSignedIn() else {
            unread = nil
            return
        }
        guard let request = try? unreadSeenRequest(
            notificationsCursor: notificationsCursor, alarmsCursor: alarmsCursor
        ) else {
            await read()
            return
        }
        if await !ask(request) {
            await read()
        }
    }

    /// Sends a request the core's `unread` reads the answer of. Returns
    /// whether an answer was read.
    @discardableResult
    private func ask(_ request: ServerRequest) async -> Bool {
        guard isSignedIn() else {
            unread = nil
            return true
        }
        do {
            let answer = try await send(request)
            let read = try VentiCore.unread(status: answer.status, body: answer.body)
            // Signed out while it was asked: nothing is shown.
            unread = isSignedIn() ? read : nil
            return true
        } catch {
            if !isSignedIn() {
                unread = nil
            }
            return false
        }
    }
}
