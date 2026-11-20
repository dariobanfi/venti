import Charts
import SwiftUI
import VentiCore

/// The flight's altitude over time, over the ground it was flown over
/// where the iPhone has that. Dragging along it sets the cursor.
struct AltitudeChart: View {
    let flight: FlightView
    /// The ground under the flight, in order of time. Empty without it.
    var ground: [GroundUnderFlight] = []
    /// A second flight laid beside this one: its altitude is a second
    /// line, on this flight's time.
    var compared: ComparedFlight?
    @Binding var cursor: Int?

    /// The time under the finger while it is down.
    @State private var touched: Date?

    var body: some View {
        let range = Self.altitudeRange(of: flight, over: ground, beside: compared?.flight)
        let marked = cursor.map { flight.track[$0] }
        let otherMarked = marked.flatMap { compared?.point(atSeconds: $0.seconds) }

        Chart {
            ForEach(flight.profile, id: \.seconds) { point in
                AreaMark(
                    x: .value("Time", time(point.seconds)),
                    yStart: .value("Altitude", range.lowerBound),
                    yEnd: .value("Altitude", point.altitudeM),
                    series: .value("Layer", "Flight")
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
                    y: .value("Altitude", point.altitudeM),
                    series: .value("Flight", "This")
                )
                .foregroundStyle(Palette.accent)
                .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
            }
            // The ground under the glider, over the foot of the flight's
            // own shape: the room between the two is the height over it.
            ForEach(ground, id: \.seconds) { moment in
                AreaMark(
                    x: .value("Time", time(moment.seconds)),
                    yStart: .value("Altitude", range.lowerBound),
                    yEnd: .value("Altitude", min(max(moment.elevationM, range.lowerBound), range.upperBound)),
                    series: .value("Layer", "Ground")
                )
                .foregroundStyle(Self.groundColor)
            }
            // The flight laid beside this one, over everything but the
            // marker: where it leaves this flight's time it is cut off.
            if let compared {
                ForEach(compared.flight.profile, id: \.seconds) { point in
                    LineMark(
                        x: .value("Time", moment(Double(point.seconds) + Double(compared.comparison.offsetS))),
                        y: .value("Altitude", point.altitudeM),
                        series: .value("Flight", "Other")
                    )
                    .foregroundStyle(Palette.otherFlight)
                    .lineStyle(StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round))
                }
            }
            if let marked, let otherMarked {
                PointMark(
                    x: .value("Time", time(marked.seconds)),
                    y: .value("Altitude", otherMarked.altitudeM)
                )
                .symbolSize(60)
                .foregroundStyle(Palette.otherFlight)
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
        // The other flight may begin before this one or end after it.
        .chartPlotStyle { plot in
            plot.clipped()
        }
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
        moment(Double(seconds))
    }

    /// The time of a moment given in seconds since the flight's first
    /// fix, which for a flight laid beside it may be before or after.
    private func moment(_ seconds: Double) -> Date {
        Date(timeIntervalSince1970: TimeInterval(flight.startUnixS) + seconds)
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
    /// The heights the chart shows: the flight's, and with the ground
    /// under it the lowest ground too.
    nonisolated static func altitudeRange(
        of flight: FlightView, over ground: [GroundUnderFlight] = [], beside other: FlightView? = nil
    ) -> ClosedRange<Double> {
        var lowest = ground.map(\.elevationM).min().map { min($0, flight.minAltitudeM) } ?? flight.minAltitudeM
        var highest = flight.maxAltitudeM
        // A flight laid beside this one is shown whole in height.
        if let other {
            lowest = min(lowest, other.minAltitudeM)
            highest = max(highest, other.maxAltitudeM)
        }
        let low = (lowest / 100).rounded(.down) * 100
        let high = (highest / 100).rounded(.up) * 100
        return low...max(high, low + 100)
    }

    /// The ground: the colour of earth, solid enough to cover the foot of
    /// the flight's own shape.
    static let groundColor = Color(red: 0.55, green: 0.48, blue: 0.4).opacity(0.85)
}
