import SwiftUI
import VentiCore

/// The few words the screens of the alarms need that the core does not
/// give: the titles of the form's fields and of the list, and the line
/// for signing in. What an alarm waits for, what fired and what is wrong
/// with a draft are the core's.
enum AlertWords {
    static let entry = "Alert me"
    static let title = "Alerts"
    static let fired = "What fired"
    static let signIn = "Alarms live on your account. Sign in to set one."
    static let none = "No alarms yet"
    static let noneText = "Set one with \"Alert me\" on a wind station's page or a take-off's page. Venti checks it as the data arrives."
    static let nothingFired = "Nothing has fired in the last 30 days."
    static let footer = "Venti checks your alarms where the data arrives. The app fetches what fired when you open it, and from time to time in the background, and tells you with a notification."
    static let hoursNote = "Hours are Central European Time."
    /// The days a day alarm may name, 0 for today.
    static let days = ["Today", "Tomorrow", "In 2 days", "In 3 days"]

    static func atLeast(_ kind: AlertDayKind) -> String {
        switch kind {
        case .flyable: "Flyable"
        case .local: "Local"
        case .crossCountry: "Cross-country"
        case .big: "Big day"
        }
    }
}

/// "Alert me" on a wind station's page or a take-off's page: the form for
/// a new alarm, filled in with the station or the place. Signed out, the
/// way to signing in, with one line.
struct AlertMeSection: View {
    let draft: AlertDraft

    @Environment(AccountModel.self) private var account
    @State private var isOpen = false

    var body: some View {
        Section {
            if account.isSignedIn {
                Button(AlertWords.entry, systemImage: "bell") {
                    isOpen = true
                }
                .accessibilityIdentifier("alert-me")
                .sheet(isPresented: $isOpen) {
                    AlertForm(draft: draft, id: nil)
                }
            } else {
                NavigationLink {
                    AccountScreen()
                } label: {
                    Label(AlertWords.entry, systemImage: "bell")
                }
                .accessibilityIdentifier("alert-me-sign-in")
            }
        } footer: {
            if !account.isSignedIn {
                Text(AlertWords.signIn)
            }
        }
    }
}

/// "Alerts" under the account: the alarms, each with its line and a
/// switch, a tap to change one; under them what fired, the newest first.
struct AlertsScreen: View {
    @Environment(AlertsModel.self) private var alerts
    @Environment(AccountModel.self) private var account

    /// The alarm open in the form.
    @State private var editing: AlertView?
    /// Why a switch did not take, in the core's words.
    @State private var problem: String?

    var body: some View {
        VentiList {
            if !account.isSignedIn {
                Section {
                    NavigationLink("Sign in") {
                        AccountScreen()
                    }
                } footer: {
                    Text(AlertWords.signIn)
                }
            } else {
                if let words = problem ?? alerts.problem {
                    Section {
                        Label {
                            Text(words)
                        } icon: {
                            Image("icon-caution")
                        }
                        .accessibilityIdentifier("alerts-problem")
                    }
                }
                if alerts.alerts.isEmpty {
                    Section {
                        VentiEmpty(title: AlertWords.none, text: AlertWords.noneText) {}
                            .accessibilityIdentifier("alerts-none")
                    }
                } else {
                    Section {
                        ForEach(alerts.alerts, id: \.id) { alert in
                            AlertRow(alert: alert, edit: { editing = alert }, problem: $problem)
                        }
                    } footer: {
                        Text(AlertWords.footer)
                    }
                }
                Section {
                    if alerts.fired.isEmpty {
                        Text(AlertWords.nothingFired)
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("alerts-fired-none")
                    }
                    ForEach(Array(alerts.fired.enumerated()), id: \.offset) { _, line in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(line.title)
                                .font(.venti(.headline))
                            Text(line.line)
                                .font(.venti(.subheadline))
                                .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 2)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("alerts-fired-line")
                    }
                } header: {
                    Text(AlertWords.fired)
                }
            }
        }
        .sheet(item: $editing) { alert in
            if let draft = alert.draft {
                AlertForm(draft: draft, id: alert.id)
            }
        }
        .navigationTitle(AlertWords.title)
        .navigationBarTitleDisplayMode(.inline)
        .refreshable {
            await alerts.sync()
        }
        .task {
            alerts.read()
            await alerts.sync()
        }
    }
}

extension AlertView: @retroactive Identifiable {}

/// One alarm in the list: its name and line, which open the form, and its
/// switch.
private struct AlertRow: View {
    let alert: AlertView
    let edit: () -> Void
    @Binding var problem: String?

    @Environment(AlertsModel.self) private var alerts
    @State private var isSwitching = false

