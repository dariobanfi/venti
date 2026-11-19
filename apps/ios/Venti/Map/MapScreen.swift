import SwiftUI
import VentiCore

/// The map tab: terrain, live wind from the stations of the pilot's Venti
/// server, and the pilot's own flights. Sites and forecasts arrive later.
struct MapScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(WindModel.self) private var wind
    @Environment(AirspaceModel.self) private var airspace

    @State private var isFollowing = false
    @State private var problem: String?
    @State private var chosen: ChosenStation?
    @State private var showsStations = false
    /// The part of the map in view, as the map last reported it.
    @State private var region: Bounds?

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            OverviewMap(
                outlines: outlines,
                stations: wind.stations,
                airspace: airspaceInView,
                isFollowing: $isFollowing,
                problem: $problem,
                onRegion: { region in
                    self.region = region
                    wind.show(region)
                    // The airspace around what the pilot is looking at,
                    // unless it is the whole continent.
                    if region.north - region.south < 4 {
                        let centre = Coordinate(
                            lat: (region.south + region.north) / 2, lon: (region.west + region.east) / 2
                        )
                        Task {
                            await airspace.refresh(around: centre)
                        }
                    }
                },
                onStation: { id in
                    if let station = wind.stations.first(where: { $0.id == id }) {
                        chosen = ChosenStation(station: station)
                    }
                }
            )
            .ignoresSafeArea()
            .accessibilityLabel("Map")

            StatusBarScrim()

            VStack(spacing: 12) {
                if wind.server != nil {
                    // Everything the arrows say, as a list: for reading the
                    // numbers side by side, and for pilots who use VoiceOver.
                    mapButton("Wind stations", systemImage: "wind") {
                        showsStations = true
                    }
                }
                mapButton("Show my position", systemImage: isFollowing ? "location.fill" : "location") {
                    isFollowing.toggle()
                }
            }
            .padding(16)
        }
        .overlay(alignment: .top) { notice }
        .overlay(alignment: .bottomLeading) { credits }
        // Asks again every minute while the map is on screen, and at once
        // when the pilot changes the server.
        .task(id: wind.address) {
            await wind.keepCurrent()
        }
        .sheet(item: $chosen) { chosen in
            NavigationStack {
                StationScreen(station: chosen.station)
            }
            .presentationDetents([.medium, .large])
        }
        .sheet(isPresented: $showsStations) {
            StationList()
        }
        .alert(
            "Position unavailable",
            isPresented: Binding(
                get: { problem != nil },
                set: { isPresented in
                    if !isPresented {
                        problem = nil
                    }
                }
            ),
            presenting: problem
        ) { _ in
            Button("OK", role: .cancel) {}
        } message: { problem in
            Text(problem)
        }
    }

    private func mapButton(_ title: String, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .labelStyle(.iconOnly)
                .font(.title3)
                .frame(width: 30, height: 30)
        }
        .buttonStyle(.glass)
        .buttonBorderShape(.circle)
        .controlSize(.large)
    }

    /// What the pilot should know about the wind on the map, if anything.
    private var noticeText: String? {
        if let problem = wind.problem {
            return problem
        }
        if wind.isTruncated {
            return "There are more stations here than fit on the map. Zoom in to see them all."
        }
        if !wind.lateProviders.isEmpty {
            return "No new wind data from \(wind.lateProviders.formatted(.list(type: .and))) for a while."
        }
        return nil
    }

    @ViewBuilder private var notice: some View {
        if let noticeText {
            Label(noticeText, systemImage: "exclamationmark.triangle")
                .font(.footnote)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(.regularMaterial, in: .rect(cornerRadius: 14))
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .accessibilityIdentifier("wind-notice")
        }
    }

    /// Whose wind and airspace are on the map. Their licences ask for the
    /// credit next to the data; the full wording is on each station's page
    /// and on the airspace screen.
    private var creditLine: String {
        var parts: [String] = []
        if !wind.credits.isEmpty {
            parts.append("Wind: \(wind.credits.map(\.provider).formatted(.list(type: .and)))")
        }
        if !airspace.credits.isEmpty, let region, region.east - region.west < 6 {
            // What the providers of airspace ask to be said is said here
            // in two words and in full on the airspace screen.
            let certified = airspace.credits.contains { $0.notice != nil } ? ", not certified" : ""
            parts.append("Airspace: \(airspace.credits.map(\.provider).formatted(.list(type: .and)))\(certified)")
        }
        return parts.joined(separator: " · ")
    }

    @ViewBuilder private var credits: some View {
        if !creditLine.isEmpty {
            Text(creditLine)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(.thinMaterial, in: .rect(cornerRadius: 10))
                // Clear of the map's own credits button on the left and of
                // the round buttons on the right.
                .padding(.leading, 44)
                .padding(.trailing, 96)
                .padding(.bottom, 8)
                .accessibilityIdentifier("wind-credits")
        }
    }

    /// The airspace in the part of the map in view. Zoomed far out, a
    /// country's airspace is a tangle and is not drawn, so it is not
    /// asked for either.
    private var airspaceInView: [AirspaceShape] {
        guard let region, region.east - region.west < 6 else { return [] }
        return airspace.shapes(in: region)
    }

    private var outlines: [[Coordinate]] {
        logbook.view.years.flatMap { year in
            year.flights.map(\.outline)
        }
    }
}

/// A station chosen on the map, to present its page.
struct ChosenStation: Identifiable {
    let station: WindStation
    var id: String { station.id }
}
