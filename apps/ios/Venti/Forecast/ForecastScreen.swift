import SwiftUI
import VentiCore

/// What the weather models say about one place: the wind through the day
/// at the ground and higher up, the cloud base, cloud, rain and
/// temperature, hour by hour.
///
/// It shows magnitudes and no verdict. Whether a day can be flown depends
/// on the pilot, the wing and the place, and a model's cell of a few
/// kilometres knows none of them.
struct ForecastScreen: View {
    /// The place, and a name for it.
    let place: Coordinate
    let title: String

    @Environment(WindModel.self) private var wind

    private enum Phase {
        case loading
        case loaded(ForecastView)
        case failed(String)
    }

    @State private var phase = Phase.loading
    /// The model looked at, by its identifier. The finest one at first.
    @State private var model: String?
    /// The day looked at, by its first moment.
    @State private var day: Int64?

    var body: some View {
        Group {
            switch phase {
            case .loading:
                ProgressView()
            case .failed(let message):
                ContentUnavailableView {
                    Label("No forecast", systemImage: "cloud.slash")
                } description: {
                    Text(message)
                } actions: {
                    Button("Try again") {
                        phase = .loading
                        Task { await load() }
                    }
                }
            case .loaded(let forecast):
                if let shown = forecast.models.first(where: { $0.model == model }) ?? forecast.models.first {
                    loaded(forecast, model: shown)
                } else {
                    ContentUnavailableView(
                        "No forecast for this place",
                        systemImage: "cloud.slash",
                        description: Text("None of the server's weather models reaches it.")
                    )
                }
            }
        }
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await load()
        }
    }

    private func load() async {
        do {
            phase = .loaded(try await wind.forecast(at: place))
        } catch {
            phase = .failed(
                error is CoreError
                    ? error.words
                    : "The server could not be reached. \(error.localizedDescription)"
            )
        }
    }

    @ViewBuilder
    private func loaded(_ forecast: ForecastView, model shown: ModelForecast) -> some View {
        let days = shown.days.filter { $0.hours.contains(where: \.isDaylight) }
        let chosen = days.first { $0.startUnixS == day } ?? Self.today(among: days)
        List {
            if days.count > 1 {
                Section {
                    Picker("Day", selection: Binding(
                        get: { chosen?.startUnixS ?? 0 },
                        set: { day = $0 }
                    )) {
                        ForEach(days, id: \.startUnixS) { day in
                            Text(Self.name(of: day)).tag(day.startUnixS)
                        }
                    }
                    .pickerStyle(.segmented)
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                    .accessibilityIdentifier("forecast-day")
                }
            }
            if let chosen {
                Section {
                    ForecastTable(model: shown, day: chosen)
                        .listRowInsets(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 0))
                } header: {
                    Text("Wind in km/h, and the sky")
                } footer: {
                    Text(Self.caveat(for: shown))
                }
            }
            Section {
                if forecast.models.count > 1 {
                    Picker("Weather model", selection: Binding(
                        get: { shown.model },
                        set: { model = $0 }
                    )) {
                        ForEach(forecast.models, id: \.model) { model in
                            Text(model.name).tag(model.model)
                        }
                    }
                } else {
                    LabeledContent("Weather model", value: shown.name)
                }
                LabeledContent("Model run", value: Format.dayAndTime(unixSeconds: shown.runUnixS))
                if let ground = shown.cellElevationM {
                    LabeledContent("The model's ground here", value: "\(Format.metres(ground)) m")
                }
            } footer: {
                Text(forecast.credits.map(\.text).joined(separator: "\n"))
            }
        }
        .accessibilityIdentifier("forecast")
    }

    /// The day that holds this moment, or the first one.
    private static func today(among days: [ForecastDay]) -> ForecastDay? {
        let now = Int64(Date().timeIntervalSince1970)
        return days.first { ($0.startUnixS..<($0.startUnixS + 86_400)).contains(now) } ?? days.first
    }

    /// "Today", or the weekday in short: seven of them fit side by side.
    static func name(of day: ForecastDay) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(day.startUnixS + 12 * 3600))
        if Calendar.current.isDateInToday(date) {
            return "Today"
        }
        return date.formatted(.dateTime.weekday(.abbreviated))
    }

    /// What the numbers are, and what they are not.
    static func caveat(for model: ModelForecast) -> String {
        var text = "What the weather model \(model.name) computes for the cell of its grid that holds this place"
        if !model.grid.isEmpty {
            text += " (\(model.grid))"
        }
        text += "."
        if let ground = model.cellElevationM {
            text += " The model's ground there is at \(Format.metres(ground)) m: its wind at 10 m is the wind 10 m above that, which in the mountains is not the wind at a take-off or in the valley."
        }
        text += " The cloud base is that of the model's convective cloud; an hour without one shows none. Nothing here says whether the day can be flown."
        return text
    }
}