    var body: some View {
        HStack(spacing: VentiSpace.s3) {
            Button(action: edit) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(alert.name)
                        .font(.venti(.headline))
                        .foregroundStyle(.primary)
                    Text(alert.line)
                        .font(.venti(.subheadline))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .disabled(alert.draft == nil)
            .accessibilityIdentifier("alert-row")
            Toggle(alert.name, isOn: enabled)
                .labelsHidden()
                .disabled(alert.draft == nil || isSwitching)
                .accessibilityIdentifier("alert-switch")
        }
    }

    private var enabled: Binding<Bool> {
        Binding(
            get: { alert.enabled },
            set: { isOn in
                isSwitching = true
                Task {
                    problem = await alerts.set(alert, enabled: isOn)
                    isSwitching = false
                }
            }
        )
    }
}

/// The form of an alarm: the core's line as a live summary over the
/// choices, and the faults the core finds under their fields from the
/// first try to save. A new alarm, or the one with `id`, which can also be
/// deleted.
struct AlertForm: View {
    let id: String?

    @Environment(AlertsModel.self) private var alerts
    @Environment(\.dismiss) private var dismiss

    @State private var draft: AlertDraft
    @State private var hasTried = false
    @State private var isWorking = false
    @State private var refusal: String?
    @State private var isConfirmingDelete = false

    init(draft: AlertDraft, id: String?) {
        self.id = id
        _draft = State(initialValue: draft)
    }

    private var faults: [AlertFaultView] {
        hasTried ? alerts.faults(of: draft) : []
    }

