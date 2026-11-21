import Foundation
import Observation
import VentiCore

/// "Programs with access": the tokens a pilot gives to a logger, a script
/// or another maker's app, as the screens see them.
///
/// The words, the requests and what the answers mean are the core's. This
/// class sends the requests with the session, as the account's others go,
/// and holds the list. The secret of a new token is held here, in memory,
/// only while its page is open: it is written nowhere.
@MainActor
@Observable
final class ProgramsModel {
    /// Sends a request with the session's token and returns the answer.
    typealias Send = @MainActor (ServerRequest) async throws -> AccountModel.Answer

    /// How the list stands.
    enum State: Equatable {
        case loading
        case loaded([TokenView])
        /// Why it could not be read, in the core's words.
        case failed(String)
    }

    /// The words of the screen and of the form.
    let form = tokenForm()
    private(set) var state: State = .loading
    /// Whether a request is under way.
    private(set) var isWorking = false
    /// The token just made, with its secret, until its page is left.
    private(set) var created: TokenCreated?
    /// Why the form's request was refused, in the core's words.
    private(set) var refusal: String?
    /// Why an access could not be withdrawn, in the core's words.
    private(set) var problem: String?

    @ObservationIgnored private let send: Send
    @ObservationIgnored private let now: () -> Int64

    init(send: @escaping Send, now: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970) }) {
        self.send = send
        self.now = now
    }

    /// With the app's account.
    convenience init(account: AccountModel) {
        self.init(send: { try await account.answer(to: $0) })
    }

    /// The programs with access, the newest first.
    var tokens: [TokenView] {
        if case .loaded(let tokens) = state {
            return tokens
        }
        return []
    }

    /// Reads the list.
    func load() async {
        do {
            let answer = try await send(tokensRequest())
            state = .loaded(try VentiCore.tokens(status: answer.status, body: answer.body, nowUnixS: now()))
        } catch {
            state = .failed(AccountModel.words(for: error))
        }
    }

    /// What is wrong with a name, in the core's words; `nil` for a name
    /// the server takes.
    func nameFault(_ name: String) -> String? {
        do {
            _ = try tokenNameCheck(name: name)
            return nil
        } catch {
            return error.words
        }
    }

    /// Makes a token: `scopes` are the chosen ones of the form's, by
    /// their names. Returns whether it was made; if so `created` holds it
    /// with its secret, and the list begins with it.
    func give(name: String, scopes: Set<String>, endsUnixS: Int64?, password: String) async -> Bool {
        guard !isWorking else { return false }
        isWorking = true
        defer { isWorking = false }
        refusal = nil
        do {
            let request = try tokenCreateRequest(
                name: name, scopes: form.scopes.map(\.scope).filter(scopes.contains),
                endsUnixS: endsUnixS, password: password
            )
            let answer = try await send(request)
            let made = try tokenCreated(status: answer.status, body: answer.body, nowUnixS: now())
            created = made
            state = .loaded([made.token] + tokens.filter { $0.id != made.token.id })
            return true
        } catch {
            refusal = AccountModel.words(for: error)
            return false
        }
    }

    /// Drops the secret: its page was left. Venti does not show it again,
    /// and neither does the app.
    func forgetSecret() {
        created = nil
    }

    /// Clears what the last try of the form was refused with.
    func clearRefusal() {
        refusal = nil
    }

    /// Withdraws a program's access, and reads the list again.
    func withdraw(_ token: TokenView) async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            let answer = try await send(try tokenWithdrawRequest(id: token.id))
            try tokenWithdrawn(status: answer.status, body: answer.body)
            problem = nil
            state = .loaded(tokens.filter { $0.id != token.id })
        } catch {
            problem = AccountModel.words(for: error)
            return
        }
        await load()
    }

    /// The moment an access that "ends on" a day is no longer good: the
    /// day's last second, counted in UTC as the core names a token's days,
    /// so that the list then says "on" the day the pilot chose.
    nonisolated static func end(of day: Date, in calendar: Calendar = .current) -> Int64? {
        // The day as the pilot's calendar has it, read by the Gregorian
        // one: the picker may count years another way.
        var gregorian = Calendar(identifier: .gregorian)
        gregorian.timeZone = calendar.timeZone
        let chosen = gregorian.dateComponents([.year, .month, .day], from: day)
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = .gmt
        var last = DateComponents()
        (last.year, last.month, last.day) = (chosen.year, chosen.month, chosen.day)
        (last.hour, last.minute, last.second) = (23, 59, 59)
        return utc.date(from: last).map { Int64($0.timeIntervalSince1970) }
    }
}
