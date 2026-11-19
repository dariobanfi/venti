import SwiftUI
import VentiCore

/// A glider chosen on the map or in the list, for a sheet. Only its
/// identifier is kept: the page shows the glider as the newest answer has
/// it.
struct ChosenGlider: Identifiable {
    let id: String
}

/// One glider in a list: the pilot, and how high, how it climbs and how
/// old the position is.
struct LiveGliderRow: View {
    @Environment(LiveModel.self) private var live
    let glider: LiveGlider

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: glider.landed ? "figure.stand" : "paperplane.fill")
                .foregroundStyle(glider.landed || glider.quiet ? Color.secondary : Color.green)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(glider.name)
                    .font(.headline)
                Text(Self.detail(of: glider, age: live.ageInWords(of: glider)))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// "1,596 m · -1.0 m/s · Now"
    static func detail(of glider: LiveGlider, age: String) -> String {
        var parts = ["\(Format.metres(Double(glider.altitudeM))) m"]
        if !glider.landed, let climb = glider.climbMs {
            parts.append("\(Format.climb(metresPerSecond: climb)) m/s")
        }
        parts.append(age)
        return parts.joined(separator: " · ")
    }
}

/// The gliders on the map as a list: for reading them side by side, and
/// for pilots who use VoiceOver.
struct LiveList: View {
    @Environment(LiveModel.self) private var live
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(live.gliders, id: \.id) { glider in
                        NavigationLink(value: glider.id) {
                            LiveGliderRow(glider: glider)
                        }
                        .accessibilityIdentifier("live-row")
                    }
                } footer: {
                    if !live.gliders.isEmpty {
                        Text(
                            live.isTruncated
                                ? "There are more than the server lists at once. Zoom the map in. " + LiveGliderScreen.caution
                                : LiveGliderScreen.caution
                        )
                    }
                }
            }
            .overlay {
                if live.gliders.isEmpty {
                    ContentUnavailableView(
                        "Nobody in the air here",
                        systemImage: "paperplane",
                        description: Text("Pilots who show their flight to everyone appear on this part of the map while they fly.")
                    )
                }
            }
            .navigationTitle("In the air")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: String.self) { id in
                LiveGliderScreen(id: id)
            }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }
}

/// One glider: who flies it, where it is and how old that is, and the way
/// to its track on the server's page.
struct LiveGliderScreen: View {
    @Environment(LiveModel.self) private var live
    @Environment(WindModel.self) private var wind
    let id: String

    static let caution =
        "Positions are as each pilot's app sent them. A glider nothing was heard of for a while has most likely no network: it is not known to have landed."

    private var glider: LiveGlider? {
        live.gliders.first { $0.id == id }
    }

    var body: some View {
        List {
            if let glider {
                Section {
                    LabeledContent("Pilot", value: glider.name)
                    LabeledContent("Position", value: live.ageInWords(of: glider))
                        .accessibilityIdentifier("live-age")
                    LabeledContent("Altitude", value: "\(Format.metres(Double(glider.altitudeM))) m")
                    if !glider.landed {
                        if let climb = glider.climbMs {
                            LabeledContent("Climb", value: "\(Format.climb(metresPerSecond: climb)) m/s")
                        }
                        if let speed = glider.groundSpeedMs {
                            LabeledContent(
                                "Speed", value: "\(Format.kilometresPerHour(metresPerSecond: speed)) km/h"
                            )
                        }
                        if let course = glider.courseDeg {
                            LabeledContent("Flying", value: Format.compassPoint(degrees: course))
                        }
                    }
                } footer: {
                    Text(Self.caution)
                }
                if let server = wind.server, let page = URL(string: server.absoluteString + glider.pagePath) {
                    Section {
                        Link(destination: page) {
                            Label("Follow the flight in the browser", systemImage: "safari")
                        }
                    } footer: {
                        Text("The server's page shows the whole track and keeps itself up to date.")
                    }
                }
            } else {
                Section {
                    Text("This glider is no longer on this part of the map: its pilot stopped showing the flight, or it has left the area.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle(glider.map { "@\($0.handle)" } ?? "Glider")
        .navigationBarTitleDisplayMode(.inline)
    }
}