/// The hours of one day as a table: a row for each height the wind is
/// given at, the highest at the top, then the sky. The row names stay in
/// place and the hours scroll.
struct ForecastTable: View {
    let model: ModelForecast
    let day: ForecastDay

    /// One row: its name, a line under the name, and what each hour holds.
    struct Row: Identifiable {
        enum Cell: Equatable {
            case wind(ForecastWind)
            case number(String)
            case cloud(percent: Double)
            /// The model has nothing for the hour.
            case empty
        }

        let id: String
        let name: String
        let detail: String
        let cells: [Cell]
    }

    private static let cellWidth: CGFloat = 34
    private static let rowHeight: CGFloat = 40
    private static let namesWidth: CGFloat = 88

    var body: some View {
        let hours = Self.hours(of: day)
        let rows = Self.rows(for: model, hours: hours)
        HStack(alignment: .top, spacing: 0) {
            // The names of the rows.
            VStack(alignment: .leading, spacing: 0) {
                Color.clear.frame(height: 22)
                ForEach(rows) { row in
                    VStack(alignment: .leading, spacing: 0) {
                        Text(row.name)
                            .font(.footnote.weight(.medium))
                        if !row.detail.isEmpty {
                            Text(row.detail)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .frame(height: Self.rowHeight, alignment: .leading)
                }
            }
            .frame(width: Self.namesWidth, alignment: .leading)

            ScrollView(.horizontal) {
                VStack(alignment: .leading, spacing: 0) {
                    HStack(spacing: 0) {
                        ForEach(hours, id: \.timeUnixS) { hour in
                            Text(Self.clock(hour.timeUnixS))
                                .font(.caption.weight(.medium))
                                .foregroundStyle(.secondary)
                                .frame(width: Self.cellWidth, height: 22)
                        }
                    }
                    ForEach(rows) { row in
                        HStack(spacing: 0) {
                            ForEach(Array(row.cells.enumerated()), id: \.offset) { _, cell in
                                ForecastCell(cell: cell)
                                    .frame(width: Self.cellWidth, height: Self.rowHeight)
                            }
                        }
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(row.detail.isEmpty ? row.name : "\(row.name), \(row.detail)")
                        .accessibilityValue(Self.spoken(row, hours: hours))
                    }
                }
                .padding(.trailing, 12)
            }
            .scrollIndicators(.hidden)
        }
        .monospacedDigit()
        .accessibilityIdentifier("forecast-table")
    }

    /// The hours worth a column: those with the sun up. A day without
    /// any, far in the north in winter, shows all it has.
    static func hours(of day: ForecastDay) -> [ForecastHourView] {
        let light = day.hours.filter(\.isDaylight)
        return light.isEmpty ? day.hours : light
    }

    /// The rows of the table for these hours: the wind from the highest
    /// level down to the ground, then cloud base, cloud, rain, temperature
    /// and the height of the 0 °C level, each only if the model has it for
    /// some hour.
    static func rows(for model: ModelForecast, hours: [ForecastHourView]) -> [Row] {
        var rows: [Row] = []
        for (index, level) in model.levels.enumerated().reversed() {
            let cells = hours.map { hour -> Row.Cell in
                guard index < hour.aloft.count, let wind = hour.aloft[index] else { return Row.Cell.empty }
                return Row.Cell.wind(wind)
            }
            guard cells.contains(where: { $0 != Row.Cell.empty }) else { continue }
            if let altitude = level.aboutAltitudeM {
                rows.append(Row(id: level.name, name: "\(Format.metres(altitude)) m", detail: level.name, cells: cells))
            } else {
                rows.append(Row(id: level.name, name: Self.short(level.name), detail: "above ground", cells: cells))
            }
        }
        rows.append(Row(
            id: "ground", name: "10 m", detail: "with gusts",
            cells: hours.map { hour -> Row.Cell in
                guard let wind = hour.wind else { return Row.Cell.empty }
                return Row.Cell.wind(wind)
            }
        ))
        let sky = [
            Row(id: "base", name: "Cloud base", detail: "m", cells: hours.map { hour -> Row.Cell in
                guard let base = hour.cloudBaseM else { return Row.Cell.empty }
                return Row.Cell.number(hundreds(base))
            }),
            Row(id: "cloud", name: "Cloud", detail: "% of sky", cells: hours.map { hour -> Row.Cell in
                guard let cloud = hour.cloudPct else { return Row.Cell.empty }
                return Row.Cell.cloud(percent: cloud)
            }),
            Row(id: "rain", name: "Rain", detail: "mm", cells: hours.map { hour -> Row.Cell in
                guard let rain = hour.precipitationMm else { return Row.Cell.empty }
                return Row.Cell.number(rain < 0.05 ? dry : rain.formatted(.number.precision(.fractionLength(1))))
            }),
            Row(id: "temperature", name: "Temperature", detail: "°C at 2 m", cells: hours.map { hour -> Row.Cell in
                guard let temperature = hour.temperatureC else { return Row.Cell.empty }
                return Row.Cell.number(Format.temperature(celsius: temperature))
            }),
            Row(id: "freezing", name: "0 °C", detail: "at m", cells: hours.map { hour -> Row.Cell in
                guard let level = hour.freezingLevelM else { return Row.Cell.empty }
                return Row.Cell.number(hundreds(level))
            }),
        ]
        for row in sky where row.cells.contains(where: { $0 != Row.Cell.empty }) {
            rows.append(row)
        }
        return rows
    }

    /// What an hour without rain shows: a dot, so that the hours with rain
    /// stand out.
    static let dry = "·"

    /// "80 m above ground" as a row's name: "80 m".
    private static func short(_ level: String) -> String {
        level.replacingOccurrences(of: " above ground", with: "")
    }

    /// A height to the nearest hundred metres, which is more than a model
    /// knows of a cloud base: "2,400".
    private static func hundreds(_ metres: Double) -> String {
        Format.metres((metres / 100).rounded() * 100)
    }

    /// The hour of the day on this iPhone's clock: "9", "14".
    static func clock(_ unixSeconds: Int64) -> String {
        let hour = Calendar.current.component(.hour, from: Date(timeIntervalSince1970: TimeInterval(unixSeconds)))
        return String(hour)
    }

    /// A row read out: every third hour, so that it can be listened to.
    private static func spoken(_ row: Row, hours: [ForecastHourView]) -> String {
        zip(hours, row.cells).enumerated()
            .filter { $0.offset % 3 == 0 }
            .compactMap { _, pair -> String? in
                let (hour, cell) = pair
                let at = "at \(clock(hour.timeUnixS))"
                switch cell {
                case .wind(let wind):
                    let from = wind.fromDeg.map { " from \(Format.compassPoint(degrees: $0))" } ?? ""
                    return "\(at) \(wind.label.replacingOccurrences(of: "/", with: " gusting "))\(from)"
                case .number(let text): return text == dry ? nil : "\(at) \(text)"
                case .cloud(let percent): return "\(at) \(Int(percent.rounded())) percent"
                case .empty: return nil
                }
            }
            .joined(separator: ", ")
    }
}

/// One hour of one row.
private struct ForecastCell: View {
    let cell: ForecastTable.Row.Cell

    var body: some View {
        switch cell {
        case .wind(let wind):
            VStack(spacing: 1) {
                if let from = wind.fromDeg {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 13, weight: .bold))
                        // Where the air moves, north at the top.
                        .rotationEffect(.degrees(from + 180))
                        .foregroundStyle(wind.band.ink)
                } else {
                    Image(systemName: "circle.fill")
                        .font(.system(size: 5))
                        .foregroundStyle(wind.band.ink)
                        .frame(height: 15)
                }
                Text(wind.label)
                    .font(.caption2.weight(.medium))
                    .minimumScaleFactor(0.7)
                    .lineLimit(1)
            }
        case .number(let text):
            Text(text)
                .font(.caption.weight(.medium))
                .minimumScaleFactor(0.6)
                .lineLimit(1)
        case .cloud(let percent):
            // More sky covered, more of the square filled in.
            RoundedRectangle(cornerRadius: 4)
                .fill(Color.primary.opacity(0.08 + 0.5 * min(max(percent, 0), 100) / 100))
                .frame(width: 22, height: 22)
                .overlay {
                    Text(Int(percent.rounded()).formatted())
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(percent > 55 ? Color(uiColor: .systemBackground) : Color.primary)
                }
        case .empty:
            Text("–")
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }
}
