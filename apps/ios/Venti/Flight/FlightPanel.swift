import SwiftUI
import VentiCore

/// The panel over the flight map: when the flight was, its headline numbers,
/// and its altitude profile.
///
/// Dragging along the profile moves a marker along the track, and the numbers
/// change to that moment. The marker stays where the finger left it, so the
/// pilot can look from the chart to the map; the button in the corner returns
/// to the totals.
struct FlightPanel: View {
    let flight: FlightView
    @Binding var cursor: Int?
    @Binding var isPlaying: Bool
    @Binding var isShowingDetails: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            if let cursor {
                moment(flight.track[cursor])
            } else {
                totals
            }
            AltitudeChart(flight: flight, cursor: $cursor)
                .frame(height: 124)
            ClimbLegend()
        }
        .padding(EdgeInsets(top: 18, leading: 18, bottom: 22, trailing: 18))
        .glassEffect(.regular, in: .rect(cornerRadius: 36))
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 4) {
            VStack(alignment: .leading, spacing: 2) {
                Text(Format.day(unixSeconds: flight.startUnixS))
                    .font(.headline)
                Text(subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            if cursor != nil {
                PanelButton(title: "Show flight totals", systemImage: "xmark.circle.fill") {
                    isPlaying = false
                    cursor = nil
                }
                .foregroundStyle(.secondary)
            }
            PanelButton(
                title: isPlaying ? "Pause" : "Play the flight",
                systemImage: isPlaying ? "pause.circle" : "play.circle"
            ) {
                isPlaying.toggle()
            }
            PanelButton(title: "Flight details", systemImage: "info.circle") {
                isShowingDetails = true
            }
        }
    }

    private var subtitle: String {
        let start = Format.time(unixSeconds: flight.startUnixS)
        let end = Format.time(unixSeconds: flight.startUnixS + flight.durationS)
        guard let glider = flight.glider, !glider.isEmpty else { return "\(start) – \(end)" }
        return "\(start) – \(end) · \(glider)"
    }

    private var totals: some View {
        HStack(alignment: .top, spacing: 8) {
            Metric(caption: "Airtime", value: Format.hoursMinutes(seconds: flight.durationS), unit: "h")
            Metric(caption: "Track", value: Format.kilometres(metres: flight.trackLengthM), unit: "km")
            Metric(caption: "Highest", value: Format.metres(flight.maxAltitudeM), unit: "m")
            Metric(caption: "Best climb", value: Format.climb(metresPerSecond: flight.maxClimbMs), unit: "m/s")
        }
    }

    private func moment(_ point: TrackPoint) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Metric(caption: "Time", value: Format.time(unixSeconds: flight.startUnixS + Int64(point.seconds)))
            Metric(caption: "In the air", value: Format.hoursMinutes(seconds: Int64(point.seconds)), unit: "h")
            Metric(caption: "Altitude", value: Format.metres(point.altitudeM), unit: "m")
            Metric(caption: "Climb", value: Format.climb(metresPerSecond: point.climbMs), unit: "m/s")
        }
    }
}

/// An icon button in the panel's corner, with a full-size touch target.
private struct PanelButton: View {
    let title: String
    let systemImage: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .labelStyle(.iconOnly)
                .font(.title2)
                .frame(width: 44, height: 44)
                .contentShape(.rect)
        }
        .buttonStyle(.borderless)
    }
}

/// What the colours of the track mean, in one line.
struct ClimbLegend: View {
    var body: some View {
        HStack(spacing: 8) {
            Text("Sink")
            HStack(spacing: 2) {
                ForEach(ClimbBand.allCases, id: \.self) { band in
                    Capsule()
                        .fill(band.color)
                        .frame(height: 6)
                }
            }
            Text("Climb")
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Track colours: blue where the glider sank, grey where it held its height, orange and red where it climbed.")
    }
}
