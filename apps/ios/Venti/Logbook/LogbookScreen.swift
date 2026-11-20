import SwiftUI
import UniformTypeIdentifiers
import VentiCore

/// The pilot's flights, newest first, by year.
struct LogbookScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(AccountModel.self) private var account
    @Environment(AppNavigation.self) private var navigation
    @Environment(WindModel.self) private var wind

    /// Whose flights the tab shows: the pilot's own, what everyone on the
    /// server shares, or the ranking of the pilots who shared.
    private enum Whose: Hashable {
        case mine, everyone, ranking
    }

    @State private var whose = Whose.mine
    @State private var isChoosingFiles = false
    @State private var notice: ImportNotice?
    @State private var flightToDelete: LogbookEntry?

    var body: some View {
        @Bindable var navigation = navigation
        NavigationStack(path: $navigation.logbookPath) {
            Group {
                if whose == .everyone, wind.server != nil {
                    PublicFlightsList()
                } else if whose == .ranking, wind.server != nil {
                    RankingList()
                } else if logbook.view.years.isEmpty {
                    emptyState
                } else {
                    list
                }
            }
            .navigationTitle("Logbook")
            .toolbar {
                // With a Venti server set: the pilot's own flights, what
                // everyone there shares, or the ranking of its pilots.
                if wind.server != nil {
                    ToolbarItem(placement: .principal) {
                        Picker("Whose flights", selection: $whose) {
                            Text("Mine").tag(Whose.mine)
                            Text("Everyone").tag(Whose.everyone)
                            Text("Ranking").tag(Whose.ranking)
                        }
                        .pickerStyle(.segmented)
                        .frame(maxWidth: 280)
                    }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("Import flights", systemImage: "plus") {
                        isChoosingFiles = true
                    }
                    .disabled(logbook.importProgress != nil)
                }
            }
            .navigationDestination(for: PublicFlight.self) { flight in
                PublicFlightScreen(flight: flight)
            }
            .navigationDestination(for: FlightRoute.self) { route in
                FlightScreen(id: route.id)
            }
            .navigationDestination(for: StatisticsRoute.self) { _ in
                StatisticsScreen()
            }
        }
        .safeAreaInset(edge: .bottom) {
            if let progress = logbook.importProgress, progress.total > 1 {
                ImportProgressBar(progress: progress)
            }
        }
        // Flight files have no type the system knows, so any file can be
        // picked; the core says which ones are flights. A folder brings in
        // every flight file in it: a whole history at once.
        .fileImporter(
            isPresented: $isChoosingFiles,
            allowedContentTypes: [.data, .folder],
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
            Text(account.deletionWarning)
        }
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No flights yet", systemImage: "book.closed")
        } description: {
            Text("Import the IGC files your vario or flight app recorded, one by one or a whole folder of them. They stay on this iPhone.")
        } actions: {
            Button("Import flights") {
                isChoosingFiles = true
            }
            .buttonStyle(.venti(.primary))
            .controlSize(.large)

            Button("Add a sample flight") {
                Task {
                    await finish(logbook.addSample())
                }
            }
        }
    }

    private var list: some View {
        VentiList {
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
                            .tint(Color.venti(.danger))
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

/// How far an import of many files has come, over the bottom of the list.
private struct ImportProgressBar: View {
    let progress: LogbookModel.ImportProgress

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Importing \(Format.count(progress.done)) of \(Format.count(progress.total))")
                .font(.venti(.subheadline, .medium))
                .monospacedDigit()
            ProgressView(value: Double(progress.done), total: Double(max(progress.total, 1)))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .ventiFloating(in: .rect(cornerRadius: 18))
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Importing flights")
        .accessibilityValue("\(progress.done) of \(progress.total)")
        .accessibilityIdentifier("import-progress")
    }
}

/// All flights counted together, at the top of the list.
private struct TotalsRow: View {
    let totals: Totals

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Metric(caption: totals.flights == 1 ? "Flight" : "Flights", value: Format.count(totals.flights))
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
                .font(.venti(.headline))
                .foregroundStyle(.primary)
            Spacer()
            Text("\(Format.flights(year.totals.flights)) · \(Format.hoursMinutes(seconds: year.totals.airtimeS)) h")
                .font(.venti(.subheadline))
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
                    .font(.venti(.headline))
                Text(subtitle)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text("\(Format.hoursMinutes(seconds: entry.durationS)) h")
                    .font(.venti(.headline))
                // The scored cross-country distance; a triangle is marked.
                HStack(spacing: 3) {
                    if let score = entry.score, score.kind != .freeFlight {
                        Image(systemName: "triangle")
                            .font(.venti(.caption2, .semibold))
                            .accessibilityLabel(score.kind == .faiTriangle ? "FAI triangle" : "Triangle")
                    }
                    Text("\(Format.kilometres(metres: entry.score?.distanceM ?? entry.trackLengthM)) km")
                        .font(.venti(.subheadline))
                }
                .foregroundStyle(.secondary)
            }
            .monospacedDigit()
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }

    /// When it began, where if the pilot has a waypoint there, and on
    /// which glider.
    private var subtitle: String {
        var parts = [Format.time(unixSeconds: entry.startUnixS)]
        if let place = entry.takeoffName, !place.isEmpty {
            parts.append(place)
        }
        if let glider = entry.glider, !glider.isEmpty {
            parts.append(glider)
        }
        return parts.joined(separator: " · ")
    }
}
