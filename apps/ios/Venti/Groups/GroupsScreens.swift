import PhotosUI
import SwiftUI
import VentiCore

// Groups as communities with a chat (owner decision 26): the pilot's
// groups as cards, the public groups near and found by name or place,
// invites into private groups, a group's page and the form that makes or
// changes one. Every word an answer carries is the core's; the core's
// `communityWords()` has the rest.

/// The groups' section: invites, the pilot's own groups as cards, and the
/// public groups near the iPhone (or the map's middle) or found by the
/// field.
struct GroupsScreen: View {
    @Environment(GroupsModel.self) private var groups
    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var typed = ""
    @State private var found: [FoundCommunity]?
    @State private var findProblem: String?
    @State private var isFinding = false
    @State private var position = PilotPosition()
    @State private var isMaking = false
    @State private var problem: String?
    /// A group just made, opened once the form has gone.
    @State private var made: CommunityView?
    @State private var justMade: CommunityView?

    private var words: CommunityWords { groups.words }

    private var place: Coordinate {
        (position.isAllowed ? position.place : nil) ?? wind.mapCentre
    }

    private var isSearching: Bool {
        !typed.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: VentiSpace.s5) {
                if !account.isSignedIn {
                    GroupsLine(words: "Groups are shared on Venti with an account. Sign in under Settings, Account.")
                } else {
                    if !groups.invites.isEmpty, !isSearching {
                        invites
                    }
                    if !isSearching {
                        mine
                    }
                    near
                    if let problem {
                        GroupsLine(words: problem, isProblem: true)
                    }
                }
            }
            .padding(.horizontal, VentiSpace.s4)
            .padding(.vertical, VentiSpace.s3)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(.venti(.bg))
        .navigationTitle(words.title)
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $typed, placement: .navigationBarDrawer(displayMode: .always), prompt: words.search)
        .autocorrectionDisabled()
        .toolbar {
            if account.isSignedIn {
                ToolbarItem(placement: .primaryAction) {
                    Button("New group", systemImage: "plus") {
                        isMaking = true
                    }
                    .accessibilityIdentifier("groups-new")
                }
            }
        }
        // The new group opens once the form has gone: a step pushed while
        // a sheet goes down is not taken.
        .sheet(isPresented: $isMaking, onDismiss: {
            made = justMade
            justMade = nil
        }) {
            GroupForm(editing: nil) { made in
                justMade = made.group
            }
        }
        .navigationDestination(item: $made) { group in
            GroupChatScreen(id: group.id, name: group.name)
        }
        .refreshable {
            await groups.loadChats()
            await find()
        }
        .task(id: account.isSignedIn) {
            position.readIfAllowed()
            await groups.loadChats()
        }
        // The search rests 350 ms before it asks: Venti counts reads.
        .task(id: typed) {
            if isSearching {
                try? await Task.sleep(nanoseconds: 350_000_000)
                if Task.isCancelled { return }
            }
            await find()
        }
        .onChange(of: position.place) { _, place in
            guard place != nil, !isSearching else { return }
            Task { await find() }
        }
    }

    private func find() async {
        guard account.isSignedIn else { return }
        isFinding = true
        defer { isFinding = false }
        do {
            // A search by name or place looks everywhere; the list under
            // no text is the groups near.
            found = try await groups.find(near: isSearching ? nil : place, text: typed)
            findProblem = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            findProblem = AccountModel.words(for: error)
        }
    }

    // MARK: The parts

    private var invites: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            GroupsHeader(title: "Invites")
            ForEach(groups.invites, id: \.group.id) { invite in
                VStack(alignment: .leading, spacing: VentiSpace.s3) {
                    HStack(spacing: VentiSpace.s3) {
                        GroupPicture(group: invite.group, size: VentiSpace.s7)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(invite.words)
                                .font(.venti(.body, .semibold))
                                .foregroundStyle(.venti(.text))
                                .fixedSize(horizontal: false, vertical: true)
                            Text(invite.group.summary)
                                .font(.venti(.footnote))
                                .foregroundStyle(.venti(.textSecondary))
                        }
                    }
                    .accessibilityElement(children: .combine)
                    HStack(spacing: VentiSpace.s3) {
                        Button(words.join) {
                            act { try await groups.enter(invite.group.id) }
                        }
                        .buttonStyle(.venti(.primary))
                        .accessibilityIdentifier("group-invite-accept")
                        Button("Decline") {
                            act { try await groups.decline(invite.group.id) }
                        }
                        .buttonStyle(.venti(.quiet))
                        .accessibilityIdentifier("group-invite-decline")
                    }
                }
                .ventiCard()
                .accessibilityIdentifier("group-invite")
            }
        }
    }

    @ViewBuilder private var mine: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            GroupsHeader(title: words.yours)
            if let chats = groups.chats {
                if chats.isEmpty {
                    GroupsLine(words: "You are in no group yet. Join one near you, or make one.")
                        .accessibilityIdentifier("groups-none")
                }
                ForEach(chats, id: \.id) { group in
                    NavigationLink {
                        GroupChatScreen(id: group.id, name: group.name)
                    } label: {
                        CommunityCard(group: group)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("community-card")
                }
            } else if let chatsProblem = groups.chatsProblem {
                GroupsLine(words: chatsProblem, isProblem: true)
            } else {
                GroupsLoading()
            }
        }
    }

    @ViewBuilder private var near: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            GroupsHeader(title: isSearching ? "Found" : words.nearYou)
            if let found {
                if found.isEmpty, !isFinding {
                    GroupsLine(words: words.noneFound)
                        .accessibilityIdentifier("groups-none-found")
                }
                ForEach(found, id: \.group.id) { found in
                    FoundCommunityCard(found: found) {
                        problem = nil
                        do {
                            try await groups.enter(found.group.id)
                        } catch {
                            problem = AccountModel.words(for: error)
                        }
                        await self.find()
                    }
                }
            } else if let findProblem {
                GroupsLine(words: findProblem, isProblem: true)
            } else {
                GroupsLoading()
            }
        }
    }

    private func act(_ work: @escaping @MainActor () async throws -> Void) {
        problem = nil
        Task {
            do {
                try await work()
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}

/// A group as a card: its picture or initials, its name, its place and how
/// many members, the last message, and how many messages are unread.
struct CommunityCard: View {
    let group: CommunityView
    /// How far its home is, for a group found near a place.
    var distance: String?
    /// Room at the card's foot for a button laid over it ("Join").
    var roomBelow = false

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s3) {
            GroupPicture(group: group, size: VentiSpace.s7)
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                    Text(group.name)
                        .font(.venti(.headline))
                        .foregroundStyle(.venti(.text))
                        .multilineTextAlignment(.leading)
                    Spacer(minLength: 0)
                    if let badge = group.unreadBadge {
                        Text(verbatim: badge)
                            .font(.venti(.caption, .semibold))
                            .monospacedDigit()
                            .foregroundStyle(.venti(.onPrimary))
                            .padding(.horizontal, VentiSpace.s2)
                            .frame(minWidth: VentiSpace.s5, minHeight: VentiSpace.s5)
                            .background(.venti(.primary), in: .capsule)
                            .accessibilityLabel("\(badge) unread")
                            .accessibilityIdentifier("group-unread")
                    }
                }
                Text([group.summary, distance].compactMap { $0 }.joined(separator: " · "))
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
                if let last = group.lastMessage {
                    Text(last)
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.text))
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                } else if let description = group.description {
                    Text(description)
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.textSecondary))
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.bottom, roomBelow ? VentiSize.touch + VentiSpace.s2 : 0)
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiCard()
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
        .accessibilityValue(group.unreadBadge.map { "\($0) unread" } ?? "")
    }
}

