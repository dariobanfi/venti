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

    /// A plan as the map marks it.
    struct Pin: Equatable {
        let id: String
        let position: Coordinate
        /// The place and, if the plan has one, its time: "Wallberg 09:30".
        let label: String
    }

    /// The plans of today and tomorrow that say where on the map they
    /// are: who flies where, for the map to mark. A plan for next week is
    /// in the list, not on the map.
    func pins(now: Date = Date(), calendar: Calendar = .current) -> [Pin] {
        let near = [now, calendar.date(byAdding: .day, value: 1, to: now) ?? now].map {
            Self.day($0, calendar: calendar)
        }
        return (board?.days ?? [])
            .filter { near.contains($0.day) }
            .flatMap(\.plans)
            .compactMap { plan in
                plan.position.map { position in
                    let label = [plan.place, plan.meetAt].compactMap { $0 }.joined(separator: " ")
                    return Pin(id: plan.id, position: position, label: label)
                }
            }
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

    /// The moment a time of day written "09:30" names on a day. `nil` for
    /// anything else.
    nonisolated static func date(ofTime time: String, on day: Date, calendar: Calendar = .current) -> Date? {
        let parts = time.split(separator: ":").compactMap { Int($0) }
        guard parts.count == 2 else { return nil }
        return calendar.date(bySettingHour: parts[0], minute: parts[1], second: 0, of: day)
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

    // MARK: Groups as communities (owner decision 26)

    /// The pilot's groups as the groups' section shows them, each with its
    /// unread messages and its last message. `nil` until Venti answered.
    private(set) var chats: [CommunityView]?
    /// Why the pilot's groups are not there, in words for the pilot.
    private(set) var chatsProblem: String?
    /// The pilot's invites into private groups.
    private(set) var invites: [CommunityInvite] = []

    /// The words of the groups' screens that no answer carries.
    let words = communityWords()

    /// Asks for the pilot's groups and their invites.
    func loadChats() async {
        guard account.isSignedIn else {
            chats = nil
            invites = []
            chatsProblem = nil
            return
        }
        let core = core
        do {
            chats = try await send({ try core.groupsRequest() }) { try core.communities(status: $0, body: $1) }
            chatsProblem = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            chatsProblem = AccountModel.words(for: error)
        }
        if let invites = try? await send({ try core.groupInvitesRequest() }, read: {
            try core.groupInvites(status: $0, body: $1)
        }) {
            self.invites = invites
        }
    }

    /// The public groups near a place, by name or place, or both.
    func find(near place: Coordinate?, text: String) async throws -> [FoundCommunity] {
        let core = core
        return try await send({ try core.findGroupsRequest(near: place, text: text) }) {
            try core.foundGroups(status: $0, body: $1)
        }
    }

    /// Both lists anew after the pilot's standing in a group changed.
    private func reloadAll() async {
        async let board: Void = load()
        async let chats: Void = loadChats()
        _ = await (board, chats)
    }

    func detail(_ id: String) async throws -> CommunityDetail {
        let core = core
        return try await send({ try core.groupRequest(id: id) }) { try core.community(status: $0, body: $1) }
    }

    /// Makes a group with the pilot as its owner, with its picture if one
    /// was chosen (a JPEG).
    func create(_ draft: CommunityDraft, isPublic: Bool, picture: Data?) async throws -> CommunityDetail {
        let core = core
        // The answer is the group as it was made, with the pilot as its
        // owner and only member: no second question for it.
        let made = try await send({ try core.createCommunityRequest(draft: draft, public: isPublic) }) {
            try core.communityMade(status: $0, body: $1)
        }
        let shown = if let picture {
            try await setPicture(made.group.id, picture)
        } else {
            made
        }
        await reloadAll()
        return shown
    }

    /// Changes a group's name, description and home.
    func change(_ id: String, to draft: CommunityDraft) async throws -> CommunityDetail {
        let core = core
        let changed = try await send({ try core.changeCommunityRequest(id: id, draft: draft) }) {
            try core.community(status: $0, body: $1)
        }
        await reloadAll()
        return changed
    }

    /// Makes a group public or private: its owner's.
    func setPublic(_ id: String, _ isPublic: Bool) async throws -> CommunityDetail {
        let core = core
        let changed = try await send({ try core.groupPublicRequest(id: id, public: isPublic) }) {
            try core.community(status: $0, body: $1)
        }
        await loadChats()
        return changed
    }

    /// Gives a group its picture, a JPEG.
    func setPicture(_ id: String, _ jpeg: Data) async throws -> CommunityDetail {
        let core = core
        return try await send({
            try core.groupOwnPictureRequest(id: id, picture: jpeg, mediaType: PictureUpload.mediaType)
        }) { try core.community(status: $0, body: $1) }
    }

    func removePicture(_ id: String) async throws -> CommunityDetail {
        let core = core
        return try await send({ try core.removeGroupOwnPictureRequest(id: id) }) {
            try core.community(status: $0, body: $1)
        }
    }

    /// Takes the pilot into a public group, or one they were invited into.
    @discardableResult
    func enter(_ id: String) async throws -> GroupJoined {
        let core = core
        let joined = try await send({ try core.enterGroupRequest(id: id) }) {
            try core.groupJoined(status: $0, body: $1)
        }
        await reloadAll()
        return joined
    }

    /// Declines an invite into a group: the pilot's own handle.
    func decline(_ id: String) async throws {
        let core = core
        let me = account.account?.handle ?? ""
        try await send({ try core.dropGroupInviteRequest(id: id, handle: me) }) {
            try core.groupDone(status: $0, body: $1)
        }
        await loadChats()
    }

    /// Invites a pilot by handle: the core's line of what was done.
    func invite(_ handle: String, to id: String) async throws -> String {
        let core = core
        return try await send({ try core.inviteToGroupRequest(id: id, handle: handle) }) {
            try core.groupInviteSent(status: $0, body: $1)
        }
    }

    /// Makes a member an admin, or an admin a member again: the owner's.
    func setAdmin(_ handle: String, in id: String, _ isAdmin: Bool) async throws {
        let core = core
        try await send({ try core.groupAdminRequest(id: id, handle: handle, admin: isAdmin) }) {
            try core.groupDone(status: $0, body: $1)
        }
    }

    func handOver(_ id: String, to handle: String) async throws -> CommunityDetail {
        let core = core
        let detail = try await send({ try core.handOverGroupRequest(id: id, handle: handle) }) {
            try core.community(status: $0, body: $1)
        }
        await reloadAll()
        return detail
    }

    /// Mutes a group's pushes, or unmutes them.
    func mute(_ id: String, _ muted: Bool) async throws -> CommunityStanding {
        let core = core
        let standing = try await send({ try core.groupMuteRequest(id: id, muted: muted) }) {
            try core.groupStanding(status: $0, body: $1)
        }
        await loadChats()
        return standing
    }

    func remove(_ handle: String, from id: String) async throws {
        let core = core
        try await send({ try core.removeGroupMemberRequest(id: id, handle: handle) }) {
            try core.groupDone(status: $0, body: $1)
        }
        await reloadAll()
    }

    func leave(_ id: String) async throws {
        let core = core
        try await send({ try core.leaveGroupRequest(id: id) }) { try core.groupDone(status: $0, body: $1) }
        await reloadAll()
    }

    func delete(group id: String) async throws {
        let core = core
        try await send({ try core.deleteGroupRequest(id: id) }) { try core.groupDone(status: $0, body: $1) }
        await reloadAll()
    }

    /// The model of one group's chat, sending with the account's token.
    func chat(_ id: String) -> ChatModel {
        let core = core
        let account = account
        return ChatModel(group: id, core: core) { try await account.answer(to: $0) }
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
    @discardableResult
    private func send<Value>(
        _ request: () throws -> ServerRequest, read: (UInt16, Data) throws -> Value
    ) async throws -> Value {
        let answer = try await account.answer(to: try request())
        return try read(answer.status, answer.body)
    }
}
