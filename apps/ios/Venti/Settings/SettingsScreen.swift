import SwiftUI

/// The pilot's settings, and what the app is made of. A page of the Today
/// tab's stack, opened from its corner: it brings no stack of its own.
struct SettingsScreen: View {
    @Environment(FlySession.self) private var fly
    @Environment(WindModel.self) private var wind
    @Environment(AccountModel.self) private var account
    @Environment(OfflineMapsModel.self) private var maps

    @AppStorage(Preferences.pilot) private var pilot = ""
    @AppStorage(Preferences.glider) private var glider = ""
    @AppStorage(Preferences.trimSpeed) private var trimSpeed = Preferences.defaultTrimSpeed
    @AppStorage(Preferences.glideRatio) private var glideRatio = Preferences.defaultGlideRatio
    @AppStorage(Preferences.hidesFaiAreas) private var hidesFaiAreas = false
    @AppStorage(Preferences.cockpitKeepsBrightness) private var keepsBrightness = false

    var body: some View {
        Group {
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

                Section {
                    Toggle(
                        "Vario sound",
                        isOn: Binding(
                            get: { !fly.isMuted },
                            set: { isOn in fly.isMuted = !isOn }
                        )
                    )
                    NavigationLink("Waypoints") {
                        WaypointsScreen()
                    }
                    NavigationLink("Airspace") {
                        AirspaceScreen()
                    }
                    Stepper(value: $trimSpeed, in: 25...60, step: 1) {
                        LabeledContent("Trim speed", value: "\(Format.metres(trimSpeed)) km/h")
                    }
                    Stepper(value: $glideRatio, in: 4...14, step: 0.5) {
                        LabeledContent("Glide ratio", value: Format.glideRatio(glideRatio))
                    }
                    Toggle(
                        "FAI triangle areas",
                        isOn: Binding(
                            get: { !hidesFaiAreas },
                            set: { isOn in hidesFaiAreas = !isOn }
                        )
                    )
                } header: {
                    Text("Cockpit")
                } footer: {
                    Text("Your glider's speed with the brakes up and its glide ratio. From them and the wind, the cockpit predicts how high you arrive at the place you fly to. The prediction assumes air that neither rises nor sinks on the way. The FAI triangle areas are drawn on the cockpit's map once you have flown three kilometres in a straight line: a turnpoint inside either of them makes that line one side of an FAI triangle.")
                }

                Section {
                    Toggle(
                        "Full brightness in the cockpit",
                        isOn: Binding(
                            get: { !keepsBrightness },
                            set: { isOn in keepsBrightness = !isOn }
                        )
                    )
                } header: {
                    Text("In the sun")
                } footer: {
                    Text("With full brightness the cockpit turns the screen all the way up while it is open and back afterwards; it costs battery. The cockpit is always drawn for the sun: near-black on white, whatever the iPhone's appearance.")
                }

                Section {
                    TextField(
                        "Server address",
                        text: Binding(
                            get: { wind.address },
                            set: { address in wind.use(address: address) }
                        )
                    )
                    .keyboardType(.URL)
                    .textContentType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    NavigationLink {
                        AccountScreen()
                    } label: {
                        LabeledContent("Account", value: account.account.map { "@\($0.handle)" } ?? "Not signed in")
                    }
                    .accessibilityIdentifier("account-link")
                    // Maps for flying without a network come from the
                    // same server.
                    NavigationLink {
                        OfflineMapsScreen()
                    } label: {
                        LabeledContent("Offline maps", value: maps.inALine)
                    }
                    .accessibilityIdentifier("offline-maps-link")
                } header: {
                    Text("Venti")
                } footer: {
                    Text(serverNote)
                }

                Section("About") {
                    LabeledContent("Version", value: Self.version)
                    NavigationLink("Data and licences") {
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

    /// What the address field means, and whether what is in it is usable.
    private var serverNote: String {
        if wind.address.trimmingCharacters(in: .whitespaces).isEmpty {
            return "Venti's address. The map shows the wind stations it collects, the assistant can look them up, and with an account your flights are kept there and can be shared. Without it, everything stays on this iPhone and the map has no wind."
        }
        if wind.server == nil {
            return "This is not an address the app can use. It should look like wind.example.org."
        }
        if let problem = wind.problem {
            return problem
        }
        return "The map shows the wind stations Venti collects, and the assistant can look them up."
    }

    private static var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "–"
    }
}

/// Whose data and software the app uses, with the notices their licences
/// ask for.
struct CreditsScreen: View {
    @Environment(WindModel.self) private var wind

    var body: some View {
        VentiList {
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
            Section("Wind") {
                // Whose stations the server delivers is the server's to
                // say: the credits come with its answers.
                if wind.credits.isEmpty {
                    Text("Wind stations come from Venti. Each station's page names whose data it is and under which licence.")
                        .font(.venti(.subheadline))
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(wind.credits, id: \.source) { source in
                        credit(source.provider, "\(source.text). Under \(source.licence).", link: source.url)
                    }
                }
            }
            Section("Venti") {
                credit(
                    "This app",
                    "Free software under the GNU Affero General Public License, version 3 or later. You may use, study, share and change it.",
                    link: "https://www.gnu.org/licenses/agpl-3.0.html"
                )
            }
        }
        .navigationTitle("Data and licences")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func credit(_ title: String, _ text: String, link: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.venti(.headline))
            Text(text)
                .font(.venti(.subheadline))
                .foregroundStyle(.secondary)
            if let url = URL(string: link) {
                Link(url.host() ?? link, destination: url)
                    .font(.venti(.subheadline))
            }
        }
        .padding(.vertical, 2)
    }
}
