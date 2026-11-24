import Foundation
import Observation
import VentiCore

/// What pilots nearby do for each other (docs/design.md, "Push to the
/// phone and what pilots nearby do" and "Help after landing"): the two
/// settings of the account, the coarse position told while the pilot may
/// be asked for help, and the calls for help that stand.
///
/// The requests, the answers and the words are the core's (`phones.rs`).
/// This class sends the requests with the session, as the account's others
/// go, and holds the last answers. Nothing of it is kept on the iPhone.
@MainActor
@Observable
final class NearbyModel {
    /// Sends a request with the session's token and returns the answer.
    typealias Send = @MainActor (ServerRequest) async throws -> AccountModel.Answer

    /// The account's settings as Venti last said them; `nil` until it has.
    private(set) var choice: NearbyChoice?
    /// Why the settings could not be read or changed, in words.
    private(set) var settingsProblem: String?
    /// The calls for help that stand, the pilot's own and those they were
    /// asked; `nil` until Venti has answered.
    private(set) var help: HelpLists?
    /// Why the calls could not be read, in words.
    private(set) var helpProblem: String?

    /// How often the position is told while the app is in front and the
    /// pilot may be asked for help (docs/design.md: every 10 minutes).
    static let positionEvery: Duration = .seconds(10 * 60)

    /// The changes of the settings sent in this run, and those not yet
    /// answered: a reading sent before a change, or during one, may carry
    /// the settings as they were.
    @ObservationIgnored private var changesSent = 0
    @ObservationIgnored private var changesUnderway = 0

    @ObservationIgnored private let send: Send
    @ObservationIgnored private let isSignedIn: @MainActor () -> Bool
    /// Venti's clock in milliseconds, for whether a call still stands: the
    /// server's time where it was read, the iPhone's where not.
    @ObservationIgnored private let nowMs: @MainActor () async -> Int64

    init(
        send: @escaping Send,
        isSignedIn: @escaping @MainActor () -> Bool,
        nowMs: @escaping @MainActor () async -> Int64 = { Int64(Date().timeIntervalSince1970 * 1000) }
    ) {
        self.send = send
        self.isSignedIn = isSignedIn
        self.nowMs = nowMs
    }

    /// With the app's account, and Venti's clock as the wind's model reads it.
    convenience init(account: AccountModel, wind: WindModel) {
        self.init(
            send: { try await account.answer(to: $0) },
            isSignedIn: { account.isSignedIn },
            nowMs: { await wind.serverNow() * 1000 }
        )
    }

    /// Whether the pilot may be asked for help, as Venti last said.
    var mayBeAskedForHelp: Bool { choice?.mayBeAskedForHelp ?? false }

    /// The pilot's own call that stands, if any.
    var mine: HelpView? { help?.mine.first { $0.stands } }

    /// The calls the pilot was asked that stand, the newest first.
    var asked: [HelpView] { help?.asked.filter(\.stands) ?? [] }

    // MARK: The settings

    /// Reads the settings again. An answer to a reading that a change
    /// overtook is dropped: the app reads them each time it comes to the
    /// front, which it also does when the pilot has answered what iOS asked
    /// on turning a switch on, while the change is still on its way.
    func loadSettings() async {
        guard isSignedIn() else { return signedOut() }
        let sentBefore = changesSent
        let wasQuiet = changesUnderway == 0
        do {
            let read = try await ask(nearbySettingsRequest()) { try nearbySettings(status: $0, body: $1) }
            guard wasQuiet, changesUnderway == 0, changesSent == sentBefore else { return }
            choice = read
            settingsProblem = nil
        } catch {
            guard !Self.isCancelled(error), wasQuiet, changesUnderway == 0, changesSent == sentBefore else { return }
            settingsProblem = Social.words(for: error)
        }
    }

    /// Sends the settings whole; returns the refusal in words, `nil` once
    /// Venti keeps them. Switching help off has Venti forget the position.
    @discardableResult
    func change(_ wanted: NearbyChoice) async -> String? {
        let before = choice
        changesSent += 1
        changesUnderway += 1
        defer { changesUnderway -= 1 }
        do {
            let kept = try await ask(try nearbySettingsChangeRequest(choice: wanted)) {
                try nearbySettings(status: $0, body: $1)
            }
            choice = kept
            settingsProblem = nil
            if before?.mayBeAskedForHelp == true, !kept.mayBeAskedForHelp {
                await forgetPosition()
            }
            return nil
        } catch {
            let words = Social.words(for: error)
            settingsProblem = words
            return words
        }
    }

    // MARK: The position

