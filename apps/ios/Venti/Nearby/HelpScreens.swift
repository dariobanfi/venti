import SwiftUI
import VentiCore

/// The first words of every screen of help after landing, before anything
/// else: the core's sentence that an injury is the emergency number's,
/// in the caution's style, with the button that dials it.
struct HelpEmergencySection: View {
    @Environment(\.openURL) private var openURL

    var body: some View {
        Section {
            CardCaution(words: helpEmergencyWords())
                .foregroundStyle(.venti(.text))
                .accessibilityIdentifier("help-emergency")
            Button {
                if let call = URL(string: "tel:112") {
                    openURL(call)
                }
            } label: {
                Text(HelpWords.call)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(.danger))
            .accessibilityIdentifier("help-call")
        }
        .listRowBackground(Color.venti(.cautionSoft))
    }
}

/// "Ask pilots nearby for help": the core's emergency words first, a note,
/// and the button that sends it with where the pilot is: the landing's
/// place from the cockpit, else the iPhone's position. Once asked, or
/// where the pilot's own call already stands, the call itself.
struct HelpAskSheet: View {
    /// Where the glider landed, from the cockpit; `nil` for the iPhone's
    /// position.
    let landedAt: Coordinate?

    @Environment(NearbyModel.self) private var nearby
    @Environment(AccountModel.self) private var account
    @Environment(\.dismiss) private var dismiss

    @State private var note = ""
    @State private var position = PilotPosition(accuracy: .best)
    @State private var isSending = false
    @State private var problem: String?
    /// The call once it is sent: the sheet shows it in place of the form.
    @State private var asked: HelpView?

    private var place: Coordinate? {
        landedAt ?? position.place
    }

    var body: some View {
        NavigationStack {
            Group {
                if let standing = asked ?? nearby.mine {
                    HelpScreen(id: standing.id, first: standing)
                } else {
                    form
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                    .accessibilityIdentifier("help-done")
                }
            }
        }
        .task {
            if landedAt == nil {
                position.readIfAllowed()
            }
            await nearby.loadHelp()
        }
    }

    private var form: some View {
        VentiList {
            HelpEmergencySection()
            if !account.isSignedIn {
                Section {
                    Text(HelpWords.signedOut)
                    NavigationLink(HelpWords.signIn) {
                        AccountScreen()
                    }
                    .accessibilityIdentifier("help-sign-in")
                }
            } else {
                Section {
                    TextField(HelpWords.noteExample, text: $note, axis: .vertical)
                        .lineLimit(2...5)
                        .accessibilityIdentifier("help-note")
                        .onChange(of: note) { _, typed in
                            if typed.count > HelpWords.noteRoom {
                                note = String(typed.prefix(HelpWords.noteRoom))
                            }
                        }
                } header: {
                    Text(HelpWords.note)
                } footer: {
                    Text(verbatim: "\(Format.count(note.count)) / \(Format.count(HelpWords.noteRoom))")
                        .monospacedDigit()
                }
                Section {
                    whereFrom
                    Button {
                        send()
                    } label: {
                        Group {
                            if isSending {
                                ProgressView()
                            } else {
                                Text(HelpWords.ask)
                            }
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.primary))
                    .disabled(place == nil || isSending)
                    .accessibilityIdentifier("help-ask")
                    if let problem {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                            .accessibilityIdentifier("help-problem")
                    }
                }
            }
        }
        .navigationTitle(HelpWords.title)
        .navigationBarTitleDisplayMode(.inline)
    }

    /// Where the call goes from, or what it waits for.
    @ViewBuilder private var whereFrom: some View {
        if let place {
            LabeledContent(HelpWords.whereYouAre) {
                Text(Format.place(lat: place.lat, lon: place.lon))
                    .monospacedDigit()
            }
            .accessibilityIdentifier("help-place")
        } else if position.isAllowed {
            HStack(spacing: VentiSpace.s2) {
                ProgressView()
                Text(HelpWords.waitingForPosition)
                    .foregroundStyle(.venti(.textSecondary))
            }
            .accessibilityIdentifier("help-waiting")
        } else {
            Text(HelpWords.noPosition)
                .foregroundStyle(.venti(.textSecondary))
            Button(HelpWords.usePosition) {
                position.ask()
            }
            .accessibilityIdentifier("help-use-position")
        }
    }

