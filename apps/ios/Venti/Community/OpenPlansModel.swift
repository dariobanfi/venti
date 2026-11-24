import Foundation
import VentiCore

/// Open plans as Community shows them (docs/design.md, "Open plans"): the
/// plans for everyone flying from an area near a place for the coming
/// days, one plan on its own page, and the pilot's posting, changing,
/// deleting and "I'm in".
///
/// Nothing of it is kept on the iPhone. The core puts the requests
/// together and reads the answers (`open_plans.rs`); a request that needs
/// the pilot goes with the session's token, the others as anyone. This
/// holds the last answer for the place asked about and lays what a write
/// answered over it. The list is asked for when Community shows, on a
/// pull, and after every write.
@MainActor
@Observable
final class OpenPlansModel {
    /// Fetches an address without a token: what everyone may read.
    typealias Fetch = @Sendable (URL) async throws -> (Int, Data)

    /// How many days the list holds, today the first.
    static let days = 14

    /// The open plans near the place last asked about. `nil` until the
    /// server has answered.
    private(set) var near: OpenPlans?
    /// Where `near` is for.
    private(set) var place: Coordinate?
    private(set) var isLoading = false
    /// Why the list is not there or not current, in words for the pilot.
    private(set) var problem: String?

    /// The Venti the lists are read from. The app sets it.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// The take-offs around a place, nearest first, for the form's choice
    /// of a place. The app sets it to Today's.
    @ObservationIgnored var takeoffsNear: (Coordinate) async -> [TodayTakeoff] = { _ in [] }

    @ObservationIgnored private let core: Core
    @ObservationIgnored private let account: AccountModel
    @ObservationIgnored private let fetch: Fetch

    init(core: Core, account: AccountModel, fetch: @escaping Fetch = SiteEditing.fresh) {
        self.core = core
        self.account = account
        self.fetch = fetch
    }

    /// Whether a pilot is signed in, for whom "I'm in" and posting are.
    var isSignedIn: Bool { account.isSignedIn }

    /// What the pilot is told before posting or saying "I'm in": the
    /// core's, from the list or from a plan.
    var publicWords: String { near?.publicWords ?? "" }

    /// The plan with an identifier, as the list has it now.
    func listed(_ id: String) -> OpenPlanView? {
        near?.days.lazy.flatMap(\.plans).first { $0.id == id }
    }

    /// The pilot's own today, written `2026-11-21`: the first day asked
    /// for and the first a plan may be for.
    static func today(_ now: Date = Date()) -> String {
        GroupsModel.day(now)
    }

    /// Asks for the open plans near a place, for the coming days.
    func show(near place: Coordinate) async {
        if place != self.place {
            near = nil
        }
        self.place = place
        await load()
    }

    /// Asks again for the place last given, from the pilot's own today on.
    func load() async {
        guard let place else { return }
        isLoading = true
        problem = nil
        defer { isLoading = false }
        let now = Date()
        let from = Self.today(now)
        let to = GroupsModel.day(Calendar.current.date(byAdding: .day, value: Self.days - 1, to: now) ?? now)
        let core = core
        do {
            let read = try await send({
                try core.openPlansNearRequest(place: place, radiusKm: nil, fromDay: from, toDay: to)
            }) { try core.openPlans(status: $0, body: $1) }
            // An answer for a place asked about before is not this one's.
            if place == self.place {
                near = read
            }
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            problem = Social.words(for: error)
        }
    }

    /// One open plan as the server has it now; anyone may read it.
    func plan(_ id: String) async throws -> OpenPlanView {
        let core = core
        return try await send({ try core.openPlanRequest(plan: id) }) { try core.openPlan(status: $0, body: $1) }
    }

    /// Posts an open plan, which everyone can read. The list is asked for
    /// again.
    func post(_ draft: OpenPlanDraft) async throws -> OpenPlanView {
        let core = core
        let posted = try await send({ try core.postOpenPlanRequest(draft: draft) }) {
            try core.openPlan(status: $0, body: $1)
        }
        await load()
        return posted
    }

    /// Changes one of the pilot's own open plans to what the draft says,
    /// whole.
    func change(_ id: String, to draft: OpenPlanDraft) async throws -> OpenPlanView {
        let core = core
        let changed = try await send({ try core.changeOpenPlanRequest(plan: id, draft: draft) }) {
            try core.openPlan(status: $0, body: $1)
        }
        await load()
        return changed
    }

    func delete(_ id: String) async throws {
        let core = core
        try await send({ try core.deleteOpenPlanRequest(plan: id) }) { try core.groupDone(status: $0, body: $1) }
        await load()
    }

    /// Says that the pilot is in on an open plan, or takes that back; the
    /// answer is laid over the list.
    func setIn(_ id: String, _ isIn: Bool) async throws -> OpenPlanView {
        let core = core
        let said = try await send({ try core.openPlanInRequest(plan: id, isIn: isIn) }) {
            try core.openPlan(status: $0, body: $1)
        }
        if var shown = near {
            shown.days = shown.days.map { day in
                var day = day
                day.plans = day.plans.map { $0.id == said.id ? said : $0 }
                return day
            }
            near = shown
        }
        return said
    }

    /// Puts a request together, sends it (with the account's token where
    /// the request needs one, as anyone where not) and reads the answer.
    @discardableResult
    private func send<Value>(
        _ request: () throws -> ServerRequest, read: (UInt16, Data) throws -> Value
    ) async throws -> Value {
        let asked = try request()
        if asked.needsToken {
            let answer = try await account.answer(to: asked)
            return try read(answer.status, answer.body)
        }
        guard let server = server(), let url = URL(string: server.absoluteString + asked.path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await fetch(url)
        return try read(UInt16(clamping: status), body)
    }
}
