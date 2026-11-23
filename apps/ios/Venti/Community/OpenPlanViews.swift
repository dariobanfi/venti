import SwiftUI
import VentiCore

/// What open plans' screens say that is not the core's to say. Android's
/// `OpenPlanWords`, word for word.
enum OpenPlanWords {
    static let title = "Open plans near"
    static let new = "New open plan"
    static let change = "Change the open plan"
    static let plan = "Open plan"
    static let iAmIn = "I'm in"
    static let iAmOut = "I'm out"
    static let signInToJoin = "Sign in to say you are in"
    static let cutShort = "There are more open plans near here: the later days are missing."
    static let whereTitle = "Where"
    static let placeHint = "The name of the place"
    static let takeoffNear = "A take-off near"
    static let onTheMap = "On the map"
    static let mapMiddle = "Where the map is"
    static let mapMiddleNote =
        "An open plan is found by its place. Take a take-off near, or move the map to the place on the Map tab and take where the map is."
    static let seatsFree = "Seats free"
    static let company = "Looking for company"
    static let seats = "Seats"
    static let note = "Note"
    static let noteHint = "Anything else"
    static let post = "Post"
    static let save = "Save"
    static let postedBy = "Posted by"
    static let offer = "Offer"
    static let whoIsIn = "Who is in"
    static let nobody = "Nobody yet"
    static let forecast = "Forecast for this place"
    static let delete = "Delete the open plan"
    static let deleteQuestion = "Delete this open plan?"
    static let deleteMeans = "It goes for everyone, with who was in."
    static let missing = "This open plan is no longer there."
    static let you = "You"

    /// How many take-offs near the form offers, the nearest first.
    static let takeoffs = 30

    /// The seats a pilot can offer, as the core takes them.
    static let seatCounts: ClosedRange<UInt8> = 1...8

    /// One plan's line for someone who cannot see it: the place is its
    /// name, this the rest.
    static func spoken(_ plan: OpenPlanView) -> String {
        [
            plan.meetAt.map { "At \($0)." },
            "\(plan.offerWords).",
            "Posted by \(plan.isMine ? you : plan.author).",
            plan.inWords.isEmpty ? nil : "\(plan.inWords).",
            plan.amIn ? "You are in." : nil,
        ].compactMap { $0 }.joined(separator: " ")
    }
}

/// One open plan as a card of Community, in the manner of a group's plan
/// card: its day and time at the top, the time large; the place with the
/// pack's site sign; the author's face and name with what they offer in
/// the core's words ("2 seats free", "Looking for company"); the note; the
/// faces of who is in with the core's sentence; and "I'm in", pressed
/// while the pilot is in. Signed out, "I'm in" leads to the account. The
/// upper part opens the plan's page.
///
/// An open plan's answer names no pictures: its faces are initials.
struct OpenPlanCard: View {
    let plan: OpenPlanView
    /// The name the pilot of this iPhone is shown by, for their face.
    let myName: String?

    @Environment(OpenPlansModel.self) private var model
    @ScaledMetric(relativeTo: .body) private var sign = VentiSpace.s5

