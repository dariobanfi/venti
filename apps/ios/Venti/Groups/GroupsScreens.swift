import SwiftUI
import VentiCore

/// The pilot's groups, and the two ways into one: making it, or joining
/// with the code its keeper hands out.
struct GroupsList: View {
    @Environment(GroupsModel.self) private var groups

    @State private var isMaking = false
    @State private var isJoining = false
    @State private var typed = ""
    @State private var problem: String?
    /// What became of the last thing done, in a line.
    @State private var outcome: String?
    /// A group just made, to show its invite code once.
    @State private var made: MadeGroup?

    var body: some View {
        VentiList {
            if !groups.groups.isEmpty {
                Section {
                    ForEach(groups.groups, id: \.id) { group in
                        NavigationLink {
                            GroupScreen(id: group.id, name: group.name)
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(group.name)
                                Text(group.summary)
                                    .font(.venti(.footnote))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .accessibilityIdentifier("group-row")
                    }
                }
            }
            Section {
                Button("Make a group", systemImage: "plus") {
                    typed = ""
                    isMaking = true
                }
                Button("Join with a code", systemImage: "person.badge.key") {
                    typed = ""
                    isJoining = true
                }
            } footer: {
                Text("A group is seen by its members and nobody else; it cannot be searched for. Whoever makes it keeps it, and hands out its invite code.")
            }
            if let outcome {
                Section {
                    Label(outcome, systemImage: "checkmark.circle")
                }
            }
            if let problem {
                Section {
                    Text(problem)
                        .foregroundStyle(.venti(.danger))
                }
            }
        }
        .navigationTitle("Groups")
        .navigationBarTitleDisplayMode(.inline)
        .alert("Make a group", isPresented: $isMaking) {
            TextField("Its name", text: $typed)
            Button("Make") {
                let name = typed
                act {
                    let group = try await groups.make(name)
                    made = MadeGroup(name: group.group.name, code: group.inviteCode)
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("You will keep it: you hand out its invite code and can remove a member.")
        }
        .alert("Join a group", isPresented: $isJoining) {
            TextField("Invite code", text: $typed)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Join") {
                let code = typed
                act {
                    outcome = try await groups.join(code).words
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The group's keeper has the code.")
        }
        .sheet(item: $made) { made in
            InviteCodeSheet(group: made.name, code: made.code)
        }
    }

    private func act(_ work: @escaping @MainActor () async throws -> Void) {
        problem = nil
        outcome = nil
        Task {
            do {
                try await work()
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}

/// A group whose invite code is to be shown.
struct MadeGroup: Identifiable {
    let name: String
    let code: String

    var id: String { code }
}

/// A group's invite code, shown once: the server keeps no copy that can
/// be read.
struct InviteCodeSheet: View {
    let group: String
    let code: String

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Text(code)
                        .font(.venti(.title2, .semibold))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                        .textSelection(.enabled)
                        .accessibilityIdentifier("invite-code")
                    ShareLink(item: code) {
                        Label("Hand the code on", systemImage: "square.and.arrow.up")
                    }
                } header: {
                    Text(group)
                } footer: {
                    Text("Whoever has this code and an account on Venti can come into the group. It is shown this once: Venti keeps no copy that can be read. A new code can be made at any time and ends this one.")
                }
            }
            .navigationTitle("Invite code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}

/// One group: its members, and what its keeper or a member can do.
struct GroupScreen: View {
    let id: String
    let name: String

    @Environment(GroupsModel.self) private var groups
    @Environment(\.dismiss) private var dismiss

    @State private var detail: GroupDetail?
    @State private var problem: String?
    @State private var isWorking = false
    @State private var isRenaming = false
    @State private var typed = ""
    @State private var code: MadeGroup?
    @State private var asking: Asking?
    @State private var reporting: Reported?

    /// Something that is asked about before it is done.
    /// What the pilot is asked about before it is done.
    private enum Asking: Identifiable {
        case newCode, leave, delete
        case remove(GroupMember)
        case handOver(GroupMember)

        var id: String {
            switch self {
            case .newCode: "new-code"
            case .leave: "leave"
            case .delete: "delete"
            case .remove(let member): "remove-\(member.handle)"
            case .handOver(let member): "hand-over-\(member.handle)"
            }
        }
    }

    var body: some View {
        VentiList {
            if let detail {
                Section("Members") {
                    ForEach(detail.members, id: \.handle) { member in
                        MemberRow(member: member)
                            .swipeActions(edge: .trailing) {
                                // Both are asked about first: a swipe
                                // is quickly made, and neither is undone
                                // by the keeper alone.
                                if detail.group.isKeptByMe, !member.isMe {
                                    Button("Remove") {
                                        asking = .remove(member)
                                    }
                                    .tint(Color.venti(.danger))
                                    Button("Make keeper") {
                                        asking = .handOver(member)
                                    }
                                    .tint(Palette.accent)
                                }
                            }
                    }
                }
                if detail.group.isKeptByMe {
                    Section {
                        Button("New invite code", systemImage: "key") {
                            asking = .newCode
                        }
                        Button("Rename the group", systemImage: "pencil") {
                            typed = detail.group.name
                            isRenaming = true
                        }
                    } footer: {
                        Text("You keep this group. Swipe a member to remove them or to hand the keeping on. Removing a member ends the invite code, which they know.")
                    }
                }
                Section {
                    Button("Leave the group", systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) {
                        asking = .leave
                    }
                    if detail.group.isKeptByMe {
                        Button("Delete the group", systemImage: "trash", role: .destructive) {
                            asking = .delete
                        }
                    } else {
                        // For a group that should not be there: to whoever
                        // runs the server, and to nobody else. Not for the
                        // keeper, who can delete it.
                        Button("Report this group", systemImage: "exclamationmark.bubble") {
                            reporting = .group(id)
                        }
                        .accessibilityIdentifier("group-report")
                    }
                } footer: {
                    Text(
                        detail.group.isKeptByMe
                            ? "When you leave, the member who has been in longest keeps the group. Your plans go with you."
                            : "Your plans, and your word on the plans of others, go with you."
                    )
                }
            } else if problem == nil {
                Section {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
            if let problem {
                Section {
                    Text(problem)
                        .foregroundStyle(.venti(.danger))
                }
            }
        }
        .disabled(isWorking)
        .navigationTitle(detail?.group.name ?? name)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: id) {
            await load()
        }
        .alert("Rename the group", isPresented: $isRenaming) {
            TextField("Its name", text: $typed)
            Button("Rename") {
                let name = typed
                act { detail = try await groups.rename(id, to: name) }
            }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog(
            asking.map(Self.question) ?? "", isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
            titleVisibility: .visible, presenting: asking
        ) { asking in
            switch asking {
            case .newCode:
                Button("Make a new code") {
                    act { code = MadeGroup(name: detail?.group.name ?? name, code: try await groups.newInvite(id)) }
                }
            case .leave:
                Button("Leave the group", role: .destructive) {
                    act(thenLeave: true) { try await groups.leave(id) }
                }
            case .delete:
                Button("Delete the group", role: .destructive) {
                    act(thenLeave: true) { try await groups.delete(group: id) }
                }
            case .remove(let member):
                Button("Remove \(member.name)", role: .destructive) {
                    act { self.detail = nil; try await groups.remove(member.handle, from: id) }
                }
            case .handOver(let member):
                Button("Make \(member.name) the keeper") {
                    act { self.detail = try await groups.handOver(id, to: member.handle) }
                }
            }
        } message: { asking in
            Text(Self.consequence(asking))
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .sheet(item: $code) { made in
            InviteCodeSheet(group: made.name, code: made.code)
        }
    }

    private static func question(_ asking: Asking) -> String {
        switch asking {
        case .newCode: "Make a new invite code?"
        case .leave: "Leave this group?"
        case .delete: "Delete this group?"
        case .remove(let member): "Remove \(member.name) from the group?"
        case .handOver(let member): "Make \(member.name) the keeper?"
        }
    }

    private static func consequence(_ asking: Asking) -> String {
        switch asking {
        case .newCode: "The code handed out before stops working. Members stay in the group."
        case .leave: "You will no longer see its plans, and yours are removed."
        case .delete: "The group goes for every member, with all its plans."
        case .remove: "They no longer see the group's plans, and their own are removed. The invite code stops working, since they know it."
        case .handOver: "Only the keeper renames the group, makes invite codes and removes members. You stay a member."
        }
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
        Task {
            defer { isWorking = false }
            do {
                try await work()
                if thenLeave {
                    dismiss()
                } else if detail == nil {
                    await load()
                }
            } catch {
                problem = AccountModel.words(for: error)
                if detail == nil {
                    await load()
                }
            }
        }
    }
}

/// One member in a line.
private struct MemberRow: View {
    let member: GroupMember

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(member.isMe ? "\(member.name) (you)" : member.name)
                Text(verbatim: "@\(member.handle)")
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            if member.isKeeper {
                Text("Keeper")
                    .font(.venti(.footnote, .medium))
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("group-member")
    }
}
