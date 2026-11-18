import Charts
import SwiftUI
import VentiCore

/// The flight's altitude over time. Dragging along it sets the cursor.
struct AltitudeChart: View {
    let flight: FlightView
    @Binding var cursor: Int?

    /// The time under the finger while it is down.
    @State private var touched: Date?

    var body: some View {
        let range = Self.altitudeRange(of: flight)
        let marked = cursor.map { flight.track[$0] }

        Chart {
            ForEach(flight.profile, id: \.seconds) { point in
                AreaMark(
                    x: .value("Time", time(point.seconds)),
                    yStart: .value("Altitude", range.lowerBound),
                    yEnd: .value("Altitude", point.altitudeM)
                )
                .foregroundStyle(
                    LinearGradient(
                        colors: [Palette.accent.opacity(0.32), Palette.accent.opacity(0.04)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
                LineMark(
                    x: .value("Time", time(point.seconds)),
                    y: .value("Altitude", point.altitudeM)
                )
                .foregroundStyle(Palette.accent)
                .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
            }
            if let marked {
                RuleMark(x: .value("Time", time(marked.seconds)))
                    .foregroundStyle(Color.primary.opacity(0.45))
                    .lineStyle(StrokeStyle(lineWidth: 1))
                PointMark(
                    x: .value("Time", time(marked.seconds)),
                    y: .value("Altitude", marked.altitudeM)
                )
                .symbolSize(90)
                .foregroundStyle(Palette.accent)
            }
        }
        .chartXScale(domain: time(0)...time(UInt32(clamping: max(flight.durationS, 1))))
        .chartYScale(domain: range)
        .chartXAxis {
            AxisMarks(values: .automatic(desiredCount: 4)) { _ in
                AxisGridLine()
                // No AM or PM: the panel's heading already says which.
                AxisValueLabel(format: .dateTime.hour(.defaultDigits(amPM: .omitted)).minute())
            }
        }
        .chartYAxis {
            AxisMarks(position: .trailing, values: .automatic(desiredCount: 3)) { value in
                AxisGridLine()
                AxisValueLabel {
                    if let metres = value.as(Double.self) {
                        Text("\(Format.metres(metres)) m")
                    }
                }
            }
        }
        .chartXSelection(value: $touched)
        .onChange(of: touched) { _, time in
            // Lifting the finger ends the touch but keeps the cursor.
            if let time {
                cursor = index(at: time)
            }
        }
        .accessibilityIdentifier("altitude-chart")
        .accessibilityLabel("Altitude over time")
        .accessibilityValue(
            "From \(Format.metres(flight.minAltitudeM)) to \(Format.metres(flight.maxAltitudeM)) metres"
        )
    }

    private func time(_ seconds: UInt32) -> Date {
        Date(timeIntervalSince1970: TimeInterval(flight.startUnixS) + TimeInterval(seconds))
    }

    /// The position in the track closest to a moment.
    private func index(at time: Date) -> Int {
        let seconds = time.timeIntervalSince1970 - TimeInterval(flight.startUnixS)
        return Self.nearestIndex(in: flight.track, toSeconds: seconds)
    }

    /// Binary search over a track, which is in time order.
    nonisolated static func nearestIndex(in track: [TrackPoint], toSeconds seconds: Double) -> Int {
        guard !track.isEmpty else { return 0 }
        var (low, high) = (0, track.count - 1)
        while low < high {
            let middle = (low + high) / 2
            if Double(track[middle].seconds) < seconds {
                low = middle + 1
            } else {
                high = middle
            }
        }
        if low > 0, seconds - Double(track[low - 1].seconds) < Double(track[low].seconds) - seconds {
            return low - 1
        }
        return low
    }

    /// The vertical extent of the chart: the flight's altitudes widened to
    /// whole hundreds of metres, and never flat.
    nonisolated static func altitudeRange(of flight: FlightView) -> ClosedRange<Double> {
        let low = (flight.minAltitudeM / 100).rounded(.down) * 100
        let high = (flight.maxAltitudeM / 100).rounded(.up) * 100
        return low...max(high, low + 100)
    }
}
