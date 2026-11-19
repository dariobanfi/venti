import SwiftUI
import VentiCore

/// One flight: its track on the map, coloured by climb, under a panel with
/// its numbers and altitude profile.
struct FlightScreen: View {
    let id: String

    @Environment(LogbookModel.self) private var logbook
    @Environment(\.dismiss) private var dismiss

    private enum Phase {
        case loading
        case loaded(FlightView)
        case failed(String)
    }

    @State private var phase = Phase.loading
    @State private var isConfirmingDelete = false

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
                FlightDetail(flight: flight)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu("More", systemImage: "ellipsis") {
                    Button("Delete flight", systemImage: "trash", role: .destructive) {
                        isConfirmingDelete = true
                    }
                }
            }
        }
        .confirmationDialog("Delete this flight?", isPresented: $isConfirmingDelete, titleVisibility: .visible) {
            Button("Delete flight", role: .destructive) {
                do {
                    try logbook.delete(id: id)
                    dismiss()
                } catch {
                    phase = .failed(error.localizedDescription)
                }
            }
        } message: {
            Text("Its file is removed from this iPhone. This cannot be undone.")
        }
        .task(id: id) {
            do {
                phase = .loaded(try await logbook.flight(id: id))
            } catch {
                phase = .failed(error.localizedDescription)
            }
        }
    }
}

/// The map fills the screen and the panel floats over its lower part, as a
/// control layer above the content. The map is told how much of it the panel
/// covers so the track is fitted into the part that shows.
private struct FlightDetail: View {
    let flight: FlightView

    /// The position in the track the pilot is inspecting, if any.
    @State private var cursor: Int?
    @State private var panelHeight: CGFloat = 0
    @State private var isShowingDetails = false

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                FlightMap(
                    flight: flight,
                    cursor: cursor.map { flight.track[$0] },
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
                    FlightPanel(flight: flight, cursor: $cursor, isShowingDetails: $isShowingDetails)
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
        .sheet(isPresented: $isShowingDetails) {
            FlightDetailsSheet(flight: flight)
                .presentationDetents([.medium, .large])
        }
    }

    private static let panelMargin: CGFloat = 8
}
