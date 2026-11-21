import SwiftUI
import VentiCore

/// How the pilot flies: what their own tracks say, observation by
/// observation. Every word of an observation is the core's
/// (`tendencies`); the screen lays them out and judges nothing, so no
/// colour of good or bad stands on it.
struct HowYouFlyScreen: View {
    @Environment(LogbookModel.self) private var logbook

    /// Absent while the flights are counted.
    @State private var tendencies: Tendencies?

    var body: some View {
        Group {
            if let tendencies {
                list(tendencies)
            } else {
                // An older logbook is counted flight by flight the first
                // time, off the main thread.
                VStack(spacing: VentiSpace.s3) {
                    ProgressView()
                    Text("Counting your flights")
                        .foregroundStyle(.venti(.textSecondary))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(.venti(.bg))
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("how-you-fly-counting")
            }
        }
        .navigationTitle("How you fly")
        .navigationBarTitleDisplayMode(.inline)
        .task {
            tendencies = await logbook.tendencies()
        }
    }

    private func list(_ tendencies: Tendencies) -> some View {
        VentiList {
            if tendencies.observations.isEmpty {
                Section {
                    Text("Nothing to say yet. The observations begin once the logbook holds ten thermals.")
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("how-you-fly-empty")
                }
            } else {
                ForEach(Array(tendencies.observations.enumerated()), id: \.offset) { _, observation in
                    Section {
                        card(observation)
                        if let id = observation.flightId {
                            NavigationLink(value: FlightRoute(id: id)) {
                                LabeledContent("The flight that shows it most") {
                                    if let start = observation.flightUnixS {
                                        Text(Format.day(unixSeconds: start))
                                    }
                                }
                            }
                        }
                    }
                }
                Section {
                    EmptyView()
                } footer: {
                    Text(tendencies.note)
                        .font(.ventiType(.caption))
                }
            }
        }
    }

    /// One observation: its title, its words, and a thin bar where it is a
    /// share. Read as one; the bar is left to the words, which say what
    /// the share is of.
    private func card(_ observation: VentiCore.Observation) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            Text(observation.title)
                .font(.ventiType(.heading))
            Text(observation.words)
                .foregroundStyle(.venti(.textSecondary))
                .fixedSize(horizontal: false, vertical: true)
            if let share = observation.share {
                ShareBar(share: share)
                    .accessibilityHidden(true)
            }
        }
        .padding(.vertical, VentiSpace.s1)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("how-you-fly-card")
    }
}

/// A thin bar in the primary colour, filled as far as the share goes.
private struct ShareBar: View {
    let share: Double

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(.venti(.surfaceRaised))
                Capsule()
                    .fill(.venti(.primary))
                    .frame(width: proxy.size.width * min(max(share, 0), 1))
            }
        }
        .frame(height: 6)
    }
}
