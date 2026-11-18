import SwiftUI
import VentiCore

/// The map tab. Today it shows terrain and the pilot's own flights; live
/// wind, sites and forecasts arrive with the server.
struct MapScreen: View {
    @Environment(LogbookModel.self) private var logbook

    @State private var isFollowing = false
    @State private var problem: String?

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            OverviewMap(outlines: outlines, isFollowing: $isFollowing, problem: $problem)
                .ignoresSafeArea()
                .accessibilityLabel("Map")

            Button {
                isFollowing.toggle()
            } label: {
                Label("Show my position", systemImage: isFollowing ? "location.fill" : "location")
                    .labelStyle(.iconOnly)
                    .font(.title3)
                    .frame(width: 30, height: 30)
            }
            .buttonStyle(.glass)
            .buttonBorderShape(.circle)
            .controlSize(.large)
            .padding(16)
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

    private var outlines: [[Coordinate]] {
        logbook.view.years.flatMap { year in
            year.flights.map(\.outline)
        }
    }
}
