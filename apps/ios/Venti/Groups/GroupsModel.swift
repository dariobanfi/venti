import Foundation
import VentiCore

/// Groups and their plans for a flying day, on the pilot's Venti server.
///
/// Nothing of it is kept on the iPhone: the board is asked for when it is
/// opened and after every change. The core puts the requests together and
/// reads the answers; the account sends them with the session's token.
@MainActor
@Observable
final class GroupsModel {
    /// The plans of the pilot's groups from today on, and the groups.
    /// `nil` until the server has answered once.
    private(set) var board: PlanBoard?
    private(set) var isLoading = false
    /// Why the board is not there, in words for the pilot.
    private(set) var problem: String?

    @ObservationIgnored private let core: Core
    @ObservationIgnored private let account: AccountModel

    init(core: Core, account: AccountModel) {
        self.core = core
        self.account = account
    }

    var groups: [GroupView] {
        board?.groups ?? []
    }

    /// The plan with an identifier, as the board has it now.
    func plan(_ id: String) -> PlanView? {
        board?.days.lazy.flatMap(\.plans).first { $0.id == id }
    }

    /// A day as the server writes it, "2026-11-21", in the pilot's own
    /// calendar: a plan is for a day where the pilot is.
    nonisolated static func day(_ date: Date, calendar: Calendar = .current) -> String {
        let parts = calendar.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0)
    }

    /// The date of a day written "2026-11-21", at noon in the pilot's own
    /// calendar. `nil` for anything else.
    nonisolated static func date(ofDay day: String, calendar: Calendar = .current) -> Date? {
        let parts = day.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        return calendar.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2], hour: 12))
    }

    /// A time of day as the server writes it, "09:30".
    nonisolated static func timeOfDay(_ date: Date, calendar: Calendar = .current) -> String {
        let parts = calendar.dateComponents([.hour, .minute], from: date)
        return String(format: "%02d:%02d", parts.hour ?? 0, parts.minute ?? 0)
    }

    /// Asks for the board: every plan of the pilot's groups from today on.
    func load() async {
        guard account.isSignedIn else {
            board = nil
            problem = nil
            return
        }
        isLoading = true
        defer { isLoading = false }
        do {
            let core = core
            board = try await send({ try core.planBoardRequest(fromDay: Self.day(Date()), toDay: nil) }) {
                try core.planBoard(status: $0, body: $1)
            }
            problem = nil
        } catch {
            problem = AccountModel.words(for: error)
        }
    }

    // MARK: Groups

    func make(_ name: String) async throws -> GroupMade {
        let core = core
        let made = try await send({ try core.createGroupRequest(name: name) }) {
            try core.groupMade(status: $0, body: $1)
        }
        await load()
        return made
    }

    func join(_ code: String) async throws -> GroupJoined {
        let core = core
        let joined = try await send({ try core.joinGroupRequest(code: code) }) {
            try core.groupJoined(status: $0, body: $1)
        }
        await load()
        return joined
    }

    func detail(_ id: String) async throws -> GroupDetail {
        let core = core
        return try await send({ try core.groupRequest(id: id) }) { try core.group(status: $0, body: $1) }
    }

    func rename(_ id: String, to name: String) async throws -> GroupDetail {
        let core = core
        let detail = try await send({ try core.renameGroupRequest(id: id, name: name) }) {
            try core.group(status: $0, body: $1)
        }
        await load()
        return detail
    }

    func handOver(_ id: String, to handle: String) async throws -> GroupDetail {
        let core = core
        let detail = try await send({ try core.handOverGroupRequest(id: id, handle: handle) }) {
            try core.group(status: $0, body: $1)
        }
        await load()
        return detail
    }

    /// A new invite code for a group. The one before it stops working.
    func newInvite(_ id: String) async throws -> String {
        let core = core
        return try await send({ try core.newGroupInviteRequest(id: id) }) {
            try core.groupInvite(status: $0, body: $1)
        }
    }

    func remove(_ handle: String, from id: String) async throws {
        let core = core
        try await send({ try core.removeGroupMemberRequest(id: id, handle: handle) }) {
            try core.groupDone(status: $0, body: $1)
        }
        await load()
    }

    func leave(_ id: String) async throws {
        let core = core
        try await send({ try core.leaveGroupRequest(id: id) }) { try core.groupDone(status: $0, body: $1) }
        await load()
    }

    func delete(group id: String) async throws {
        let core = core
        try await send({ try core.deleteGroupRequest(id: id) }) { try core.groupDone(status: $0, body: $1) }
        await load()
    }

    // MARK: Plans

    func post(_ draft: PlanDraft, to group: String) async throws {
        let core = core
        _ = try await send({ try core.postPlanRequest(group: group, draft: draft) }) {
            try core.plan(status: $0, body: $1)
        }
        await load()
    }

    func change(_ plan: PlanView, to draft: PlanDraft) async throws {
        let core = core
        _ = try await send({ try core.changePlanRequest(group: plan.groupId, plan: plan.id, draft: draft) }) {
            try core.plan(status: $0, body: $1)
        }
        await load()
    }

    func delete(_ plan: PlanView) async throws {
        let core = core
        try await send({ try core.deletePlanRequest(group: plan.groupId, plan: plan.id) }) {
            try core.groupDone(status: $0, body: $1)
        }
        await load()
    }

    /// Says that the pilot is in on a plan, or takes that back.
    func setIn(_ plan: PlanView, _ isIn: Bool) async throws {
        let core = core
        _ = try await send({ try core.planInRequest(group: plan.groupId, plan: plan.id, isIn: isIn) }) {
            try core.plan(status: $0, body: $1)
        }
        await load()
    }

    /// Puts a request together, sends it with the account's token and
    /// reads the answer.
    private func send<Value>(
        _ request: () throws -> ServerRequest, read: (UInt16, Data) throws -> Value
    ) async throws -> Value {
        let answer = try await account.answer(to: try request())
        return try read(answer.status, answer.body)
    }
}