    /// The fields the form has a place for; a fault of another stands at
    /// the head.
    private static let fieldsShown: Set<String> = [
        "name", "rule.directions", "rule.min_kmh", "rule.max_kmh", "rule.gusts_max_kmh", "rule.hours",
        "rule.days_ahead",
    ]

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Text(alertLine(draft: draft))
                        .font(.venti(.body, .semibold))
                        .accessibilityIdentifier("alert-line")
                    if let refusal {
                        FaultLine(words: refusal)
                            .accessibilityIdentifier("alert-refusal")
                    }
                    ForEach(faults.filter { !Self.fieldsShown.contains($0.field) }, id: \.field) { fault in
                        FaultLine(words: fault.message)
                            .accessibilityIdentifier("alert-fault-\(fault.field)")
                    }
                }

                Section {
                    TextField("Name", text: $draft.name)
                        .accessibilityIdentifier("alert-name")
                    faultLines("name")
                } header: {
                    Text("Name")
                }

                switch draft.kind {
                case .wind:
                    windFields
                case .day:
                    dayFields
                }

                Section {
                    Toggle("On", isOn: $draft.enabled)
                        .accessibilityIdentifier("alert-enabled")
                }

                if let id {
                    Section {
                        Button("Delete alarm", role: .destructive) {
                            isConfirmingDelete = true
                        }
                        .disabled(isWorking)
                        .accessibilityIdentifier("alert-delete")
                        .confirmationDialog("Delete this alarm?", isPresented: $isConfirmingDelete, titleVisibility: .visible) {
                            Button("Delete alarm", role: .destructive) {
                                Task { await remove(id) }
                            }
                        }
                    }
                }
            }
            .navigationTitle(id == nil ? AlertWords.entry : draft.name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await save() }
                    }
                    .disabled(isWorking)
                    .accessibilityIdentifier("alert-save")
                }
            }
        }
    }

    // MARK: Wind

    /// The choices of a wind alarm: from and to among the sixteen points,
    /// as the sites' filter shows directions; the least and most wind,
    /// the gusts and the hours as steppers.
    @ViewBuilder private var windFields: some View {
        let any = sitesFilterChoices().anyWind
        let points: [(value: Int, name: String)] =
            [(-1, any)] + alerts.points.enumerated().map { ($0.offset, $0.element) }
        Section {
            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                Text("From")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                VentiSegmented(options: points, selection: point(\.fromPoint))
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("alert-from")
                Text("To")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                VentiSegmented(options: points, selection: point(\.toPoint))
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("alert-to")
            }
            .padding(.vertical, VentiSpace.s1)
            faultLines("rule.directions")
        } header: {
            Text("Wind direction at \(draft.stationName)")
                .textCase(nil)
        }

        Section {
            KmhStepper(title: "Least wind", value: $draft.minKmh)
                .accessibilityIdentifier("alert-min")
            faultLines("rule.min_kmh")
            KmhStepper(title: "Most wind", value: $draft.maxKmh)
                .accessibilityIdentifier("alert-max")
            faultLines("rule.max_kmh")
            KmhStepper(title: "Gusts at most", value: $draft.gustsMaxKmh)
                .accessibilityIdentifier("alert-gusts")
            faultLines("rule.gusts_max_kmh")
        } header: {
            Text("Wind speed")
        }

        Section {
            Toggle("Only between hours", isOn: hasHours)
                .accessibilityIdentifier("alert-hours")
            if hasHours.wrappedValue {
                Stepper(value: hour(\.fromHour, otherwise: 10), in: 0...23) {
                    LabeledContent("From", value: Self.hourWords(draft.fromHour.map(Int.init) ?? 10))
                }
                Stepper(value: hour(\.toHour, otherwise: 18), in: 1...24) {
                    LabeledContent("To", value: Self.hourWords(draft.toHour.map(Int.init) ?? 18))
                }
            }
            faultLines("rule.hours")
        } footer: {
            Text(AlertWords.hoursNote)
        }
    }

    private func point(_ path: WritableKeyPath<AlertDraft, UInt8?>) -> Binding<Int> {
        Binding(
            get: { draft[keyPath: path].map(Int.init) ?? -1 },
            set: { draft[keyPath: path] = $0 < 0 ? nil : UInt8($0) }
        )
    }

    private var hasHours: Binding<Bool> {
        Binding(
            get: { draft.fromHour != nil || draft.toHour != nil },
            set: { isOn in
                draft.fromHour = isOn ? 10 : nil
                draft.toHour = isOn ? 18 : nil
            }
        )
    }

    private func hour(_ path: WritableKeyPath<AlertDraft, UInt8?>, otherwise: Int) -> Binding<Int> {
        Binding(
            get: { draft[keyPath: path].map(Int.init) ?? otherwise },
            set: { draft[keyPath: path] = UInt8(clamping: $0) }
        )
    }

    static func hourWords(_ hour: Int) -> String {
        String(format: "%02d:00", hour)
    }

    // MARK: Day

    /// The choices of a day alarm: the days ahead as switches, and the
    /// least kind of day.
    @ViewBuilder private var dayFields: some View {
        Section {
            ForEach(Array(AlertWords.days.enumerated()), id: \.offset) { day, words in
                Toggle(words, isOn: dayAhead(UInt8(day)))
                    .accessibilityIdentifier("alert-day-\(day)")
            }
            faultLines("rule.days_ahead")
        } header: {
            Text("Days at \(draft.placeName)")
                .textCase(nil)
        } footer: {
            Text(AlertWords.hoursNote)
        }
        Section {
            VentiSegmented(
                options: [AlertDayKind.flyable, .local, .crossCountry, .big].map { ($0, AlertWords.atLeast($0)) },
                selection: $draft.atLeast
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("alert-at-least")
        } header: {
            Text("At least")
        }
    }

    private func dayAhead(_ day: UInt8) -> Binding<Bool> {
        Binding(
            get: { draft.daysAhead.contains(day) },
            set: { isOn in
                var days = Set(draft.daysAhead)
                if isOn {
                    days.insert(day)
                } else {
                    days.remove(day)
                }
                draft.daysAhead = Data(days.sorted())
            }
        )
    }

    // MARK: Faults and saving

    @ViewBuilder private func faultLines(_ field: String) -> some View {
        ForEach(Array(faults.filter { $0.field == field }.enumerated()), id: \.offset) { _, fault in
            FaultLine(words: fault.message)
                .accessibilityIdentifier("alert-fault-\(field)")
        }
    }

    private func save() async {
        hasTried = true
        refusal = nil
        guard alerts.faults(of: draft).isEmpty, !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        if let words = await alerts.save(draft, id: id) {
            refusal = words
        } else {
            dismiss()
        }
    }

    private func remove(_ id: String) async {
        isWorking = true
        defer { isWorking = false }
        if let words = await alerts.remove(id) {
            refusal = words
        } else {
            dismiss()
        }
    }
}

/// A fault under its field, in the danger ink.
private struct FaultLine: View {
    let words: String

    var body: some View {
        Text(words)
            .font(.venti(.footnote))
            .foregroundStyle(.venti(.danger))
    }
}

/// A speed in km/h that may be left out: a stepper in fives, down to "Any".
private struct KmhStepper: View {
    let title: String
    @Binding var value: UInt16?

    var body: some View {
        Stepper(value: kmh, in: 0...150, step: 5) {
            LabeledContent(title, value: value.map { "\(Format.count(Int($0))) km/h" } ?? sitesFilterChoices().anyWind)
        }
    }

    private var kmh: Binding<Int> {
        Binding(
            get: { Int(value ?? 0) },
            set: { value = $0 <= 0 ? nil : UInt16(clamping: $0) }
        )
    }
}
