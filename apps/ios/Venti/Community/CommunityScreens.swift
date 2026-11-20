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

    /// "Sat 15 Aug 2026 · Sample Wing"
    static func detail(of flight: PublicFlight) -> String {
        let day = Format.day(unixSeconds: flight.startUnixS)
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
            // The flight's page on the server, to hand on.
            if let server = wind.server, let page = URL(string: server.absoluteString + flight.pagePath) {
                ToolbarItem(placement: .primaryAction) {
                    ShareLink(item: page) {
                        Label("Share the flight's page", systemImage: "square.and.arrow.up")
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