    private func send() {
        guard let place, !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                asked = try await nearby.ask(at: place, note: note)
            } catch {
                problem = Social.words(for: error)
            }
        }
    }
}

/// One call for help, as the pilot may see it. Their own: its line ("Asked
/// 12 pilots nearby · 2 can help"), who can help by name with their pages,
/// and "I'm fine, close". One they were asked: who asks, the note, its line
/// ("About 8 km from you"), "I can help" and "I can't", and once they can,
/// where the caller is with the way there in the iPhone's maps.
struct HelpScreen: View {
    let id: String
    /// The call as a list or a sending had it, until Venti answers.
    var first: HelpView?

    @Environment(NearbyModel.self) private var nearby
    @Environment(\.openURL) private var openURL

    @State private var read: HelpView?
    @State private var missing: String?
    @State private var isWorking = false
    @State private var problem: String?
    @State private var reporting: Reported?

    private var shown: HelpView? {
        read ?? first
    }

    var body: some View {
        VentiList {
            HelpEmergencySection()
            if let call = shown {
                Section {
                    head(call)
                    Text(call.line)
                        .foregroundStyle(.venti(.textSecondary))
                        .accessibilityIdentifier("help-line")
                    if let note = call.note, !note.isEmpty {
                        Text(note)
                            .foregroundStyle(.venti(.text))
                            .accessibilityIdentifier("help-note-text")
                    }
                    if !call.stands {
                        Text(HelpWords.ended)
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("help-ended")
                    }
                }
                if call.isMine {
                    mine(call)
                } else {
                    asked(call)
                }
                if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                            .accessibilityIdentifier("help-problem")
                    }
                }
            } else if let missing {
                Section {
                    Text(missing)
                        .foregroundStyle(.venti(.textSecondary))
                        .accessibilityIdentifier("help-missing")
                }
            } else {
                Section {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
        }
        .navigationTitle(HelpWords.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let call = shown, !call.isMine {
                ToolbarItem(placement: .primaryAction) {
                    VentiBarMenu(title: commonWords().more) {
                        Button(HelpWords.report) {
                            reporting = Reported(subject: .helpRequest(id: call.id), title: HelpWords.report)
                        }
                        .accessibilityIdentifier("help-report")
                    }
                    .accessibilityIdentifier("help-more")
                }
            }
        }
        .refreshable {
            await reread()
        }
        .task(id: id) {
            await reread()
            // The pilot's own call while it stands: who can help is read
            // again now and then, for a phone that is told nothing.
            while !Task.isCancelled, let call = shown, call.isMine, call.stands {
                try? await Task.sleep(for: .seconds(15))
                if Task.isCancelled { break }
                await reread()
            }
        }
        // A call or an answer pushed while the screen is open.
        .onChange(of: AlertNotices.taps.helpArrived) {
            Task { await reread() }
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
    }

    /// Who asks, with their face; another pilot's leads to their page.
    @ViewBuilder private func head(_ call: HelpView) -> some View {
        let face = HStack(spacing: VentiSpace.s3) {
            PilotFace(name: call.pilot.name, size: VentiSpace.s6, isMe: call.isMine)
            Text(call.pilot.name)
                .font(.ventiType(.heading))
                .foregroundStyle(.venti(.text))
        }
        .accessibilityElement(children: .combine)
        if call.isMine {
            face
                .accessibilityIdentifier("help-pilot")
        } else {
            NavigationLink {
                PilotFlightsScreen(pilot: PilotRoute(handle: call.pilot.handle, name: call.pilot.name))
            } label: {
                face
            }
            .accessibilityIdentifier("help-pilot")
        }
    }

    /// The pilot's own call: who can help, and closing it.
    @ViewBuilder private func mine(_ call: HelpView) -> some View {
        if !call.helpers.isEmpty {
            Section(HelpWords.canHelp) {
                ForEach(call.helpers, id: \.handle) { helper in
                    NavigationLink {
                        PilotFlightsScreen(pilot: PilotRoute(handle: helper.handle, name: helper.name))
                    } label: {
                        HStack(spacing: VentiSpace.s3) {
                            PilotFace(name: helper.name, size: VentiSpace.s6)
                            Text(helper.name)
                        }
                        .accessibilityElement(children: .combine)
                    }
                    .accessibilityIdentifier("help-helper")
                }
            }
        }
        if call.stands {
            Section {
                Button {
                    act { try await nearby.close(call.id) }
                } label: {
                    Text(HelpWords.close)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.quiet))
                .disabled(isWorking)
                .accessibilityIdentifier("help-close")
            }
        }
    }

