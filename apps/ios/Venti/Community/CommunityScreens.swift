import SwiftUI
import VentiCore

/// Everyone's public flights on the pilot's Venti server: the newest
/// first, or the best.
struct PublicFlightsList: View {
    @Environment(CommunityModel.self) private var community

    var body: some View {
        List {
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
                    Text("Flights their pilots share with everyone on your Venti server. Distances and points are the server's own reckoning by the World XContest's rules, not a league's.")
                }
            }
        }
        .listStyle(.insetGrouped)
        .overlay {
            if community.flights.isEmpty, !community.isLoading {
                ContentUnavailableView(
                    "No public flights yet",
                    systemImage: "person.2",
                    description: Text(
                        community.problem
                            ?? "Flights that pilots on your Venti server share with everyone are listed here."
                    )
                )
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
                    .font(.headline)
                    .lineLimit(1)
                Text(Self.detail(of: flight))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                HStack(spacing: 3) {
                    if let score = flight.score, score.kind != .freeFlight {
                        Image(systemName: "triangle")
                            .font(.caption.weight(.semibold))
                            .accessibilityLabel(score.kind == .faiTriangle ? "FAI triangle" : "Triangle")
                    }
                    Text("\(Format.kilometres(metres: flight.score?.distanceM ?? flight.trackLengthM)) km")
                        .font(.system(.headline, design: .rounded))
                }
                Text("\(Format.hoursMinutes(seconds: flight.airtimeS)) h")
                    .font(.system(.subheadline, design: .rounded))
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
    @Environment(AppNavigation.self) private var navigation

    private enum Phase {
        case loading
        case loaded(FlightView, FlightAnalysis?)
        case failed(String)
    }

    @State private var phase = Phase.loading

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
        // The name stands over a map: it needs ground to be read on.
        .toolbarBackgroundVisibility(.visible, for: .navigationBar)
        .toolbarVisibility(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu("More", systemImage: "ellipsis") {
                    // Everything else this pilot shares with everyone.
                    Button("Flights of \(flight.pilot)", systemImage: "person") {
                        navigation.logbookPath.append(PilotRoute(handle: flight.handle, name: flight.pilot))
                    }
                    // The flight's page on the server, to hand on.
                    if let server = wind.server, let page = URL(string: server.absoluteString + flight.pagePath) {
                        ShareLink(item: page) {
                            Label("Share the flight's page", systemImage: "square.and.arrow.up")
                        }
                    }
                }
            }
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

    var body: some View {
        List {
            Section {
                ForEach(flights, id: \.id) { flight in
                    NavigationLink(value: flight) {
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
