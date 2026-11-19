import SwiftUI
import UniformTypeIdentifiers
import VentiCore

/// The pilot's flights, newest first, by year.
struct LogbookScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(AppNavigation.self) private var navigation

    @State private var isChoosingFiles = false
    @State private var notice: ImportNotice?
    @State private var flightToDelete: LogbookEntry?

    var body: some View {
        @Bindable var navigation = navigation
        NavigationStack(path: $navigation.logbookPath) {
            Group {
                if logbook.view.years.isEmpty {
                    emptyState
                } else {
                    list
                }
            }
            .navigationTitle("Logbook")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button("Import flights", systemImage: "plus") {
                        isChoosingFiles = true
                    }
                }
            }
            .navigationDestination(for: FlightRoute.self) { route in
                FlightScreen(id: route.id)
            }
            .navigationDestination(for: StatisticsRoute.self) { _ in
                StatisticsScreen()
            }
        }
        // Flight files have no type the system knows, so any file can be
        // picked; the core says which ones are flights.
        .fileImporter(
            isPresented: $isChoosingFiles,
            allowedContentTypes: [.data],
            allowsMultipleSelection: true
        ) { result in
            guard case .success(let urls) = result else { return }
            Task {
                await finish(logbook.importFiles(at: urls))
            }
        }
        .importNotice($notice)
        .confirmationDialog(
            "Delete this flight?",
            isPresented: Binding(
                get: { flightToDelete != nil },
                set: { isPresented in
                    if !isPresented {
                        flightToDelete = nil
                    }
                }
            ),
            titleVisibility: .visible,
            presenting: flightToDelete
        ) { entry in
            Button("Delete flight", role: .destructive) {
                delete(entry)
            }
        } message: { _ in
            Text("Its file is removed from this iPhone. This cannot be undone.")
        }
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No flights yet", systemImage: "book.closed")
        } description: {
            Text("Import the IGC files your vario or flight app recorded. They stay on this iPhone.")
        } actions: {
            Button("Import flights") {
                isChoosingFiles = true
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)

            Button("Add a sample flight") {
                Task {
                    await finish(logbook.addSample())
                }
            }
        }
    }

    private var list: some View {
        List {
            Section {
                NavigationLink(value: StatisticsRoute()) {
                    TotalsRow(totals: logbook.view.totals)
                }
                .accessibilityIdentifier("logbook-totals")
            }
            ForEach(logbook.view.years, id: \.year) { year in
                Section {
                    ForEach(year.flights, id: \.id) { entry in
                        NavigationLink(value: FlightRoute(id: entry.id)) {
                            LogbookRow(entry: entry)
                        }
                        .accessibilityIdentifier("logbook-row")
                        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                            // Not a destructive-role button: that removes the
                            // row at once, before the pilot has confirmed.
                            Button("Delete", systemImage: "trash") {
                                flightToDelete = entry
                            }
                            .tint(.red)
                        }
                    }
                } header: {
                    YearHeader(year: year)
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    /// After an import: open the flight if there was exactly one new one,
    /// and say so if something needs saying.
    private func finish(_ report: ImportReport) {
        notice = report.notice
        if report.failures.isEmpty, report.added.count == 1, let id = report.added.first {
            navigation.show(flight: id)
        }
    }

    private func delete(_ entry: LogbookEntry) {
        do {
            try withAnimation {
                try logbook.delete(id: entry.id)
            }
        } catch {
            notice = ImportNotice(title: "The flight could not be deleted", text: error.words)
        }
    }
}

/// All flights counted together, at the top of the list.
private struct TotalsRow: View {
    let totals: Totals

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Metric(caption: totals.flights == 1 ? "Flight" : "Flights", value: totals.flights.formatted())
            Metric(caption: "Airtime", value: Format.hoursMinutes(seconds: totals.airtimeS), unit: "h")
            Metric(caption: "Distance", value: Format.wholeKilometres(metres: totals.distanceM), unit: "km")
        }
        .padding(.vertical, 4)
    }
}

private struct YearHeader: View {
    let year: LogbookYear

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(verbatim: String(year.year))
                .font(.headline)
                .foregroundStyle(.primary)
            Spacer()
            Text("\(Format.flights(year.totals.flights)) · \(Format.hoursMinutes(seconds: year.totals.airtimeS)) h")
                .font(.subheadline)
                .monospacedDigit()
        }
        .textCase(nil)
    }
}

/// One flight in the list: its shape, when it was, and how long and far.
private struct LogbookRow: View {
    let entry: LogbookEntry

    var body: some View {
        HStack(spacing: 12) {
            TrackThumbnail(outline: entry.outline)
            VStack(alignment: .leading, spacing: 2) {
                Text(Format.dayInYear(unixSeconds: entry.startUnixS))
                    .font(.headline)
                Text(subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text("\(Format.hoursMinutes(seconds: entry.durationS)) h")
                    .font(.system(.headline, design: .rounded))
                // The scored cross-country distance; a triangle is marked.
                HStack(spacing: 3) {
                    if let score = entry.score, score.kind != .freeFlight {
                        Image(systemName: "triangle")
                            .font(.caption2.weight(.semibold))
                            .accessibilityLabel(score.kind == .faiTriangle ? "FAI triangle" : "Triangle")
                    }
                    Text("\(Format.kilometres(metres: entry.score?.distanceM ?? entry.trackLengthM)) km")
                        .font(.system(.subheadline, design: .rounded))
                }
                .foregroundStyle(.secondary)
            }
            .monospacedDigit()
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }

    private var subtitle: String {
        let time = Format.time(unixSeconds: entry.startUnixS)
        guard let glider = entry.glider, !glider.isEmpty else { return time }
        return "\(time) · \(glider)"
    }
}
