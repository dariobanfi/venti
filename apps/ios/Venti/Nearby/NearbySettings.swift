import SwiftUI
import VentiCore

/// "Pilots nearby" on the account's page (docs/design.md, "Push to the
/// phone and what pilots nearby do"): two switches, both off until the
/// pilot turns them on (owner decision 15), in the core's words. "New open
/// plans near" with a place, by default where the map is, and a distance;
/// "Pilots nearby may ask me for help". Turning one on is the moment the
/// app asks whether it may notify; turning help on asks for the position,
/// which the app then tells Venti roughly while it is in front.
struct NearbySection: View {
    @Environment(NearbyModel.self) private var nearby
    @Environment(WindModel.self) private var wind

    @State private var position = PilotPosition()
    @State private var isWorking = false

    /// The distances to choose from, Android's: the server takes 1 to 100 km.
    private static let radii: [(value: UInt32, name: String)] = [10, 30, 50, 100].map {
        (value: $0, name: "\(Format.count($0)) km")
    }

    /// The core's words: open plans' title and line, then help's.
    private let words = nearbySettingsWords()

    var body: some View {
        Section {
            if let choice = nearby.choice {
                Toggle(isOn: plansSwitch) {
                    titled(words[safe: 0], words[safe: 1])
                }
                .disabled(isWorking)
                .accessibilityIdentifier("nearby-open-plans")
                if let at = choice.openPlansNear {
                    Text(verbatim: "Around \(Format.place(lat: at.lat, lon: at.lon))")
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.textSecondary))
                        .monospacedDigit()
                        .accessibilityIdentifier("nearby-place")
                    VentiSegmented(options: Self.radii, selection: radius)
                        .disabled(isWorking)
                        .accessibilityIdentifier("nearby-radius")
                    Button(HelpWords.whereTheMapIs) {
                        change { $0.openPlansNear = wind.mapCentre }
                    }
                    .disabled(isWorking)
                    .accessibilityIdentifier("nearby-place-map")
                }
                Toggle(isOn: helpSwitch) {
                    titled(words[safe: 2], words[safe: 3])
                }
                .disabled(isWorking)
                .accessibilityIdentifier("nearby-help")
            } else if let problem = nearby.settingsProblem {
                Text(problem)
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("nearby-problem")
                Button(commonWords().tryAgain) {
                    Task { await nearby.loadSettings() }
                }
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity)
            }
        } header: {
            Text(HelpWords.pilotsNearby)
        } footer: {
            if nearby.choice != nil, let problem = nearby.settingsProblem {
                Text(problem)
                    .accessibilityIdentifier("nearby-problem")
            }
        }
        .task {
            position.readIfAllowed()
            await nearby.loadSettings()
        }
    }

    private func titled(_ title: String, _ line: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .foregroundStyle(.venti(.text))
            Text(line)
                .font(.venti(.footnote))
                .foregroundStyle(.venti(.textSecondary))
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var plansSwitch: Binding<Bool> {
        Binding {
            nearby.choice?.openPlansNear != nil
        } set: { on in
            change(notifies: on) { choice in
                choice.openPlansNear = on ? wind.mapCentre : nil
            }
        }
    }

    private var helpSwitch: Binding<Bool> {
        Binding {
            nearby.choice?.mayBeAskedForHelp ?? false
        } set: { on in
            if on {
                // The position is told while this is on: asked of the
                // system now, in answer to the pilot's own switch.
                position.ask()
            }
            change(notifies: on) { $0.mayBeAskedForHelp = on }
        }
    }

    private var radius: Binding<UInt32> {
        Binding {
            nearby.choice?.openPlansRadiusKm ?? 30
        } set: { km in
            change { $0.openPlansRadiusKm = km }
        }
    }

    /// Sends the settings whole, changed as asked; with `notifies`, first
    /// asks whether the app may notify and registers with Apple.
    private func change(notifies: Bool = false, _ edit: @escaping (inout NearbyChoice) -> Void) {
        guard var wanted = nearby.choice, !isWorking else { return }
        edit(&wanted)
        isWorking = true
        Task {
            defer { isWorking = false }
            if notifies {
                await PhonePush.shared.askAndRegister()
            }
            await nearby.change(wanted)
            if nearby.mayBeAskedForHelp, let place = position.place {
                await nearby.tellPosition(at: place)
            }
        }
    }
}

extension Array {
    /// The element at an index, or an empty one where the list is short.
    subscript(safe index: Int) -> Element where Element == String {
        indices.contains(index) ? self[index] : ""
    }
}