    /// A call the pilot was asked: the answer, and where the caller is
    /// once they said they can help.
    @ViewBuilder private func asked(_ call: HelpView) -> some View {
        if let at = call.position, call.stands {
            Section(HelpWords.whereTheyAre) {
                LabeledContent(HelpWords.whereTheyAre) {
                    Text(Format.place(lat: at.lat, lon: at.lon))
                        .monospacedDigit()
                }
                .accessibilityIdentifier("help-where")
                Button(HelpWords.directions) {
                    if let maps = Self.directions(to: at) {
                        openURL(maps)
                    }
                }
                .accessibilityIdentifier("help-directions")
            }
        }
        if call.stands {
            Section {
                answerButton(HelpWords.can, canHelp: true, chosen: call.canHelp == true, call: call)
                    .accessibilityIdentifier("help-can")
                // Venti says `false` both for "I can't" and for no answer
                // yet: only "I can help" is ever shown as chosen.
                answerButton(HelpWords.cannot, canHelp: false, chosen: false, call: call)
                    .accessibilityIdentifier("help-cannot")
            }
        }
    }

    private func answerButton(_ words: String, canHelp: Bool, chosen: Bool, call: HelpView) -> some View {
        Button {
            act { read = try await nearby.answer(call.id, canHelp: canHelp) }
        } label: {
            Label {
                Text(words)
            } icon: {
                if chosen {
                    ChipTick()
                }
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.venti(chosen ? .primary : .quiet))
        .disabled(isWorking)
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }

    /// The way to a place in Apple's maps, which opens in the Maps app.
    static func directions(to place: Coordinate) -> URL? {
        URL(string: "https://maps.apple.com/?daddr=\(place.lat),\(place.lon)&dirflg=d")
    }

    private func reread() async {
        do {
            read = try await nearby.one(id)
            missing = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            if shown == nil {
                missing = Social.words(for: error)
            } else {
                problem = Social.words(for: error)
            }
        }
    }

    private func act(_ work: @escaping () async throws -> Void) {
        guard !isWorking else { return }
        isWorking = true
        problem = nil
        Task {
            defer { isWorking = false }
            do {
                try await work()
                if read?.isMine == true || first?.isMine == true {
                    read = try? await nearby.one(id)
                }
            } catch {
                problem = Social.words(for: error)
            }
        }
    }
}

/// What a tapped push opens over the app: a call for help or an open plan,
/// in a sheet of its own.
struct PushedScreen: View {
    let target: PhoneTarget

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                switch target {
                case .help(let id):
                    HelpScreen(id: id)
                case .openPlan(let id):
                    OpenPlanScreen(id: id)
                case .alert:
                    EmptyView()
                case .groupChat(let id):
                    GroupChatScreen(id: id, name: "")
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                    .accessibilityIdentifier("pushed-done")
                }
            }
        }
    }
}

extension PhoneTarget: @retroactive Identifiable {
    public var id: String {
        switch self {
        case .alert(let id): "alert \(id)"
        case .openPlan(let id): "open plan \(id)"
        case .help(let id): "help \(id)"
        case .groupChat(let id): "group chat \(id)"
        }
    }
}