    /// Tells Venti roughly where the phone is, while the pilot may be
    /// asked for help. Quiet: a position that does not arrive is told the
    /// next time.
    func tellPosition(at place: Coordinate) async {
        guard isSignedIn(), mayBeAskedForHelp else { return }
        _ = try? await ask(try positionRequest(at: place)) { try phoneRequestDone(status: $0, body: $1) }
    }

    func forgetPosition() async {
        guard isSignedIn() else { return }
        _ = try? await ask(positionForgetRequest()) { try phoneRequestDone(status: $0, body: $1) }
    }

    // MARK: Help after landing

    /// The calls that stand.
    func loadHelp() async {
        guard isSignedIn() else { return signedOut() }
        let now = await nowMs()
        do {
            help = try await ask(helpListRequest()) { try helpRequests(status: $0, body: $1, nowMs: now) }
            helpProblem = nil
        } catch {
            if Self.isCancelled(error) { return }
            helpProblem = Social.words(for: error)
        }
    }

    /// Asks the pilots nearby, from where the pilot is, with what they
    /// wrote (which may be empty).
    func ask(at place: Coordinate, note: String) async throws -> HelpView {
        let now = await nowMs()
        let asked = try await ask(try helpAskRequest(at: place, note: note)) {
            try helpRequest(status: $0, body: $1, nowMs: now)
        }
        return wrote(asked)
    }

    /// One call, as the pilot may see it.
    func one(_ id: String) async throws -> HelpView {
        let now = await nowMs()
        return wrote(try await ask(helpOneRequest(id: id)) { try helpRequest(status: $0, body: $1, nowMs: now) })
    }

    /// Closes the pilot's own call: "I'm fine, close".
    func close(_ id: String) async throws {
        try await ask(helpCloseRequest(id: id)) { try phoneRequestDone(status: $0, body: $1) }
        await loadHelp()
    }

    /// Says whether the pilot can help with a call they were asked.
    func answer(_ id: String, canHelp: Bool) async throws -> HelpView {
        let now = await nowMs()
        let answered = try await ask(try helpAnswerRequest(id: id, canHelp: canHelp)) {
            try helpRequest(status: $0, body: $1, nowMs: now)
        }
        return wrote(answered)
    }

    /// Signed out: nothing of the account is held.
    func signedOut() {
        choice = nil
        help = nil
        settingsProblem = nil
        helpProblem = nil
    }

    // MARK: Plumbing

    /// Lays a call Venti answered with over the lists.
    private func wrote(_ view: HelpView) -> HelpView {
        guard var lists = help else {
            help = HelpLists(mine: view.isMine ? [view] : [], asked: view.isMine ? [] : [view])
            return view
        }
        if view.isMine {
            lists.mine = [view] + lists.mine.filter { $0.id != view.id }
        } else if lists.asked.contains(where: { $0.id == view.id }) {
            lists.asked = lists.asked.map { $0.id == view.id ? view : $0 }
        } else {
            lists.asked.insert(view, at: 0)
        }
        help = lists
        return view
    }

    @discardableResult
    private func ask<Value>(
        _ request: @autoclosure () throws -> ServerRequest, read: (UInt16, Data) throws -> Value
    ) async throws -> Value {
        let answer = try await send(try request())
        return try read(answer.status, answer.body)
    }

    private static func isCancelled(_ error: any Error) -> Bool {
        error is CancellationError || (error as? URLError)?.code == .cancelled
    }
}

/// What the screens of help after landing and of the settings for pilots
/// nearby say that is not the core's to say: Android's words, word for
/// word (`nearby/HelpSheet.kt`).
enum HelpWords {
    static let entry = "Ask pilots nearby for help"
    static let title = "Help after landing"
    static let note = "What do you need?"
    static let noteExample = "By the river at Völs, anyone driving to Innsbruck?"
    static let ask = "Ask pilots nearby"
    static let call = "Call 112"
    static let close = "I'm fine, close"
    static let can = "I can help"
    static let cannot = "I can't"
    static let canHelp = "Can help"
    static let whereTheyAre = "Where they are"
    /// Where the call goes from: the landing, or the iPhone's position.
    static let whereYouAre = "Where you are"
    static let directions = "Directions"
    static let asked = "Pilots nearby ask for help"
    static let ended = "This request has ended."
    static let noPosition = "Venti needs to know where you are to ask pilots nearby."
    static let usePosition = "Use my position"
    static let waitingForPosition = "Finding where you are"
    static let signedOut = "Sign in to ask pilots nearby for help, or to be asked."
    static var signIn: String { commonWords().signIn }
    static let report = "Report this request"
    static let noteRoom = 280
    /// The settings' heading on the account's page.
    static let pilotsNearby = "Pilots nearby"
    static let whereTheMapIs = "Where the map is"
    static let distance = "Distance"
}