    @State private var isSending = false
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            NavigationLink {
                OpenPlanScreen(id: plan.id)
            } label: {
                VStack(alignment: .leading, spacing: VentiSpace.s2) {
                    HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                        Text(PlanWords.title(ofDay: plan.day))
                            .font(.venti(.subheadline, .semibold))
                            .foregroundStyle(.venti(.link))
                        Spacer(minLength: 0)
                        if let time = plan.meetAt {
                            Text(time)
                                .font(.ventiType(.instrumentM))
                                .foregroundStyle(.venti(.text))
                        }
                    }
                    HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                        Image("icon-site")
                            .resizable()
                            .frame(width: sign, height: sign)
                            .foregroundStyle(.venti(.textMuted))
                            .alignmentGuide(.firstTextBaseline) { $0[.bottom] * 0.8 }
                            .accessibilityHidden(true)
                        Text(plan.place)
                            .font(.ventiType(.heading))
                            .foregroundStyle(.venti(.text))
                            .multilineTextAlignment(.leading)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    // Who offers what.
                    HStack(spacing: VentiSpace.s3) {
                        PilotFace(
                            name: plan.isMine ? (myName ?? plan.author) : plan.author,
                            size: VentiSpace.s6, isMe: plan.isMine
                        )
                        VStack(alignment: .leading, spacing: 0) {
                            Text(plan.isMine ? OpenPlanWords.you : plan.author)
                                .font(.venti(.footnote))
                                .foregroundStyle(.venti(.textSecondary))
                                .lineLimit(1)
                            Text(plan.offerWords)
                                .font(.venti(.body, .semibold))
                                .foregroundStyle(.venti(.text))
                        }
                    }
                    if let note = plan.note, !note.isEmpty {
                        Text(note)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                            .lineLimit(3)
                            .multilineTextAlignment(.leading)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(plan.place)
            .accessibilityValue("\(PlanWords.title(ofDay: plan.day)). \(OpenPlanWords.spoken(plan))")
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("open-plan-card")

            if !plan.inWords.isEmpty {
                HStack(spacing: VentiSpace.s2) {
                    if !plan.pilotsIn.isEmpty {
                        PlanFaces(names: plan.pilotsIn, firstIsMe: plan.amIn, myName: myName)
                    }
                    Text(plan.inWords)
                        .font(.venti(.footnote))
                        .foregroundStyle(.venti(.textSecondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
            }

            if model.isSignedIn {
                Button {
                    toggle()
                } label: {
                    InLabel(isIn: plan.amIn)
                }
                .buttonStyle(.venti(plan.amIn ? .primary : .quiet))
                .overlay {
                    if !plan.amIn {
                        Capsule().strokeBorder(.venti(.borderStrong), lineWidth: 1)
                    }
                }
                .disabled(isSending)
                .accessibilityAddTraits(plan.amIn ? .isSelected : [])
                // Which plan it is, for a test among several cards.
                .accessibilityValue(plan.place)
                .accessibilityIdentifier("open-plan-card-in")
            } else {
                // Saying so needs an account: the way to one.
                NavigationLink {
                    AccountScreen()
                } label: {
                    InLabel(isIn: false)
                }
                .buttonStyle(.venti(.quiet))
                .overlay {
                    Capsule().strokeBorder(.venti(.borderStrong), lineWidth: 1)
                }
                .accessibilityValue(plan.place)
                .accessibilityIdentifier("open-plan-card-in")
            }

            if let problem {
                Text(problem)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.danger))
                    .accessibilityIdentifier("open-plan-card-problem")
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .ventiCard()
    }

    private func toggle() {
        guard !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                _ = try await model.setIn(plan.id, !plan.amIn)
            } catch {
                problem = OpenPlansModel.words(for: error)
            }
        }
    }
}

/// "I'm in", with the pack's tick before it while the pilot is in.
private struct InLabel: View {
    let isIn: Bool

    var body: some View {
        HStack(spacing: VentiSpace.s2) {
            if isIn {
                ChipTick()
            }
            Text(OpenPlanWords.iAmIn)
        }
        .frame(maxWidth: .infinity)
    }
}

/// One open plan: everything it says, who posted it, who is in, and the
/// pilot's own word on it. Its author changes and deletes it; another
/// pilot, signed in, reports it.
struct OpenPlanScreen: View {
    let id: String

    @Environment(OpenPlansModel.self) private var model
    @Environment(AccountModel.self) private var account
    @Environment(\.dismiss) private var dismiss

    /// The plan as the server last said it; until then the list's.
    @State private var read: OpenPlanView?
    @State private var missing: String?
    @State private var isWorking = false
    @State private var problem: String?
    @State private var isEditing = false
    @State private var isConfirmingDelete = false
    @State private var reporting: Reported?

    private var shown: OpenPlanView? {
        read ?? model.listed(id)
    }

