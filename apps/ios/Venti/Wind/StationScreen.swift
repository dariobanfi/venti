import Charts
import SwiftUI
import VentiCore

/// One wind station: what it measures now, what that says about the air,
/// how the last 24 hours or a past day went, and whose data it is.
struct StationScreen: View {
    let station: WindStation

    @Environment(WindModel.self) private var wind

    private enum Phase {
        case loading
        case ready(StationHistory)
        /// A past day the pilot chose.
        case day(StationDay)
        case failed(String)
    }

    @State private var phase = Phase.loading
    /// The moment under the finger on the chart.
    @State private var touched: Date?
    /// The moment the date picker was set to, for showing it again.
    @State private var chosenMoment: Date?
    /// The past day chosen, by its date; none for the last 24 hours.
    @State private var date: String?
    /// Whether what the core reads of the air here and how the chart's
    /// days are to be read is unfolded: both stand behind a row at the end.
    @State private var showsMoreAir = false

    var body: some View {
        VentiList {
            Section {
                now
            } footer: {
                Text(ageText)
            }

            Section {
                dayPicker
                switch phase {
                case .loading:
                    ProgressView()
                        .frame(maxWidth: .infinity, minHeight: 120)
                case .ready(let history):
                    charts(Self.samples(from: history.readings), over: nil)
                case .day(let day):
                    if let empty = day.emptyWords {
                        Text(empty)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, minHeight: 120, alignment: .leading)
                            .accessibilityIdentifier("station-day-empty")
                    } else {
                        charts(
                            Self.samples(from: day.readings),
                            over: Self.time(day.startUnixS)...Self.time(day.endUnixS)
                        )
                    }
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

            // A wind alarm for this station, on the account.
            AlertMeSection(draft: AlertsModel.draft(for: station))

            if let credit {
                Section("Source") {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(credit.text)
                        Text("\(credit.provider), under \(credit.licence).")
                            .font(.venti(.subheadline))
                            .foregroundStyle(.secondary)
                        if let url = URL(string: credit.url) {
                            Link(url.host() ?? credit.url, destination: url)
                                .font(.venti(.subheadline))
                        }
                    }
                    .padding(.vertical, 2)
                }
            }
            // What the core reads of the air from the readings, and how
            // the chart's days are to be read: behind a row at the end.
            Section {
                Button {
                    showsMoreAir.toggle()
                } label: {
                    HStack {
                        Text(stationSheetWords().moreAirTitle)
                        Spacer(minLength: VentiSpace.s2)
                        Image(systemName: showsMoreAir ? "chevron.up" : "chevron.down")
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textMuted))
                            .accessibilityHidden(true)
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityValue(showsMoreAir ? "Shown" : "Hidden")
                .accessibilityIdentifier("station-more-air")
            } footer: {
                if showsMoreAir {
                    Text(days.note)
                        .accessibilityIdentifier("station-days-note")
                }
            }
            if showsMoreAir {
                air
            }
        }
        .navigationTitle(station.name)
        // The network, quietly under the name: what tells apart two
        // stations of one name.
        .subtitled(station.network)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: date) {
            await load()
        }
    }

    // MARK: The air

    /// What the station's readings say about the air: the cloud base air
    /// rising from it would have, and the step in temperature to a station
    /// higher or lower nearby. The words, the limits and the notes are the
    /// core's; nothing is shown that the core leaves out.
    @ViewBuilder private var air: some View {
        let air = stationAir(station: station, stations: wind.stations)
        if let base = air.cloudBase {
            Section {
                HStack(alignment: .top) {
                    Metric(caption: base.words, value: Format.metres(base.baseM), unit: "m")
                    Metric(caption: "Above the station", value: Format.metres(base.aboveStationM), unit: "m")
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("station-cloud-base")
            } footer: {
                Text(base.note)
            }
        }
        if let step = air.temperatureStep {
            Section {
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    Text(step.words)
                        .font(.venti(.headline))
                    Text(Self.stepLine(step))
                        .monospacedDigit()
                    Text(step.rule)
                        .font(.venti(.footnote))
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, VentiSpace.s1)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("station-temperature-step")
            } header: {
                Text("Temperature up the slope")
            } footer: {
                Text(step.note)
            }
        }
    }

    /// The other station and the figures of the step: "Wallberg, 4.2 km
    /// away, 920 m higher, 3 °C: −0.6 °C per 100 m".
    static func stepLine(_ step: TemperatureStep) -> String {
        let height = step.higherByM >= 0
            ? "\(Format.metres(step.higherByM)) m higher"
            : "\(Format.metres(-step.higherByM)) m lower"
        return "\(step.otherName), \(Format.kilometres(metres: step.distanceM)) km away, \(height), "
            + "\(Format.temperature(celsius: step.otherTemperatureC)) °C: "
            + "\(Format.signedDecimal(step.per100mC, places: 1)) °C per 100 m"
    }

    // MARK: Past days

    /// The days the station's readings can be asked for, by this
    /// iPhone's clock.
    private var days: StationDays {
        stationDays(nowUnixS: wind.now, utcOffsetS: WindModel.utcOffsetS)
    }

    /// The day the chart shows, chosen with the system's compact date
    /// picker: today, which is the last 24 hours, or a day back to the
    /// first the server keeps.
    private var dayPicker: some View {
        let days = days
        let range = Self.time(days.firstUnixS)...Self.time(max(days.lastUnixS, days.firstUnixS))
        return VStack(alignment: .leading, spacing: VentiSpace.s1) {
            DatePicker(
                date == nil ? "Last 24 hours" : "Day",
                selection: Binding(
                    get: {
                        min(max(chosenMoment ?? range.upperBound, range.lowerBound), range.upperBound)
                    },
                    set: { moment in
                        let picked = stationDate(atUnixS: Int64(moment.timeIntervalSince1970), utcOffsetS: WindModel.utcOffsetS)
                        chosenMoment = moment
                        date = picked == days.lastDate ? nil : picked
                    }
                ),
                in: range,
                displayedComponents: .date
            )
            .datePickerStyle(.compact)
            .font(.venti(.headline))
            .accessibilityIdentifier("station-day")
        }
    }

    // MARK: Now

    @ViewBuilder private var now: some View {
        let reading = station.latest
        // A value the station did not give is a dash without a unit.
        HStack(alignment: .top) {
            Metric(
                caption: "Wind",
                value: reading?.speedMs.map { Format.kilometresPerHour(metresPerSecond: $0) } ?? "–",
                unit: reading?.speedMs == nil ? "" : "km/h"
            )
            Metric(
                caption: "Gusts",
                value: reading?.gustMs.map { Format.kilometresPerHour(metresPerSecond: $0) } ?? "–",
                unit: reading?.gustMs == nil ? "" : "km/h"
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
                unit: reading?.temperatureC == nil ? "" : "°C"
            )
            Metric(
                caption: "Measured",
                value: reading.map { Format.time(unixSeconds: $0.timeUnixS) } ?? "–"
            )
            // A station whose source gives no height has none here: not
            // "0 m", not a dash. The room stays, so the row keeps the
            // columns of the row above.
            if let elevation = station.elevationM {
                Metric(caption: "Height", value: Format.metres(elevation), unit: "m")
                    .accessibilityIdentifier("station-height")
            } else {
                Color.clear.frame(maxWidth: .infinity, maxHeight: 0)
            }
        }
    }

    /// How old the reading is, and what that means if it is old.
    private var ageText: String {
        guard let reading = station.latest else {
            return "The server has no reading from this station."
        }
        let age = Format.readingAge(ageMin: station.ageMin, timeUnixS: reading.timeUnixS)
        return station.isOld
            ? "Measured \(age). Too old to say what the wind is doing now."
            : "Measured \(age)."
    }

    private var credit: Credit? {
        if case .ready(let history) = phase, let credit = history.credits.first {
            return credit
        }
        if case .day(let day) = phase, let credit = day.credits.first {
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

    nonisolated static func time(_ unixSeconds: Int64) -> Date {
        Date(timeIntervalSince1970: TimeInterval(unixSeconds))
    }

    nonisolated static func samples(from readings: [WindReading]) -> [Sample] {
        readings.map { reading in
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

    /// The charts of a day's readings, over the whole day when `over` is
    /// given, else over the readings themselves.
    @ViewBuilder
    private func charts(_ samples: [Sample], over day: ClosedRange<Date>?) -> some View {
        if samples.count < 2 {
            Text("The server has no history for this station yet.")
                .foregroundStyle(.secondary)
        } else {
            let marked = touched.flatMap { Self.nearest(to: $0, in: samples) }
            VStack(alignment: .leading, spacing: 10) {
                Text(caption(for: marked))
                    .font(.venti(.footnote))
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
                            .foregroundStyle(Color.venti(.text).opacity(0.45))
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
                                Text(Format.decimal(speed, places: 0))
                            }
                        }
                    }
                }
                .chartXScale(domain: day ?? Self.span(of: samples))
                .chartXSelection(value: $touched)
                .frame(height: 170)
                .accessibilityIdentifier("wind-chart")
                .accessibilityLabel("Wind and gusts, in kilometres per hour")

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
                            .foregroundStyle(Color.venti(.text).opacity(0.45))
                            .lineStyle(StrokeStyle(lineWidth: 1))
                    }
                }
                .chartYScale(domain: 0...360)
                .chartXScale(domain: day ?? Self.span(of: samples))
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
                .accessibilityLabel("Wind direction")
            }
            .padding(.vertical, 4)
        }
    }

    /// From the first sample to the last.
    nonisolated static func span(of samples: [Sample]) -> ClosedRange<Date> {
        let times = samples.map(\.time)
        guard let first = times.min(), let last = times.max(), first < last else {
            return Date(timeIntervalSince1970: 0)...Date(timeIntervalSince1970: 1)
        }
        return first...last
    }

    /// The reading under the finger in words, or what the charts show.
    private func caption(for sample: Sample?) -> String {
        guard let sample else {
            return "Wind and gusts in km/h, and where the wind came from."
        }
        var parts = [sample.time.formatted(date: .omitted, time: .shortened)]
        if let wind = sample.wind {
            parts.append("\(Format.decimal(wind, places: 0)) km/h")
        }
        if let gusts = sample.gusts {
            parts.append("gusts \(Format.decimal(gusts, places: 0))")
        }
        if let from = sample.from {
            parts.append("from \(Format.compassPoint(degrees: from))")
        }
        return parts.joined(separator: " · ")
    }

    private func load() async {
        phase = .loading
        touched = nil
        do {
            if let date {
                phase = .day(try await wind.day(of: station, date: date))
            } else {
                phase = .ready(try await wind.history(of: station))
            }
        } catch {
            phase = .failed(
                error is CoreError
                    ? error.words
                    : "The history could not be loaded. \(error.localizedDescription)"
            )
        }
    }
}

private extension View {
    /// A quiet line under the bar's title, where there is one.
    @ViewBuilder func subtitled(_ text: String?) -> some View {
        if let text, !text.isEmpty {
            navigationSubtitle(text)
        } else {
            self
        }
    }
}
