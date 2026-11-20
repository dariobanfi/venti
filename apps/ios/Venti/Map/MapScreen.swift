import SwiftUI
import VentiCore

/// The map tab: terrain, live wind from the stations Venti knows, its
/// flying sites, airspace, the pilots in the air and the pilot's own
/// flights. The chips along the top switch its layers on and off. A long
/// press shows the forecast for the place pressed.
struct MapScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(WindModel.self) private var wind
    @Environment(AirspaceModel.self) private var airspace
    @Environment(SitesModel.self) private var sites
    @Environment(LiveModel.self) private var live
    @Environment(AccountModel.self) private var account
    @Environment(GroupsModel.self) private var groups
    @Environment(OfflineMapsModel.self) private var maps
    @Environment(AppNavigation.self) private var navigation

    @AppStorage(Preferences.mapHidesWind) private var hidesWind = false
    @AppStorage(Preferences.mapHidesAirspace) private var hidesAirspace = false
    @AppStorage(Preferences.mapHidesSites) private var hidesSites = false
    @AppStorage(Preferences.mapHidesPilots) private var hidesPilots = false
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false

    @State private var isFollowing = false
    @State private var showsPlans = false
    /// The plan whose mark on the map the pilot tapped.
    @State private var chosenPlan: ChosenPlan?
    @State private var problem: String?
    @State private var chosen: ChosenStation?
    @State private var showsStations = false
    @State private var chosenSite: ChosenSite?
    @State private var showsSites = false
    @State private var chosenGlider: ChosenGlider?
    @State private var showsGliders = false
    /// The glider chosen in the list, to open once the list has closed.
    @State private var gliderFromList: String?
    /// A place the pilot pressed on, to show its forecast.
    @State private var pressed: ChosenPlace?
    /// The part of the map in view, as the map last reported it.
    @State private var region: Bounds?

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            OverviewMap(
                outlines: outlines,
                stations: hidesWind ? [] : wind.stations,
                airspace: hidesAirspace ? [] : airspaceInView,
                sites: hidesSites ? [] : sites.sites,
                gliders: hidesPilots ? [] : live.gliders,
                plans: groups.pins(),
                followed: live.followed?.track ?? [],
                isFollowing: $isFollowing,
                problem: $problem,
                onRegion: { region in
                    self.region = region
                    wind.show(region)
                    sites.show(region)
                    live.show(region)
                    // The airspace around what the pilot is looking at,
                    // unless it is the whole continent.
                    if region.north - region.south < 4 {
                        let centre = Coordinate(
                            lat: (region.south + region.north) / 2, lon: (region.west + region.east) / 2
                        )
                        Task {
                            await airspace.refresh(around: centre)
                        }
                    }
                },
                onStation: { id in
                    if let station = wind.stations.first(where: { $0.id == id }) {
                        chosen = ChosenStation(station: station)
                    }
                },
                onSite: { id in
                    if let site = sites.sites.first(where: { $0.id == id }) {
                        chosenSite = ChosenSite(site: site)
                    }
                },
                onGlider: { id in
                    chosenGlider = ChosenGlider(id: id)
                },
                onPlan: { id in
                    chosenPlan = ChosenPlan(id: id)
                },
                // A long press: the forecast for the place under the
                // finger, where there is a server to ask.
                onPlace: { place in
                    if wind.server != nil {
                        pressed = ChosenPlace(place: place)
                    }
                },
                source: maps.browsingSource(at: nil),
                aerial: showsAerial
            )
            // Without a network the map is drawn from a region kept on
            // the iPhone, and from the network again when it is back: the
            // map is made anew when that changes.
            .id(maps.browsingSource(at: nil).key)
            .ignoresSafeArea()
            // One element, not the map's hundreds of labels: everything it
            // shows that can be chosen is in the lists the round buttons
            // open, and reading the labels made every look at this screen
            // by VoiceOver or by a test slow enough to time out.
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Map")
            .accessibilityIdentifier("overview-map")
            .accessibilityHint("Wind stations, flying sites, pilots in the air and flying plans are in the lists.")
            // How many plans the map marks: said, since the marks
            // themselves are not read.
            .accessibilityValue(plansOnTheMap)

            StatusBarScrim()

            VStack(spacing: 12) {
                if wind.server != nil {
                    // Everything the arrows say, as a list: for reading the
                    // numbers side by side, and for pilots who use VoiceOver.
                    mapButton("Wind stations", icon: Image("icon-wind")) {
                        showsStations = true
                    }
                }
                if !sites.sites.isEmpty {
                    // The signs on the map, as a list.
                    mapButton("Flying sites", icon: Image("icon-site")) {
                        showsSites = true
                    }
                }
                if !live.gliders.isEmpty {
                    // Who is in the air on this part of the map, as a list.
                    mapButton("Pilots in the air", icon: Image("icon-live")) {
                        showsGliders = true
                    }
                }
                if account.isSignedIn || wind.server != nil {
                    // Who flies where today and on the days ahead: the
                    // plans of the pilot's groups.
                    mapButton("Flying plans", icon: Image(systemName: "calendar")) {
                        showsPlans = true
                    }
                }
                // The ground under everything: the drawn map, or
                // photographs from above.
                AerialButton()
                mapButton("Show my position", icon: Image(systemName: isFollowing ? "location.fill" : "location")) {
                    isFollowing.toggle()
                }
            }
            .padding(16)
        }
        .overlay(alignment: .top) {
            VStack(spacing: 4) {
                layerChips
                notice
            }
        }
        .overlay(alignment: .bottomLeading) { credits }
        // Asks again every minute while the map is on screen, and at once
        // when the pilot changes the server.
        .task(id: wind.address) {
            await wind.keepCurrent()
        }
        .sheet(item: $chosen) { chosen in
            NavigationStack {
                StationScreen(station: chosen.station)
            }
            .presentationDetents([.medium, .large])
        }
        .sheet(isPresented: $showsPlans) {
            PlansSheet()
        }
        .sheet(item: $chosenPlan) { chosen in
            NavigationStack {
                PlanScreen(id: chosen.id)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") {
                                chosenPlan = nil
                            }
                        }
                    }
            }
            .presentationDetents([.medium, .large])
        }
        // The plans of the pilot's groups, for their marks on the map:
        // asked for when the map comes up, and again after signing in.
        .task(id: account.isSignedIn) {
            await groups.load()
        }
        .sheet(isPresented: $showsStations) {
            StationList()
        }
        .sheet(item: $chosenSite) { chosen in
            NavigationStack {
                SiteScreen(site: chosen.site)
            }
            .presentationDetents([.medium, .large])
        }
        .sheet(isPresented: $showsSites) {
            SiteList()
        }
        // Other pilots are asked for again every quarter of a minute
        // while the map is on screen, and not when it is not.
        .task(id: wind.address) {
            await live.keepCurrent()
        }
        .sheet(item: $chosenGlider) { chosen in
            NavigationStack {
                LiveGliderScreen(id: chosen.id)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") {
                                chosenGlider = nil
                            }
                        }
                    }
            }
            .presentationDetents([.medium, .large])
        }
        .sheet(
            isPresented: $showsGliders,
            onDismiss: {
                // One sheet at a time: the glider's page follows the list.
                if let id = gliderFromList {
                    gliderFromList = nil
                    chosenGlider = ChosenGlider(id: id)
                }
            }
        ) {
            LiveList { id in
                gliderFromList = id
            }
        }
        .sheet(item: $pressed) { chosen in
            NavigationStack {
                ForecastScreen(place: chosen.place, title: Format.place(lat: chosen.place.lat, lon: chosen.place.lon))
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") {
                                pressed = nil
                            }
                        }
                    }
            }
            // At half height first, like a site's and a station's page:
            // the place that was pressed stays in view above it.
            .presentationDetents([.medium, .large])
        }
        // A sheet of the map that sends the pilot to another tab, a site's
        // "Start flight" for one, closes behind them.
        .onChange(of: navigation.tab) { _, tab in
            if tab != .map {
                chosen = nil
                chosenSite = nil
                chosenPlan = nil
                chosenGlider = nil
                pressed = nil
                showsStations = false
                showsSites = false
                showsGliders = false
                showsPlans = false
            }
        }
        .alert(
            "Position unavailable",
            isPresented: Binding(
                get: { problem != nil },
                set: { isPresented in
                    if !isPresented {
                        problem = nil
                    }
                }
            ),
            presenting: problem
        ) { _ in
            Button("OK", role: .cancel) {}
        } message: { problem in
            Text(problem)
        }
    }

    /// One of the round buttons over the map, as the design language has
    /// them: the sign alone, the title for VoiceOver.
    private func mapButton(_ title: String, icon: Image, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label {
                Text(title)
            } icon: {
                icon
            }
        }
        .buttonStyle(.ventiRound)
    }

    /// The chips that switch the map's layers on and off: what Venti
    /// gives the map, each only where there is something to switch.
    private var layerChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                if wind.server != nil {
                    VentiChip(title: "Wind", icon: "icon-wind", isOn: shown($hidesWind))
                        .accessibilityIdentifier("layer-wind")
                }
                VentiChip(title: "Airspace", icon: "icon-airspace", isOn: shown($hidesAirspace))
                    .accessibilityIdentifier("layer-airspace")
                if wind.server != nil {
                    VentiChip(title: "Sites", icon: "icon-site", isOn: shown($hidesSites))
                        .accessibilityIdentifier("layer-sites")
                    VentiChip(title: "Pilots", icon: "icon-live", isOn: shown($hidesPilots))
                        .accessibilityIdentifier("layer-pilots")
                }
            }
            .padding(.horizontal, 16)
            // Room for the chips' shadows, which the scrolling row would
            // cut off.
            .padding(.bottom, 12)
        }
        .scrollClipDisabled()
    }

    /// A chip is on while its layer is not hidden.
    private func shown(_ isHidden: Binding<Bool>) -> Binding<Bool> {
        Binding {
            !isHidden.wrappedValue
        } set: { isShown in
            isHidden.wrappedValue = !isShown
        }
    }

    /// What the pilot should know about the wind on the map, if anything.
    private var noticeText: String? {
        if let problem = wind.problem {
            return problem
        }
        if wind.isTruncated {
            return "There are more stations here than fit on the map. Zoom in to see them all."
        }
        if !wind.lateProviders.isEmpty {
            return "No new wind data from \(wind.lateProviders.formatted(.list(type: .and))) for a while."
        }
        return nil
    }

    @ViewBuilder private var notice: some View {
        if let noticeText {
            Label(noticeText, systemImage: "exclamationmark.triangle")
                .font(.venti(.footnote))
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .ventiFloating(in: .rect(cornerRadius: 14))
                .padding(.horizontal, 16)
                .accessibilityIdentifier("wind-notice")
        }
    }

    /// Whose wind and airspace are on the map. Their licences ask for the
    /// credit next to the data; the full wording is on each station's page
    /// and on the airspace screen.
    private var creditLine: String {
        var parts: [String] = []
        if !wind.credits.isEmpty, !hidesWind {
            parts.append("Wind: \(wind.credits.map(\.provider).formatted(.list(type: .and)))")
        }
        if !airspace.credits.isEmpty, !hidesAirspace, let region, region.east - region.west < 6 {
            // What the providers of airspace ask to be said is said here
            // in two words and in full on the airspace screen.
            let certified = airspace.credits.contains { $0.notice != nil } ? ", not certified" : ""
            parts.append("Airspace: \(airspace.credits.map(\.provider).formatted(.list(type: .and)))\(certified)")
        }
        if !sites.credits.isEmpty, !sites.sites.isEmpty, !hidesSites, let region, region.east - region.west <= SitesModel.widestMap {
            parts.append("Sites: \(sites.credits.map(\.provider).formatted(.list(type: .and)))")
        }
        let line = parts.joined(separator: " · ")
        // Who made the photographs, on a line of its own while they show.
        guard showsAerial else { return line }
        return line.isEmpty ? AerialButton.credit : line + "\n" + AerialButton.credit
    }

    @ViewBuilder private var credits: some View {
        if !creditLine.isEmpty {
            Text(creditLine)
                .font(.venti(.caption2))
                .foregroundStyle(.venti(.textSecondary))
                .lineLimit(5)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .ventiFloating(in: .rect(cornerRadius: 10))
                // Clear of the map's own credits button on the left and of
                // the round buttons on the right.
                .padding(.leading, 44)
                .padding(.trailing, 96)
                .padding(.bottom, 8)
                .accessibilityIdentifier("wind-credits")
        }
    }

    /// "2 flying plans marked", or nothing where the map marks none.
    private var plansOnTheMap: String {
        let count = groups.pins().count
        switch count {
        case 0: return ""
        case 1: return "1 flying plan marked"
        default: return "\(count) flying plans marked"
        }
    }

    /// The airspace in the part of the map in view. Zoomed far out, a
    /// country's airspace is a tangle and is not drawn, so it is not
    /// asked for either.
    private var airspaceInView: [AirspaceShape] {
        guard let region, region.east - region.west < 6 else { return [] }
        return airspace.shapes(in: region)
    }

    private var outlines: [[Coordinate]] {
        logbook.view.years.flatMap { year in
            year.flights.map(\.outline)
        }
    }
}

/// A station chosen on the map, to present its page.
/// A flying site chosen on the map, for a sheet.
/// A plan chosen on the map, by its identifier: the sheet reads the plan
/// from the board, so that it shows what the pilot changes in it.
struct ChosenPlan: Identifiable {
    let id: String
}

struct ChosenSite: Identifiable {
    let site: SiteView
    var id: String { site.id }
}

/// A place pressed on the map, for a sheet.
struct ChosenPlace: Identifiable {
    let place: Coordinate
    var id: String { "\(place.lat),\(place.lon)" }
}

struct ChosenStation: Identifiable {
    let station: WindStation
    var id: String { station.id }
}