    var body: some View {
        VentiList {
            if let plan = shown {
                Section {
                    VStack(alignment: .leading, spacing: VentiSpace.s1) {
                        HStack(alignment: .firstTextBaseline) {
                            Text(PlanWords.title(ofDay: plan.day))
                                .font(.venti(.subheadline, .semibold))
                                .foregroundStyle(.venti(.link))
                            Spacer(minLength: 0)
                            if let time = plan.meetAt {
                                Text(time)
                                    .font(.ventiType(.instrumentM))
                                    .foregroundStyle(.venti(.text))
                            }
                        }
                        Text(plan.place)
                            .font(.ventiType(.heading))
                            .foregroundStyle(.venti(.text))
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("open-plan-head")
                    author(plan)
                    LabeledContent(OpenPlanWords.offer, value: plan.offerWords)
                        .accessibilityIdentifier("open-plan-offer")
                    NavigationLink {
                        ForecastScreen(place: plan.position, title: plan.place)
                    } label: {
                        LabeledContent(OpenPlanWords.forecast) {
                            Text(Format.place(lat: plan.position.lat, lon: plan.position.lon))
                                .monospacedDigit()
                        }
                    }
                    .accessibilityIdentifier("open-plan-forecast")
                }
                if let note = plan.note, !note.isEmpty {
                    Section(OpenPlanWords.note) {
                        Text(note)
                            .accessibilityIdentifier("open-plan-note-text")
                    }
                }
                Section {
                    if plan.pilotsIn.isEmpty {
                        Text(OpenPlanWords.nobody)
                            .foregroundStyle(.secondary)
                    }
                    ForEach(Array(plan.pilotsIn.enumerated()), id: \.offset) { index, name in
                        let isMe = plan.amIn && index == 0
                        HStack(spacing: VentiSpace.s3) {
                            PilotFace(name: isMe ? (account.account?.displayName ?? name) : name, size: VentiSpace.s6, isMe: isMe)
                            Text(name)
                        }
                        .accessibilityIdentifier("open-plan-pilot")
                    }
                    if model.isSignedIn {
                        Button(plan.amIn ? OpenPlanWords.iAmOut : OpenPlanWords.iAmIn) {
                            act { read = try await model.setIn(plan.id, !plan.amIn) }
                        }
                        .disabled(isWorking)
                        .accessibilityIdentifier("open-plan-in")
                    } else {
                        NavigationLink(OpenPlanWords.signInToJoin) {
                            AccountScreen()
                        }
                        .accessibilityIdentifier("open-plan-sign-in")
                    }
                } header: {
                    Text(OpenPlanWords.whoIsIn)
                } footer: {
                    // Said before the pilot says "I'm in": everyone reads
                    // it, with their name.
                    let words = plan.publicWords.isEmpty ? model.publicWords : plan.publicWords
                    if !words.isEmpty {
                        Text(words)
                            .accessibilityIdentifier("open-plan-public")
                    }
                }
                if plan.isMine {
                    Section {
                        Button(OpenPlanWords.change) {
                            isEditing = true
                        }
                        .accessibilityIdentifier("open-plan-change")
                        Button(OpenPlanWords.delete, role: .destructive) {
                            isConfirmingDelete = true
                        }
                        .accessibilityIdentifier("open-plan-delete")
                    }
                    .disabled(isWorking)
                } else if model.isSignedIn {
                    // Another pilot's plan that should not be there: to
                    // whoever runs Venti.
                    Section {
                        Button(Reported.openPlan(plan.id).title) {
                            reporting = .openPlan(plan.id)
                        }
                        .accessibilityIdentifier("open-plan-report")
                    }
                }
                if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                            .accessibilityIdentifier("open-plan-problem")
                    }
                }
            } else if let missing {
                Section {
                    Text(missing)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("open-plan-missing")
                }
            } else {
                Section {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
        }
        .navigationTitle(OpenPlanWords.plan)
        .navigationBarTitleDisplayMode(.inline)
        .refreshable {
            await reread()
        }
        // Read when the page opens and when the pilot signs in or out:
        // whose it is and who is "You" follow the account.
        .task(id: model.isSignedIn) {
            await reread()
        }
        .sheet(isPresented: $isEditing) {
            OpenPlanForm(editing: shown, startingAt: nil) { changed in
                read = changed
            }
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .confirmationDialog(OpenPlanWords.deleteQuestion, isPresented: $isConfirmingDelete, titleVisibility: .visible) {
            Button(OpenPlanWords.delete, role: .destructive) {
                act(thenLeave: true) { try await model.delete(id) }
            }
            .accessibilityIdentifier("open-plan-delete-confirm")
        } message: {
            Text(OpenPlanWords.deleteMeans)
        }
    }

    /// Who posted it, with their face; another pilot's leads to their page.
    @ViewBuilder private func author(_ plan: OpenPlanView) -> some View {
        let face = HStack(spacing: VentiSpace.s3) {
            PilotFace(
                name: plan.isMine ? (account.account?.displayName ?? plan.author) : plan.author,
                size: VentiSpace.s6, isMe: plan.isMine
            )
            VStack(alignment: .leading, spacing: 0) {
                Text(OpenPlanWords.postedBy)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                Text(plan.isMine ? OpenPlanWords.you : plan.author)
                    .foregroundStyle(.venti(.text))
            }
        }
        .accessibilityElement(children: .combine)
        if plan.isMine {
            face
                .accessibilityIdentifier("open-plan-author")
        } else {
            NavigationLink {
                PilotFlightsScreen(pilot: PilotRoute(handle: plan.authorHandle, name: plan.author))
            } label: {
                face
            }
            .accessibilityIdentifier("open-plan-author")
        }
    }

    private func reread() async {
        do {
            read = try await model.plan(id)
            missing = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            if shown == nil {
                missing = error is CoreError ? error.words : OpenPlanWords.missing
            }
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
                problem = OpenPlansModel.words(for: error)
            }
        }
    }
}

/// The form for an open plan: a new one, or the pilot's own to change.
/// The group plan's form with three differences (docs/design.md, "Open
/// plans"): the place is marked on the map, from a take-off near or where
/// the map is; the seats free or "Looking for company"; and the core's
/// line that everyone reads it above "Post". What the core refuses, it
/// says in its own words under the form.
struct OpenPlanForm: View {
    /// The plan to change; `nil` for a new one.
    let editing: OpenPlanView?
    /// Where a new plan's place starts: Community's place; else where the
    /// map is.
    let startingAt: Coordinate?
    /// Hands on what the server answered.
    var done: (OpenPlanView) -> Void = { _ in }

