import SwiftUI
import VentiCore

/// One flight: its track on the map, coloured by climb, under a panel with
/// its numbers and altitude profile.
struct FlightScreen: View {
    let id: String

    @Environment(LogbookModel.self) private var logbook
    @Environment(AirspaceModel.self) private var airspace
    @Environment(AccountModel.self) private var account
    @Environment(FlySession.self) private var fly
    @Environment(AppNavigation.self) private var navigation
    @Environment(\.dismiss) private var dismiss

    private enum Phase {
        case loading
        case loaded(FlightView)
        case failed(String)
    }

    @State private var phase = Phase.loading
    /// Arrives a moment after the flight itself; the screen does not wait
    /// for it.
    @State private var analysis: FlightAnalysis?
    /// The flight checked against the airspace the app has. Arrives last.
    @State private var airspaceCheck: FlightAirspace?
    /// The flight's file, written out for sharing.
    @State private var exported: URL?
    @State private var isConfirmingDelete = false
    @State private var isSharing = false

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
            case .loaded(let flight):
                FlightDetail(
                    flight: flight, analysis: analysis, airspaceCheck: airspaceCheck,
                    // The airspace around the flight, for its map.
                    airspace: airspace.shapes(in: flight.bounds.grown(by: 0.2))
                )
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu("More", systemImage: "ellipsis") {
                    Button("Share with a link", systemImage: "link") {
                        isSharing = true
                    }
                    if let exported {
                        ShareLink("Share flight file", item: exported)
                    }
                    // The flight once more, as the cockpit would have
                    // shown it in the air: its wind, its thermals, the
                    // airspace it came near. Not while a flight is on.
                    if fly.mode == nil {
                        Menu("Replay in the cockpit", systemImage: "play") {
                            Button("In real time") {
                                replay(speed: 1)
                            }
                            Button("10 times faster") {
                                replay(speed: 10)
                            }
                            Button("20 times faster") {
                                replay(speed: 20)
                            }
                        }
                    }
                    Button("Delete flight", systemImage: "trash", role: .destructive) {
                        isConfirmingDelete = true
                    }
                }
            }
        }
        .sheet(isPresented: $isSharing) {
            ShareFlightSheet(flight: id)
                .presentationDetents([.medium, .large])
        }
        .confirmationDialog("Delete this flight?", isPresented: $isConfirmingDelete, titleVisibility: .visible) {
            Button("Delete flight", role: .destructive) {
                do {
                    try logbook.delete(id: id)
                    dismiss()
                } catch {
                    phase = .failed(error.words)
                }
            }
        } message: {
            Text(account.deletionWarning)
        }
        .task(id: id) {
            do {
                phase = .loaded(try await logbook.flight(id: id))
            } catch {
                phase = .failed(error.words)
                return
            }
            // Without the analysis the flight is still shown in full.
            analysis = try? await logbook.analysis(id: id)
            exported = try? await logbook.exportFile(id: id)
            airspaceCheck = try? await logbook.airspace(of: id)
        }
    }
}

extension FlightScreen {
    /// Plays this flight through the cockpit, on the Fly tab, which is
    /// where the cockpit is. Nothing is recorded and nothing is shared.
    private func replay(speed: Double) {
        Task {
            do {
                let text = try await logbook.fileText(id: id)
                navigation.tab = .fly
                fly.startReplay(igcText: text, speed: speed)
            } catch {
                fly.problem = error.words
            }
        }
    }
}

/// The map fills the screen and the panel floats over its lower part, as a
/// control layer above the content. The map is told how much of it the panel
/// covers so the track is fitted into the part that shows.
struct FlightDetail: View {
    let flight: FlightView
    let analysis: FlightAnalysis?
    let airspaceCheck: FlightAirspace?
    let airspace: [AirspaceShape]

    @Environment(TerrainModel.self) private var terrain

    /// The position in the track the pilot is inspecting, if any.
    @State private var cursor: Int?
    /// The ground under the flight, once its tiles are there.
    @State private var ground: [GroundUnderFlight] = []
    /// Whether the marker is running along the track by itself.
    @State private var isPlaying = false
    @State private var panelHeight: CGFloat = 0
    @State private var isShowingDetails = false

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                FlightMap(
                    flight: flight,
                    cursor: cursor.map { flight.track[$0] },
                    score: analysis?.score,
                    airspace: airspace,
                    covered: UIEdgeInsets(
                        top: proxy.safeAreaInsets.top,
                        left: 0,
                        bottom: panelHeight + 2 * Self.panelMargin,
                        right: 0
                    )
                )
                .ignoresSafeArea()
                .accessibilityLabel("Map of the flight")

                StatusBarScrim()

                VStack(spacing: 0) {
                    Spacer(minLength: 0)
                    FlightPanel(
                        flight: flight, score: analysis?.score, ground: ground,
                        cursor: $cursor, isPlaying: $isPlaying, isShowingDetails: $isShowingDetails
                    )
                        .onGeometryChange(for: CGFloat.self) { panel in
                            panel.size.height
                        } action: { height in
                            panelHeight = height
                        }
                        .padding(Self.panelMargin)
                }
                // The panel sits near the bottom edge like a system sheet;
                // its own padding keeps its content clear of the home bar.
                .ignoresSafeArea(.container, edges: .bottom)
            }
        }
        // The ground under the flight, for its profile: fetched once per
        // flight, and drawn when it is there.
        .task(id: flight.startUnixS) {
            ground = await terrain.ground(under: flight)
        }
        // Playing moves the marker from where it is to the end of the
        // flight in about half a minute, whatever the flight's length.
        .task(id: isPlaying) {
            guard isPlaying, flight.track.count > 1 else { return }
            let step = max(1, flight.track.count / 450)
            var index = cursor ?? 0
            if index >= flight.track.count - 1 {
                index = 0
            }
            while !Task.isCancelled, index < flight.track.count - 1 {
                index = min(index + step, flight.track.count - 1)
                cursor = index
                try? await Task.sleep(for: .milliseconds(66))
            }
            isPlaying = false
        }
        .sheet(isPresented: $isShowingDetails) {
            FlightDetailsSheet(flight: flight, analysis: analysis, airspace: airspaceCheck) { index in
                // Show a thermal or glide the pilot picked from the list.
                isPlaying = false
                cursor = min(index, flight.track.count - 1)
                isShowingDetails = false
            }
            .presentationDetents([.medium, .large])
        }
    }

    private static let panelMargin: CGFloat = 8
}
