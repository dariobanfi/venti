import SwiftUI
import VentiCore

/// The map tab: terrain, live wind from the stations Venti knows, its
/// flying sites, airspace, the pilots in the air, the pilot's own flights
/// the forecast map, the kind of day square by square, and the rain
/// radar. The chips
/// along the top switch its layers on and off. A long press shows the
/// forecast for the place pressed.
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
    @Environment(ThermalMapModel.self) private var thermals
    @Environment(RadarModel.self) private var radar
    @Environment(\.scenePhase) private var scenePhase

    @AppStorage(Preferences.mapHidesWind) private var hidesWind = false
    @AppStorage(Preferences.mapHidesAirspace) private var hidesAirspace = false
    @AppStorage(Preferences.mapHidesSites) private var hidesSites = false
    @AppStorage(Preferences.mapHidesPilots) private var hidesPilots = false
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false
    @AppStorage(Preferences.mapShowsForecast) private var showsForecast = false
    @AppStorage(Preferences.mapShowsRadar) private var showsRadar = false
    @AppStorage(Preferences.mapAirspaceCeilingM) private var airspaceCeilingM = 0.0

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
    /// The date of the forecast map's day the pilot chose; the first day
    /// the map has until then.
    @State private var forecastDate = ""
    /// A square of the forecast map the pilot tapped.
    @State private var chosenSquare: ChosenSquare?
    /// The forecast map's layer and the wind's level, as the pilot left
    /// them; the hour is chosen anew.
    @AppStorage(Preferences.mapForecastLayer) private var forecastLayerStored = "thermals"
    @AppStorage(Preferences.mapForecastLevelM) private var forecastLevelStored = -1
    /// The hour of the layer chosen, by its place in the day's hours;
    /// `nil` for the day as a whole.
    @State private var forecastHour: Int?
    /// The layer of a day whose first hour was chosen last.
    @State private var openedHours: ThermalMapModel.HoursKey?
    /// The hour the map draws, worked out when the hour or the layer
    /// changes and not with every drawing of the screen.
    @State private var hourDrawing: ForecastHourDrawing?
    /// A square of an hour the pilot tapped.
    @State private var chosenHourSquare: ChosenHourSquare?
    /// A place where airspace is drawn, tapped.
    @State private var chosenAirspace: ChosenAirspacePlace?
    /// Who made the photographs in view on the map.
    @State private var photos = PhotoCredits()

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
                aerial: showsAerial,
                photos: photos,
                forecast: dayOnTheMap,
                onSquare: { index in
                    guard let day = forecastDay, let map = thermals.map, day.cells.indices.contains(index) else {
                        return
                    }
                    chosenSquare = ChosenSquare(
                        dayName: day.name, cell: day.cells[index], note: map.note, credits: map.credits,
                        opening: ForecastOpening(date: day.date, model: day.model),
                        id: "\(day.date)-\(index)"
                    )
                },
                // A tap where airspace is drawn and nothing else of the
                // app's lies: what is over the place.
                onAirspace: { place in
                    guard !hidesAirspace, let over = airspace.over(place),
                          Self.isDrawn(over, among: airspaceInView)
                    else { return }
                    chosenAirspace = ChosenAirspacePlace(over: over)
                },
                forecastHour: hourDrawing,
                onHourPlace: { place in
                    openHourSquare(at: place)
                },
                radar: isRadarOn ? radar.picture : nil
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
        .overlay(alignment: .bottomLeading) {
            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                if isForecastOn {
                    forecastCard
                }
                if isRadarOn {
                    radarCard
                }
                credits
            }
            // Under the row of layer chips, however high the card is with
            // its key and notes open and the text large.
            .padding(.top, VentiSize.touch + VentiSpace.s3)
        }
        // Asks again every minute while the map is on screen, and at once
        // when the pilot changes the server.
        .task(id: wind.address) {
            await wind.keepCurrent()
        }
        // The forecast map: asked for when the layer is switched on, and
        // again when the answer is half an hour old, while it is on.
        .task(id: isForecastOn ? wind.address : "") {
            guard isForecastOn else { return }
            await thermals.keepCurrent()
        }
        // The rain radar: asked for when the layer is switched on, again
        // every five minutes while it is on and the map in front, and when
        // the app comes back to the front.
        .task(id: RadarWatch(isOn: isRadarOn, address: wind.address, isInFront: scenePhase == .active)) {
            guard isRadarOn, scenePhase == .active else {
                radar.stop()
                return
            }
            await radar.keepCurrent()
        }
        // The hours of the layer chosen, for the day chosen: asked for
        // when one of them changes, or a new map of the days comes.
        .task(id: HoursRequest(key: hoursKey, map: thermals.map?.computedUnixS)) {
            await thermals.showHours(hoursKey)
        }
        .onChange(of: HoursShown(key: thermals.hoursKey, state: thermals.hoursState)) { _, _ in
            openHours()
        }
        .onChange(of: forecastHour) { _, _ in
            drawHour()
        }
        .sheet(item: $chosenHourSquare) { chosen in
            ForecastHourSquareSheet(chosen: chosen, choose: { date in
                forecastDate = date
            })
            .presentationDetents([.medium, .large])
        }
        .sheet(item: $chosenAirspace) { chosen in
            AirspaceOverPlaceSheet(over: chosen.over)
                .presentationDetents([.medium, .large])
        }
        .sheet(item: $chosenSquare) { square in
            ForecastSquareSheet(square: square)
                // At half height first: the square stays in view above it.
                .presentationDetents([.medium, .large])
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
                chosenSquare = nil
                chosenAirspace = nil
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
                    // The kind of day, square by square: off until the
                    // pilot switches it on.
                    VentiChip(title: "Forecast", icon: "icon-forecast", isOn: $showsForecast)
                        .accessibilityIdentifier("forecast-chip")
                    VentiChip(title: "Wind", icon: "icon-wind", isOn: shown($hidesWind))
                        .accessibilityIdentifier("layer-wind")
                    // The rain the radars saw: off until the pilot
                    // switches it on. The pack has no sign for rain.
                    VentiChip(title: "Radar", isOn: $showsRadar)
                        .accessibilityIdentifier("radar-chip")
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
        if forecastDay != nil, let map = thermals.map, !map.credits.isEmpty {
            parts.append("Forecast: \(map.credits.map(\.provider).formatted(.list(type: .and)))")
        }
        // The radar's source asks for its credit at the data: here by
        // name, and in full behind the card's "Key and notes".
        if isRadarOn, radar.picture != nil, let credit = radar.layer?.credit {
            parts.append("Radar: \(credit.provider)")
        }
        let line = parts.joined(separator: " · ")
        // Who made the photographs, on a line of its own while they show.
        guard showsAerial else { return line }
        guard !photos.line.isEmpty else { return line }
        return line.isEmpty ? photos.line : line + "\n" + photos.line
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

    /// Whether the forecast map is on: switched on, with a Venti to ask.
    private var isForecastOn: Bool {
        showsForecast && wind.server != nil
    }

    /// Whether the rain radar is on: switched on, with a Venti to ask.
    private var isRadarOn: Bool {
        showsRadar && wind.server != nil
    }

    /// The rain radar's card, under the forecast map's where both are on.
    private var radarCard: some View {
        RadarCard(
            radar: radar,
            tryAgain: {
                Task {
                    await radar.load()
                }
            }
        )
        .padding(.leading, VentiSpace.s4)
        .padding(.trailing, VentiSize.touch + 2 * VentiSpace.s4)
        .padding(.bottom, creditLine.isEmpty ? VentiSize.touch : 0)
    }

    /// The day of the forecast map that is drawn, while the layer is on.
    private var forecastDay: ThermalMapDay? {
        guard isForecastOn, let map = thermals.map else { return nil }
        return ThermalSquares.day(forecastDate, in: map)
    }

    /// Whether a square of that day lies in the part of the map in view.
    private var isForecastInView: Bool {
        guard let day = forecastDay else { return false }
        guard let region else { return true }
        return ThermalSquares.anyDrawn(of: day, in: region)
    }

    /// The day shown, as the card's control chooses it.
    private var chosenForecastDate: Binding<String> {
        Binding {
            forecastDay?.date ?? ""
        } set: { date in
            forecastDate = date
        }
    }

    /// The layer of the forecast map chosen.
    private var forecastLayer: Binding<ThermalHoursLayer> {
        Binding {
            ThermalHoursLayer(stored: forecastLayerStored)
        } set: { layer in
            forecastLayerStored = layer.stored
        }
    }

    /// The height of the wind chosen; `nil` at the ground.
    private var forecastLevel: Binding<Int32?> {
        Binding {
            forecastLevelStored < 0 ? nil : Int32(forecastLevelStored)
        } set: { height in
            forecastLevelStored = height.map(Int.init) ?? -1
        }
    }

    /// The layer of the day to ask for, where the map offers layers.
    private var hoursKey: ThermalMapModel.HoursKey? {
        guard let day = forecastDay, let map = thermals.map, !map.layers.isEmpty else { return nil }
        let layer = ThermalHoursLayer(stored: forecastLayerStored)
        let level = forecastLevel.wrappedValue
        let offered = map.levels.contains { $0.heightM == level }
        return ThermalMapModel.HoursKey(
            date: day.date, layer: layer, heightM: layer == .wind && offered ? level : nil
        )
    }

    /// The day's squares, where the map shows the day: not while an hour
    /// is shown, nor while one is asked for, when the squares of the layer
    /// before are taken away.
    private var dayOnTheMap: ThermalMapDay? {
        if hourDrawing != nil || thermals.hoursState == .loading {
            return nil
        }
        return forecastDay
    }

    /// When the hours of a layer have come: the hour it opens at, "Day"
    /// on "Thermals", the core's first hour on the others; then drawn.
    private func openHours() {
        if thermals.hoursState == .loaded, let key = thermals.hoursKey, key != openedHours,
           let hours = thermals.hours
        {
            openedHours = key
            let first = Int(hours.view().firstHour)
            forecastHour = key.layer == .thermals ? nil : first
        }
        drawHour()
    }

    /// Works out the hour the map draws.
    private func drawHour() {
        guard isForecastOn, thermals.hoursState == .loaded, let hours = thermals.hours,
              let key = thermals.hoursKey, let hour = forecastHour,
              let choice = thermals.map?.layers.first(where: { $0.layer == key.layer })
        else {
            hourDrawing = nil
            return
        }
        let view = hours.view()
        guard view.hours.indices.contains(hour) else {
            hourDrawing = nil
            return
        }
        let shapes = hours.shapes(hour: UInt32(hour))
        hourDrawing = ForecastHourDrawing(
            id: "\(key.date) \(key.layer.stored) \(key.heightM.map(String.init) ?? "ground") \(hour)",
            shapes: shapes, key: choice.legend
        )
    }

    /// A tap on the map while an hour is drawn: the square at the place,
    /// drawn or not, with its days side by side.
    private func openHourSquare(at place: Coordinate) {
        guard let hours = thermals.hours, let hour = forecastHour, let key = thermals.hoursKey,
              let cell = hours.cellAt(lat: place.lat, lon: place.lon),
              let square = hours.square(cell: cell, hour: UInt32(hour)),
              let map = thermals.map
        else { return }
        let view = hours.view()
        let middle = Coordinate(lat: (square.south + square.north) / 2, lon: (square.west + square.east) / 2)
        Task {
            let strip = await thermals.strip(at: middle)
            chosenHourSquare = ChosenHourSquare(
                square: square, layer: key.layer, days: strip, date: key.date,
                note: [map.layers.first { $0.layer == key.layer }?.note ?? "", view.note]
                    .filter { !$0.isEmpty }.joined(separator: "\n\n"),
                credits: view.credits,
                opening: ForecastOpening(date: key.date, model: view.model),
                id: "\(key.date)-\(cell)-\(hour)"
            )
        }
    }

    /// The forecast map's card, clear of the round buttons on the right
    /// and over the line of credits, which stands clear of the map's own
    /// credits button.
    private var forecastCard: some View {
        ForecastCard(
            state: thermals.state,
            map: thermals.map,
            day: forecastDay,
            isInView: isForecastInView,
            date: chosenForecastDate,
            tryAgain: {
                Task {
                    await thermals.load()
                }
            },
            layer: forecastLayer,
            heightM: forecastLevel,
            hoursState: thermals.hoursState,
            hours: thermals.hours?.view(),
            hour: $forecastHour,
            tryHoursAgain: {
                Task {
                    await thermals.showHours(hoursKey)
                }
            }
        )
        .padding(.leading, VentiSpace.s4)
        .padding(.trailing, VentiSize.touch + 2 * VentiSpace.s4)
        // Room under it for the map's credits button where there is no
        // line of credits beside that button.
        .padding(.bottom, creditLine.isEmpty && !isRadarOn ? VentiSize.touch : 0)
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
        return airspace.shapes(in: region, below: airspaceCeilingM)
    }

    /// Whether an airspace over the place is one the map draws: one the
    /// filter by height took off the map is not tapped on.
    static func isDrawn(_ over: AirspaceOverPlace, among drawn: [AirspaceShape]) -> Bool {
        let ids = Set(drawn.map(\.id))
        return over.airspaces.contains { ids.contains($0.id) }
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

/// What the hours are asked for by: the layer of the day, and the map of
/// the days it belongs to.
private struct HoursRequest: Hashable {
    let key: ThermalMapModel.HoursKey?
    let map: Int64?
}

/// What the rain radar is asked by: whether its layer is on, which Venti,
/// and whether the app is in front.
private struct RadarWatch: Hashable {
    let isOn: Bool
    let address: String
    let isInFront: Bool
}

/// What the hours shown are, and how far they are there.
private struct HoursShown: Equatable {
    let key: ThermalMapModel.HoursKey?
    let state: ThermalMapModel.HoursState
}
