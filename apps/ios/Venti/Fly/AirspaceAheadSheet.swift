import Charts
import SwiftUI
import VentiCore

/// The line ahead, seen from the side: the airspace the glider would fly
/// under, into or over in the next ten kilometres and, where the iPhone
/// has it, the ground, with the glide drawn through both.
///
/// It is a picture to plan a glide with. The warnings are the banner's,
/// with their margins and their sound; this neither replaces them nor
/// knows more than they do. It reads the running session, so it follows
/// the glider while it is open.
struct AirspaceAheadSheet: View {
    @Environment(FlySession.self) private var fly
    @Environment(\.dismiss) private var dismiss

    static let caution =
        "From the airspace this app holds, on a straight line over the ground. The app does not have the height of the ground here: a limit given above ground is drawn as far as it may reach. Airspace a paraglider needs nothing for is left out. The warnings stay what they are."

    static let cautionWithGround =
        "From the airspace this app holds and the map's model of the ground, on a straight line. The model is of the bare ground, about 13 m to a point: it knows no trees, cables or buildings, and the glide is the one drawn, not the one the air will give. Airspace a paraglider needs nothing for is left out. The warnings stay what they are."

    var body: some View {
        NavigationStack {
            List {
                // Reading the cockpit's view makes this follow each of its
                // refreshes.
                if fly.view != nil, let ahead = fly.airspaceAhead() {
                    Section {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(ahead.words)
                                .font(.headline)
                                .accessibilityIdentifier("ahead-words")
                            if let ground = ahead.groundWords {
                                Text(ground)
                                    .font(.subheadline.weight(.medium))
                                    .accessibilityIdentifier("ahead-ground")
                            }
                            Text(verbatim: "\(ahead.toward) · \(ahead.lineWords)")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                        AheadChart(ahead: ahead)
                            .frame(height: 240)
                            .padding(.vertical, 4)
                    } footer: {
                        Text(ahead.ground.isEmpty ? Self.caution : Self.cautionWithGround)
                    }
                    if !ahead.spans.isEmpty {
                        Section("On this line") {
                            ForEach(Array(ahead.spans.enumerated()), id: \.offset) { _, span in
                                AheadRow(span: span)
                            }
                        }
                    }
                } else {
                    Section {
                        Text("Nothing to show yet: the side view needs a position, a height and a way to look.")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("Ahead, from the side")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }
}

/// Height against distance: each airspace a block, the glider a dot at
/// the left, its line running from it.
private struct AheadChart: View {
    let ahead: AirspaceAhead

    /// Where the glider's line ends in the picture: where it meets the
    /// ground, at the far end, or where it leaves the picture's foot.
    /// `nil` when there is no line.
    private var lineEnd: (km: Double, heightM: Double)? {
        guard ahead.lineEndM < ahead.altitudeM else { return nil }
        let reach = ahead.distanceM / 1000
        let fall = ahead.altitudeM - ahead.lineEndM
        if let meets = ahead.lineMeetsGroundM {
            return (meets / 1000, ahead.altitudeM - fall * meets / ahead.distanceM)
        }
        guard ahead.lineEndM < ahead.bottomM else {
            return (reach, ahead.lineEndM)
        }
        return (reach * (ahead.altitudeM - ahead.bottomM) / fall, ahead.bottomM)
    }

    var body: some View {
        Chart {
            // The ground, where the iPhone has it: a filled shape along
            // the foot of the picture.
            ForEach(ahead.ground, id: \.distanceM) { place in
                AreaMark(
                    x: .value("Distance", place.distanceM / 1000),
                    yStart: .value("Height", ahead.bottomM),
                    yEnd: .value("Height", min(max(place.elevationM, ahead.bottomM), ahead.topM))
                )
                .foregroundStyle(Self.ground)
            }
            ForEach(Array(ahead.spans.enumerated()), id: \.offset) { _, span in
                RectangleMark(
                    xStart: .value("Distance", span.fromM / 1000),
                    xEnd: .value("Distance", span.toM / 1000),
                    yStart: .value("Height", min(max(span.floorM, ahead.bottomM), ahead.topM)),
                    yEnd: .value("Height", max(min(span.ceilingM ?? ahead.topM, ahead.topM), ahead.bottomM))
                )
                // What could not be placed exactly is drawn fainter.
                .foregroundStyle(Self.color(span.policy).opacity(span.uncertain ? 0.2 : 0.36))
                .annotation(position: .overlay, alignment: .top) {
                    Text(span.name)
                        .font(.caption2.weight(.semibold))
                        .lineLimit(1)
                        .padding(.top, 3)
                }
            }
            if let lineEnd {
                LineMark(
                    x: .value("Distance", 0.0),
                    y: .value("Height", ahead.altitudeM)
                )
                .foregroundStyle(Color.primary)
                .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, dash: [6, 4]))
                LineMark(
                    x: .value("Distance", lineEnd.km),
                    y: .value("Height", lineEnd.heightM)
                )
                .foregroundStyle(Color.primary)
                .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, dash: [6, 4]))
            }
            PointMark(
                x: .value("Distance", 0.0),
                y: .value("Height", ahead.altitudeM)
            )
            .symbolSize(110)
            .foregroundStyle(Palette.accent)
        }
        .chartXScale(domain: 0...(ahead.distanceM / 1000))
        .chartYScale(domain: ahead.bottomM...ahead.topM)
        .chartXAxis {
            AxisMarks(values: .automatic(desiredCount: 5)) { value in
                AxisGridLine()
                AxisValueLabel {
                    if let kilometres = value.as(Double.self) {
                        Text(verbatim: "\(kilometres.formatted(.number.precision(.fractionLength(0)))) km")
                    }
                }
            }
        }
        .chartYAxis {
            AxisMarks(position: .trailing, values: .automatic(desiredCount: 4)) { value in
                AxisGridLine()
                AxisValueLabel {
                    if let metres = value.as(Double.self) {
                        Text(verbatim: "\(Format.metres(metres)) m")
                    }
                }
            }
        }
        .accessibilityIdentifier("ahead-chart")
        .accessibilityLabel("The line ahead, from the side")
        .accessibilityValue([ahead.words, ahead.groundWords].compactMap { $0 }.joined(separator: ". "))
    }

    static func color(_ policy: AirspacePolicy) -> Color {
        Color(uiColor: AirspaceLayers.color(for: policy))
    }

    /// The ground: the colour of earth, quiet under the airspace's.
    static let ground = Color(red: 0.52, green: 0.44, blue: 0.34).opacity(0.55)
}

/// One airspace on the line: what it is, how far, and whether the line
/// runs through it.
private struct AheadRow: View {
    let span: AirspaceAheadSpan

    private var place: String {
        if span.fromM <= 0 {
            return "Here"
        }
        return span.fromM < 995
            ? "In \(Format.metres((span.fromM / 10).rounded() * 10)) m"
            : "In \(Format.kilometres(metres: span.fromM)) km"
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(span.name)
                    .font(.body.weight(.semibold))
                Text(verbatim: "\(span.kind) · \(span.lower) to \(span.upper)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                if span.uncertain {
                    Text("Its height could not be placed exactly.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text(place)
                    .font(.system(.body, design: .rounded, weight: .semibold))
                    .monospacedDigit()
                if span.inTheWay {
                    Text(span.uncertain ? "Possibly on your line" : "On your line")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(AheadChart.color(.avoid))
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("ahead-row")
    }
}
