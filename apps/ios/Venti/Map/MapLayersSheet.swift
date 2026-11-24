import SwiftUI
import VentiCore

/// What the map tab lays over what it shows: every layer as a switch
/// (sites, the pilot's own flights, airspace, pilots, cables, the thermal
/// map, the aerial view, climbs and flown thermals), and the lists of what
/// the map shows as rows. The map itself has the chips of what it shows,
/// one at a time, and three round buttons (airspace, this sheet and the
/// position), and this sheet behind the second of those.
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
    /// What the layer "Climbs now" says of itself while it is on: how
    /// many climbs, why none, or to zoom in. The core's words.
    var climbsWords = ""
    /// The same for where thermals have been.
    var flownWords = ""
    /// What the cables' layer cannot promise, in the core's words: behind
    /// the layer's "i".
    var cablesNote = ""
    /// Whether the pilot has flights to draw: the switch is there only then.
    var hasMyFlights = false
    let open: (Opening) -> Void

    @Environment(\.dismiss) private var dismiss
    @AppStorage(Preferences.mapHidesSites) private var hidesSites = false
    @AppStorage(Preferences.mapShowsMyFlights) private var showsMyFlights = false
    @AppStorage(Preferences.mapShowsAirspace) private var showsAirspace = false
    @AppStorage(Preferences.mapHidesPilots) private var hidesPilots = false
    @AppStorage(Preferences.mapHidesCables) private var hidesCables = false
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false
    @AppStorage(Preferences.mapShowsClimbs) private var showsClimbs = false
    @AppStorage(Preferences.mapShowsFlownThermals) private var showsFlown = false
    @AppStorage(Preferences.mapShowsThermalMap) private var showsThermalMap = false
    /// Whether the cables' note behind its "i" is shown.
    @State private var showsCablesNote = false

    var body: some View {
        let words = mapLayersWords()
        NavigationStack {
            VentiList {
                Section {
                    // Every layer laid over what the map shows is a switch
                    // here, in the order of Android and the web: the sites,
                    // airspace, pilots and cables, then the pilot's own
                    // flights, off until switched on.
                    if offers.hasServer {
                        Toggle(isOn: shown($hidesSites)) {
                            Label(words.sites, image: "icon-site")
                        }
                        .accessibilityIdentifier("layer-sites")
                    }
                    Toggle(isOn: $showsAirspace) {
                        Label(words.airspace, image: "icon-airspace")
                    }
                    .accessibilityIdentifier("layer-airspace")
                    if offers.hasServer {
                        Toggle(isOn: shown($hidesPilots)) {
                            Label(words.pilots, image: "icon-live")
                        }
                        .accessibilityIdentifier("layer-pilots")
                        // A cable is a hazard on the way: the pack's
                        // caution sign, so that the row lines up with
                        // the others.
                        Toggle(isOn: shown($hidesCables)) {
                            Label {
                                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                                    HStack(spacing: VentiSpace.s1) {
                                        Text(words.cables)
                                        if !cablesNote.isEmpty {
                                            // What the layer cannot promise,
                                            // behind its "i".
                                            Button {
                                                showsCablesNote.toggle()
                                            } label: {
                                                Image(systemName: "info.circle")
                                                    .foregroundStyle(.venti(showsCablesNote ? .link : .textSecondary))
                                                    .frame(minWidth: VentiSize.touch, minHeight: VentiSize.touch)
                                                    .contentShape(.rect)
                                            }
                                            .buttonStyle(.borderless)
                                            .padding(.vertical, -VentiSpace.s3)
                                            .accessibilityLabel("About the cables")
                                            .accessibilityValue(showsCablesNote ? "Shown" : "Hidden")
                                            .accessibilityIdentifier("cable-limits-more")
                                        }
                                    }
                                    if showsCablesNote {
                                        Text(cablesNote)
                                            .font(.venti(.footnote))
                                            .foregroundStyle(.venti(.textSecondary))
                                            .fixedSize(horizontal: false, vertical: true)
                                            .accessibilityIdentifier("cable-limits")
                                    }
                                }
                            } icon: {
                                Image("icon-caution")
                            }
                        }
                        .accessibilityIdentifier("layer-cables")
                    }
                    if hasMyFlights || showsMyFlights {
                        Toggle(isOn: $showsMyFlights) {
                            Label(words.myFlights, image: "icon-track")
                        }
                        .accessibilityIdentifier("layer-my-flights")
                    }
                    // Where thermals are found, from thermal.kk7.ch: off
                    // until switched on, and only where the app can name
                    // itself to its author.
                    if MapStyle.appDomain != nil {
                        Toggle(isOn: $showsThermalMap) {
                            Label(words.thermalMap, image: "icon-thermal")
                        }
                        .accessibilityIdentifier("layer-thermal-map")
                    }
                    // The ground under everything: the drawn map, or
                    // photographs from above.
                    Toggle(isOn: $showsAerial) {
                        Label(words.aerialView, image: "icon-layers")
                    }
                    .accessibilityIdentifier("map-aerial")
                    // What others found in the air: off until the pilot
                    // asks for it, each with the core's line about what
                    // it is, and what it says of itself while it is on.
                    if offers.hasServer {
                        airLayer(
                            words.climbsNow, icon: "icon-thermal", note: words.climbsNowNote, says: climbsWords,
                            isOn: $showsClimbs
                        )
                        .accessibilityIdentifier("layer-climbs")
                        airLayer(
                            words.flownThermals, icon: "icon-track", note: words.flownThermalsNote, says: flownWords,
                            isOn: $showsFlown
                        )
                        .accessibilityIdentifier("layer-flown")
                    }
                } header: {
                    Text(words.onTheMap)
                }
                Section {
                    if offers.hasServer {
                        // Everything the arrows say, as a list: for reading
                        // the numbers side by side, and for pilots who use
                        // VoiceOver.
                        row(words.windStations, icon: Image("icon-wind"), opens: .stations)
                    }
                    if offers.sites {
                        // The signs on the map, as a list.
                        row(words.flyingSites, icon: Image("icon-site"), opens: .sites)
                    }
                    if offers.sitesFilter {
                        // Which of the sites the map and the list show.
                        row(words.filterSites, icon: Image(systemName: SitesFilterIcon.name), value: filterWords, opens: .sitesFilter)
                            .accessibilityIdentifier("sites-filter")
                    }
                    if offers.pilots {
                        // Who is in the air on this part of the map, and
                        // how many in the core's words.
                        row(words.pilotsInTheAir, icon: Image("icon-live"), value: pilotsWords, opens: .pilots)
                    }
                    if offers.plans {
                        // Who flies where today and on the days ahead: the
                        // plans of the pilot's groups.
                        row(words.flyingPlans, icon: Image(systemName: "calendar"), opens: .plans)
                    }
                } header: {
                    Text(words.listsTitle)
                }
            }
            .navigationTitle(words.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                }
            }
        }
    }

    /// A layer of what others found in the air: its name, the core's line
    /// about what it is, and under that what it says of itself while on.
    private func airLayer(
        _ title: String, icon: String, note: String, says: String, isOn: Binding<Bool>
    ) -> some View {
        Toggle(isOn: isOn) {
            Label {
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    Text(title)
                    ForEach([note, says].filter { !$0.isEmpty }, id: \.self) { line in
                        Text(line)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                            .monospacedDigit()
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            } icon: {
                Image(icon)
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
    /// Whose the base map and terrain are where a downloaded region is
    /// drawn in place of the map from the network: the region's own
    /// credits. `nil` where the map from the network is drawn.
    var region: [String]?

    /// The base map and the terrain drawn: the region's credits, or the
    /// core's words for the map from the network.
    static func map(drawnFrom region: [String]?) -> [String] {
        if let region, !region.isEmpty {
            return region
        }
        let words = mapLayersWords()
        return [words.baseMapCredit, words.terrainCredit]
    }

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
                    ForEach(Self.map(drawnFrom: region), id: \.self) { line in
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
            .navigationTitle(mapLayersWords().creditsTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