/// A public group found: its card, which opens its chat, and "Join" while
/// the pilot is not in it.
private struct FoundCommunityCard: View {
    let found: FoundCommunity
    let join: () async -> Void

    @Environment(GroupsModel.self) private var groups
    @State private var isJoining = false

    var body: some View {
        NavigationLink {
            GroupChatScreen(id: found.group.id, name: found.group.name)
        } label: {
            CommunityCard(group: found.group, distance: found.distance, roomBelow: !found.isMember)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("found-group")
        // On the card, under its words: a button of its own beside the
        // link, not inside it.
        .overlay(alignment: .bottomTrailing) {
            if !found.isMember {
                Button(groups.words.join) {
                    isJoining = true
                    Task {
                        await join()
                        isJoining = false
                    }
                }
                .buttonStyle(.venti(.quiet))
                .overlay {
                    Capsule().strokeBorder(.venti(.borderStrong), lineWidth: 1)
                }
                .disabled(isJoining)
                .padding(VentiSpace.s4)
                .accessibilityValue(found.group.name)
                .accessibilityIdentifier("group-join")
            }
        }
    }
}

/// A group's picture, square with the pack's corners, or its initials
/// where it has none.
struct GroupPicture: View {
    let group: CommunityView
    var size: CGFloat = VentiSpace.s7

