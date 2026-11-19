import Charts
import SwiftUI
import VentiCore

/// One wind station: what it measures now, how the last 24 hours went, and
/// whose data it is.
struct StationScreen: View {
    let station: WindStation

    @Environment(WindModel.self) private var wind

    private enum Phase {
        case loading
        case ready(StationHistory)
        case failed(String)
    }

    @State private var phase = Phase.loading
    /// The moment under the finger on the chart.
    @State private var touched: Date?

    var body: some View {
        List {
            Section {
                now
            } footer: {
                Text(ageText)
            }

            Section("Last 24 hours") {
                switch phase {
                case .loading:
                    ProgressView()
                        .frame(maxWidth: .infinity, minHeight: 120)
                case .ready(let history):
                    charts(Self.samples(from: history))
                case .failed(let message):
                    VStack(alignment: .leading, spacing: 8) {
                        Text(message)
                            .foregroundStyle(.secondary)
                        Button("Try again") {
                            Task { await load() }
                        }
                    }
                }
            }

            Section {
                NavigationLink {
                    ForecastScreen(place: station.position, title: station.name)
                } label: {
                    Label("Forecast for this place", systemImage: "cloud.sun")
                }
                .accessibilityIdentifier("station-forecast")
            }

            if let credit {
                Section("Source") {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(credit.text)
                        Text("\(credit.provider), under \(credit.licence).")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        if let url = URL(string: credit.url) {
                            Link(url.host() ?? credit.url, destination: url)
                                .font(.subheadline)
                        }
                    }
                    .padding(.vertical, 2)
                }
            }
        }
        .navigationTitle(station.name)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await load()
        }
    }

    // MARK: Now

    @ViewBuilder private var now: some View {
        let reading = station.latest
        HStack(alignment: .top) {
            Metric(
                caption: "Wind",
                value: reading?.speedMs.map { Format.kilometresPerHour(metresPerSecond: $0) } ?? "–",
                unit: "km/h"
            )
            Metric(
                caption: "Gusts",
                value: reading?.gustMs.map { Format.kilometresPerHour(metresPerSecond: $0) } ?? "–",
                unit: "km/h"
            )
            Metric(
                caption: "From",
                value: reading?.directionDeg.map { Format.compassPoint(degrees: $0) } ?? "–",
                unit: reading?.directionDeg.map { "\(Format.metres($0))°" } ?? ""
            )
        }
        HStack(alignment: .top) {
            Metric(
                caption: "Temperature",
                value: reading?.temperatureC.map { Format.temperature(celsius: $0) } ?? "–",
                unit: "°C"
            )
            Metric(
                caption: "Height",
                value: station.elevationM.map { Format.metres($0) } ?? "–",
                unit: "m"
            )
            Metric(
                caption: "Measured",
                value: reading.map { Format.time(unixSeconds: $0.timeUnixS) } ?? "–"
            )
        }
    }

    /// How old the reading is, and what that means if it is old.
    private var ageText: String {
        guard let reading = station.latest else {
            return "The server has no reading from this station."
        }
        let age = Format.age(minutes: Format.minutes(from: reading.timeUnixS, to: wind.now))
        return station.isOld
            ? "Measured \(age). Too old to say what the wind is doing now."
            : "Measured \(age)."
    }

    private var credit: Credit? {
        if case .ready(let history) = phase, let credit = history.credits.first {
            return credit
        }
        return wind.credits.first { $0.source == station.source }
    }

    // MARK: History

    /// One reading, in the units the charts draw.
    struct Sample: Equatable {
        let time: Date
        /// Mean wind, km/h.
        let wind: Double?
        /// Gusts, km/h.
        let gusts: Double?
        /// Where the wind comes from, degrees.
        let from: Double?
    }

    nonisolated static func samples(from history: StationHistory) -> [Sample] {
        history.readings.map { reading in
            Sample(
                time: Date(timeIntervalSince1970: TimeInterval(reading.timeUnixS)),
                wind: reading.speedMs.map { $0 * 3.6 },
                gusts: reading.gustMs.map { $0 * 3.6 },
                from: reading.directionDeg
            )
        }
    }

    /// The sample closest to a moment.
    nonisolated static func nearest(to time: Date, in samples: [Sample]) -> Sample? {
        samples.min { first, second in
            abs(first.time.timeIntervalSince(time)) < abs(second.time.timeIntervalSince(time))
        }
    }

    @ViewBuilder
    private func charts(_ samples: [Sample]) -> some View {
        if samples.count < 2 {
            Text("The server has no history for this station yet.")
                .foregroundStyle(.secondary)
        } else {
            let marked = touched.flatMap { Self.nearest(to: $0, in: samples) }
            VStack(alignment: .leading, spacing: 10) {
                Text(caption(for: marked))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()

                Chart {
                    ForEach(samples, id: \.time) { sample in
                        if let wind = sample.wind, let gusts = sample.gusts {
                            AreaMark(
                                x: .value("Time", sample.time),
                                yStart: .value("Wind", wind),
                                yEnd: .value("Gusts", max(gusts, wind))
                            )
                            .foregroundStyle(by: .value("Measure", "Gusts"))
                        }
                        if let wind = sample.wind {
                            LineMark(
                                x: .value("Time", sample.time),
                                y: .value("Wind", wind)
                            )
                            .foregroundStyle(by: .value("Measure", "Wind"))
                            .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                        }
                    }
                    if let marked {
                        RuleMark(x: .value("Time", marked.time))
                            .foregroundStyle(Color.primary.opacity(0.45))
                            .lineStyle(StrokeStyle(lineWidth: 1))
                    }
                }
                .chartForegroundStyleScale(["Wind": Palette.wind, "Gusts": Palette.gusts])
                .chartXAxis {
                    // Over a whole day a time needs its half of the day, and
                    // the labels at both ends stay inside the chart.
                    AxisMarks(preset: .aligned, values: .stride(by: .hour, count: 6)) { _ in
                        AxisGridLine()
                        AxisValueLabel(format: .dateTime.hour())
                    }
                }
                .chartYAxis {
                    AxisMarks(position: .trailing, values: .automatic(desiredCount: 4)) { value in
                        AxisGridLine()
                        AxisValueLabel {
                            if let speed = value.as(Double.self) {
                                Text(speed.formatted(.number.precision(.fractionLength(0))))
                            }
                        }
                    }
                }
                .chartXSelection(value: $touched)
                .frame(height: 170)
                .accessibilityIdentifier("wind-chart")
                .accessibilityLabel("Wind and gusts over the last 24 hours, in kilometres per hour")

                // Its own small chart, not a second scale on the first:
                // degrees and km/h have nothing to do with each other.
                Chart {
                    ForEach(samples, id: \.time) { sample in
                        if let from = sample.from {
                            PointMark(
                                x: .value("Time", sample.time),
                                y: .value("From", from)
                            )
                            .symbolSize(14)
                            .foregroundStyle(Palette.wind)
                        }
                    }
                    if let marked {
                        RuleMark(x: .value("Time", marked.time))
                            .foregroundStyle(Color.primary.opacity(0.45))
                            .lineStyle(StrokeStyle(lineWidth: 1))
                    }
                }
                .chartYScale(domain: 0...360)
                .chartXAxis {
                    // Over a whole day a time needs its half of the day, and
                    // the labels at both ends stay inside the chart.
                    AxisMarks(preset: .aligned, values: .stride(by: .hour, count: 6)) { _ in
                        AxisGridLine()
                        AxisValueLabel(format: .dateTime.hour())
                    }
                }
                .chartYAxis {
                    AxisMarks(position: .trailing, values: [0, 90, 180, 270, 360]) { value in
                        AxisGridLine()
                        AxisValueLabel {
                            if let degrees = value.as(Int.self) {
                                Text(Format.compassPoint(degrees: Double(degrees)))
                            }
                        }
                    }
                }
                .chartXSelection(value: $touched)
                .frame(height: 96)
                .accessibilityLabel("Wind direction over the last 24 hours")
            }
            .padding(.vertical, 4)
        }
    }

    /// The reading under the finger in words, or what the charts show.
    private func caption(for sample: Sample?) -> String {
        guard let sample else {
            return "Wind and gusts in km/h, and where the wind came from."
        }
        var parts = [sample.time.formatted(date: .omitted, time: .shortened)]
        if let wind = sample.wind {
            parts.append("\(wind.formatted(.number.precision(.fractionLength(0)))) km/h")
        }
        if let gusts = sample.gusts {
            parts.append("gusts \(gusts.formatted(.number.precision(.fractionLength(0))))")
        }
        if let from = sample.from {
            parts.append("from \(Format.compassPoint(degrees: from))")
        }
        return parts.joined(separator: " · ")
    }

    private func load() async {
        phase = .loading
        do {
            phase = .ready(try await wind.history(of: station))
        } catch {
            phase = .failed(
                error is CoreError
                    ? error.words
                    : "The history could not be loaded. \(error.localizedDescription)"
            )
        }
    }
}
