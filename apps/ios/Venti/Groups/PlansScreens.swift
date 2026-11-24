import SwiftUI
import VentiCore

/// Who flies where on which day: the plans of every group the pilot is
/// in, day by day from today on. Opened from the map.
///
/// A plan is seen by the members of its group and by nobody else. The
/// server removes it two days after its day.
struct PlansSheet: View {
    @Environment(GroupsModel.self) private var groups
    @Environment(AccountModel.self) private var account
    @Environment(\.dismiss) private var dismiss

    @State private var isAddingPlan = false

    var body: some View {
        NavigationStack {
            VentiList {
                if !account.isSignedIn {
                    Section {
                        Text("Plans for a flying day are shared in groups on Venti. Sign in under Settings, Account, then make a group or join one.")
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("plans-signed-out")
                    }
                } else if let board = groups.board {
                    if board.groups.isEmpty {
                        Section {
                            Text("You are in no group yet. A group is a few pilots who tell each other where they mean to fly; only its members see it.")
                                .foregroundStyle(.secondary)
                        }
                    } else if board.days.isEmpty {
                        Section {
                            Text("Nobody in your groups has posted a plan for today or a later day.")
                                .foregroundStyle(.secondary)
                                .accessibilityIdentifier("plans-none")
                        }
                    }
                    ForEach(board.days, id: \.day) { day in
                        Section(PlanWords.title(ofDay: day.day)) {
                            ForEach(day.plans, id: \.id) { plan in
                                NavigationLink {
                                    PlanScreen(id: plan.id)
                                } label: {
                                    PlanRow(plan: plan)
                                }
                                .accessibilityIdentifier("plan-row")
                            }
                        }
                    }
                    if board.isCutShort {
                        Section {
                            Text("Your groups hold more plans than fit here: the later days are missing.")
                                .foregroundStyle(.secondary)
                        }
                    }
                    Section {
                        NavigationLink {
                            GroupsScreen()
                        } label: {
                            Label(board.groups.isEmpty ? "Make or join a group" : "Groups", systemImage: "person.3")
                        }
                        .accessibilityIdentifier("plans-groups")
                    } footer: {
                        Text("A plan is seen by the members of its group and nobody else, and is removed two days after its day.")
                    }
                } else if let problem = groups.problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.secondary)
                    }
                } else {
                    Section {
                        ProgressView()
                            .frame(maxWidth: .infinity)
                    }
                }
            }
            .navigationTitle("Flying plans")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
                if !groups.groups.isEmpty {
                    ToolbarItem(placement: .primaryAction) {
                        Button("New plan", systemImage: "plus") {
                            isAddingPlan = true
                        }
                    }
                }
            }
            .refreshable {
                await groups.load()
            }
            .sheet(isPresented: $isAddingPlan) {
                PlanForm(editing: nil)
            }
        }
        .task {
            await groups.load()
        }
    }
}

/// How a plan's day and time are said.
enum PlanWords {
    /// "Today", "Tomorrow", or the day with its weekday.
    static func title(ofDay day: String, now: Date = Date(), calendar: Calendar = .current) -> String {
        guard let date = GroupsModel.date(ofDay: day, calendar: calendar) else { return day }
        if calendar.isDate(date, inSameDayAs: now) {
            return "Today"
        }
        if let tomorrow = calendar.date(byAdding: .day, value: 1, to: now), calendar.isDate(date, inSameDayAs: tomorrow) {
            return "Tomorrow"
        }
        return date.formatted(.dateTime.weekday(.wide).day().month(.abbreviated))
    }
}

