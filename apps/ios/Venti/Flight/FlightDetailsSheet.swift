import SwiftUI
import VentiCore

/// Everything known about a flight, as a list. Opened from the flight panel.
///
/// Choosing a thermal or a glide closes the sheet and marks that moment on
/// the map and the profile.
struct FlightDetailsSheet: View {
    let flight: FlightView
    /// What happened in the flight, once it has been worked out.
    let analysis: FlightAnalysis?
    /// Called with a position in the flight's track to mark.
    let onSelect: (Int) -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section("Flight") {
                    row("Date", Format.day(unixSeconds: flight.startUnixS))
                    row("Take-off", Format.time(unixSeconds: flight.startUnixS))
                    row("Landing", Format.time(unixSeconds: flight.startUnixS + flight.durationS))
                    row("Airtime", "\(Format.hoursMinutes(seconds: flight.durationS)) h")
                    row("Track length", "\(Format.kilometres(metres: flight.trackLengthM)) km")
                }
                if let score = analysis?.score {
                    Section {
                        row("Scored as", Self.name(of: score))
                        row("Distance", "\(Format.kilometres(metres: score.distanceM)) km")
                        row("Points", score.points.formatted(.number.precision(.fractionLength(2))))
                        if let closing = score.closingM {
                            row("Start to finish", "\(Format.kilometres(metres: closing)) km")
                        }
                    } header: {
                        Text("Cross-country")
                    } footer: {
                        Text("Worked out by Venti from the World XContest's published rules, at \(score.pointsPerKm == 1 ? "1 point" : "\(score.pointsPerKm.formatted()) points") per kilometre for this kind of route. The dashed line on the map is the scored route. A league's own result can differ a little.")
                    }
                }
                if let analysis, analysis.takeoff != nil {
                    Section {
                        row("In thermals", share(of: analysis.thermallingS, in: analysis))
                        row("On glides", share(of: analysis.glidingS, in: analysis))
                        row("Gained in thermals", "\(Format.metres(analysis.heightGainedM)) m")
                    } header: {
                        Text("In the air")
                    } footer: {
                        if analysis.landing == nil {
                            Text("The recording ends in the air.")
                        }
                    }
                    if !analysis.thermals.isEmpty {
                        Section("Thermals") {
                            ForEach(analysis.thermals, id: \.entry.trackIndex) { thermal in
                                Button {
                                    onSelect(Int(thermal.entry.trackIndex))
                                } label: {
                                    ThermalRow(thermal: thermal, flightStart: flight.startUnixS)
                                        .contentShape(.rect)
                                }
                                // Plain, so the row keeps its own text
                                // colours instead of the button tint.
                                .buttonStyle(.plain)
                                .accessibilityIdentifier("thermal-row")
                            }
                        }
                    }
                    if !analysis.glides.isEmpty {
                        Section("Glides") {
                            ForEach(analysis.glides, id: \.start.trackIndex) { glide in
                                Button {
                                    onSelect(Int(glide.start.trackIndex))
                                } label: {
                                    GlideRow(glide: glide, flightStart: flight.startUnixS)
                                        .contentShape(.rect)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                Section("Altitude and climb") {
                    row("Highest", "\(Format.metres(flight.maxAltitudeM)) m")
                    row("Lowest", "\(Format.metres(flight.minAltitudeM)) m")
                    row("Best climb", "\(Format.climb(metresPerSecond: flight.maxClimbMs)) m/s")
                    row("Strongest sink", "\(Format.climb(metresPerSecond: flight.maxSinkMs)) m/s")
                }
                Section {
                    ForEach(ClimbBand.allCases.reversed(), id: \.self) { band in
                        Label {
                            Text(band.label)
                        } icon: {
                            Capsule()
                                .fill(band.color)
                                .frame(width: 22, height: 6)
                        }
                    }
                } header: {
                    Text("Track colours")
                } footer: {
                    Text("Climb and sink along the track are averaged over ten seconds. Best climb and strongest sink are averaged over half a minute.")
                }
                Section("Recording") {
                    if let pilot = flight.pilot {
                        row("Pilot", pilot)
                    }
                    if let glider = flight.glider {
                        row("Glider", glider)
                    }
                    row("Recorder", flight.recorder)
                    row("Positions", flight.track.count.formatted())
                    if flight.skippedLines > 0 {
                        row("Unreadable lines", flight.skippedLines.formatted())
                    }
                }
            }
            .monospacedDigit()
            .navigationTitle("Flight details")
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

    /// "FAI triangle, closed", "Free flight".
    private static func name(of score: FlightScore) -> String {
        let kind =
            switch score.kind {
            case .freeFlight: "Free flight"
            case .freeTriangle: "Free triangle"
            case .faiTriangle: "FAI triangle"
            }
        return score.closed ? "\(kind), closed" : kind
    }

    private func row(_ title: String, _ value: String) -> some View {
        LabeledContent(title, value: value)
    }

    /// A duration with its share of the airtime: "0:07 h, 24%".
    private func share(of seconds: Int64, in analysis: FlightAnalysis) -> String {
        let time = "\(Format.hoursMinutes(seconds: seconds)) h"
        guard analysis.airtimeS > 0 else { return time }
        let percent = (Double(seconds) / Double(analysis.airtimeS)).formatted(.percent.precision(.fractionLength(0)))
        return "\(time), \(percent)"
    }
}

/// One thermal: when, how much height, how fast, and the wind it showed.
private struct ThermalRow: View {
    let thermal: ThermalSummary
    let flightStart: Int64

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text(Format.time(unixSeconds: flightStart + Int64(thermal.entry.seconds)))
                    .foregroundStyle(.primary)
                Text(detail)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text("\(Format.signedMetres(thermal.gainM)) m")
                    .foregroundStyle(.primary)
                Text("\(Format.climb(metresPerSecond: thermal.climbMs)) m/s")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var detail: String {
        let minutes = Format.minutesSeconds(seconds: Int64(thermal.exit.seconds) - Int64(thermal.entry.seconds))
        guard let wind = thermal.wind else { return "\(minutes) min" }
        let speed = Format.kilometresPerHour(metresPerSecond: wind.speedMs)
        return "\(minutes) min · wind \(Format.compassPoint(degrees: wind.fromDeg)) \(speed) km/h"
    }
}

/// One glide: when, how far, at what glide ratio and speed.
private struct GlideRow: View {
    let glide: GlideSummary
    let flightStart: Int64

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text(Format.time(unixSeconds: flightStart + Int64(glide.start.seconds)))
                    .foregroundStyle(.primary)
                Text("\(Format.kilometresPerHour(metresPerSecond: glide.speedMs)) km/h")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text("\(Format.kilometres(metres: glide.distanceM)) km")
                    .foregroundStyle(.primary)
                Text(glide.glideRatio.map { "glide \(Format.glideRatio($0))" } ?? "no height lost")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }
}
