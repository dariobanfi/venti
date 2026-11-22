import Charts
import SwiftUI
import VentiCore

/// How a level of foehn is drawn: the pack's status of the same weight.
/// "Possible" is a caution and "likely" a danger; no sign and not known
/// have no status, and are never the good one's green.
extension FoehnLevel {
    var status: VentiStatus.Kind? {
        switch self {
        case .possible: .caution
        case .likely: .danger
        case .none, .unknown: nil
        }
    }
}

/// Today's one line about foehn, in the core's words and the colour of
/// its level, with the pack's sign for it. A tap opens the view.
struct FoehnTodayLine: View {
    let line: FoehnLine
    let open: () -> Void

    private var isLikely: Bool { line.level == .likely }

    var body: some View {
        Button(action: open) {
            HStack(alignment: .center, spacing: VentiSpace.s2) {
                Image(isLikely ? "icon-danger" : "icon-caution")
                    .resizable()
                    .frame(width: VentiSize.icon, height: VentiSize.icon)
                    .accessibilityHidden(true)
                Text(line.text)
                    .font(.ventiType(.label))
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.ventiType(.caption))
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, VentiSpace.s3)
            .padding(.vertical, VentiSpace.s2)
            .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
            .foregroundStyle(.venti(isLikely ? .danger : .caution))
            .background(.venti(isLikely ? .dangerSoft : .cautionSoft), in: .rect(cornerRadius: VentiRadius.s))
            .contentShape(.rect(cornerRadius: VentiRadius.s))
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("today-foehn")
    }
}

/// The way to the foehn view at the foot of Today, in small print: there
/// whether Today has a line or not.
struct FoehnFootRow: View {
    let title: String
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: VentiSpace.s2) {
                Text(title)
                    .font(.ventiType(.label))
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.ventiType(.caption))
                    .accessibilityHidden(true)
            }
            .foregroundStyle(.venti(.textSecondary))
            .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("today-foehn-row")
    }
}

/// The foehn view: each pair of stations with what its pressures say, its
/// level, its last 24 hours, and under them what the number means, what
/// it does not say, and whose data it is. Every word is the core's.
struct FoehnSheet: View {
    let model: FoehnModel
    /// Asks Venti again.
    let tryAgain: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                if let view = model.view {
                    read(view)
                } else if case .failed(let problem) = model.state {
                    Section {
                        CardProblem(words: problem, identifier: "foehn-problem", tryAgain: tryAgain)
                            .padding(.vertical, VentiSpace.s2)
                    }
                } else {
                    Section {
                        ProgressView()
                            .frame(maxWidth: .infinity)
                            .accessibilityIdentifier("foehn-loading")
                    }
                    .listRowBackground(Color.clear)
                }
            }
            .navigationTitle(model.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                    .accessibilityIdentifier("foehn-done")
                }
            }
        }
    }

    @ViewBuilder private func read(_ view: FoehnView) -> some View {
        if view.rows.isEmpty {
            Section {
                Text(view.empty)
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("foehn-empty")
            }
        }
        ForEach(view.rows, id: \.id) { row in
            Section {
                FoehnRowView(row: row, view: view)
            }
        }
        Section {
            Text(view.explanation)
                .font(.ventiType(.body))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("foehn-explanation")
            if !view.caution.isEmpty {
                CardCaution(words: view.caution)
                    .accessibilityIdentifier("foehn-caution")
            }
        }
        if !view.credits.isEmpty {
            Section {
                VStack(alignment: .leading, spacing: VentiSpace.s2) {
                    ForEach(view.credits, id: \.source) { credit in
                        Text(credit.text)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .font(.ventiType(.caption))
                .foregroundStyle(.venti(.textSecondary))
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("foehn-credits")
            }
        }
    }
}

/// One pair of stations: its name and level, what it says now and how old
/// that is, its last 24 hours, and where the pair and its limits are from.
private struct FoehnRowView: View {
    let row: FoehnRow
    let view: FoehnView

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            // Side by side where both fit, as at the usual text sizes.
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .center, spacing: VentiSpace.s2) {
                    title
                    Spacer(minLength: VentiSpace.s2)
                    FoehnBadge(level: row.level, word: row.levelLabel)
                }
                VStack(alignment: .leading, spacing: VentiSpace.s2) {
                    title
                    FoehnBadge(level: row.level, word: row.levelLabel)
                }
            }
            Text(row.words)
                .font(.ventiType(.body))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("foehn-words")
            if !row.when.isEmpty {
                Text(row.when)
                    .font(.ventiType(.caption))
                    .monospacedDigit()
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("foehn-when")
            }
            if !row.points.isEmpty {
                FoehnChart(row: row, title: view.chartTitle, unit: view.unit, limits: view.limits)
                    .padding(.top, VentiSpace.s2)
            }
            if !row.note.isEmpty {
                Text(row.note)
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, VentiSpace.s2)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("foehn-row")
    }

    private var title: some View {
        Text(row.title)
            .font(.ventiType(.heading))
            .lineLimit(1)
            .fixedSize()
            .accessibilityAddTraits(.isHeader)
    }
}

