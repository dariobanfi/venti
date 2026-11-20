import SwiftUI
import VentiCore

/// Everyone's public flights on the pilot's Venti server: the newest
/// first, or the best.
struct PublicFlightsList: View {
    @Environment(CommunityModel.self) private var community

    var body: some View {
        VentiList {
            Section {
                Picker(
                    "Order",
                    selection: Binding(
                        get: { community.order },
                        set: { order in
                            Task {
                                await community.show(order)
                            }
                        }
                    )
                ) {
                    Text("Newest").tag(PublicFlightsOrder.newest)
                    Text("Best").tag(PublicFlightsOrder.best)
                }
                .pickerStyle(.segmented)
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
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
        .listStyle(.insetGrouped)
        .overlay {
            if community.flights.isEmpty, !community.isLoading {
                if let problem = community.problem {
                    // Something went wrong: said plainly, without art.
                    ContentUnavailableView {
                        Label {
                            Text("No public flights")
                        } icon: {
                            Image("icon-caution")
                        }
                    } description: {
                        Text(problem)
                    }
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
}

/// One public flight in the list: its shape, who flew it and when, and
/// how far.
struct PublicFlightRow: View {
    let flight: PublicFlight

    var body: some View {
        HStack(spacing: 12) {
            TrackThumbnail(outline: flight.outline)
            VStack(alignment: .leading, spacing: 2) {
                Text(flight.pilot)
                    .font(.venti(.headline))
                    .lineLimit(1)
                Text(Self.detail(of: flight))
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
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

    /// "15 Aug 2026 · Sample Wing". Without the weekday: the glider's name
    /// is the part that would be cut off.
    static func detail(of flight: PublicFlight) -> String {
        let day = Format.dayShort(unixSeconds: flight.startUnixS)
        return flight.glider.map { "\(day) · \($0)" } ?? day
    }
}

/// Another pilot's flight, fetched from the server and shown like one of
/// the pilot's own: the track on the map, coloured by climb, with its
/// numbers, its altitude profile and its thermals. Nothing is kept.
struct PublicFlightScreen: View {
    let flight: PublicFlight

    @Environment(CommunityModel.self) private var community
    @Environment(AirspaceModel.self) private var airspace
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
                    airspace: airspace.shapes(in: view.bounds.grown(by: 0.2))
                )
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
                Text(flight.pilot)
                    .font(.venti(.headline))
                    .lineLimit(1)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .ventiFloating(in: .capsule)
                    .accessibilityAddTraits(.isHeader)
            }
            ToolbarItem(placement: .primaryAction) {
                Menu("More", systemImage: "ellipsis") {
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

/// A pilot, as a step in a navigation stack: to their public flights.
struct PilotRoute: Hashable {
    let handle: String
    let name: String
}

/// One pilot's public flights, the newest first.
struct PilotFlightsScreen: View {
    let pilot: PilotRoute

    @Environment(CommunityModel.self) private var community

    @State private var flights: [PublicFlight] = []
    @State private var next: String?
    @State private var isLoading = true
    @State private var problem: String?
    @State private var reporting: Reported?

    var body: some View {
        VentiList {
            Section {
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
                Text("@\(pilot.handle)")
            }
        }
        .listStyle(.insetGrouped)
        .overlay {
            if flights.isEmpty, !isLoading {
                ContentUnavailableView(
                    "No public flights",
                    systemImage: "person",
                    description: Text(problem ?? "This pilot shares no flight with everyone at the moment.")
                )
            }
        }
        .navigationTitle(pilot.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu("More", systemImage: "ellipsis") {
                    Button("Report this pilot", systemImage: "exclamationmark.bubble") {
                        reporting = .pilot(pilot.handle)
                    }
                }
            }
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .task(id: pilot.handle) {
            flights = []
            next = nil
            await load()
        }
    }

    /// The first page, or the one after what is listed.
    private func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            let page = try await community.pilot(pilot.handle, after: flights.isEmpty ? nil : next)
            flights += page.flights
            next = page.next
            problem = nil
        } catch {
            problem = error is CoreError
                ? error.words
                : "The server could not be reached. \(error.localizedDescription)"
        }
    }
}

/// What was flown from a place: the newest public flights that took off
/// there, as a section of a flying site's page. The page fetches them; a
/// site nobody shared a flight from has no such section.
struct FlownFromHere: View {
    let flights: [PublicFlight]

    /// The most flights a site's page lists.
    static let atMost = 5

    var body: some View {
        Section {
            ForEach(flights.prefix(Self.atMost), id: \.id) { flight in
                NavigationLink {
                    PublicFlightScreen(flight: flight)
                } label: {
                    PublicFlightRow(flight: flight)
                }
                .accessibilityIdentifier("site-flight")
            }
        } header: {
            Text("Flown from here")
        } footer: {
            Text("The newest flights that pilots share with everyone on Venti and that took off within a kilometre and a half of this place.")
        }
    }
}
