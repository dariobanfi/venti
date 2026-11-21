import Charts
import SwiftUI
import VentiCore

/// The logbook as a whole: the last twelve months, one year beside
/// another, when in the year the pilot flies, what a flight is usually
/// like, the kinds of flight, the places flown from, the gliders flown,
/// the pilot's bests, and the way to how the pilot flies.
/// Every figure is the core's.
struct StatisticsScreen: View {
    @Environment(LogbookModel.self) private var logbook

    private enum Measure: String, CaseIterable, Identifiable {
        case airtime = "Airtime"
        case distance = "Distance"
        case flights = "Flights"

        var id: Self { self }

        var unit: String {
            switch self {
            case .airtime: "hours"
            case .distance: "kilometres"
            case .flights: "flights"
            }
        }
    }

    @State private var measure = Measure.airtime
    /// Whether every place flown from is listed, not only the first ones.
    @State private var showsAllPlaces = false

    /// How many places are listed until the pilot asks for all.
    static let placesShown = 8

    var body: some View {
        // Worked out by the core from the logbook as it is now.
        let statistics = logbook.statistics()
        let places = logbook.siteStatistics()
        VentiList {
            Section {
                VentiSegmented(
                    options: Measure.allCases.map { (value: $0, name: $0.rawValue) },
                    selection: $measure
                )
                .listRowSeparator(.hidden)

                Chart(statistics.months, id: \.self) { month in
                    BarMark(
                        x: .value("Month", Self.date(of: month), unit: .month),
                        y: .value(measure.rawValue, value(of: month))
                    )
                    .foregroundStyle(Palette.accent)
                    .clipShape(.rect(cornerRadius: 3))
                }
                .chartXAxis {
                    AxisMarks(values: .stride(by: .month)) { _ in
                        AxisValueLabel(format: .dateTime.month(.narrow), centered: true)
                    }
                }
                .frame(height: 170)
                .padding(.vertical, 4)
                .accessibilityLabel("\(measure.rawValue) by month, in \(measure.unit)")
            } header: {
                Text("Last twelve months")
            } footer: {
                Text("In \(measure.unit).")
            }

            if !statistics.years.isEmpty {
                Section("Years") {
                    ForEach(statistics.years, id: \.year) { year in
                        row(
                            String(year.year),
                            value: "\(Format.hoursMinutes(seconds: year.airtimeS)) h",
                            words: Self.words(of: year)
                        )
                        .accessibilityIdentifier("statistics-year-\(year.year)")
                    }
                }
            }

            if !statistics.seasons.isEmpty {
                Section {
                    Chart(statistics.seasons, id: \.month) { season in
                        BarMark(
                            x: .value("Month", Self.letter(ofMonth: season.month)),
                            y: .value(measure.rawValue, value(of: season))
                        )
                        .foregroundStyle(Palette.accent)
                        .clipShape(.rect(cornerRadius: 3))
                    }
                    .chartXScale(domain: (1...12).map { Self.letter(ofMonth: UInt32($0)) })
                    // The months' letters and no line up from each, as
                    // in the chart above.
                    .chartXAxis {
                        AxisMarks { _ in
                            AxisValueLabel()
                        }
                    }
                    .frame(height: 140)
                    .padding(.vertical, 4)
                    .accessibilityLabel("\(measure.rawValue) by month of the year, in \(measure.unit)")
                    .accessibilityIdentifier("statistics-seasons")
                } header: {
                    Text("Through the year")
                } footer: {
                    Text("January to December, all years together, in \(measure.unit).")
                }
            }

            if let typical = statistics.typical {
                Section {
                    LabeledContent("Airtime", value: "\(Format.hoursMinutes(seconds: typical.airtimeS)) h")
                    if let distance = typical.distanceM {
                        LabeledContent("Distance", value: "\(Format.kilometres(metres: distance)) km")
                    }
                    LabeledContent("Highest", value: "\(Format.metres(Double(typical.highestM))) m")
                    LabeledContent("Best climb", value: "\(Format.climb(metresPerSecond: typical.bestClimbMs)) m/s")
                } header: {
                    Text("A typical flight")
                } footer: {
                    Text("The middle one of all your flights for each figure, so that one long day does not speak for the rest.")
                }
            }

            if !statistics.routes.isEmpty {
                Section("Kinds of flight") {
                    ForEach(statistics.routes, id: \.kind) { route in
                        if let farthest = route.farthest {
                            NavigationLink(value: FlightRoute(id: farthest.flightId)) {
                                row(
                                    Self.name(of: route.kind),
                                    value: "\(Format.kilometres(metres: route.distanceM)) km",
                                    words: Self.words(of: route, farthest: farthest)
                                )
                            }
                        }
                    }
                }
            }

            if !places.isEmpty {
                Section("By take-off") {
                    let shown = showsAllPlaces ? places : Array(places.prefix(Self.placesShown))
                    ForEach(Array(shown.enumerated()), id: \.offset) { index, place in
                        placeRow(place)
                            .accessibilityIdentifier(index == 0 ? "statistics-by-take-off" : "")
                    }
                    if !showsAllPlaces, places.count > Self.placesShown {
                        Button("All \(places.count) take-offs") {
                            showsAllPlaces = true
                        }
                    }
                }
            }

            if !statistics.gliders.isEmpty {
                Section("Gliders") {
                    ForEach(statistics.gliders, id: \.glider) { glider in
                        LabeledContent {
                            Text("\(Format.flights(glider.flights)) · \(Format.hoursMinutes(seconds: glider.airtimeS)) h")
                        } label: {
                            Text(glider.glider ?? "Not named")
                        }
                    }
                }
            }

            Section("Your bests") {
                best("Longest flight", statistics.longest) { "\(Format.hoursMinutes(seconds: Int64($0))) h" }
                best("Farthest", statistics.farthest) { "\(Format.kilometres(metres: $0)) km" }
                best("Most points", statistics.mostPoints) { Format.decimal($0, places: 2) }
                best("Highest", statistics.highest) { "\(Format.metres($0)) m" }
                best("Best climb", statistics.bestClimb) { "\(Format.climb(metresPerSecond: $0)) m/s" }
            }

            // What the pilot's own tracks say about how they fly.
            Section {
                NavigationLink {
                    HowYouFlyScreen()
                } label: {
                    Text("How you fly")
                }
                .accessibilityIdentifier("statistics-how-you-fly")
            }
        }
        .monospacedDigit()
        .navigationTitle("Statistics")
        .navigationBarTitleDisplayMode(.inline)
    }