    @Environment(OpenPlansModel.self) private var model
    @Environment(WindModel.self) private var wind
    @Environment(\.dismiss) private var dismiss

    private enum Offer: Hashable {
        case seats
        case company
    }

    @State private var day = Date()
    @State private var place = ""
    @State private var position: Coordinate?
    @State private var hasTime = false
    @State private var time = Date()
    @State private var offer = Offer.seats
    @State private var seats: UInt8 = 1
    @State private var note = ""
    @State private var takeoffs: [TodayTakeoff] = []
    @State private var isSaving = false
    @State private var problem: String?
    @State private var hasFilled = false

    var body: some View {
        NavigationStack {
            VentiList {
                // What the core refused, first: under "Post" in the bar,
                // where the pilot looks, on a form longer than the screen.
                if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                            .accessibilityIdentifier("open-plan-problem")
                    }
                }
                Section {
                    TextField(OpenPlanWords.placeHint, text: $place)
                        .accessibilityIdentifier("open-plan-place")
                    if !takeoffs.isEmpty {
                        Menu {
                            ForEach(takeoffs.prefix(OpenPlanWords.takeoffs), id: \.site.id) { takeoff in
                                Button(takeoff.site.name) {
                                    place = takeoff.site.name
                                    position = takeoff.site.position
                                }
                            }
                        } label: {
                            Label(OpenPlanWords.takeoffNear, image: "icon-takeoff")
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(.rect)
                        }
                        .accessibilityIdentifier("open-plan-takeoff")
                    }
                    Button {
                        position = wind.mapCentre
                    } label: {
                        Label(OpenPlanWords.mapMiddle, image: "icon-map")
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(.rect)
                    }
                    .accessibilityIdentifier("open-plan-map-middle")
                    if let position {
                        LabeledContent(OpenPlanWords.onTheMap) {
                            Text(Format.place(lat: position.lat, lon: position.lon))
                                .monospacedDigit()
                        }
                        .accessibilityIdentifier("open-plan-position")
                    }
                } header: {
                    Text(OpenPlanWords.whereTitle)
                } footer: {
                    Text(OpenPlanWords.mapMiddleNote)
                }
                Section {
                    DatePicker("Day", selection: $day, in: Calendar.current.startOfDay(for: Date())..., displayedComponents: .date)
                        .accessibilityIdentifier("open-plan-day")
                    Toggle("Set a time", isOn: $hasTime)
                        .accessibilityIdentifier("open-plan-has-time")
                    if hasTime {
                        DatePicker("Time", selection: $time, displayedComponents: .hourAndMinute)
                    }
                }
                Section(OpenPlanWords.offer) {
                    VentiSegmented(
                        options: [(Offer.seats, OpenPlanWords.seatsFree), (Offer.company, OpenPlanWords.company)],
                        selection: $offer
                    )
                    .accessibilityIdentifier("open-plan-offer-choice")
                    if offer == .seats {
                        Stepper(value: $seats, in: OpenPlanWords.seatCounts) {
                            LabeledContent(OpenPlanWords.seats) {
                                Text(Format.count(seats))
                                    .monospacedDigit()
                            }
                        }
                        .accessibilityIdentifier("open-plan-seats")
                    }
                }
                Section {
                    TextField(OpenPlanWords.noteHint, text: $note, axis: .vertical)
                        .lineLimit(3...8)
                        .accessibilityIdentifier("open-plan-note")
                } header: {
                    Text(OpenPlanWords.note)
                } footer: {
                    // The core's: everyone reads it, with the pilot's
                    // handle and name.
                    let words = editing?.publicWords.isEmpty == false ? editing?.publicWords ?? "" : model.publicWords
                    if !words.isEmpty {
                        Text(words)
                            .accessibilityIdentifier("open-plan-public")
                    }
                }
            }
            .navigationTitle(editing == nil ? OpenPlanWords.new : OpenPlanWords.change)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(commonWords().cancel) {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(editing == nil ? OpenPlanWords.post : OpenPlanWords.save) {
                        save()
                    }
                    .disabled(isSaving)
                    .accessibilityIdentifier("open-plan-save")
                }
            }
            // A change takes the refusal away: it was about what stood
            // before.
            .onChange(of: place) { problem = nil }
            .onChange(of: note) { problem = nil }
            .task {
                fill()
                if let around = position {
                    takeoffs = await model.takeoffsNear(around)
                }
            }
        }
    }

    /// Starts from the plan being changed, or from today at the place
    /// Community looks around.
    private func fill() {
        guard !hasFilled else { return }
        hasFilled = true
        guard let editing else {
            position = startingAt ?? wind.mapCentre
            return
        }
        place = editing.place
        position = editing.position
        note = editing.note ?? ""
        day = GroupsModel.date(ofDay: editing.day) ?? Date()
        if let count = editing.seats {
            offer = .seats
            seats = count
        } else {
            offer = .company
        }
        if let meetAt = editing.meetAt {
            let parts = meetAt.split(separator: ":").compactMap { Int($0) }
            if parts.count == 2,
               let at = Calendar.current.date(bySettingHour: parts[0], minute: parts[1], second: 0, of: day)
            {
                hasTime = true
                time = at
            }
        }
    }

    private func save() {
        guard !isSaving else { return }
        isSaving = true
        problem = nil
        // A place not marked is the core's to refuse, in its words: a
        // position off the globe.
        let draft = OpenPlanDraft(
            day: GroupsModel.day(day), place: place,
            position: position ?? Coordinate(lat: .nan, lon: .nan),
            meetAt: hasTime ? GroupsModel.timeOfDay(time) : nil,
            seats: offer == .seats ? seats : nil,
            note: note
        )
        Task {
            defer { isSaving = false }
            do {
                let saved: OpenPlanView
                if let editing {
                    saved = try await model.change(editing.id, to: draft)
                } else {
                    saved = try await model.post(draft)
                }
                done(saved)
                dismiss()
            } catch {
                problem = OpenPlansModel.words(for: error)
            }
        }
    }
}
