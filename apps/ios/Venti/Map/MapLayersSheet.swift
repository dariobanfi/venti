import SwiftUI
import VentiCore

/// What the map tab keeps off the map: the layers without a chip as
/// switches, and the lists of what the map shows as rows. The map itself
/// has four chips and two round buttons, and this sheet behind the first
/// of those.
struct MapLayersSheet: View {
    /// A list or a form the sheet's rows open, once the sheet has closed.
    enum Opening {
        case stations, sites, sitesFilter, pilots, plans
    }

    /// What there are rows for: each only where there is something to
    /// switch or to list.
    struct Offers {
        var hasServer = false
        var sites = false
        var sitesFilter = false
        var pilots = false
        var plans = false
    }

    let offers: Offers
    /// The filter of the sites in the core's words while one is on.
    var filterWords = ""
    /// How many are in the air on this part of the map, in the core's words.
    var pilotsWords = ""
    let open: (Opening) -> Void

    @Environment(\.dismiss) private var dismiss
    @AppStorage(Preferences.mapHidesAirspace) private var hidesAirspace = false
    @AppStorage(Preferences.mapHidesPilots) private var hidesPilots = false
    @AppStorage(Preferences.mapHidesCables) private var hidesCables = false
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Toggle(isOn: shown($hidesAirspace)) {
                        Label("Airspace", image: "icon-airspace")
                    }
                    .accessibilityIdentifier("layer-airspace")
                    if offers.hasServer {
                        Toggle(isOn: shown($hidesPilots)) {
                            Label("Pilots", image: "icon-live")
                        }
                        .accessibilityIdentifier("layer-pilots")
                        Toggle("Cables", isOn: shown($hidesCables))
                            .accessibilityIdentifier("layer-cables")
                    }
                    // The ground under everything: the drawn map, or
                    // photographs from above.
                    Toggle(isOn: $showsAerial) {
                        Label("Aerial view", image: "icon-layers")
                    }
                    .accessibilityIdentifier("map-aerial")
                }
                Section {
                    if offers.hasServer {
                        // Everything the arrows say, as a list: for reading
                        // the numbers side by side, and for pilots who use
                        // VoiceOver.
                        row("Wind stations", icon: Image("icon-wind"), opens: .stations)
                    }
                    if offers.sites {
                        // The signs on the map, as a list.
                        row("Flying sites", icon: Image("icon-site"), opens: .sites)
                    }
                    if offers.sitesFilter {
                        // Which of the sites the map and the list show.
                        row("Filter the sites", icon: Image(systemName: SitesFilterIcon.name), value: filterWords, opens: .sitesFilter)
                            .accessibilityIdentifier("sites-filter")
                    }
                    if offers.pilots {
                        // Who is in the air on this part of the map, and
                        // how many in the core's words.
                        row("Pilots in the air", icon: Image("icon-live"), value: pilotsWords, opens: .pilots)
                    }
                    if offers.plans {
                        // Who flies where today and on the days ahead: the
                        // plans of the pilot's groups.
                        row("Flying plans", icon: Image(systemName: "calendar"), opens: .plans)
                    }
                }
            }
            .navigationTitle("Layers")
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

    /// A row that closes the sheet and opens a list in its place.
    private func row(_ title: String, icon: Image, value: String = "", opens opening: Opening) -> some View {
        Button {
            open(opening)
        } label: {
            HStack(spacing: VentiSpace.s3) {
                Label {
                    Text(title)
                } icon: {
                    icon
                }
                Spacer(minLength: VentiSpace.s2)
                if !value.isEmpty {
                    Text(value)
                        .font(.venti(.footnote))
                        .foregroundStyle(.venti(.textSecondary))
                        .multilineTextAlignment(.trailing)
                }
                Image(systemName: "chevron.right")
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textMuted))
                    .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .accessibilityValue(value)
    }

    /// A switch is on while its layer is not hidden.
    private func shown(_ isHidden: Binding<Bool>) -> Binding<Bool> {
        Binding {
            !isHidden.wrappedValue
        } set: { isShown in
            isHidden.wrappedValue = !isShown
        }
    }
}

/// Whose data and whose map the map tab shows, in full: what stood in a
/// block over the map's lower edge, a line for each, and under it whose
/// the base map and its terrain are, which the map engine's own credits
/// button listed. One tap from the map, as the licences ask.
///
/// The map's own credits are written here and not read from the map: the
/// engine words them from HTML, which it parses on the main thread with a
/// run loop of its own, and asked while the map moved that stopped the app.
struct MapCreditsSheet: View {
    /// The credits of what is drawn on the map now, a line for each.
    let data: [String]
    /// Who made the photographs in view, while the aerial view is on.
    var photos = ""

    /// The base map and the terrain, as "Data and licences" names them.
    private static let map = [
        "Base map: © OpenStreetMap contributors. Tiles from OpenFreeMap, © OpenMapTiles.",
        "Terrain: © Mapterhorn.",
    ]

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                // One row, a line for each: whose wind, airspace, sites
                // and the rest the map draws now.
                if !data.isEmpty || !photos.isEmpty {
                    Section {
                        VStack(alignment: .leading, spacing: VentiSpace.s2) {
                            ForEach(data + (photos.isEmpty ? [] : [photos]), id: \.self) { line in
                                Text(line)
                                    .font(.venti(.subheadline))
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("wind-credits")
                    }
                }
                Section {
                    ForEach(Self.map, id: \.self) { line in
                        Text(line)
                            .font(.venti(.subheadline))
                            .foregroundStyle(.venti(.textSecondary))
                    }
                    // Every provider's full wording and licence.
                    NavigationLink("Data and licences") {
                        CreditsScreen()
                    }
                    .accessibilityIdentifier("map-credits-all")
                } header: {
                    Text("Map")
                }
            }
            .navigationTitle("Credits")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
