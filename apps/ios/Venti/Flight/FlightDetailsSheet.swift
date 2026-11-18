import SwiftUI
import VentiCore

/// Everything known about a flight, as a list. Opened from the flight panel.
struct FlightDetailsSheet: View {
    let flight: FlightView
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section("Flight") {
                    row("Date", Format.day(unixSeconds: flight.startUnixS))
                    row("Take-off", Format.time(unixSeconds: flight.startUnixS))
                    row("Landing", Format.time(unixSeconds: flight.startUnixS + flight.durationS))
                    row("Airtime", "\(Format.hoursMinutes(seconds: flight.durationS)) h")
                    row("Track length", "\(Format.kilometres(metres: flight.trackLengthM)) km")
                }
                Section("Altitude and climb") {
                    row("Highest", "\(Format.metres(flight.maxAltitudeM)) m")
                    row("Lowest", "\(Format.metres(flight.minAltitudeM)) m")
                    row("Best climb", "\(Format.climb(metresPerSecond: flight.maxClimbMs)) m/s")
                    row("Strongest sink", "\(Format.climb(metresPerSecond: flight.maxSinkMs)) m/s")
                }
                Section {
                    ForEach(ClimbBand.allCases.reversed(), id: \.self) { band in
                        Label {
                            Text(band.label)
                        } icon: {
                            Capsule()
                                .fill(band.color)
                                .frame(width: 22, height: 6)
                        }
                    }
                } header: {
                    Text("Track colours")
                } footer: {
                    Text("Climb and sink along the track are averaged over ten seconds. Best climb and strongest sink are averaged over half a minute.")
                }
                Section("Recording") {
                    if let pilot = flight.pilot {
                        row("Pilot", pilot)
                    }
                    if let glider = flight.glider {
                        row("Glider", glider)
                    }
                    row("Recorder", flight.recorder)
                    row("Positions", flight.track.count.formatted())
                    if flight.skippedLines > 0 {
                        row("Unreadable lines", flight.skippedLines.formatted())
                    }
                }
            }
            .monospacedDigit()
            .navigationTitle("Flight details")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    private func row(_ title: String, _ value: String) -> some View {
        LabeledContent(title, value: value)
    }
}
