import SwiftUI
import VentiCore

/// Everyone's public flights on the pilot's Venti server: the newest
/// first, or the best. A pilot is found by name on the Community page
/// (`CommunityHub`).
struct PublicFlightsList: View {
    @Environment(CommunityModel.self) private var community

    /// The order shown; choosing the other asks the server for it.
    private var order: Binding<PublicFlightsOrder> {
        Binding {
            community.order
        } set: { order in
            Task {
                await community.show(order)
            }
        }
    }

    var body: some View {
        VentiList {
            flights
        }
        .listStyle(.insetGrouped)
        .overlay {
            if community.flights.isEmpty, !community.isLoading {
                if let problem = community.problem {
                    // Something went wrong: said plainly, without art.
                    ContentUnavailableView {
                        Label {
                            Text(community.search.isSearching ? "The flights could not be listed" : "No public flights")
                        } icon: {
                            Image("icon-caution")
                        }
                    } description: {
                        Text(problem)
                    }
                } else if community.search.isSearching {
                    // Under the order and the filter, which stay in reach.
                    ContentUnavailableView(
                        "No flights match",
                        systemImage: SitesFilterIcon.name,
                        description: Text("Nobody shares a flight that matches these choices. Remove one, or widen the area.")
                    )
                    .padding(.top, VentiSpace.s8)
                    .accessibilityIdentifier("public-flights-none-match")
                } else {
                    VentiEmpty(
                        title: "No public flights yet",
                        text: "Flights that pilots share with everyone on Venti are listed here."
                    )
                }
            }
        }
        .refreshable {
            await community.refresh()
        }
        .task {
            await community.loadIfNeeded()
        }
    }

    @ViewBuilder private var flights: some View {
        Section {
            HStack(spacing: VentiSpace.s3) {
                VentiSegmented(
                    options: [
                        (value: PublicFlightsOrder.newest, name: "Newest"),
                        (value: PublicFlightsOrder.best, name: "Best"),
                    ],
                    selection: order
                )
                FlightSearchButton(search: community.search) { search in
                    Task {
                        await community.find(search)
                    }
                }
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())
            if community.search.isSearching {
                FlightSearchLine(search: community.search)
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: VentiSpace.s2, leading: VentiSpace.s1, bottom: 0, trailing: 0))
                    .listRowSeparator(.hidden)
            }
        }
        Section {
            ForEach(community.flights, id: \.id) { flight in
                NavigationLink(value: flight) {
                    PublicFlightRow(flight: flight)
                }
                .accessibilityIdentifier("public-flight-row")
                .onAppear {
                    // The last row in view: fetch what follows it.
                    if flight.id == community.flights.last?.id {
                        Task {
                            await community.loadMore()
                        }
                    }
                }
            }
            if community.isLoading {
                ProgressView()
                    .frame(maxWidth: .infinity)
            }
        } footer: {
            if !community.flights.isEmpty {
                Text("Flights their pilots share with everyone on Venti. Distances and points are Venti's own reckoning by the World XContest's rules, not a league's.")
            }
        }
    }
}

/// One public flight in the list: its shape, who flew it with their face
/// and when, and how far.
struct PublicFlightRow: View {
    let flight: PublicFlight
    /// A line under the date, in the column of the words: in the feed,
    /// who cheered and how many wrote. Nothing where empty.
    var note = ""

