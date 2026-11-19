import Charts
import SwiftUI
import VentiCore

/// The logbook as a whole: the last twelve months, the gliders flown, and
/// the pilot's best flights.
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

    var body: some View {
        // Worked out by the core from the logbook as it is now.
        let statistics = logbook.statistics()
        List {
            Section {
                Picker("Shown", selection: $measure) {
                    ForEach(Measure.allCases) { measure in
                        Text(measure.rawValue).tag(measure)
                    }
                }
                .pickerStyle(.segmented)
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
                best("Most points", statistics.mostPoints) { $0.formatted(.number.precision(.fractionLength(2))) }
                best("Highest", statistics.highest) { "\(Format.metres($0)) m" }
                best("Best climb", statistics.bestClimb) { "\(Format.climb(metresPerSecond: $0)) m/s" }
            }
        }
        .monospacedDigit()
        .navigationTitle("Statistics")
        .navigationBarTitleDisplayMode(.inline)
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