    @Environment(WindModel.self) private var wind
    @ScaledMetric(relativeTo: .body) private var scale = 1

    var body: some View {
        let side = size * scale
        Group {
            if let url = PictureAddress.url(group.picturePath, on: wind.server) {
                AsyncImage(url: url) { phase in
                    if let image = phase.image {
                        image.resizable().scaledToFill()
                    } else {
                        letters
                    }
                }
            } else {
                letters
            }
        }
        .frame(width: side, height: side)
        .clipShape(.rect(cornerRadius: VentiRadius.s))
        .overlay {
            RoundedRectangle(cornerRadius: VentiRadius.s).strokeBorder(.venti(.border), lineWidth: 1)
        }
        .accessibilityHidden(true)
    }

    private var letters: some View {
        Text(verbatim: Initials.of(group.name))
            .font(.ventiType(.heading))
            .minimumScaleFactor(0.5)
            .lineLimit(1)
            .padding(VentiSpace.s1)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .foregroundStyle(.venti(.text))
            .background(.venti(.surfaceRaised))
    }
}

struct GroupsHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.ventiType(.heading))
            .foregroundStyle(.venti(.text))
            .accessibilityAddTraits(.isHeader)
    }
}

struct GroupsLine: View {
    let words: String
    var isProblem = false

