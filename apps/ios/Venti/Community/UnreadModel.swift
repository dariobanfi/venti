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

    /// The cursor that moves no mark: the server's marks only move
    /// forward, so a list named with it is left as it was
    /// (`crates/server/src/api/unread.rs`, `seen`). A list the request
    /// leaves out is seen up to its newest line, which is not what the
    /// pilot saw when they opened the other list.
    static let notShown = "0"

    @ObservationIgnored private let send: Send
    @ObservationIgnored private let isSignedIn: @MainActor () -> Bool

    init(send: @escaping Send, isSignedIn: @escaping @MainActor () -> Bool) {
        self.send = send
        self.isSignedIn = isSignedIn
    }

    /// With the app's account.
    convenience init(account: AccountModel) {
        self.init(send: { try await account.answer(to: $0) }, isSignedIn: { account.isSignedIn })
    }

    /// What the badge says: "3", "99+"; `nil` for none.
    var badge: String? { unread?.badge }
    /// What VoiceOver says of it: "3 new".
    var label: String? { unread?.label }

    /// Reads the count anew; signed out there is none.
    func read() async {
        await ask(unreadRequest())
    }

    /// The pilot was shown the notifications up to `cursor`, the cursor
    /// of the last answer the list read.
    func sawNotifications(upTo cursor: String) async {
        await seen(notificationsCursor: cursor, alarmsCursor: Self.notShown)
    }

    /// The pilot was shown what the alarms fired: the core keeps that
    /// fetch's cursor to itself, and the list is everything the last sync
    /// brought, so the mark goes up to the newest.
    func sawAlarms() async {
        await seen(notificationsCursor: Self.notShown, alarmsCursor: nil)
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
