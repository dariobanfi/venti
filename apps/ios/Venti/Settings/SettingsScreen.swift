import SwiftUI
import VentiCore

/// The pilot's settings, and what the app is made of. A page of the Today
/// tab's stack, opened from its corner: it brings no stack of its own.
struct SettingsScreen: View {
    @Environment(FlySession.self) private var fly
    @Environment(WindModel.self) private var wind
    @Environment(AccountModel.self) private var account
    @Environment(OfflineMapsModel.self) private var maps

    @AppStorage(Preferences.pilot) private var pilot = ""
    @AppStorage(Preferences.glider) private var glider = ""

    var body: some View {
        Group {
            // The pack's list, on the page's ground with the rows on a
            // card's surface: a `Form` stands on the system's grey.
            VentiList {
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
                    NavigationLink("Your take-offs") {
                        KeptSitesScreen()
                    }
                    .accessibilityIdentifier("kept-sites-link")
                } footer: {
                    Text("The sites you kept as your take-offs. Today lists them first.")
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
                    // The glider, the FAI triangle areas and the
                    // brightness: on a page of their own.
                    NavigationLink(pagesWords().gliderAndCockpit) {
                        CockpitSettingsScreen()
                    }
                    .accessibilityIdentifier("cockpit-settings-link")
                } header: {
                    Text("Cockpit")
                }

                Section {
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

    /// What Venti gives, or why it cannot be reached at the moment.
    private var serverNote: String {
        if let problem = wind.problem {
            return problem
        }
        return "The map shows the wind stations Venti collects, and the assistant can look them up. With an account your flights are kept on Venti and can be shared."
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
            // The aerial view's photographs: every provider in the
            // shared style, in its own words, whatever the map shows.
            Section("Aerial photographs") {
                ForEach(Self.photographs, id: \.layer) { maker in
                    credit(
                        maker.credit,
                        maker.wording + (Self.wordingIsOurs.contains(maker.layer) ? Self.oursNote : ""),
                        link: maker.link ?? ""
                    )
                }
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

    /// The layers of photographs of the shared style, with their wording.
    private static let photographs = (try? MapStyle.aerialMakers()) ?? []

    /// Providers whose credit is worded by Venti, since they prescribe
    /// none (docs/data-sources.md): South Tyrol's.
    private static let wordingIsOurs: Set<String> = ["aerial-south-tyrol"]
    private static let oursNote = " Worded by Venti: the province prescribes no credit."

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