    var body: some View {
        if isProblem {
            Label {
                Text(words).fixedSize(horizontal: false, vertical: true)
            } icon: {
                Image("icon-caution").foregroundStyle(.venti(.caution))
            }
            .foregroundStyle(.venti(.textSecondary))
        } else {
            Text(words)
                .foregroundStyle(.venti(.textSecondary))
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

struct GroupsLoading: View {
    var body: some View {
        ProgressView()
            .frame(maxWidth: .infinity, minHeight: VentiSize.touch)
    }
}

// MARK: A group's page

/// One group: its description, home and members with their roles; for
/// its owner and admins inviting by handle, the members' roles, removing
/// members and changing the group; for every member muting and leaving.
struct GroupPage: View {
    let id: String

    @Environment(GroupsModel.self) private var groups
    @Environment(\.dismiss) private var dismiss

    @State private var detail: CommunityDetail?
    @State private var problem: String?
    @State private var outcome: String?
    @State private var isWorking = false
    @State private var isInviting = false
    @State private var handle = ""
    @State private var isEditing = false
    @State private var acting: CommunityMember?
    @State private var asking: Asking?
    @State private var reporting: Reported?
    @State private var picture: PhotosPickerItem?

    private enum Asking: String, Identifiable {
        case leave, delete
        var id: String { rawValue }
    }

    private var words: CommunityWords { groups.words }

    var body: some View {
        VentiList {
            // What became of the last thing done, at the top where it is
            // seen.
            if let outcome {
                Section {
                    Label(outcome, systemImage: "checkmark.circle")
                        .accessibilityIdentifier("group-outcome")
                }
            }
            if let problem {
                Section {
                    Text(problem)
                        .foregroundStyle(.venti(.danger))
                        .accessibilityIdentifier("group-problem")
                }
            }
            if let detail {
                head(detail)
                members(detail)
                if detail.canModerate {
                    moderation(detail)
                }
                if detail.canPost {
                    Section {
                        Toggle(isOn: mutedBinding(detail)) {
                            Text(words.mute)
                        }
                        .accessibilityIdentifier("group-mute")
                    } footer: {
                        Text(detail.group.isPublic
                            ? "A public group's messages reach your phone only where they mention you."
                            : "Each new message reaches your phone unless the group is muted.")
                    }
                    Section {
                        Button(words.leave, systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) {
                            asking = .leave
                        }
                        .accessibilityIdentifier("group-leave")
                        if detail.group.myRole == .owner {
                            Button("Delete the group", systemImage: "trash", role: .destructive) {
                                asking = .delete
                            }
                            .accessibilityIdentifier("group-delete")
                        }
                    } footer: {
                        if detail.group.myRole == .owner {
                            Text("Tap a member to hand the group on to them before you leave.")
                        }
                    }
                } else if detail.canJoin {
                    Section {
                        Button(words.join) {
                            act { try await groups.enter(id) }
                        }
                        .accessibilityIdentifier("group-page-join")
                    }
                }
                if detail.group.myRole != .owner {
                    Section {
                        Button("Report this group", systemImage: "exclamationmark.bubble") {
                            reporting = .group(id)
                        }
                        .accessibilityIdentifier("group-report")
                    }
                }
            } else if problem == nil {
                Section {
                    ProgressView().frame(maxWidth: .infinity)
                }
            }
        }
        .disabled(isWorking)
        .navigationTitle(detail?.group.name ?? words.title)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: id) {
            await load()
        }
        .alert("Invite a pilot", isPresented: $isInviting) {
            TextField("@handle", text: $handle)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("group-invite-handle")
            Button(words.invite) {
                let handle = handle
                act { outcome = try await groups.invite(handle, to: id) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("By the handle on their pilot's page.")
        }
        .confirmationDialog(
            acting?.name ?? "", isPresented: Binding(get: { acting != nil }, set: { if !$0 { acting = nil } }),
            titleVisibility: .visible, presenting: acting
        ) { member in
            memberActions(member)
        }
        .confirmationDialog(
            asking == .delete ? "Delete this group?" : "Leave this group?",
            isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
            titleVisibility: .visible, presenting: asking
        ) { asking in
            switch asking {
            case .leave:
                Button(words.leave, role: .destructive) {
                    act(thenLeave: true) { try await groups.leave(id) }
                }
            case .delete:
                Button("Delete the group", role: .destructive) {
                    act(thenLeave: true) { try await groups.delete(group: id) }
                }
            }
        } message: { asking in
            Text(asking == .delete
                ? "The group goes for every member, with its messages and plans."
                : "Its messages stay; your plans in it go with you.")
        }
        .sheet(isPresented: $isEditing) {
            if let detail {
                GroupForm(editing: detail) { changed in
                    self.detail = changed
                }
            }
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .onChange(of: picture) { _, item in
            guard let item else { return }
            act {
                defer { picture = nil }
                guard let data = try await item.loadTransferable(type: Data.self),
                      let jpeg = PictureUpload.jpeg(data)
                else {
                    throw CoreError.Server(message: PictureUpload.unreadable)
                }
                detail = try await groups.setPicture(id, jpeg)
            }
        }
    }

    @ViewBuilder private func head(_ detail: CommunityDetail) -> some View {
        let group = detail.group
        Section {
            HStack(spacing: VentiSpace.s3) {
                GroupPicture(group: group, size: VentiSpace.s8)
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    Text(group.name)
                        .font(.ventiType(.heading))
                        .foregroundStyle(.venti(.text))
                    Text("\(group.kindWords) · \(group.summary)")
                        .font(.venti(.footnote))
                        .foregroundStyle(.venti(.textSecondary))
                }
            }
            .accessibilityElement(children: .combine)
            if let description = group.description {
                Text(description)
                    .foregroundStyle(.venti(.text))
                    .accessibilityIdentifier("group-description")
            }
            if let place = group.place, let radius = group.radiusKm {
                LabeledContent("Home", value: "\(Format.decimal(radius, places: 0)) km around \(place)")
                    .accessibilityIdentifier("group-home")
            }
        }
    }

    @ViewBuilder private func members(_ detail: CommunityDetail) -> some View {
        Section("Members") {
            ForEach(detail.members, id: \.handle) { member in
                let canAct = !member.isMe && Self.canAct(on: member, as: detail.group.myRole)
                Button {
                    if canAct {
                        acting = member
                    }
                } label: {
                    HStack(spacing: VentiSpace.s3) {
                        PilotFace(name: member.name, size: VentiSpace.s6, isMe: member.isMe)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(member.isMe ? "\(member.name) (you)" : member.name)
                                .foregroundStyle(.venti(.text))
                            Text(verbatim: "@\(member.handle)")
                                .font(.venti(.footnote))
                                .foregroundStyle(.venti(.textSecondary))
                        }
                        Spacer(minLength: VentiSpace.s2)
                        if !member.roleWords.isEmpty {
                            Text(member.roleWords)
                                .font(.venti(.footnote, .medium))
                                .foregroundStyle(.venti(.textSecondary))
                        }
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("group-member")
            }
        }
    }

    /// Whether the pilot may do something with a member: the owner with
    /// anyone, an admin with a member who is no admin.
    private static func canAct(on member: CommunityMember, as role: CommunityRole) -> Bool {
        switch role {
        case .owner: true
        case .admin: member.role == .member
        default: false
        }
    }

    @ViewBuilder private func memberActions(_ member: CommunityMember) -> some View {
        let isOwner = detail?.group.myRole == .owner
        if isOwner {
            if member.role == .admin {
                Button("Make a member again") {
                    act {
                        try await groups.setAdmin(member.handle, in: id, false)
                        await load()
                    }
                }
            } else {
                Button("Make admin") {
                    act {
                        try await groups.setAdmin(member.handle, in: id, true)
                        await load()
                    }
                }
            }
            Button("Make \(member.name) the owner") {
                act { detail = try await groups.handOver(id, to: member.handle) }
            }
        }
        if isOwner || member.role == .member {
            Button("Remove from the group", role: .destructive) {
                act {
                    try await groups.remove(member.handle, from: id)
                    await load()
                }
            }
        }
    }

    @ViewBuilder private func moderation(_ detail: CommunityDetail) -> some View {
        Section {
            Button("Invite by handle", systemImage: "person.badge.plus") {
                handle = ""
                isInviting = true
            }
            .accessibilityIdentifier("group-invite")
            Button("Change name, description and home", systemImage: "pencil") {
                isEditing = true
            }
            .accessibilityIdentifier("group-edit")
            PhotosPicker(selection: $picture, matching: .images) {
                Label(detail.group.picturePath == nil ? "Give it a picture" : "Change the picture", systemImage: "photo")
            }
            .accessibilityIdentifier("group-picture")
            if detail.group.picturePath != nil {
                Button("Remove the picture", systemImage: "xmark.circle", role: .destructive) {
                    act { self.detail = try await groups.removePicture(id) }
                }
            }
            if detail.group.myRole == .owner {
                Toggle(isOn: publicBinding(detail)) {
                    Text(words.public)
                }
                .accessibilityIdentifier("group-public")
            }
        } footer: {
            Text(detail.group.isPublic
                ? "Anyone signed in finds, reads and joins a public group."
                : "Only members read a private group; you and its admins invite pilots into it.")
        }
    }

    private func mutedBinding(_ detail: CommunityDetail) -> Binding<Bool> {
        Binding(
            get: { self.detail?.group.muted ?? detail.group.muted },
            set: { muted in
                act {
                    let standing = try await groups.mute(id, muted)
                    self.detail?.group.muted = standing.muted
                }
            }
        )
    }

    private func publicBinding(_ detail: CommunityDetail) -> Binding<Bool> {
        Binding(
            get: { self.detail?.group.isPublic ?? detail.group.isPublic },
            set: { isPublic in
                act { self.detail = try await groups.setPublic(id, isPublic) }
            }
        )
    }

    private func load() async {
        do {
            detail = try await groups.detail(id)
            problem = nil
        } catch {
            problem = AccountModel.words(for: error)
        }
    }

    private func act(thenLeave: Bool = false, _ work: @escaping @MainActor () async throws -> Void) {
        guard !isWorking else { return }
        isWorking = true
        problem = nil
        outcome = nil
        Task {
            defer { isWorking = false }
            do {
                try await work()
                if thenLeave {
                    dismiss()
                }
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}

// MARK: Making and changing a group

/// The form for a group: a new one (name, public or private, description,
/// home, picture), or the name, description and home of one the pilot
/// owns or is an admin of.
struct GroupForm: View {
    let editing: CommunityDetail?
    /// Called with the group as Venti has it after saving.
    var saved: (CommunityDetail) -> Void = { _ in }

    @Environment(GroupsModel.self) private var groups
    @Environment(WindModel.self) private var wind
    @Environment(\.dismiss) private var dismiss

    /// Where the group's home is.
    private enum Home: Hashable {
        case map, position, kept, none
    }

    @State private var name = ""
    @State private var isPublic = true
    @State private var description = ""
    @State private var home = Home.map
    @State private var place = ""
    @State private var radius = 30.0
    @State private var position = PilotPosition()
    @State private var item: PhotosPickerItem?
    @State private var jpeg: Data?
    @State private var isSaving = false
    @State private var problem: String?

    private var words: CommunityWords { groups.words }

    private var homes: [(value: Home, name: String)] {
        var homes: [(value: Home, name: String)] = []
        if editing?.group.center != nil {
            homes.append((value: .kept, name: "As it is"))
        }
        homes += [(value: .map, name: "Where the map is"), (value: .position, name: "My position"), (value: .none, name: "None")]
        return homes
    }

    /// The middle of the home as chosen; `nil` for none, and while the
    /// position is not there yet.
    private var center: Coordinate? {
        switch home {
        case .map: wind.mapCentre
        case .position: position.place
        case .kept: editing?.group.center
        case .none: nil
        }
    }

    var body: some View {
        NavigationStack {
            VentiList {
                if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                            .accessibilityIdentifier("group-form-problem")
                    }
                }
                Section {
                    TextField("Name", text: $name)
                        .accessibilityIdentifier("group-form-name")
                    TextField("What the group is about", text: $description, axis: .vertical)
                        .lineLimit(2...6)
                        .accessibilityIdentifier("group-form-description")
                }
                if editing == nil {
                    Section {
                        VentiSegmented(
                            options: [(value: true, name: words.public), (value: false, name: words.private)],
                            selection: $isPublic
                        )
                    } footer: {
                        Text(isPublic
                            ? "Anyone signed in finds, reads and joins it."
                            : "Only its members read it; you and its admins invite pilots by handle.")
                    }
                }
                Section {
                    VentiSegmented(options: homes, selection: $home)
                    if home != .none {
                        TextField("The place, such as Inn valley", text: $place)
                            .accessibilityIdentifier("group-form-place")
                        Stepper(value: $radius, in: 5...100, step: 5) {
                            Text(verbatim: "\(Format.decimal(radius, places: 0)) km around")
                        }
                        .accessibilityIdentifier("group-form-radius")
                    }
                    if home == .position, position.place == nil {
                        Text(position.isDenied ? "Venti may not read the position." : "Waiting for the position…")
                            .foregroundStyle(.venti(.textSecondary))
                    }
                } header: {
                    Text("Home")
                } footer: {
                    Text("Pilots near it find the group.")
                }
                if editing == nil {
                    Section {
                        let pictureWords = jpeg == nil ? "Give it a picture" : "Another picture"
                        PhotosPicker(selection: $item, matching: .images) {
                            Label(pictureWords, systemImage: "photo")
                        }
                        .accessibilityIdentifier("group-form-picture")
                        if let jpeg, let image = UIImage(data: jpeg) {
                            Image(uiImage: image)
                                .resizable()
                                .scaledToFill()
                                .frame(width: VentiSpace.s8, height: VentiSpace.s8)
                                .clipShape(.rect(cornerRadius: VentiRadius.s))
                                .accessibilityLabel("The chosen picture")
                        }
                    } footer: {
                        Text("Optional. Cut square.")
                    }
                }
            }
            .navigationTitle(editing == nil ? "New group" : "Change the group")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .disabled(isSaving)
                        .accessibilityIdentifier("group-form-save")
                }
            }
            .onAppear(perform: fill)
            .onChange(of: home) { _, home in
                if home == .position {
                    position.ask()
                }
            }
            .onChange(of: item) { _, item in
                guard let item else { return }
                Task {
                    if let data = try? await item.loadTransferable(type: Data.self), let made = PictureUpload.jpeg(data) {
                        jpeg = made
                    } else {
                        problem = PictureUpload.unreadable
                    }
                }
            }
        }
    }

    private func fill() {
        guard let group = editing?.group, name.isEmpty else { return }
        name = group.name
        description = group.description ?? ""
        place = group.place ?? ""
        radius = group.radiusKm ?? radius
        home = group.center == nil ? .none : .kept
    }

    private func save() {
        guard !isSaving else { return }
        problem = nil
        let draft = CommunityDraft(
            name: name, description: description,
            home: center.map { HomeDraft(center: $0, radiusKm: radius, place: place) }
        )
        if home == .position, center == nil {
            problem = "Venti does not know the position yet."
            return
        }
        isSaving = true
        Task {
            defer { isSaving = false }
            do {
                let detail = if let editing {
                    try await groups.change(editing.group.id, to: draft)
                } else {
                    try await groups.create(draft, isPublic: isPublic, picture: jpeg)
                }
                saved(detail)
                dismiss()
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}