/// One plan in a line: where, when, in which group, and who is in.
struct PlanRow: View {
    let plan: PlanView

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(plan.place)
                    .font(.venti(.body, .semibold))
                Text(plan.inWords)
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
                if !plan.groupName.isEmpty {
                    Text(plan.groupName)
                        .font(.venti(.footnote))
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 3) {
                if let time = plan.meetAt {
                    Text(time)
                        .font(.venti(.body, .semibold))
                        .monospacedDigit()
                }
                if plan.amIn {
                    Label("You are in", systemImage: "checkmark.circle.fill")
                        .labelStyle(.iconOnly)
                        .foregroundStyle(Palette.accent)
                }
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// One plan: everything it says, who is in, and the pilot's own word on
/// it.
struct PlanScreen: View {
    let id: String

    @Environment(GroupsModel.self) private var groups
    @Environment(\.dismiss) private var dismiss

    @State private var isWorking = false
    @State private var problem: String?
    @State private var isEditing = false
    @State private var isConfirmingDelete = false
    @State private var reporting: Reported?

    var body: some View {
        VentiList {
            if let plan = groups.plan(id) {
                Section {
                    LabeledContent("Where", value: plan.place)
                    LabeledContent("Day", value: PlanWords.title(ofDay: plan.day))
                    if let time = plan.meetAt {
                        LabeledContent("Time", value: time)
                    }
                    if !plan.groupName.isEmpty {
                        LabeledContent("Group", value: plan.groupName)
                    }
                    LabeledContent("Posted by", value: plan.isMine ? "You" : plan.author)
                }
                if let note = plan.note {
                    Section("Note") {
                        Text(note)
                    }
                }
                if let position = plan.position {
                    // The plan says where on the map: the forecast for
                    // that place is one step away.
                    Section {
                        NavigationLink {
                            ForecastScreen(place: position, title: plan.place)
                        } label: {
                            Label("Forecast for this place", systemImage: "cloud.sun")
                        }
                    } footer: {
                        Text(Format.place(lat: position.lat, lon: position.lon))
                    }
                }
                Section("Who is in") {
                    if plan.pilotsIn.isEmpty {
                        Text("Nobody yet")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(Array(plan.pilotsIn.enumerated()), id: \.offset) { index, name in
                        HStack(spacing: VentiSpace.s3) {
                            PilotFace(name: name, size: VentiSpace.s6, isMe: plan.amIn && index == 0)
                            Text(name)
                        }
                    }
                    Button(plan.amIn ? "I am out" : "I am in", systemImage: plan.amIn ? "xmark" : "checkmark") {
                        act { try await groups.setIn(plan, !plan.amIn) }
                    }
                    .disabled(isWorking)
                    .accessibilityIdentifier("plan-in")
                }
                if plan.isMine || plan.canDelete {
                    Section {
                        if plan.isMine {
                            Button("Change the plan", systemImage: "pencil") {
                                isEditing = true
                            }
                        }
                        if plan.canDelete {
                            Button("Delete the plan", systemImage: "trash", role: .destructive) {
                                isConfirmingDelete = true
                            }
                        }
                    }
                    .disabled(isWorking)
                }
                // Another member's plan that should not be there: to
                // whoever runs the server, and to nobody else. One's own
                // is changed or deleted instead.
                if !plan.isMine {
                    Section {
                        Button("Report this plan", systemImage: "exclamationmark.bubble") {
                            reporting = .plan(groupId: plan.groupId, id: plan.id)
                        }
                        .accessibilityIdentifier("plan-report")
                    }
                }
                if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                    }
                }
            } else {
                Section {
                    Text("This plan is no longer there.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Plan")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $isEditing) {
            PlanForm(editing: groups.plan(id))
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .confirmationDialog("Delete this plan?", isPresented: $isConfirmingDelete, titleVisibility: .visible) {
            Button("Delete the plan", role: .destructive) {
                if let plan = groups.plan(id) {
                    act(thenLeave: true) { try await groups.delete(plan) }
                }
            }
        } message: {
            Text("It goes for everyone in the group, with who was in.")
        }
    }

    /// Does one thing on the server and says what went wrong, if anything.
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
                }
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}

/// The form for a plan: a new one, or the pilot's own to change.
struct PlanForm: View {
    /// The plan to change; `nil` for a new one.
    let editing: PlanView?
    /// The group a new plan is posted to, where the form was opened from
    /// its chat; else the pilot chooses.
    var inGroup: String?

    @Environment(GroupsModel.self) private var groups
    @Environment(WaypointsModel.self) private var waypoints
    @Environment(\.dismiss) private var dismiss

    @State private var group = ""
    @State private var day = Date()
    @State private var place = ""
    /// Where the place is on the map, if the pilot took it from a
    /// waypoint.
    @State private var position: Coordinate?
    @State private var hasTime = false
    @State private var time = Date()
    @State private var note = ""
    @State private var isSaving = false
    @State private var problem: String?

    var body: some View {
        NavigationStack {
            VentiList {
                if editing == nil, inGroup == nil, groups.groups.count > 1 {
                    Section {
                        Picker("Group", selection: $group) {
                            ForEach(groups.groups, id: \.id) { group in
                                Text(group.name).tag(group.id)
                            }
                        }
                    }
                }
                Section {
                    TextField("Where", text: $place)
                        .accessibilityIdentifier("plan-place")
                    // The pilot's own waypoints know where they are: a
                    // plan made from one can lead to the forecast there.
                    if !waypoints.all.isEmpty {
                        Menu {
                            ForEach(waypoints.all.prefix(40), id: \.id) { waypoint in
                                Button(waypoint.name) {
                                    place = waypoint.name
                                    position = waypoint.position
                                }
                            }
                        } label: {
                            Label("From my waypoints", systemImage: "mappin.and.ellipse")
                        }
                    }
                    if let position {
                        LabeledContent("On the map") {
                            HStack(spacing: 8) {
                                Text(Format.place(lat: position.lat, lon: position.lon))
                                    .monospacedDigit()
                                Button("Take the position off", systemImage: "xmark.circle.fill") {
                                    self.position = nil
                                }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.borderless)
                                .foregroundStyle(.secondary)
                            }
                        }
                    }
                    DatePicker("Day", selection: $day, in: Date()..., displayedComponents: .date)
                    Toggle("Set a time", isOn: $hasTime)
                    if hasTime {
                        DatePicker("Time", selection: $time, displayedComponents: .hourAndMinute)
                    }
                }
                Section {
                    TextField("Anything else", text: $note, axis: .vertical)
                        .lineLimit(3...8)
                } header: {
                    Text("Note")
                } footer: {
                    Text("The plan is posted in the group's chat, with \"I'm in\" on its card. It is removed two days after its day.")
                }
                if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                    }
                }
            }
            .navigationTitle(editing == nil ? "New plan" : "Change the plan")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        save()
                    }
                    .disabled(isSaving || place.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .onAppear(perform: fill)
        }
    }

    /// Starts from the plan being changed, or from today in the first
    /// group.
    private func fill() {
        if let editing {
            group = editing.groupId
            place = editing.place
            position = editing.position
            note = editing.note ?? ""
            day = GroupsModel.date(ofDay: editing.day) ?? Date()
            if let meetAt = editing.meetAt {
                let parts = meetAt.split(separator: ":").compactMap { Int($0) }
                if parts.count == 2,
                   let at = Calendar.current.date(bySettingHour: parts[0], minute: parts[1], second: 0, of: day)
                {
                    hasTime = true
                    time = at
                }
            }
        } else if let inGroup {
            group = inGroup
        } else if group.isEmpty {
            group = groups.groups.first?.id ?? ""
        }
    }

    private func save() {
        guard !isSaving else { return }
        isSaving = true
        problem = nil
        let draft = PlanDraft(
            day: GroupsModel.day(day), place: place, position: position,
            meetAt: hasTime ? GroupsModel.timeOfDay(time) : nil,
            note: note
        )
        Task {
            defer { isSaving = false }
            do {
                if let editing {
                    try await groups.change(editing, to: draft)
                } else {
                    try await groups.post(draft, to: group)
                }
                dismiss()
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}