/// A pair's level in the core's word: the pack's status for "possible" and
/// "likely"; the word alone, plain, for no sign; the word in the muted ink
/// on the border's grey where nothing is known.
struct FoehnBadge: View {
    let level: FoehnLevel
    let word: String

    var body: some View {
        Group {
            if let kind = level.status {
                VentiStatus(kind: kind, word: word)
            } else if level == .unknown {
                Text(word)
                    .font(.venti(.subheadline, .semibold))
                    .lineLimit(1)
                    .padding(.horizontal, VentiSpace.s2)
                    .padding(.vertical, VentiSpace.s1)
                    .foregroundStyle(.venti(.textSecondary))
                    .background(.venti(.border), in: .rect(cornerRadius: VentiRadius.s))
            } else {
                Text(word)
                    .font(.venti(.subheadline, .semibold))
                    .lineLimit(1)
                    .foregroundStyle(.venti(.text))
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(word)
        .accessibilityIdentifier("foehn-level")
    }
}

/// A pair's last 24 hours: the difference in pressure as a line, zero in
/// the middle, the core's limits as thin lines in their level's colour,
/// and what each side of zero means at its end.
struct FoehnChart: View {
    let row: FoehnRow
    let title: String
    let unit: String
    let limits: [FoehnLimit]

    /// How far the chart reaches on either side of zero: past the widest
    /// limit and the largest difference, the same up and down, so that
    /// zero is the middle.
    nonisolated static func reach(of points: [FoehnPoint], limits: [FoehnLimit]) -> Double {
        let widest = (points.map(\.differenceHpa) + limits.map(\.differenceHpa)).map(abs).max() ?? 0
        return (widest + 1).rounded(.up)
    }

    var body: some View {
        let reach = Self.reach(of: row.points, limits: limits)
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                    .font(.ventiType(.label))
                Spacer(minLength: VentiSpace.s2)
                Text(unit)
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
            }
            Text(row.aboveLabel)
                .font(.ventiType(.caption))
                .foregroundStyle(.venti(.textSecondary))
                .accessibilityIdentifier("foehn-above")
            Chart {
                RuleMark(y: .value("Difference", 0))
                    .foregroundStyle(Color.venti(.borderStrong))
                    .lineStyle(StrokeStyle(lineWidth: 1))
                ForEach(limits, id: \.differenceHpa) { limit in
                    RuleMark(y: .value("Difference", limit.differenceHpa))
                        .foregroundStyle(Color.venti(limit.level == .likely ? .danger : .caution))
                        .lineStyle(StrokeStyle(lineWidth: 1))
                        .annotation(
                            position: limit.differenceHpa > 0 ? .top : .bottom, alignment: .leading, spacing: 1
                        ) {
                            Text(limit.label)
                                .font(.ventiType(.caption))
                                .foregroundStyle(.venti(limit.level == .likely ? .danger : .caution))
                        }
                }
                ForEach(row.points, id: \.timeUnixS) { point in
                    LineMark(
                        x: .value("Time", Date(timeIntervalSince1970: TimeInterval(point.timeUnixS))),
                        y: .value("Difference", point.differenceHpa)
                    )
                    .foregroundStyle(Color.venti(.text))
                    .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                }
            }
            .chartYScale(domain: -reach...reach)
            .chartYAxis {
                AxisMarks(position: .trailing, values: [0] + limits.map(\.differenceHpa)) { value in
                    AxisValueLabel {
                        if let difference = value.as(Double.self) {
                            Text(Format.axis(difference))
                                .monospacedDigit()
                        }
                    }
                }
            }
            .chartXAxis {
                AxisMarks(preset: .aligned, values: .stride(by: .hour, count: 6)) { _ in
                    AxisGridLine()
                    AxisValueLabel(format: .dateTime.hour())
                }
            }
            .frame(height: 170)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("\(title), \(unit)")
            .accessibilityValue(row.words)
            .accessibilityIdentifier("foehn-chart")
            Text(row.belowLabel)
                .font(.ventiType(.caption))
                .foregroundStyle(.venti(.textSecondary))
                .accessibilityIdentifier("foehn-below")
        }
    }
}
