import SwiftUI
import UniformTypeIdentifiers
import VentiCore

/// The pilot's flights, newest first, by day. What everyone shares, and
/// the ranking, are on the Community tab.
struct LogbookScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(AccountModel.self) private var account
    @Environment(AppNavigation.self) private var navigation
    @Environment(WatchedFolder.self) private var watched

    @State private var isChoosingFiles = false
    /// The picker for the folder new flights come in from by themselves.
    @State private var isChoosingWatched = false
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
                    importMenu
                }
            }
            // A watched folder that is lost: said over the flights, with
            // the two things to do about it.
            .safeAreaInset(edge: .top) {
                if watched.isLost, let name = watched.name {
                    lostFolder(name)
                }
            }
            // The folder to look into again whenever the app comes to the
            // front: what is in it now comes in at once. On a view of its
            // own, since two file pickers on one view do not both open.
            .fileImporter(isPresented: $isChoosingWatched, allowedContentTypes: [.folder]) { result in
                guard case .success(let folder) = result else { return }
                Task {
                    do {
                        if let report = try await watched.watch(folder) {
                            finish(report)
                        }
                    } catch {
                        notice = ImportNotice(title: WatchedFolder.Words.cannotBeKept, text: error.words)
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

    /// What the plus in the bar offers: files or a folder to bring in
    /// once, a folder to bring in from by itself from now on, and, while
    /// one is watched, which it is and the way to stop.
    private var importMenu: some View {
        VentiBarMenu(title: "Import flights", systemImage: "plus") {
            Button("Flight files or a folder", systemImage: "doc") {
                isChoosingFiles = true
            }
            .accessibilityIdentifier("import-files")
            Button(
                watched.name.map(WatchedFolder.Words.watchAnother) ?? WatchedFolder.Words.watch,
                systemImage: "arrow.clockwise"
            ) {
                isChoosingWatched = true
            }
            .accessibilityIdentifier("watch-folder")
            if let name = watched.name {
                Button(WatchedFolder.Words.stopWatching(name), systemImage: "xmark") {
                    watched.stop()
                }
                .accessibilityIdentifier("watch-folder-stop")
            }
        }
        .disabled(logbook.importProgress != nil)
        .accessibilityIdentifier("logbook-import")
    }

    /// The watched folder can no longer be found: the caution's words,
    /// and under them naming it again or giving it up.
    private func lostFolder(_ name: String) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            CardCaution(words: WatchedFolder.Words.lost(name))
                .accessibilityIdentifier("watched-folder-lost")
            Button {
                isChoosingWatched = true
            } label: {
                Text(WatchedFolder.Words.chooseAgain)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(.primary))
            .accessibilityIdentifier("watched-folder-again")
            Button {
                watched.stop()
            } label: {
                Text(WatchedFolder.Words.stopWatching(name))
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(.quiet))
            .accessibilityIdentifier("watched-folder-stop")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiCard()
        .padding(.horizontal, VentiSpace.s4)
        .padding(.bottom, VentiSpace.s2)
    }

    private var emptyState: some View {
        VentiEmpty(
            title: "No flights yet",
            text: "Import the IGC files your vario or flight app recorded, one by one or a whole folder of them. They stay on this iPhone."
        ) {
            Button {
                isChoosingFiles = true
            } label: {
                Text("Import flights")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(.primary))

            Button {
                Task {
                    await finish(logbook.addSample())
                }
            } label: {
                Text("Add a sample flight")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(.quiet))
        }
        // An import that has found no flight yet: its bar counts the files.
        .disabled(logbook.importProgress != nil)
    }

    /// The totals at the top, then the flights by day, newest first: each
    /// day a card of its own under its heading, the first day of a year
    /// with the year's count above it.
    private var list: some View {
        VentiList {
            Section {
                NavigationLink(value: StatisticsRoute()) {
                    TotalsRow(totals: logbook.view.totals)
                }
                .accessibilityIdentifier("logbook-totals")
            }
            ForEach(logbook.view.years, id: \.year) { year in
                let days = LogbookDay.days(of: year.flights)
                ForEach(days) { day in
                    Section {
                        ForEach(day.flights, id: \.id) { entry in
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
                        DayHeader(day: day, year: day.id == days.first?.id ? year : nil)
                    }
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

/// The flights of one day, by the iPhone's own clock, newest first.
struct LogbookDay: Identifiable, Equatable {
    /// The start of the day, seconds since 1970.
    let id: Int64
    let flights: [LogbookEntry]

    /// The day's time in the air.
    var airtimeS: Int64 { flights.reduce(0) { $0 + $1.durationS } }

    /// The flights, already newest first, in runs of one calendar day.
    static func days(of flights: [LogbookEntry], in calendar: Calendar = .current) -> [LogbookDay] {
        var days: [LogbookDay] = []
        for entry in flights {
            let start = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(entry.startUnixS)))
            let id = Int64(start.timeIntervalSince1970)
            if let last = days.last, last.id == id {
                days[days.count - 1] = LogbookDay(id: id, flights: last.flights + [entry])
            } else {
                days.append(LogbookDay(id: id, flights: [entry]))
            }
        }
        return days
    }
}

/// All flights counted together, at the top of the list, quieter than the
/// flights under it.
private struct TotalsRow: View {
    let totals: Totals

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s3) {
            Figure(caption: totals.flights == 1 ? "Flight" : "Flights", value: Format.count(totals.flights))
            Figure(caption: "Airtime", value: Format.hoursMinutes(seconds: totals.airtimeS), unit: "h")
            Figure(caption: "Distance", value: Format.wholeKilometres(metres: totals.distanceM), unit: "km")
        }
        .padding(.vertical, VentiSpace.s1)
    }

    /// One of the totals: its number with the unit, its name under it.
    private struct Figure: View {
        let caption: String
        let value: String
        var unit = ""

        var body: some View {
            VStack(alignment: .leading, spacing: 0) {
                Text("\(Text(value).font(.venti(.headline)))\(Text(unit.isEmpty ? "" : " \(unit)").font(.ventiType(.caption)).foregroundStyle(.venti(.textSecondary)))")
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(caption)
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(caption)
            .accessibilityValue(unit.isEmpty ? value : "\(value) \(unit)")
        }
    }
}

/// A day's heading: the day, and with several flights their count and
/// airtime. The first day of a year has the year above it, with the
/// year's count and airtime, small.
private struct DayHeader: View {
    let day: LogbookDay
    let year: LogbookYear?

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            if let year {
                HStack(alignment: .firstTextBaseline) {
                    Text(verbatim: String(year.year))
                        .font(.ventiType(.label))
                    Spacer()
                    Text("\(Format.flights(year.totals.flights)) · \(Format.hoursMinutes(seconds: year.totals.airtimeS)) h")
                        .font(.ventiType(.caption))
                        .monospacedDigit()
                }
                .foregroundStyle(.venti(.textSecondary))
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("logbook-year-\(year.year)")
            }
            HStack(alignment: .firstTextBaseline) {
                Text(Format.dayInYear(unixSeconds: day.id))
                    .font(.venti(.headline))
                    .foregroundStyle(.venti(.text))
                    .accessibilityAddTraits(.isHeader)
                Spacer()
                if day.flights.count > 1 {
                    Text("\(Format.flights(UInt32(day.flights.count))) · \(Format.hoursMinutes(seconds: day.airtimeS)) h")
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.textSecondary))
                        .monospacedDigit()
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("logbook-day")
        }
        .textCase(nil)
    }
}

/// One flight in the list, made to be told apart at a glance: the track's
/// shape on a little map of its own; the take-off's time first and large,
/// and the airtime large across from it; where it took off; the glider,
/// with the distance across from it.
private struct LogbookRow: View {
    let entry: LogbookEntry

    @Environment(\.dynamicTypeSize) private var typeSize
    /// The pack's icons beside the smaller lines, growing with the text.
    @ScaledMetric(relativeTo: .subheadline) private var sign = VentiSpace.s4

    var body: some View {
        // With the largest text sizes the shape stands above the words,
        // which then have the row's whole width.
        let stacked = typeSize.isAccessibilitySize
        let layout = stacked
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: VentiSpace.s2))
            : AnyLayout(HStackLayout(alignment: .center, spacing: VentiSpace.s3))
        layout {
            TrackThumbnail(outline: entry.outline, large: true)
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                    Text(Format.time(unixSeconds: entry.startUnixS))
                        .font(.ventiType(.instrumentM))
                        .accessibilityLabel("Took off at \(Format.time(unixSeconds: entry.startUnixS))")
                    Spacer(minLength: 0)
                    Text("\(Text(Format.hoursMinutes(seconds: entry.durationS)).font(.ventiType(.instrumentM)))\(Text(" h").font(.venti(.subheadline, .medium)).foregroundStyle(.venti(.textSecondary)))")
                        .accessibilityLabel("Airtime \(Format.hoursMinutes(seconds: entry.durationS)) h")
                }
                .lineLimit(1)
                HStack(spacing: VentiSpace.s1) {
                    icon("icon-takeoff")
                    Text(place)
                        .font(.venti(.body, .semibold))
                        .lineLimit(1)
                }
                .accessibilityElement(children: .combine)
                HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                    if let glider = entry.glider, !glider.isEmpty {
                        HStack(spacing: VentiSpace.s1) {
                            icon("icon-wing")
                            Text(glider)
                                .lineLimit(1)
                        }
                        .accessibilityElement(children: .combine)
                    }
                    Spacer(minLength: 0)
                    distance
                }
                .font(.venti(.subheadline))
                .foregroundStyle(.venti(.textSecondary))
            }
            .monospacedDigit()
        }
        .padding(.vertical, VentiSpace.s1)
        .accessibilityElement(children: .combine)
    }

    /// The scored cross-country distance; a triangle is marked.
    private var distance: some View {
        HStack(spacing: VentiSpace.s1) {
            if let score = entry.score, score.kind != .freeFlight {
                icon("icon-triangle", label: score.kind == .faiTriangle ? "FAI triangle" : "Triangle")
            }
            Text("\(Format.kilometres(metres: entry.score?.distanceM ?? entry.trackLengthM)) km")
        }
        .fixedSize()
    }

    /// Where it took off: the name the core gives (the pilot's waypoint
    /// there, or the server's site once synced), else where it is.
    private var place: String {
        if let name = entry.takeoffName, !name.isEmpty {
            return name
        }
        if let first = entry.outline.first {
            return Format.place(lat: first.lat, lon: first.lon)
        }
        return Format.dayInYear(unixSeconds: entry.startUnixS)
    }

    /// One of the pack's icons, in the quiet ink. Without a `label` it is
    /// decoration and VoiceOver passes it by.
    private func icon(_ name: String, label: String? = nil) -> some View {
        Image(name)
            .resizable()
            .frame(width: sign, height: sign)
            .foregroundStyle(.venti(.textMuted))
            .accessibilityLabel(label ?? "")
            .accessibilityHidden(label == nil)
    }
}
