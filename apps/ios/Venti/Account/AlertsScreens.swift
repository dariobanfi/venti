import SwiftUI
import VentiCore

/// "Alert me" on a wind station's page or a take-off's page: the form for
/// a new alarm, filled in with the station or the place. Signed out, the
/// way to signing in, with one line.
struct AlertMeSection: View {
    let draft: AlertDraft

    /// The core's words of the alarms' screens, as every word of them.
    private let words = alertFormWords()

    @Environment(AccountModel.self) private var account
    @State private var isOpen = false

    var body: some View {
        Section {
            if account.isSignedIn {
                Button(words.alertMe, systemImage: "bell") {
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
                    Label(words.alertMe, systemImage: "bell")
                }
                .accessibilityIdentifier("alert-me-sign-in")
            }
        } footer: {
            if !account.isSignedIn {
                Text(words.signedOut)
            }
        }
    }
}

/// "Alerts" under the account: the alarms, each with its line and a
/// switch, a tap to change one; under them what fired, the newest first.
struct AlertsScreen: View {
    /// The core's words of the alarms' screens.
    private let words = alertFormWords()
    @Environment(AlertsModel.self) private var alerts
    @Environment(AccountModel.self) private var account
    @Environment(UnreadModel.self) private var unread

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
                    Text(words.signedOut)
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
                    // The heading and a line, as on Android and the web.
                    Section {
                        Text(words.listEmpty)
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("alerts-none")
                    } header: {
                        Text(words.listTitle)
                    }
                } else {
                    Section {
                        ForEach(alerts.alerts, id: \.id) { alert in
                            AlertRow(alert: alert, edit: { editing = alert }, problem: $problem)
                        }
                    } header: {
                        Text(words.listTitle)
                    }
                }
                Section {
                    if alerts.fired.isEmpty {
                        Text(words.firedEmpty)
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("alerts-fired-none")
                    }
                    ForEach(Array(alerts.fired.enumerated()), id: \.offset) { _, line in
                        // The core's line, then the day and time it fired,
                        // as on Android and the web.
                        VStack(alignment: .leading, spacing: 2) {
                            Text(line.line)
                            Text(verbatim: "\(Format.dayInYear(unixSeconds: line.firedAtUnixS)), \(Format.time(unixSeconds: line.firedAtUnixS))")
                                .font(.venti(.subheadline))
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 2)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("alerts-fired-line")
                    }
                } header: {
                    Text(words.firedTitle)
                } footer: {
                    Text(words.firedNote)
                }
            }
        }
        .sheet(item: $editing) { alert in
            if let draft = alert.draft {
                AlertForm(draft: draft, id: alert.id)
            }
        }
        .navigationTitle(words.title)
        .navigationBarTitleDisplayMode(.inline)
        .refreshable {
            await alerts.sync()
            if alerts.problem == nil {
                await unread.sawAlarms()
            }
        }
        // What fired is shown once the sync has brought it: seen up to
        // there, which takes it off the Community tab's badge.
        .task {
            alerts.read()
            await alerts.sync()
            if alerts.problem == nil {
                await unread.sawAlarms()
            }
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
    /// The core's words of the alarms' screens.
    private let words = alertFormWords()
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
        "rule.days_ahead", "rule.min_pilots", "rule.rest_hours",
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
                    TextField(words.name, text: $draft.name)
                        .accessibilityIdentifier("alert-name")
                    faultLines("name")
                    switch draft.kind {
                    case .wind:
                        LabeledContent(words.station, value: draft.stationName)
                    case .day, .pilots:
                        LabeledContent(words.place, value: draft.placeName)
                    }
                    // After the name and the station or place, as on the
                    // web.
                    Toggle(words.on, isOn: $draft.enabled)
                        .accessibilityIdentifier("alert-enabled")
                } header: {
                    Text(words.name)
                }

                // A new alarm for a place is one of two kinds: of the
                // day there, or of pilots in the air there. An alarm that
                // stands keeps its kind.
                if id == nil, draft.kind != .wind {
                    Section {
                        VentiSegmented(
                            options: [(AlertKind.day, mapLayersWords().alarmDayKind), (AlertKind.pilots, words.pilotsKind)],
                            selection: kind
                        )
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("alert-kind")
                    }
                }

                switch draft.kind {
                case .wind:
                    windFields
                case .day:
                    dayFields
                case .pilots:
                    pilotsFields
                }

                if let id {
                    Section {
                        Button(words.delete, role: .destructive) {
                            isConfirmingDelete = true
                        }
                        .disabled(isWorking)
                        .accessibilityIdentifier("alert-delete")
                        .confirmationDialog(words.deleteQuestion, isPresented: $isConfirmingDelete, titleVisibility: .visible) {
                            Button(words.delete, role: .destructive) {
                                Task { await remove(id) }
                            }
                        }
                    }
                }
            }
            .navigationTitle(id == nil ? words.alertMe : words.changeTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(commonWords().cancel) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(words.save) {
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
        let any = words.any
        let named: [(value: Int, name: String)] = alerts.points.enumerated().map { ($0.offset, $0.element) }
        // "Any" is a choice of the first only, which clears both; each
        // side chosen alone takes the other along, as on Android and the
        // web.
        let points: [(value: Int, name: String)] = [(-1, any)] + named
        Section {
            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                Text(words.windFrom)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                VentiSegmented(options: points, selection: fromPoint)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("alert-from")
                Text(words.windTo)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                VentiSegmented(options: named, selection: toPoint)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("alert-to")
            }
            .padding(.vertical, VentiSpace.s1)
            faultLines("rule.directions")
        }

        Section {
            KmhStepper(title: words.least, value: $draft.minKmh)
                .accessibilityIdentifier("alert-min")
            faultLines("rule.min_kmh")
            KmhStepper(title: words.most, value: $draft.maxKmh)
                .accessibilityIdentifier("alert-max")
            faultLines("rule.max_kmh")
            KmhStepper(title: words.gustsAtMost, value: $draft.gustsMaxKmh)
                .accessibilityIdentifier("alert-gusts")
            faultLines("rule.gusts_max_kmh")
        }

        Section {
            // Below the first hour is "Any": no bound.
            Stepper(value: hour(\.fromHour, lowest: 0), in: -1...23) {
                LabeledContent(words.fromHour, value: hourWords(draft.fromHour))
            }
            .accessibilityIdentifier("alert-from-hour")
            Stepper(value: hour(\.toHour, lowest: 1), in: 0...24) {
                LabeledContent(words.toHour, value: hourWords(draft.toHour))
            }
            .accessibilityIdentifier("alert-to-hour")
            faultLines("rule.hours")
        } header: {
            Text(words.hoursTitle)
        }
    }

    /// Where the wind comes from at least: "Any" clears both sides, a
    /// point chosen where "To" has none sets it too.
    private var fromPoint: Binding<Int> {
        Binding(
            get: { draft.fromPoint.map(Int.init) ?? -1 },
            set: { chosen in
                if chosen < 0 {
                    draft.fromPoint = nil
                    draft.toPoint = nil
                } else {
                    draft.fromPoint = UInt8(chosen)
                    draft.toPoint = draft.toPoint ?? UInt8(chosen)
                }
            }
        )
    }

    /// Where the wind comes from at most: a point chosen where "From" has
    /// none sets it too.
    private var toPoint: Binding<Int> {
        Binding(
            get: { draft.toPoint.map(Int.init) ?? -1 },
            set: { chosen in
                guard chosen >= 0 else { return }
                draft.toPoint = UInt8(chosen)
                draft.fromPoint = draft.fromPoint ?? UInt8(chosen)
            }
        )
    }

    /// An hour that may be left out: one under the lowest is none.
    private func hour(_ path: WritableKeyPath<AlertDraft, UInt8?>, lowest: Int) -> Binding<Int> {
        Binding(
            get: { draft[keyPath: path].map(Int.init) ?? lowest - 1 },
            set: { draft[keyPath: path] = $0 < lowest ? nil : UInt8(clamping: $0) }
        )
    }

    private func hourWords(_ hour: UInt8?) -> String {
        hour.map { String(format: "%02d:00", Int($0)) } ?? words.any
    }

    // MARK: Day

    /// The choices of a day alarm: the days ahead as switches, and the
    /// least kind of day.
    @ViewBuilder private var dayFields: some View {
        Section {
            ForEach(Array(words.dayNames.enumerated()), id: \.offset) { day, name in
                Toggle(name, isOn: dayAhead(UInt8(day)))
                    .accessibilityIdentifier("alert-day-\(day)")
            }
            faultLines("rule.days_ahead")
        } header: {
            Text(words.days)
        }
        Section {
            VentiSegmented(
                options: words.dayKinds.map { ($0.kind, $0.name) },
                selection: $draft.atLeast
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("alert-at-least")
        } header: {
            Text(words.atLeast)
        }
    }

    /// The kind of a new alarm for a place, as the choice sets it.
    private var kind: Binding<AlertKind> {
        Binding(
            get: { draft.kind },
            set: { draft = AlertsModel.draft(draft, ofKind: $0) }
        )
    }

    /// An alarm about pilots in the air at a place: its two numbers under
    /// the core's words, within the core's bounds (`pilotsAlarmNumbers()`)
    /// and at its defaults where the pilot chose none.
    @ViewBuilder private var pilotsFields: some View {
        let numbers = pilotsAlarmNumbers()
        Section {
            Stepper(
                value: number(\.minPilots, else: numbers.minPilotsDefault),
                in: Int(numbers.minPilotsLeast)...Int(numbers.minPilotsMost)
            ) {
                LabeledContent(
                    words.pilotsAtLeast, value: Format.metres(Double(draft.minPilots ?? numbers.minPilotsDefault))
                )
            }
            .accessibilityIdentifier("alert-min-pilots")
            faultLines("rule.min_pilots")
            Stepper(
                value: number(\.restHours, else: numbers.restHoursDefault),
                in: Int(numbers.restHoursLeast)...Int(numbers.restHoursMost)
            ) {
                LabeledContent(
                    words.pilotsRest, value: Format.metres(Double(draft.restHours ?? numbers.restHoursDefault))
                )
            }
            .accessibilityIdentifier("alert-rest-hours")
            faultLines("rule.rest_hours")
        } footer: {
            Text(words.pilotsNote)
        }
    }

    private func number<N: FixedWidthInteger>(
        _ field: WritableKeyPath<AlertDraft, N?>, else unset: N
    ) -> Binding<Int> {
        Binding(
            get: { Int(draft[keyPath: field] ?? unset) },
            set: { draft[keyPath: field] = N(clamping: $0) }
        )
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
    /// The core's words of the alarms' screens.
    private let words = alertFormWords()
    let title: String
    @Binding var value: UInt16?

    var body: some View {
        Stepper(value: kmh, in: 0...150, step: 5) {
            LabeledContent(title, value: value.map { "\(Format.count(Int($0))) \(words.kmh)" } ?? words.any)
        }
    }

    private var kmh: Binding<Int> {
        Binding(
            get: { Int(value ?? 0) },
            set: { value = $0 <= 0 ? nil : UInt16(clamping: $0) }
        )
    }
}