    /// One place flown from: its name or where it is, how many flights,
    /// and leading to the farthest flight from it where one was scored.
    @ViewBuilder
    private func placeRow(_ place: SiteStatistics) -> some View {
        let title = place.name ?? Format.place(lat: place.lat, lon: place.lon)
        let value = Format.flights(place.flights)
        if let farthest = place.farthest {
            NavigationLink(value: FlightRoute(id: farthest.flightId)) {
                row(title, value: value, words: Self.words(of: place))
            }
        } else {
            row(title, value: value, words: Self.words(of: place))
        }
    }

    /// One best, leading to its flight.
    @ViewBuilder
    private func best(_ title: String, _ best: BestFlight?, text: (Double) -> String) -> some View {
        if let best {
            NavigationLink(value: FlightRoute(id: best.flightId)) {
                LabeledContent {
                    Text(text(best.value))
                } label: {
                    Text(title)
                    Text(Format.day(unixSeconds: best.startUnixS))
                }
            }
        }
    }

    /// A row whose second line is too long to stand beside a figure: the
    /// name and the figure on one line, the words under them on as many
    /// lines as they need.
    private func row(_ title: String, value: String, words: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                Spacer(minLength: VentiSpace.s2)
                Text(value)
                    .foregroundStyle(.venti(.textSecondary))
            }
            Text(words)
                .font(.venti(.subheadline))
                .foregroundStyle(.venti(.textSecondary))
        }
        .accessibilityElement(children: .combine)
    }

    private func value(of season: SeasonStatistics) -> Double {
        switch measure {
        case .airtime: Double(season.airtimeS) / 3600
        case .distance: season.distanceM / 1000
        case .flights: Double(season.flights)
        }
    }

    /// "12 flights · 320 km · longest 3:34 h · highest 2626 m"
    private static func words(of year: YearStatistics) -> String {
        var parts = [Format.flights(year.flights)]
        if year.distanceM > 0 {
            parts.append("\(Format.kilometres(metres: year.distanceM)) km")
        }
        parts.append("longest \(Format.hoursMinutes(seconds: year.longestS)) h")
        parts.append("highest \(Format.metres(Double(year.highestM))) m")
        return parts.joined(separator: " · ")
    }

    /// "12 flights · 3 closed · farthest 44.3 km on Sun, Aug 31, 2025":
    /// how many, how many of them came back to where they began if any
    /// did, and the farthest, which the row leads to.
    private static func words(of route: RouteStatistics, farthest: BestFlight) -> String {
        var parts = [Format.flights(route.flights)]
        if route.closed > 0 {
            parts.append("\(route.closed) closed")
        }
        parts.append("farthest \(Format.kilometres(metres: farthest.value)) km on \(Format.day(unixSeconds: farthest.startUnixS))")
        return parts.joined(separator: " · ")
    }

    /// "5:48 h · farthest 44.3 km on Sun, Aug 31, 2025": the airtime from
    /// a place, and its farthest flight where one was scored.
    static func words(of place: SiteStatistics) -> String {
        var parts = ["\(Format.hoursMinutes(seconds: place.airtimeS)) h"]
        if let farthest = place.farthest {
            parts.append("farthest \(Format.kilometres(metres: farthest.value)) km on \(Format.day(unixSeconds: farthest.startUnixS))")
        }
        return parts.joined(separator: " · ")
    }

    private static func name(of kind: RouteKind) -> String {
        switch kind {
        case .freeFlight: "Free flight"
        case .freeTriangle: "Triangle"
        case .faiTriangle: "FAI triangle"
        }
    }

    /// A month of the year as the chart's axis writes it: its place and
    /// its first letter, so that the two Js and the two Ms stay apart.
    private static func letter(ofMonth month: UInt32) -> String {
        let letters = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"]
        let index = Int(month) - 1
        guard letters.indices.contains(index) else { return "" }
        // A zero-width mark that differs per month keeps equal letters
        // from being one bar.
        return letters[index] + String(repeating: "\u{200B}", count: index)
    }

    private func value(of month: MonthStatistics) -> Double {
        switch measure {
        case .airtime: Double(month.airtimeS) / 3600
        case .distance: month.distanceM / 1000
        case .flights: Double(month.flights)
        }
    }

    /// The first day of a month, in UTC like the core's months.
    private static func date(of month: MonthStatistics) -> Date {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .gmt
        let components = DateComponents(year: Int(month.year), month: Int(month.month), day: 1, hour: 12)
        return calendar.date(from: components) ?? .distantPast
    }
}