    var body: some View {
        HStack(spacing: 12) {
            TrackThumbnail(outline: flight.outline)
            VStack(alignment: .leading, spacing: 2) {
                // The pilot's face and their country by their name, once
                // they set one.
                HStack(spacing: VentiSpace.s1) {
                    PilotFace(name: flight.pilot, picturePath: Self.picturePath(of: flight))
                        .padding(.trailing, VentiSpace.s1)
                    Text(flight.pilot)
                        .lineLimit(1)
                    Flag(code: flight.pilotCountry)
                }
                .font(.venti(.headline))
                // The day and the place, and the glider on a line of its
                // own after its class, so that it is not cut off behind a
                // long place ("Sample Wi…").
                VStack(alignment: .leading, spacing: 0) {
                    Text(
                        [Format.day(unixSeconds: flight.startUnixS, zone: flight.timeZone), Self.place(of: flight)]
                            .compactMap { $0 }
                            .joined(separator: " · ")
                    )
                    .lineLimit(2)
                    if let glider = Self.glider(of: flight) {
                        Text(glider)
                            .lineLimit(1)
                    }
                }
                .font(.venti(.subheadline))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(Self.spokenDetail(of: flight))
                if !note.isEmpty {
                    Text(note)
                        .font(.venti(.footnote))
                        .foregroundStyle(.venti(.textSecondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                HStack(spacing: 3) {
                    if let score = flight.score, score.kind != .freeFlight {
                        Image(systemName: "triangle")
                            .font(.venti(.caption, .semibold))
                            .accessibilityLabel(score.kind == .faiTriangle ? "FAI triangle" : "Triangle")
                    }
                    Text("\(Format.kilometres(metres: flight.score?.distanceM ?? flight.trackLengthM)) km")
                        .font(.venti(.headline))
                }
                Text("\(Format.hoursMinutes(seconds: flight.airtimeS)) h")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
            }
            .monospacedDigit()
        }
        .accessibilityElement(children: .combine)
    }

    /// "15 Aug 2026 · 🇦🇹 Wallberg · EN B · Sample Wing": the day, the
    /// site the server says the flight took off from with its country's
    /// flag, and the glider after its class. Without the weekday.
    static func detail(of flight: PublicFlight) -> String {
        [Format.day(unixSeconds: flight.startUnixS, zone: flight.timeZone), place(of: flight), glider(of: flight)]
            .compactMap { $0 }
            .joined(separator: " · ")
    }

    /// The detail as VoiceOver reads it: the take-off's country by its
    /// name, after the take-off.
    static func spokenDetail(of flight: PublicFlight) -> String {
        [
            Format.day(unixSeconds: flight.startUnixS, zone: flight.timeZone), flight.takeoffName,
            flight.takeoffCountry.map(Countries.name), glider(of: flight),
        ]
        .compactMap { $0 }
        .filter { !$0.isEmpty }
        .joined(separator: ", ")
    }

    /// The take-off, with its country's flag before it; the flag alone
    /// where the site has no name.
    static func place(of flight: PublicFlight) -> String? {
        let place = [Countries.flag(flight.takeoffCountry), flight.takeoffName]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        return place.isEmpty ? nil : place
    }

    /// The glider's class in the core's name before the glider: "EN B ·
    /// Sample Wing", so that the class reads where a long name is cut;
    /// either alone.
    static func glider(of flight: PublicFlight) -> String? {
        let glider = [flight.gliderClass.map { gliderClassName(class: $0) }, flight.glider]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
        return glider.isEmpty ? nil : glider
    }

    /// The path of the pilot's picture on the server, where they have one.
    static func picturePath(of flight: PublicFlight) -> String? {
        flight.pilotPicture
    }
}

/// Another pilot's flight, fetched from the server and shown like one of
/// the pilot's own: the track on the map, coloured by climb, with its
/// numbers, its altitude profile and its thermals. Nothing is kept.
struct PublicFlightScreen: View {
    let flight: PublicFlight

    @Environment(CommunityModel.self) private var community
    @Environment(AirspaceModel.self) private var airspace
    /// The height up to which the maps draw airspace, as chosen on the map.
    @AppStorage(Preferences.mapAirspaceCeilingM) private var airspaceCeilingM = 0.0
    @Environment(WindModel.self) private var wind

    private enum Phase {
        case loading
        case loaded(FlightView, FlightAnalysis?)
        case failed(String)
    }

    @State private var phase = Phase.loading
    @State private var showsPilot = false
    @State private var reporting: Reported?

    var body: some View {
        Group {
            switch phase {
            case .loading:
                ProgressView()
            case .failed(let message):
                ContentUnavailableView(
                    "This flight could not be opened",
                    systemImage: "exclamationmark.triangle",
                    description: Text(message)
                )
            case .loaded(let view, let analysis):
                FlightDetail(
                    flight: view, analysis: analysis, airspaceCheck: nil,
                    airspace: airspace.shapes(in: view.bounds.grown(by: 0.2), below: airspaceCeilingM)
                )
                // Under the head: who cheered, the cheer, and the way to
                // the comments, between the map's two upper corners.
                .overlay(alignment: .top) {
                    FlightSocialBar(flight: flight)
                        .padding(.horizontal, VentiSize.touch + VentiSpace.s5)
                }
            }
        }
        .navigationTitle(flight.pilot)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.hidden, for: .tabBar)
        .toolbar {
            // The name stands over a map, among the map's own names: it
            // gets ground of its own to be read on, as the buttons beside
            // it have.
            ToolbarItem(placement: .principal) {
                // The pilot with their country's flag, and under them the
                // take-off with its flag and the glider with its class,
                // where the server names them.
                let line = [PublicFlightRow.place(of: flight), PublicFlightRow.glider(of: flight)]
                    .compactMap { $0 }
                    .joined(separator: " · ")
                VStack(spacing: 0) {
                    HStack(spacing: VentiSpace.s1) {
                        PilotFace(name: flight.pilot, picturePath: PublicFlightRow.picturePath(of: flight))
                            .padding(.trailing, VentiSpace.s1)
                        Text(flight.pilot)
                            .lineLimit(1)
                            .accessibilityIdentifier("public-flight-pilot")
                            .accessibilityAddTraits(.isHeader)
                        Flag(code: flight.pilotCountry)
                            .accessibilityIdentifier("public-flight-pilot-country")
                    }
                    .font(.venti(.headline))
                    if !line.isEmpty {
                        Text(line)
                            .font(.venti(.caption))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .accessibilityLabel(
                                [flight.takeoffName, flight.takeoffCountry.map(Countries.name), PublicFlightRow.glider(of: flight)]
                                    .compactMap { $0 }
                                    .filter { !$0.isEmpty }
                                    .joined(separator: ", ")
                            )
                            .accessibilityIdentifier("public-flight-line")
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, line.isEmpty ? 8 : 4)
                .ventiFloating(in: .capsule)
            }
            ToolbarItem(placement: .primaryAction) {
                VentiBarMenu(title: "More") {
                    // Everything else this pilot shares with everyone.
                    Button("Flights of \(flight.pilot)", systemImage: "person") {
                        showsPilot = true
                    }
                    // The flight's page on the server, to hand on.
                    if let server = wind.server, let page = URL(string: server.absoluteString + flight.pagePath) {
                        ShareLink(item: page) {
                            Label("Share the flight's page", systemImage: "square.and.arrow.up")
                        }
                    }
                    // For what should not be here: to whoever runs the
                    // server, and to nobody else.
                    Button("Report this flight", systemImage: "exclamationmark.bubble") {
                        reporting = .flight(flight.id)
                    }
                }
            }
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        // Wherever this page stands, in the logbook's list or on a site's
        // page over the map, the pilot's flights follow it there.
        .navigationDestination(isPresented: $showsPilot) {
            PilotFlightsScreen(pilot: PilotRoute(handle: flight.handle, name: flight.pilot))
        }
        .task(id: flight.id) {
            do {
                let (view, analysis) = try await community.open(flight)
                phase = .loaded(view, analysis)
            } catch {
                phase = .failed(CommunityModel.words(for: error))
            }
        }
    }
}

/// The head of a pilot's page: their face, large, with the name and the
/// handle beside it.
private struct PilotHead: View {
    let name: String
    let handle: String
    let picturePath: String?

    var body: some View {
        Section {
            HStack(spacing: VentiSpace.s4) {
                PilotFace(name: name, picturePath: picturePath, size: VentiSpace.s8, isOwnElement: true)
                    .accessibilityElement()
                    .accessibilityLabel(name)
                    .accessibilityValue(picturePath == nil ? "Initials" : "Picture")
                    .accessibilityIdentifier("pilot-picture")
                VStack(alignment: .leading, spacing: 2) {
                    Text(name)
                        .font(.ventiType(.heading))
                        .accessibilityAddTraits(.isHeader)
                    Text(verbatim: "@\(handle)")
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.textSecondary))
                }
            }
            .padding(.vertical, VentiSpace.s1)
        }
    }
}

/// A pilot, as a step in a navigation stack: to their public flights.
struct PilotRoute: Hashable {
    let handle: String
    let name: String
}

/// One pilot's page: what their public flights add up to, by the core,
/// and under that the flights, the newest first.
struct PilotFlightsScreen: View {
    let pilot: PilotRoute

    @Environment(CommunityModel.self) private var community

    @State private var flights: [PublicFlight] = []
    @State private var next: String?
    @State private var isLoading = true
    @State private var problem: String?
    @State private var reporting: Reported?
    /// What the pilot's public flights add up to, once Venti has said.
    @State private var statistics: PilotStatistics?
    /// The year of the season whose totals are shown; 0 is all years.
    @State private var year: Int32 = 0
    /// Who follows the pilot and whom they follow, once Venti has said.
    @State private var follows: FollowList?
    /// What the pilot's flights are searched by: everything, until chosen.
    @State private var search = FlightSearch.everything

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    var body: some View {
        VentiList {
            PilotHead(
                name: statistics?.name ?? flights.first?.pilot ?? pilot.name, handle: pilot.handle,
                picturePath: statistics?.picturePath
            )
            FollowSection(handle: pilot.handle, follows: $follows)
            if let statistics, !flights.isEmpty || year != 0 || search.isSearching {
                PilotTotalsSections(statistics: statistics, year: $year)
            }
            Section {
                if flights.isEmpty, !isLoading, search.isSearching {
                    Text(problem ?? "Nobody shares a flight that matches these choices. Remove one, or widen the area.")
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.textSecondary))
                        .accessibilityIdentifier("pilot-flights-none-match")
                }
                ForEach(flights, id: \.id) { flight in
                    NavigationLink {
                        PublicFlightScreen(flight: flight)
                    } label: {
                        PublicFlightRow(flight: flight)
                    }
                    .accessibilityIdentifier("pilot-flight-row")
                    .onAppear {
                        if flight.id == flights.last?.id, next != nil, !isLoading {
                            Task {
                                await load()
                            }
                        }
                    }
                }
                if isLoading {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            } header: {
                // The search, out of the way, over the flights it narrows.
                if !flights.isEmpty || search.isSearching {
                    VStack(alignment: .trailing, spacing: 0) {
                        FlightSearchButton(search: search) { chosen in
                            search = chosen
                        }
                        FlightSearchLine(search: search)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .textCase(nil)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                }
            }
        }
        .listStyle(.insetGrouped)
        .overlay {
            if flights.isEmpty, !isLoading, year == 0, !search.isSearching {
                ContentUnavailableView(
                    "No public flights",
                    systemImage: "person",
                    description: Text(problem ?? "This pilot shares no flight with everyone at the moment.")
                )
                // Under the pilot's head, the counts and the follow
                // button, which a pilot who shares nothing has too.
                .padding(.top, VentiSpace.s8 + VentiSpace.s5 + (follows == nil ? 0 : VentiSize.touch * 2))
            }
        }
        // A page opened by the handle alone has the name once a flight
        // is read.
        .navigationTitle(flights.first?.pilot ?? pilot.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                VentiBarMenu(title: "More") {
                    Button("Report this pilot", systemImage: "exclamationmark.bubble") {
                        reporting = .pilot(pilot.handle)
                    }
                    // A picture that should not be there, on its own.
                    if statistics?.picturePath != nil {
                        Button(pictureWords().report, systemImage: "person.crop.circle.badge.exclamationmark") {
                            reporting = .picture(pilot.handle)
                        }
                    }
                }
            }
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        // A season narrows the flights under the totals to its own;
        // all years are the pilot's page again.
        .onChange(of: year) {
            flights = []
            next = nil
            Task {
                await load()
            }
        }
        .onChange(of: search) {
            flights = []
            next = nil
            Task {
                await load()
            }
        }
        .task(id: pilot.handle) {
            flights = []
            next = nil
            statistics = nil
            year = 0
            follows = nil
            async let counts = try? Social(account: account, wind: wind).follows(pilot.handle)
            // The totals beside the flights: the page is the flights'
            // where Venti does not add them up.
            async let totals = try? totals()
            await load()
            statistics = await totals
            follows = await counts
        }
    }

    /// What the pilot's flights add up to. On the pilot's own page all
    /// of their flights, the private ones too, with the core's note that
    /// says so (`ownStatistics`); on another's the public ones.
    private func totals() async throws -> PilotStatistics {
        let social = Social(account: account, wind: wind)
        guard social.isSignedIn, social.ownHandle == pilot.handle else {
            return try await community.statistics(of: pilot.handle)
        }
        let answer = try await account.answer(to: ownStatisticsRequest())
        return try ownStatistics(status: answer.status, body: answer.body)
    }

    /// The first page, or the one after what is listed.
    private func load() async {
        isLoading = true
        defer { isLoading = false }
        // The season chosen, where it is one: its bounds are the core's.
        let asked = year
        let searched = search
        let season = statistics?.seasons.first { $0.year == asked }
        do {
            let after = flights.isEmpty ? nil : next
            let page: (flights: [PublicFlight], next: String?)
            if season != nil || searched.isSearching {
                let found = try await community.pilot(
                    pilot.handle, fromUnixS: season?.fromUnixS, toUnixS: season?.toUnixS, search: searched, after: after
                )
                page = (found.flights, found.next)
            } else {
                let found = try await community.pilot(pilot.handle, after: after)
                page = (found.flights, found.next)
            }
            // Another season or search was chosen while this was under way.
            guard asked == year, searched == search else { return }
            flights += page.flights
            next = page.next
            problem = nil
        } catch {
            problem = error is CoreError ? error.words : pilotsWords().unreachable
        }
    }
}
