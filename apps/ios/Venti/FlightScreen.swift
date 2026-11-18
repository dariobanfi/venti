import SwiftUI
import VentiCore

/// Shows one recorded flight: its track on the map and its headline numbers.
struct FlightScreen: View {
    @State private var model = FlightModel()

    var body: some View {
        NavigationStack {
            Group {
                switch model.state {
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
            .navigationTitle("Venti")
            .navigationBarTitleDisplayMode(.inline)
        }
        .task {
            await model.loadSample()
        }
    }
}

private struct FlightDetail: View {
    let flight: FlightView

    var body: some View {
        VStack(spacing: 0) {
            FlightMap(flight: flight)
                .frame(height: 380)
                .accessibilityLabel("Map of the flight")
            numbers
        }
    }

    private var numbers: some View {
        List {
            Section("Flight") {
                LabeledContent("Date", value: Format.date(unixSeconds: flight.startUnixS))
                LabeledContent("Duration", value: Format.duration(seconds: flight.durationS))
                LabeledContent("Track length", value: Format.distance(metres: flight.trackLengthM))
                LabeledContent("Highest", value: Format.altitude(metres: flight.maxAltitudeM))
                LabeledContent("Lowest", value: Format.altitude(metres: flight.minAltitudeM))
                LabeledContent("Best climb", value: Format.verticalSpeed(metresPerSecond: flight.maxClimbMs))
                LabeledContent("Strongest sink", value: Format.verticalSpeed(metresPerSecond: flight.maxSinkMs))
            }
            Section("Recording") {
                if let pilot = flight.pilot {
                    LabeledContent("Pilot", value: pilot)
                }
                if let glider = flight.glider {
                    LabeledContent("Glider", value: glider)
                }
                LabeledContent("Recorder", value: flight.recorder)
                LabeledContent("Positions", value: flight.track.count.formatted())
                if flight.skippedLines > 0 {
                    LabeledContent("Unreadable lines", value: flight.skippedLines.formatted())
                }
            }
        }
    }
}

#Preview {
    FlightScreen()
}
