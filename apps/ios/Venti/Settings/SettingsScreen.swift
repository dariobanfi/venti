import SwiftUI

/// The pilot's settings, and what the app is made of.
struct SettingsScreen: View {
    @Environment(FlySession.self) private var fly

    @AppStorage(Preferences.pilot) private var pilot = ""
    @AppStorage(Preferences.glider) private var glider = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Your name", text: $pilot)
                        .textContentType(.name)
                        .autocorrectionDisabled()
                    TextField("Glider", text: $glider)
                        .autocorrectionDisabled()
                } header: {
                    Text("Pilot")
                } footer: {
                    Text("Written into the flights you record. They stay on this iPhone.")
                }

                Section("Cockpit") {
                    Toggle(
                        "Vario sound",
                        isOn: Binding(
                            get: { !fly.isMuted },
                            set: { isOn in fly.isMuted = !isOn }
                        )
                    )
                }

                Section("About") {
                    LabeledContent("Version", value: Self.version)
                    NavigationLink("Map data and licences") {
                        CreditsScreen()
                    }
                    if let source = URL(string: "https://github.com/dariobanfi/venti") {
                        Link("Source code", destination: source)
                    }
                }
            }
            .navigationTitle("Settings")
        }
    }

    private static var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "–"
    }
}

/// Whose data and software the app uses, with the notices their licences
/// ask for.
struct CreditsScreen: View {
    var body: some View {
        List {
            Section("Map") {
                credit(
                    "Base map",
                    "© OpenStreetMap contributors, under the Open Database License. Tiles from OpenFreeMap in the OpenMapTiles schema, © OpenMapTiles.",
                    link: "https://www.openstreetmap.org/copyright"
                )
                credit(
                    "Terrain",
                    "Mapterhorn, built from the open elevation data of many survey agencies. The full list is on its attribution page.",
                    link: "https://mapterhorn.com/attribution"
                )
                credit(
                    "Labels",
                    "Noto Sans, under the SIL Open Font License.",
                    link: "https://openfontlicense.org"
                )
                credit(
                    "Map engine",
                    "MapLibre Native, under the BSD 2-Clause License.",
                    link: "https://maplibre.org"
                )
            }
            Section("Venti") {
                credit(
                    "This app",
                    "Free software under the GNU Affero General Public License, version 3 or later. You may use, study, share and change it.",
                    link: "https://www.gnu.org/licenses/agpl-3.0.html"
                )
            }
        }
        .navigationTitle("Map data and licences")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func credit(_ title: String, _ text: String, link: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.headline)
            Text(text)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            if let url = URL(string: link) {
                Link(url.host() ?? link, destination: url)
                    .font(.subheadline)
            }
        }
        .padding(.vertical, 2)
    }
}
