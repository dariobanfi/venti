import SwiftUI
import VentiCore

/// The panel under the flight map, as the design language's flight screen
/// has it: the flight in one large number, what kind of flight it was and
/// when, three readings, its altitude profile, and for the pilot's own
/// flight the button that sends it to a league.
///
/// Dragging along the profile moves a marker along the track, and the numbers
/// change to that moment. The marker stays where the finger left it, so the
/// pilot can look from the chart to the map; the button in the corner returns
/// to the totals.
struct FlightPanel: View {
    let flight: FlightView
    /// How the flight scores, once that is worked out.
    var score: FlightScore?
    /// The ground under the flight, where the iPhone has it.
    var ground: [GroundUnderFlight] = []
    /// A second flight laid beside this one.
    var compared: ComparedFlight?
    /// Called when the pilot taps the line about the second flight.
    var onShowComparison: () -> Void = {}
    /// Hands the flight to a league. Absent for a flight that is not the
    /// pilot's own.
    var onSendToLeague: (() -> Void)?
    @Binding var cursor: Int?
    @Binding var isPlaying: Bool
    @Binding var isShowingDetails: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            header
            if let cursor {
                moment(flight.track[cursor])
            } else {
                totals
            }
            if let compared {
                beside(compared)
            }
            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                AltitudeChart(flight: flight, ground: ground, compared: compared, cursor: $cursor)
                    .frame(height: 124)
                ClimbLegend()
            }
            .ventiCard()
            if let onSendToLeague {
                Button(action: onSendToLeague) {
                    Text("Send to a league")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
                .accessibilityIdentifier("send-to-league")
            }
        }
        // Down to the screen's edge, like the page it is: the last of the
        // padding keeps its content clear of the home bar.
        .padding(EdgeInsets(top: VentiSpace.s4, leading: VentiSpace.s4, bottom: 30, trailing: VentiSpace.s4))
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.venti(.bg))
        .overlay(alignment: .top) {
            Rectangle()
                .fill(.venti(.border))
                .frame(height: 1)
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 4) {
            VStack(alignment: .leading, spacing: 2) {
                // The flight in one number, the one the logbook's list
                // has: the scored distance. Until it is worked out, and
                // for a flight too short to score, the length of the track.
                Text("\(Format.kilometres(metres: score?.distanceM ?? flight.trackLengthM)) km")
                    .font(.ventiType(.hero))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                    .accessibilityLabel(score == nil ? "Track" : "Distance")
                    .accessibilityValue(
                        "\(Format.kilometres(metres: score?.distanceM ?? flight.trackLengthM)) kilometres"
                    )
                    .accessibilityIdentifier("flight-headline")
                Text(kindAndDay)
                    .lineLimit(1)
                Text(subtitle)
                    .font(.ventiType(.label))
                    .foregroundStyle(.venti(.textSecondary))
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

    /// The flight laid beside this one, in a line: whose it is, and at
    /// a marked moment how high its glider was and how far away; else how
    /// the two lie to each other. A tap opens the numbers side by side.
    private func beside(_ compared: ComparedFlight) -> some View {
        let now = cursor.flatMap { compared.words(at: flight.track[$0]) }
        return Button {
            onShowComparison()
        } label: {
            HStack(spacing: 8) {
                Circle()
                    .fill(Palette.otherFlight)
                    .frame(width: 10, height: 10)
                    .accessibilityHidden(true)
                Text(compared.name)
                    .fontWeight(.semibold)
                    .lineLimit(1)
                    .layoutPriority(1)
                Text(now ?? (cursor == nil ? compared.comparison.words : "Not in the air then"))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .monospacedDigit()
                Spacer(minLength: 4)
                Image(systemName: "chevron.right")
                    .font(.venti(.footnote, .semibold))
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            }
            .font(.venti(.subheadline))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("flight-beside")
        .accessibilityHint("Shows the two flights side by side in numbers")
    }

    /// What kind of flight it was and on which day: "FAI triangle · Sat,
    /// Jul 4, 2026". The day alone until the flight is scored.
    private var kindAndDay: String {
        let day = Format.day(unixSeconds: flight.startUnixS)
        guard let score else { return day }
        let kind =
            switch score.kind {
            case .freeFlight: "Free flight"
            case .freeTriangle: score.closed ? "Closed triangle" : "Triangle"
            case .faiTriangle: score.closed ? "Closed FAI triangle" : "FAI triangle"
            }
        return "\(kind) · \(day)"
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
            Metric(caption: "Highest", value: Format.metres(flight.maxAltitudeM), unit: "m")
            Metric(caption: "Best climb", value: Format.climb(metresPerSecond: flight.maxClimbMs), unit: "m/s")
        }
    }

    private func moment(_ point: TrackPoint) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Metric(caption: "Time", value: Format.time(unixSeconds: flight.startUnixS + Int64(point.seconds)))
            // How high over the ground the glider was, where the ground
            // is known; else how long it had been in the air.
            if let above = Self.heightAboveGround(at: point.seconds, in: ground) {
                Metric(caption: "Over ground", value: Format.metres(max(above, 0)), unit: "m")
            } else {
                Metric(caption: "In the air", value: Format.hoursMinutes(seconds: Int64(point.seconds)), unit: "h")
            }
            Metric(caption: "Altitude", value: Format.metres(point.altitudeM), unit: "m")
            Metric(caption: "Climb", value: Format.climb(metresPerSecond: point.climbMs), unit: "m/s")
        }
    }
}

extension FlightPanel {
    /// The height over the ground at a moment of the flight: that of the
    /// moment the ground was read at nearest to it. The ground is read at
    /// a few hundred moments, the track has thousands. `nil` where the
    /// nearest lies further off than those moments lie apart: there the
    /// iPhone has no ground.
    nonisolated static func heightAboveGround(at seconds: UInt32, in ground: [GroundUnderFlight]) -> Double? {
        guard let first = ground.first, let last = ground.last else { return nil }
        // In order of time: the first that is not earlier, by halving.
        var low = 0
        var high = ground.count - 1
        while low < high {
            let middle = (low + high) / 2
            if ground[middle].seconds < seconds {
                low = middle + 1
            } else {
                high = middle
            }
        }
        func gap(_ moment: GroundUnderFlight) -> Int64 {
            abs(Int64(moment.seconds) - Int64(seconds))
        }
        var nearest = ground[low]
        if low > 0, gap(ground[low - 1]) < gap(nearest) {
            nearest = ground[low - 1]
        }
        let apart = max(1, (Int64(last.seconds) - Int64(first.seconds)) / Int64(max(ground.count - 1, 1)))
        return gap(nearest) <= apart ? nearest.heightAboveM : nil
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
                .font(.venti(.title2))
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
        .font(.venti(.caption))
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Track colours: teal where the glider sank, grey where it held its height, coral where it climbed.")
    }
}
