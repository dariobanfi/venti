import SwiftUI
import VentiCore

/// The pages of the cockpit. A swipe sideways goes from one to the next,
/// as an instrument's pages do, and a tap on the dots in the top line goes
/// around them.
enum CockpitPage: CaseIterable {
    /// The readings over the map: what the pilot flies by.
    case instruments
    /// The map as large as it gets, with the way to the place flown to.
    case map
    /// How the flight stands, and what can be switched.
    case flight
    /// A competition task's readings, as an instrument's competition page
    /// has them: there only while a task is flown.
    case task

    /// The page after this one; the task's only while there is a task.
    func next(hasTask: Bool) -> CockpitPage {
        switch self {
        case .instruments: .map
        case .map: .flight
        case .flight: hasTask ? .task : .instruments
        case .task: .instruments
        }
    }

    /// The page before this one: the way back of `next`.
    func previous(hasTask: Bool) -> CockpitPage {
        switch self {
        case .instruments: hasTask ? .task : .flight
        case .map: .instruments
        case .flight: .map
        case .task: .flight
        }
    }

    /// The pages there are, in their order.
    static func all(hasTask: Bool) -> [CockpitPage] {
        hasTask ? [.instruments, .map, .flight, .task] : [.instruments, .map, .flight]
    }

    /// The page in a word, for a pilot who has the screen read out.
    var name: String {
        switch self {
        case .instruments: "Instruments"
        case .map: "Map"
        case .flight: "Flight"
        case .task: "Task"
        }
    }
}

/// The cockpit: what the pilot looks at in the air, in the design
/// language's flight theme: a white ground and near-black ink whatever the
/// iPhone is set to, one very large reading and four large ones, and no
/// button that is not a reading: the screen is the readings'.
///
/// It has three pages: the readings over the map, the map with the way to
/// the place flown to, and the flight with what can be switched. A swipe
/// sideways changes the page; the dots in the top line say which it is,
/// and a tap on them goes on. "End", held, at the end of the top line ends
/// the session. Warnings stand at the top of every page. Nothing needs to
/// be touched in flight:
/// while the core says the glider is circling in lift the readings change
/// from cruise numbers to thermal numbers and the map zooms in.
struct CockpitScreen: View {
    /// Called when the pilot has held "End" in the top line.
    let onFinish: () -> Void

    @Environment(FlySession.self) private var fly
    @Environment(AirspaceModel.self) private var airspace
    @Environment(TaskModel.self) private var tasks
    @Environment(OfflineMapsModel.self) private var maps
    @Environment(CablesModel.self) private var cables
    @State private var page = CockpitPage.instruments
    @State private var isShowingTask = false
    @State private var isChoosingTarget = false
    @State private var isShowingAirspace = false
    @State private var isShowingWinds = false
    /// Whether the wind's sheet begins with the wind on the ground: it
    /// was opened from that badge or from its warning.
    @State private var windsBeginOnTheGround = false
    @State private var isShowingAhead = false
    @State private var isShowingLive = false
    /// Whether the screen takes no touches: against a glove, a line or
    /// rain pressing a button in flight.
    @State private var isLocked = false
    /// Whether the map shows the whole flight instead of following closely.
    @State private var showsWholeFlight = false
    @State private var isShowingDetails = false
    @AppStorage(Preferences.showsFaiAreas) private var showsFaiAreas = false
    /// The aerial view: one choice for every map of the app.
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false
    /// Who made the photographs in view on the map.
    @State private var photos = PhotoCredits()

    /// Whether the map shows the photographs: as chosen, on the pages where
    /// the map is large. The first page's small map keeps the drawn ground,
    /// which has no credit to carry.
    private var mapIsAerial: Bool {
        showsAerial && page != .instruments
    }

    /// Whether the cockpit's map is drawn from a region kept on the iPhone.
    private var mapIsKept: Bool {
        if case .kept = maps.flyingSource {
            return true
        }
        return false
    }

    var body: some View {
        if let view = fly.view {
            VStack(spacing: 0) {
                StatusLine(
                    view: view, mode: fly.mode, varioName: fly.vario.connectedName, mapIsKept: mapIsKept,
                    pages: CockpitPage.all(hasTask: hasTask), page: page,
                    nextPage: { page = page.next(hasTask: hasTask) },
                    previousPage: { page = page.previous(hasTask: hasTask) },
                    end: onFinish
                )
                alerts(view)
                switch page {
                case .instruments:
                    Instruments(view: view, way: fly.way) {
                        windsBeginOnTheGround = false
                        isShowingWinds = true
                    }
                case .map:
                    EmptyView()
                case .task:
                    if let readings = view.taskReadings {
                        TaskReadingsPage(readings: readings)
                    }
                case .flight:
                    FlightPage(view: view, score: fly.score, isReplay: isReplay) {
                        isLocked = true
                    } showLive: {
                        isShowingLive = true
                    } showDetails: {
                        isShowingDetails = true
                    }
                }
                CockpitRule()
                // The map alone gives way when a warning takes room at the
                // top: the readings keep their size and their place.
                map(view)
                    .layoutPriority(-1)
                CockpitRule()
                if page == .map {
                    // The airspace that matters next, large, while the
                    // cockpit has airspace to warn from.
                    if fly.airspaceCheck != .noAirspace {
                        AirspaceProximityStrip(proximity: view.airspaceProximity) {
                            isShowingAirspace = true
                        }
                        CockpitRule()
                    }
                    // With a task: where the flight stands in it, above
                    // the line that says where to fly, which is then its
                    // next turnpoint unless the pilot chose another place.
                    if let status = fly.task {
                        TaskStrip(status: status) {
                            isShowingTask = true
                        }
                    }
                    TargetStrip(way: fly.way, courseDeg: Self.course(of: view)) {
                        isChoosingTarget = true
                    }
                    MapReadings(view: view)
                }
            }
            .background(.venti(.bg))
            // A swipe sideways, anywhere, changes the page: to the left
            // the next, to the right the one before, as an instrument's
            // pages go. One more up or down than sideways is not one.
            .simultaneousGesture(
                DragGesture(minimumDistance: VentiSpace.s6)
                    .onEnded { drag in
                        guard !isLocked else { return }
                        let sideways = drag.translation.width
                        guard abs(sideways) >= VentiSpace.s7,
                            abs(sideways) > 1.5 * abs(drag.translation.height)
                        else { return }
                        page = sideways < 0 ? page.next(hasTask: hasTask) : page.previous(hasTask: hasTask)
                    }
            )
            // With the first position, and a network, the airspace around
            // it is fetched if what the iPhone keeps is old or from
            // elsewhere. A change reaches the running session by itself.
            .task(id: view.position != nil) {
                if let position = view.position {
                    await airspace.refresh(around: position)
                }
            }
            // The cables around the glider, as the airspace is, and again
            // once it has flown far from where they were asked for.
            .task(id: Self.cablesKey(view.position)) {
                if let position = view.position {
                    await cables.refresh(around: position)
                }
            }
            // Without a task its page is not there.
            .onChange(of: view.taskReadings == nil) { _, hasNone in
                if hasNone, page == .task {
                    page = .instruments
                }
            }
            // Locked: a clear sheet over everything takes the touches,
            // swipes and "End" among them, and a lock at the foot of the
            // screen opens it when held. The
            // cockpit goes on showing and sounding as before.
            .overlay {
                if isLocked {
                    Color.clear
                        .contentShape(.rect)
                        .ignoresSafeArea()
                        .accessibilityHidden(true)
                        .overlay(alignment: .bottom) {
                            Label("Locked. Hold to unlock", systemImage: "lock.fill")
                                .font(.ventiType(.heading))
                                .frame(maxWidth: .infinity, minHeight: VentiSize.touchInFlight)
                                .ventiFloating(in: .capsule)
                                .contentShape(.capsule)
                                .onLongPressGesture(minimumDuration: 0.8) {
                                    isLocked = false
                                }
                                .accessibilityElement(children: .ignore)
                                .accessibilityLabel("Unlock the screen")
                                .accessibilityHint("Hold to unlock")
                                .accessibilityAddTraits(.isButton)
                                .accessibilityAction {
                                    isLocked = false
                                }
                                .accessibilityIdentifier("cockpit-unlock")
                                .padding(.horizontal, VentiSpace.s4)
                                .padding(.vertical, VentiSpace.s3)
                        }
                }
            }
            .sheet(isPresented: $isShowingAirspace) {
                AirspaceWarningsSheet()
                    .presentationDetents([.medium, .large])
                    .ventiFlightThemed()
            }
            .sheet(isPresented: $isShowingWinds) {
                WindByHeightSheet(groundFirst: windsBeginOnTheGround)
                    .presentationDetents([.medium, .large])
                    .ventiFlightThemed()
            }
            .sheet(isPresented: $isShowingDetails) {
                CockpitDetailsSheet()
                    .presentationDetents([.medium, .large])
                    .ventiFlightThemed()
            }
            .sheet(isPresented: $isShowingAhead) {
                AirspaceAheadSheet()
                    .presentationDetents([.large])
                    .ventiFlightThemed()
            }
            .sheet(isPresented: $isShowingTask) {
                TaskProgressSheet()
                    .presentationDetents([.medium, .large])
                    .ventiFlightThemed()
            }
            .sheet(isPresented: $isShowingLive) {
                NavigationStack {
                    LiveScreen()
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button("Done") {
                                    isShowingLive = false
                                }
                            }
                        }
                }
                .presentationDetents([.medium, .large])
                .ventiFlightThemed()
            }
            .sheet(isPresented: $isChoosingTarget) {
                TargetPicker(
                    position: view.position,
                    chosen: fly.chosenPlace,
                    fliesTask: fly.task != nil
                ) { waypoint in
                    fly.goTo(waypoint?.place)
                }
                .ventiFlightThemed()
            }
            .ventiFlightThemed()
        }
    }

    /// What the pilot must know now, at the top of every page: airspace,
    /// a strong wind at the landing, and where the flight is in its life
    /// while it is not in the air.
    private func alerts(_ view: CockpitView) -> some View {
        VStack(spacing: VentiSpace.s2) {
            AirspaceBanner(warnings: fly.airspaceWarnings, check: fly.airspaceCheck) {
                isShowingAirspace = true
            }
            // The cable ahead, under the airspace warning.
            if let cable = fly.cableWarning {
                CableBanner(warning: cable) {
                    fly.acknowledgeCable(cable.id)
                }
            }
            // The wind carries the glider backwards on a glide: the core's
            // words, among the cautions.
            if let words = view.carriedBack {
                CautionBand(text: words)
                    .accessibilityIdentifier("carried-back")
            }
            if let warning = fly.windWarning {
                GroundWindBanner(alert: warning) {
                    windsBeginOnTheGround = true
                    isShowingWinds = true
                } putAway: {
                    fly.putWindWarningAway()
                }
            }
            if let notice = PhaseNotice(view: view, isReplay: isReplay) {
                notice
            }
            // Something that went wrong while the cockpit is open is said
            // here, among its notices, and put away with a tap. A box over
            // the screen would hide the readings, and one asked for while
            // the cockpit opens keeps it from opening.
            if let problem = fly.problem {
                ProblemNotice(text: problem) {
                    fly.problem = nil
                }
            }
        }
        .padding(.horizontal, VentiSpace.s4)
        .padding(.bottom, VentiSpace.s2)
        .fixedSize(horizontal: false, vertical: true)
    }

    /// The room under the readings. On the first page it is shared while
    /// the core says the glider is circling, and for as long after as the
    /// core keeps the circles: the thermal view on the left, a little
    /// under half the width, and the map on the right. On a glide, and on
    /// the other pages, the map has all of it.
    private func map(_ view: CockpitView) -> some View {
        GeometryReader { proxy in
            HStack(spacing: 0) {
                if page == .instruments, view.circling || view.lift != nil {
                    ThermalCloseUp(view: view)
                        .frame(width: (proxy.size.width * 0.48).rounded())
                    Rectangle()
                        .fill(.venti(.border))
                        .frame(width: 2)
                }
                theMap(view)
            }
        }
        .frame(minHeight: 120)
    }

    /// The map, with what lies over it. On the first page that is little:
    /// the map is small there, and the wind is among the readings.
    private func theMap(_ view: CockpitView) -> some View {
        CockpitMap(
            view: view, way: fly.way, airspace: fly.airspace(),
            airspaceDistance: fly.airspaceWarnings.first(where: { !$0.silenced })?.distanceM,
            route: fly.score?.route ?? [],
            fai: showsFaiAreas ? fly.faiAreas : nil,
            range: fly.range,
            gliders: fly.gliders,
            cables: cables.lines,
            closing: fly.score?.closing,
            takeoffs: fly.takeoffsNear?.takeoffs ?? [],
            others: fly.othersNear?.aircraft ?? [],
            showsWholeFlight: showsWholeFlight,
            task: fly.task == nil ? nil : tasks.task,
            taskStatus: fly.task,
            source: maps.flyingSource,
            // On the first page the thermal view beside it is where the
            // circle is looked at.
            followsTheCircle: page != .instruments,
            aerial: mapIsAerial,
            photos: photos
        )
            // Drawn from the region kept on the iPhone wherever one covers
            // the glider: the map is made anew when the glider comes into
            // one or leaves it.
            .id(maps.flyingSource.key)
            // The map is left out of the accessibility tree: what it shows
            // is in the readings, and its hundreds of map labels made
            // reading the screen by code or by VoiceOver time out. The
            // badges laid over it stay in.
            .accessibilityHidden(true)
            .frame(minHeight: 120)
            .overlay(alignment: .topTrailing) {
                VStack(alignment: .trailing, spacing: VentiSpace.s2) {
                    if page != .instruments, let wind = view.wind {
                        // A tap lists the wind of every band of height the
                        // glider has circled in.
                        WindBadge(wind: wind, ageS: view.windAgeS)
                            .contentShape(.capsule)
                            .onTapGesture {
                                windsBeginOnTheGround = false
                                isShowingWinds = true
                            }
                            .accessibilityAddTraits(.isButton)
                            .accessibilityHint("Shows the wind by height")
                            .accessibilityIdentifier("wind-badge")
                    }
                    // A wind that leaves the glider little way into it,
                    // said where the wind is read.
                    if let words = view.strongWind {
                        StrongWindBadge(words: words)
                    }
                    // The wind on the ground: the station that speaks for
                    // the landing, else the nearest. A tap lists the
                    // stations around.
                    if let station = fly.windBelow?.stations.first {
                        GroundWindBadge(station: station)
                            .contentShape(.capsule)
                            .onTapGesture {
                                windsBeginOnTheGround = true
                                isShowingWinds = true
                            }
                            .accessibilityAddTraits(.isButton)
                            .accessibilityHint("Shows the wind stations around")
                            .accessibilityIdentifier("ground-wind-badge")
                    }
                    // While circling: which way the lift is stronger, in
                    // words beside the picture on the map. On the first
                    // page they stand under the thermal view.
                    if page != .instruments, let centre = view.lift?.centre {
                        LiftBadge(centre: centre)
                    }
                }
                .padding(VentiSpace.s2)
            }
            .overlay(alignment: .bottomLeading) {
                if page == .map {
                    VStack(alignment: .leading, spacing: VentiSpace.s2) {
                        // The ground under everything: the drawn map, or
                        // photographs from above, as on the app's other maps.
                        CockpitAerialButton(isOn: $showsAerial)
                        HStack(spacing: VentiSpace.s2) {
                            // The whole flight at a glance, with the scored
                            // route and where to go for an FAI triangle; again
                            // for the close view that follows the glider.
                            MapButton(
                                name: showsWholeFlight ? "Follow the glider" : "Show the whole flight",
                                symbol: showsWholeFlight ? "location.fill" : "arrow.up.left.and.arrow.down.right"
                            ) {
                                showsWholeFlight.toggle()
                            }
                            // With airspace to warn from, or the ground to
                            // draw: what lies on the line ahead, seen from the
                            // side.
                            if fly.airspaceCheck != .noAirspace || view.groundElevationM != nil {
                                MapButton(name: "Ahead, from the side", symbol: "chart.line.downtrend.xyaxis") {
                                    isShowingAhead = true
                                }
                            }
                        }
                    }
                    .padding(VentiSpace.s2)
                    // Clear of the map's own credits in the corner.
                    .padding(.bottom, VentiSpace.s5)
                }
            }
            .overlay(alignment: .topLeading) {
                VStack(alignment: .leading, spacing: VentiSpace.s2) {
                    if let daylight = fly.daylight {
                        DaylightBadge(daylight: daylight)
                    }
                    // The landing to think of first, and whether the glide
                    // reaches it. A tap lists the places to fly to, each
                    // with its reach.
                    if let landing = fly.landing {
                        LandingBadge(landing: landing)
                            .contentShape(.capsule)
                            .onTapGesture {
                                isChoosingTarget = true
                            }
                            .accessibilityAddTraits(.isButton)
                            .accessibilityHint("Lists the places to fly to")
                    }
                    // The nearest landing field within glide, or the core's
                    // caution that none is: one line, on the map page.
                    if page == .map, let nearest = view.nearestLanding {
                        MapWords(text: nearest.words, identifier: "nearest-landing", isCaution: !nearest.withinGlide)
                    }
                    // Beside the large map, the distance so far and how it
                    // scores, as the third page has them.
                    if page == .map || page == .task, let score = fly.score {
                        ScoreBadge(score: score)
                        // An open triangle's closing circle, in the core's
                        // words, under the score.
                        if let closing = score.closing {
                            MapWords(text: closing.words, identifier: "cockpit-closing-words")
                        }
                    }
                    // How many of the other networks' aircraft are drawn.
                    if page != .instruments, let near = fly.othersNear, !near.countWords.isEmpty {
                        MapWords(text: near.countWords, identifier: "cockpit-others-count")
                    }
                }
                .padding(VentiSpace.s2)
            }
            .overlay(alignment: .bottomTrailing) {
                VStack(alignment: .trailing, spacing: 0) {
                    // The airspace that matters next, small, on the first
                    // page: clear of the landing and the daylight at the
                    // upper left and of the credits' button at the lower
                    // left. It stays while a warning is up.
                    if page == .instruments, let proximity = view.airspaceProximity {
                        AirspaceProximityBadge(proximity: proximity)
                            .padding([.top, .horizontal], VentiSpace.s2)
                            .padding(.bottom, Self.airspaceCredit(airspace.credits) == nil ? VentiSpace.s2 : 0)
                    }
                    // Who made the photographs, while they show: in the
                    // flight theme's smallest size, which is the label's.
                    if mapIsAerial, !photos.line.isEmpty {
                        Text(photos.line)
                            .font(.ventiType(.label))
                            .multilineTextAlignment(.trailing)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.horizontal, VentiSpace.s2)
                            .padding(.vertical, VentiSpace.s1)
                            .ventiFloating(in: .rect(cornerRadius: VentiRadius.s))
                            .padding([.top, .horizontal], VentiSpace.s2)
                            .accessibilityIdentifier("photo-credits")
                    }
                    // Whose airspace the map draws: its licence asks for the
                    // credit next to the data.
                    if let credit = Self.airspaceCredit(airspace.credits) {
                        Text(credit)
                            .font(.ventiType(.label))
                            .padding(.horizontal, VentiSpace.s2)
                            .padding(.vertical, VentiSpace.s1)
                            .ventiFloating(in: .rect(cornerRadius: VentiRadius.s))
                            .padding(VentiSpace.s2)
                            .accessibilityIdentifier("cockpit-airspace-credit")
                    }
                    // And whose cables, aircraft and take-offs it draws.
                    if page != .instruments, let credit = otherCredits {
                        Text(credit)
                            .font(.ventiType(.label))
                            .multilineTextAlignment(.trailing)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.horizontal, VentiSpace.s2)
                            .padding(.vertical, VentiSpace.s1)
                            .ventiFloating(in: .rect(cornerRadius: VentiRadius.s))
                            .padding([.bottom, .horizontal], VentiSpace.s2)
                            .accessibilityIdentifier("cockpit-map-credits")
                    }
                }
                // Clear of the two round buttons side by side in the other
                // corner.
                .padding(.leading, page == .map ? 2 * VentiSize.touchInFlight + VentiSpace.s2 + VentiSpace.s4 : 0)
            }
    }

    /// Whether a task is flown, and so has its page.
    private var hasTask: Bool {
        fly.view?.taskReadings != nil
    }

    private var isReplay: Bool {
        fly.mode != .live
    }

    /// The credits of the cables, the other networks' aircraft and the
    /// take-offs the map draws, in a line; `nil` while it draws none.
    private var otherCredits: String? {
        var parts: [String] = []
        if let words = cables.creditWords {
            parts.append(words)
        }
        if let near = fly.othersNear {
            parts.append(contentsOf: near.credits.map(\.text))
        }
        if let takeoffs = fly.takeoffsNear, !takeoffs.takeoffs.isEmpty, !takeoffs.credits.isEmpty {
            parts.append("Sites: \(takeoffs.credits.map(\.provider).formatted(.list(type: .and)))")
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// What the cables are asked for by: the glider's place to a tenth of
    /// a degree, so that the model looks again as the glider goes on; it
    /// asks only where its answer is far behind.
    static func cablesKey(_ position: Coordinate?) -> String {
        guard let position else { return "" }
        return "\((position.lat * 10).rounded()),\((position.lon * 10).rounded())"
    }

    /// The credit for the airspace fetched from the server, in a line:
    /// "Airspace: openAIP, not certified". `nil` when the cockpit draws
    /// none from a server.
    static func airspaceCredit(_ credits: [Credit]) -> String? {
        guard !credits.isEmpty else { return nil }
        let certified = credits.contains { $0.notice != nil } ? ", not certified" : ""
        return "Airspace: \(credits.map(\.provider).formatted(.list(type: .and)))\(certified)"
    }

    /// The direction of travel, once the glider moves fast enough for it
    /// to mean something.
    private static func course(of view: CockpitView) -> Double? {
        guard let speed = view.groundSpeedMs, speed >= 2 else { return nil }
        return view.courseDeg
    }
}

/// The line the design language draws between the readings and the map.
private struct CockpitRule: View {
    var body: some View {
        Rectangle()
            .fill(.venti(.border))
            .frame(height: 2)
    }
}

/// A round button over the map, as large as a finger in a glove needs.
private struct MapButton: View {
    let name: String
    let symbol: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.ventiType(.heading))
                .frame(width: VentiSize.touchInFlight, height: VentiSize.touchInFlight)
                .contentShape(.circle)
        }
        .buttonStyle(.plain)
        .ventiFloating(in: .circle)
        .accessibilityLabel(name)
    }
}

/// The round layers button over the cockpit's map, at the flight theme's
/// size: the drawn ground or photographs from above. The same choice as
/// `AerialButton` everywhere else, with the pack's layers sign; on, the
/// sign is in the link colour.
private struct CockpitAerialButton: View {
    @Binding var isOn: Bool

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            Image("icon-layers")
                .resizable()
                .frame(width: VentiSize.icon, height: VentiSize.icon)
                .foregroundStyle(.venti(isOn ? .link : .text))
                .frame(width: VentiSize.touchInFlight, height: VentiSize.touchInFlight)
                .contentShape(.circle)
        }
        .buttonStyle(.plain)
        .ventiFloating(in: .circle)
        .accessibilityLabel("Aerial view")
        .accessibilityAddTraits(isOn ? .isSelected : [])
        .accessibilityIdentifier("cockpit-aerial")
    }
}

/// "End" at the end of the top line: the session ends when it is held for
/// a second, so that a glove, a line or a drop of rain ends nothing. It
/// fills while the finger stays. A pilot who has the screen read out ends
/// with the usual double tap, which is deliberate enough. The landing ends
/// a flight by itself; this is for the pilot who ends it first.
private struct EndControl: View {
    let action: () -> Void

    /// How long the finger has to stay, in seconds.
    private static let hold = 1.0

    @State private var isHeld = false
    /// Counts the holds that were long enough, for the tap that says so.
    @State private var ended = 0

    var body: some View {
        Text("End")
            .foregroundStyle(.venti(.onPrimary))
            .padding(.horizontal, VentiSpace.s3)
            .padding(.vertical, VentiSpace.s1)
            .background {
                GeometryReader { proxy in
                    ZStack(alignment: .leading) {
                        Rectangle()
                            .fill(.venti(.primary))
                        Rectangle()
                            .fill(VentiBrand.purple)
                            .frame(width: isHeld ? proxy.size.width : 0)
                            .animation(
                                isHeld ? .linear(duration: Self.hold) : .easeOut(duration: 0.15), value: isHeld
                            )
                    }
                }
                .clipShape(.capsule)
            }
            // A finger has more room than the word: the touch reaches past
            // it without making the line taller.
            .padding(VentiSpace.s3)
            .contentShape(.rect)
            .padding(-VentiSpace.s3)
            .onLongPressGesture(minimumDuration: Self.hold, maximumDistance: VentiSpace.s7) {
                isHeld = false
                ended += 1
                action()
            } onPressingChanged: { isPressing in
                isHeld = isPressing
            }
            .sensoryFeedback(.success, trigger: ended)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("End")
            .accessibilityHint("Hold to end the session")
            .accessibilityAddTraits(.isButton)
            .accessibilityAction(.default, action)
            .accessibilityIdentifier("cockpit-end")
    }
}

/// The cockpit's pages as dots in the top line, the one in front drawn
/// long: the flight theme has one ink, so the page in front is told by its
/// shape, not by a lighter colour of the others. A tap goes to the next
/// page, for a gloved thumb that would rather not swipe; a pilot who has
/// the screen read out swipes up or down on it.
private struct PageDots: View {
    let pages: [CockpitPage]
    let page: CockpitPage
    let next: () -> Void
    let previous: () -> Void

    var body: some View {
        HStack(spacing: VentiSpace.s1) {
            ForEach(pages, id: \.self) { each in
                Capsule()
                    .fill(.venti(.text))
                    .frame(width: each == page ? VentiSpace.s5 : VentiSpace.s2, height: VentiSpace.s2)
            }
        }
        // The touch reaches past the dots, which are small, without making
        // the line taller.
        .padding(VentiSpace.s3)
        .contentShape(.rect)
        .padding(-VentiSpace.s3)
        .onTapGesture(perform: next)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Page")
        .accessibilityValue("\(page.name), \((pages.firstIndex(of: page) ?? 0) + 1) of \(pages.count)")
        .accessibilityHint("Goes to the next page")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction(.default, next)
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: next()
            case .decrement: previous()
            @unknown default: break
            }
        }
        .accessibilityIdentifier("cockpit-pages")
    }
}

/// Where the flight is in its life, while it is not in the air: ready on
/// the ground, or landed and about to be saved.
private struct PhaseNotice: View {
    let title: String
    let detail: String?
    let icon: String

    /// Nothing in the air: the readings say all there is to say.
    init?(view: CockpitView, isReplay: Bool) {
        switch view.phase {
        case .flying:
            return nil
        case .ground:
            icon = "icon-takeoff"
            if view.waitingForTakeoff {
                title = "Ready for take-off"
                detail = "Recording starts by itself."
            } else if view.recording {
                title = "Recording"
                detail = "Waiting for take-off."
            } else {
                title = "Waiting for take-off"
                detail = nil
            }
        case .landed:
            icon = "icon-landing"
            title = "Landed"
            if let left = view.savesInS, !isReplay {
                detail = "The flight is saved in \(Format.minutesSeconds(seconds: left))."
            } else {
                detail = nil
            }
        }
    }

    var body: some View {
        HStack(spacing: VentiSpace.s3) {
            Image(icon)
                .resizable()
                .frame(width: VentiSize.icon, height: VentiSize.icon)
            VStack(alignment: .leading, spacing: 0) {
                Text(title)
                    .font(.ventiType(.heading))
                if let detail {
                    Text(detail)
                        .font(.ventiType(.label))
                        .monospacedDigit()
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, VentiSpace.s4)
        .padding(.vertical, VentiSpace.s2)
        .frame(minHeight: VentiSize.touch)
        .background(.venti(.surfaceRaised), in: .rect(cornerRadius: VentiRadius.m))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("cockpit-phase")
    }
}

/// Something that went wrong, as a band among the cockpit's notices: the
/// caution sign, what it is, and that a tap puts it away.
private struct ProblemNotice: View {
    let text: String
    let putAway: () -> Void

    var body: some View {
        Button(action: putAway) {
            HStack(alignment: .top, spacing: VentiSpace.s3) {
                Image("icon-caution")
                    .resizable()
                    .frame(width: VentiSize.icon, height: VentiSize.icon)
                    .foregroundStyle(.venti(.caution))
                Text(text)
                    .font(.ventiType(.label))
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                Image(systemName: "xmark")
                    .font(.system(size: 14, weight: .semibold))
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, VentiSpace.s4)
            .padding(.vertical, VentiSpace.s3)
            .frame(minHeight: VentiSize.touch)
            .foregroundStyle(.venti(.text))
            .background(.venti(.cautionSoft), in: .rect(cornerRadius: VentiRadius.m))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityHint("Puts this message away")
        .accessibilityIdentifier("cockpit-problem")
    }
}

/// The word "Replay" at the top of the cockpit, as a menu: a replay is
/// watched, not flown, so it can be held at a moment and run slower or
/// faster from here.
///
/// A view of its own, which reads nothing of the session but the mode. The
/// line it stands in is drawn anew five times a second; a menu built
/// there was rebuilt as often while it was open, and never came to rest.
private struct ReplaySpeedMenu: View {
    @Environment(FlySession.self) private var fly

    /// The speeds a replay offers: held, the flight's own, and two for
    /// getting to the part worth watching.
    private static let speeds: [Double] = [0, 1, 10, 20]

    private static func choice(_ speed: Double) -> String {
        switch speed {
        case 0: "Paused"
        case 1: "Real time"
        default: "\(Int(speed)) times faster"
        }
    }

    private static func words(_ speed: Double) -> String {
        switch speed {
        case 0: "Replay paused"
        case 1: "Replay"
        default: "Replay ×\(Int(speed))"
        }
    }

    var body: some View {
        if case .replay(let speed) = fly.mode {
            Menu {
                Picker(
                    "Speed",
                    selection: Binding(get: { speed }, set: { fly.setReplaySpeed($0) })
                ) {
                    ForEach(Self.speeds, id: \.self) { choice in
                        Text(Self.choice(choice)).tag(choice)
                    }
                }
            } label: {
                Label(Self.words(speed), systemImage: speed == 0 ? "pause.fill" : "play.fill")
            }
            .accessibilityIdentifier("replay-speed")
        }
    }
}

/// The line at the top: is it recording, how long, which page is in
/// front, and are the sensors there, with "End" at its end. What is lost
/// is said in words: "No GPS".
private struct StatusLine: View {
    let view: CockpitView
    let mode: FlySession.Mode?
    let varioName: String?
    /// Whether the map is drawn from a region kept on the iPhone, and so
    /// stays when the network goes.
    var mapIsKept = false
    /// The cockpit's pages, and the one in front.
    let pages: [CockpitPage]
    let page: CockpitPage
    let nextPage: () -> Void
    let previousPage: () -> Void
    /// Ends the session, once "End" has been held.
    let end: () -> Void

    var body: some View {
        HStack(spacing: VentiSpace.s2) {
            if case .replay = mode {
                ReplaySpeedMenu()
            } else if view.recording {
                Label {
                    Text("Recording")
                } icon: {
                    Image(systemName: "record.circle.fill")
                        .foregroundStyle(.venti(.accentText))
                }
            } else if view.waitingForTakeoff {
                Text("Ready")
            }
            if let seconds = view.flightTimeS {
                Text(Format.stopwatch(seconds: seconds))
                    .monospacedDigit()
            }
            Spacer(minLength: VentiSpace.s2)
            PageDots(pages: pages, page: page, next: nextPage, previous: previousPage)
            Spacer(minLength: VentiSpace.s2)
            if positionIsLost {
                Label("No GPS", systemImage: "location.slash.fill")
                    .foregroundStyle(.venti(.danger))
            }
            if mapIsKept {
                // One glyph: the line is full as it is.
                Image(systemName: "arrow.down.circle.fill")
                    .accessibilityLabel("Map from this iPhone")
                    .accessibilityIdentifier("map-kept")
            }
            // The words of the vario give way first in a narrow line, and
            // whole: where the core's words do not fit, its short form
            // stands, and where that does not either, a caution sign for
            // a vario that fell silent. Never cut short.
            let source = source
            ViewThatFits(in: .horizontal) {
                Text(source.words)
                    .fixedSize()
                Text(source.short)
                    .fixedSize()
                    .accessibilityLabel(source.words)
                Group {
                    if varioIsLost {
                        Image(systemName: "exclamationmark.triangle.fill")
                    } else {
                        Color.clear.frame(width: 1, height: 1)
                    }
                }
                .accessibilityLabel(source.words)
            }
            .foregroundStyle(.venti(varioIsLost ? .danger : .text))
            .layoutPriority(-1)
            .accessibilityIdentifier("cockpit-vario-source")
            EndControl(action: end)
        }
        .font(.ventiType(.label))
        .lineLimit(1)
        .padding(.horizontal, VentiSpace.s4)
        .padding(.vertical, VentiSpace.s2)
    }

    /// No position for five seconds means the fix is gone.
    private var positionIsLost: Bool {
        view.positionAgeS.map { $0 > 5 } ?? true
    }

    /// An external vario that was talking and has stopped.
    private var varioIsLost: Bool {
        view.instrument.map { !$0.talking } ?? false
    }

    /// Where the climb comes from, in the core's words and their short
    /// form. In a replay the pressure is the recording's, not this
    /// phone's, which the core is told.
    private var source: CockpitSourceWords {
        var isReplay = false
        if case .replay = mode {
            isReplay = true
        }
        return cockpitSourceWords(
            varioSource: view.varioSource, instrument: view.instrument, varioName: varioName, isReplay: isReplay
        )
    }
}

/// The readings of the first page, as the design language's flight screen
/// has them: the climb very large beside its scale, and four large ones in
/// fixed places. Altitude and wind never move; the other two are for
/// cruising or for thermalling.
private struct Instruments: View {
    let view: CockpitView
    let way: WayToTarget?
    /// Lists the wind of every band of height the glider has circled in.
    let showWinds: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            HStack(alignment: .top, spacing: VentiSpace.s3) {
                Reading(
                    name: "Climb",
                    value: view.climbMs.map(Format.climb(metresPerSecond:)),
                    unit: "m/s",
                    // On a glide, for a pilot who set a glider class, the
                    // core's hint to speed up stands here while it holds.
                    note: view.speedToFly.flatMap { $0.speedUp ? $0.words : nil }
                        ?? view.averageClimbMs.map { "\(Format.climb(metresPerSecond: $0)) m/s average" },
                    size: .veryLarge
                )
                VarioScale(climb: view.climbMs)
                    .padding(.top, VentiSpace.s3)
            }
            HStack(alignment: .top, spacing: VentiSpace.s3) {
                Reading(
                    name: "Altitude",
                    value: view.altitudeM.map(Format.metres),
                    unit: "m",
                    // Over the ground where the iPhone has the ground;
                    // over the take-off where it has not.
                    note: view.heightAboveGroundM.map { "\(Format.metres(max($0, 0))) m over ground" }
                        ?? view.heightAboveTakeoffM.map { "\(Format.signedMetres($0)) m over take-off" }
                )
                if view.circling {
                    Reading(
                        name: "Gained",
                        value: view.thermal.map { Format.signedMetres($0.gainM) },
                        unit: "m"
                    )
                } else {
                    // Under it the course, in the core's words, while the
                    // glider moves fast enough for one to mean something.
                    Reading(
                        name: "Ground speed",
                        value: view.groundSpeedMs.map(Format.kilometresPerHour(metresPerSecond:)),
                        unit: "km/h",
                        note: view.courseWords
                    )
                }
            }
            HStack(alignment: .top, spacing: VentiSpace.s3) {
                if view.circling {
                    Reading(
                        name: "Thermal",
                        value: view.thermal.map { Format.climb(metresPerSecond: $0.climbMs) },
                        unit: "m/s"
                    )
                } else {
                    // Under the glide the glider is making: the glide it
                    // needs to reach the place it is flying to.
                    Reading(
                        name: "Glide",
                        value: view.glideRatio.map(Format.glideRatio),
                        unit: "",
                        note: way?.requiredGlide.map { "Needs \(Format.glideRatio($0))" }
                    )
                }
                // The wind as last measured while circling. One from a
                // while ago is a memory of another place and height: its
                // age stands under it.
                Reading(
                    name: view.wind.map { "Wind from \(Format.compassPoint(degrees: $0.fromDeg))" } ?? "Wind",
                    spoken: "Wind",
                    value: view.wind.map { Format.kilometresPerHour(metresPerSecond: $0.speedMs) },
                    unit: "km/h",
                    note: WindBadge.age(view.windAgeS).map { "\($0) old" }
                )
                .contentShape(.rect)
                .onTapGesture(perform: showWinds)
                .accessibilityAddTraits(.isButton)
                .accessibilityHint("Shows the wind by height")
            }
        }
        .padding(.horizontal, VentiSpace.s4)
        .padding(.bottom, VentiSpace.s2)
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// The readings under the map of the second page: three, of middle size.
private struct MapReadings: View {
    let view: CockpitView

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s3) {
            Reading(
                name: "Climb",
                value: view.climbMs.map(Format.climb(metresPerSecond:)),
                unit: "m/s",
                size: .medium
            )
            Reading(
                name: "Altitude",
                value: view.altitudeM.map(Format.metres),
                unit: "m",
                size: .medium
            )
            Reading(
                name: "Ground speed",
                value: view.groundSpeedMs.map(Format.kilometresPerHour(metresPerSecond:)),
                unit: "km/h",
                size: .medium
            )
        }
        .padding(.horizontal, VentiSpace.s4)
        .padding(.top, VentiSpace.s1)
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// The third page: how the flight stands, and what can be switched. Its
/// buttons are large, for gloves, and none of them ends anything.
private struct FlightPage: View {
    let view: CockpitView
    /// How the flight scores so far.
    let score: LiveScore?
    let isReplay: Bool
    let lock: () -> Void
    let showLive: () -> Void
    /// Opens what the cockpit knows beyond its pages' readings.
    let showDetails: () -> Void

    @Environment(FlySession.self) private var fly

    var body: some View {
        VStack(spacing: VentiSpace.s2) {
            HStack(alignment: .top, spacing: VentiSpace.s3) {
                Reading(
                    name: "Flight time",
                    value: view.flightTimeS.map(Format.stopwatch(seconds:)),
                    unit: ""
                )
                // The cross-country distance so far, and how it scores.
                Reading(
                    name: "Distance",
                    spoken: "Cross-country distance",
                    value: score.map { Format.kilometres(metres: $0.distanceM) },
                    unit: "km",
                    note: score.map(ScoreBadge.kind)
                )
            }
            HStack(spacing: VentiSpace.s3) {
                Button {
                    fly.isMuted.toggle()
                } label: {
                    Label(fly.isMuted ? "Unmute vario" : "Mute vario", systemImage: fly.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                        .frame(maxWidth: .infinity)
                }
                .accessibilityIdentifier("cockpit-mute")
                // Nothing on the screen answers a touch until it is
                // unlocked by holding a finger on the lock.
                Button(action: lock) {
                    Label("Lock screen", systemImage: "lock.fill")
                        .frame(maxWidth: .infinity)
                }
                .accessibilityIdentifier("cockpit-lock")
            }
            HStack(spacing: VentiSpace.s3) {
                // Who watches the flight, and whether it is going out. Not
                // in a replay: what is replayed is not shared, nor recorded.
                if !isReplay {
                    LiveButton(live: fly.live, open: showLive)
                }
                // The readings that have no place on a page: where the
                // wind is from, the airspeed, the heading, the altitude's
                // source, the QNH.
                Button(action: showDetails) {
                    Label(cockpitDetailsWords().title, systemImage: "list.bullet")
                        .frame(maxWidth: .infinity)
                }
                .accessibilityIdentifier("cockpit-details")
            }
            // For a pilot who would rather not wait for the take-off to
            // be noticed.
            if !isReplay, view.waitingForTakeoff {
                Button {
                    fly.recordNow()
                } label: {
                    Text("Record now")
                        .frame(maxWidth: .infinity)
                }
                .accessibilityIdentifier("cockpit-record-now")
            }
        }
        .buttonStyle(.venti(.quiet))
        .padding(.horizontal, VentiSpace.s4)
        .padding(.bottom, VentiSpace.s3)
        .fixedSize(horizontal: false, vertical: true)
    }

}

/// One reading of the cockpit: the number, and under it its name.
///
/// The numbers are the design language's instrument sizes and do not grow
/// with the text size: a reading has to be in the same place at the same
/// size every time the pilot glances at it. A value the sensors have not
/// supplied is two dashes, never an old number.
private struct Reading: View {
    enum Size {
        /// The one reading a page may have of this size.
        case veryLarge
        case large
        case medium
    }

    /// The name under the number.
    let name: String
    /// The name as it is read out, where the one shown says more than a
    /// name: "Wind" for "Wind from W".
    var spoken: String?
    /// The value as text, or `nil` while the sensors have not supplied it.
    let value: String?
    let unit: String
    /// What is worth knowing beside it, under the name.
    var note: String?
    var size = Size.large
    /// The number's ink, where its colour says something.
    var ink = VentiColor.Role.text

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s1) {
                Text(value ?? "– –")
                    .font(numberFont)
                    .foregroundStyle(.venti(ink))
                    .monospacedDigit()
                    .minimumScaleFactor(0.6)
                if !unit.isEmpty {
                    Text(unit)
                        .font(unitFont)
                }
            }
            .lineLimit(1)
            // The typeface leaves room under its figures for descenders
            // that numbers do not have: the name moves up into it.
            .padding(.bottom, size == .veryLarge ? -10 : size == .large ? -4 : 0)
            Text(name)
                .font(.ventiType(.label))
                .lineLimit(1)
            // Its line is kept when there is nothing to say, so that the
            // readings below stay where they are.
            if size != .medium {
                Text(note ?? " ")
                    .font(.ventiType(.label))
                    .monospacedDigit()
                    .foregroundStyle(.venti(.textMuted))
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken ?? name)
        .accessibilityValue(spokenValue)
    }

    private var numberFont: Font {
        switch size {
        case .veryLarge: .custom("FiraSans-Bold", fixedSize: 84)
        case .large: .custom("FiraSans-Bold", fixedSize: 44)
        case .medium: .custom("FiraSans-SemiBold", fixedSize: 24)
        }
    }

    private var unitFont: Font {
        switch size {
        case .veryLarge: .custom("FiraSans-SemiBold", fixedSize: 24)
        case .large: .custom("FiraSans-SemiBold", fixedSize: 16)
        case .medium: .custom("FiraSans-Medium", fixedSize: 14)
        }
    }

    private var spokenValue: String {
        guard let value else { return "not available" }
        let number = unit.isEmpty ? value : "\(value) \(unit)"
        return note.map { "\(number), \($0)" } ?? number
    }
}

/// Climb and sink at a glance, as the design language draws it: three
/// steps above a line for the climb and three below for the sink, each lit
/// in its colour of the vario scale once the glider climbs or sinks that
/// fast. It is read by its shape: more steps, more climb.
private struct VarioScale: View {
    let climb: Double?

    @Environment(\.venti) private var theme

    private static let up: [ClimbBand] = [.strongClimb, .climb, .lightClimb]
    private static let down: [ClimbBand] = [.lightSink, .sink, .strongSink]

    var body: some View {
        let band = climb.map { ClimbBand(metresPerSecond: $0) } ?? .level
        VStack(spacing: VentiSpace.s1) {
            ForEach(Self.up, id: \.self) { step in
                segment(step, isLit: band.rawValue >= step.rawValue)
            }
            Rectangle()
                .fill(theme.text)
                .frame(height: 3)
            ForEach(Self.down, id: \.self) { step in
                segment(step, isLit: band.rawValue <= step.rawValue)
            }
        }
        .frame(width: 44)
        .accessibilityHidden(true)
    }

    private func segment(_ step: ClimbBand, isLit: Bool) -> some View {
        Rectangle()
            .fill(isLit ? theme.vario[step.rawValue] : theme.surfaceRaised)
            .frame(height: 12)
    }
}

/// How much daylight is left, shown over the map once it is less than two
/// hours: time to sunset, then time until night, then "Night".
private struct DaylightBadge: View {
    let daylight: DaylightLeft

    /// What to show, if anything, and whether it is past sunset.
    private var text: (words: String, isLate: Bool)? {
        if let toSunset = daylight.minutesToSunset, toSunset >= 0 {
            guard toSunset < 120 else { return nil }
            return ("Sunset in \(Self.hoursMinutes(toSunset))", false)
        }
        if let toDusk = daylight.minutesToDusk, toDusk >= 0 {
            return ("Night in \(Self.hoursMinutes(toDusk))", true)
        }
        // Where the sun does not set that day, there is nothing to say.
        guard daylight.minutesToSunset != nil || daylight.minutesToDusk != nil else { return nil }
        return ("Night", true)
    }

    var body: some View {
        if let text {
            Label(text.words, systemImage: text.isLate ? "moon.fill" : "sunset.fill")
                .font(.venti(.subheadline, .semibold))
                .monospacedDigit()
                .foregroundStyle(.venti(text.isLate ? .danger : .text))
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .ventiFloating(in: .capsule)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Daylight")
                .accessibilityValue(text.words)
        }
    }

    private static func hoursMinutes(_ minutes: Int64) -> String {
        Format.hoursMinutes(seconds: minutes * 60)
    }
}

/// Where the lift is in the thermal being circled in: an arrow toward the
/// side where the climb is stronger and how far to move the circle, or
/// that the circle is centred.
private struct LiftBadge: View {
    let centre: LiftCentreView

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: centre.isCentred ? "scope" : "arrow.up")
                .font(.venti(.subheadline, .bold))
                // The map is north-up, so the arrow points the way to move.
                .rotationEffect(.degrees(centre.isCentred ? 0 : centre.towardDeg))
            Text(centre.words)
                .font(.venti(.subheadline, .semibold))
                .monospacedDigit()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .ventiFloating(in: .capsule)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Lift")
        .accessibilityValue(centre.words)
        .accessibilityIdentifier("lift-badge")
    }
}

/// The wind over the map: an arrow pointing the way the air moves, and its
/// speed.
struct WindBadge: View {
    let wind: WindView
    /// Seconds since it was measured, if known.
    var ageS: Double?

    var body: some View {
        HStack(spacing: 6) {
            Image("icon-arrow-wind")
                .resizable()
                .frame(width: 20, height: 20)
                // The arrow shows where the wind goes; the map is north-up.
                .rotationEffect(.degrees(wind.fromDeg + 180))
            Text("\(Format.kilometresPerHour(metresPerSecond: wind.speedMs)) km/h")
                .font(.venti(.headline))
                .monospacedDigit()
            // A wind from a while ago is a memory of another place and
            // height: its age stands beside it.
            if let age = Self.age(ageS) {
                Text(age)
                    .font(.venti(.subheadline))
                    .monospacedDigit()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .ventiFloating(in: .capsule)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Wind")
        .accessibilityValue(
            "\(Format.spokenKilometresPerHour(metresPerSecond: wind.speedMs)) from \(Format.compassPoint(degrees: wind.fromDeg))"
                + (Self.age(ageS).map { ", measured \($0) ago" } ?? "")
        )
    }

    /// The age of a wind once it is five minutes old or more: "12 min",
    /// "1 h". A fresh wind has none to show.
    static func age(_ seconds: Double?) -> String? {
        guard let seconds, seconds >= 300 else { return nil }
        let minutes = Int(seconds / 60)
        return minutes < 60 ? "\(minutes) min" : "\(minutes / 60) h"
    }
}

/// The wind on the ground in one badge: a windsock, where the wind goes
/// and its mean and gusts, as the station that speaks for the landing
/// measured it, or else the nearest. Red when it is strong for a landing.
struct GroundWindBadge: View {
    let station: GroundWindStation

    var body: some View {
        HStack(spacing: 6) {
            Image("icon-windsock")
                .resizable()
                .frame(width: 20, height: 20)
            if let from = station.fromDeg {
                Image("icon-arrow-wind")
                    .resizable()
                    .frame(width: 16, height: 16)
                    .rotationEffect(.degrees(from + 180))
            }
            // Mean and gusts, and how far off the station is when it is
            // neither the landing's nor near.
            Text(station.badge)
                .font(.venti(.subheadline, .semibold))
                .monospacedDigit()
        }
        .foregroundStyle(.venti(station.isStrong ? .danger : .text))
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .ventiFloating(in: .capsule)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(station.isAtLanding ? "Wind at the landing" : "Wind on the ground")
        .accessibilityValue("\(station.name), \(station.wind), \(station.age)")
    }
}

/// A strong wind where the pilot means to land: what a station there
/// measured. A tap lists the stations around; the cross puts it away.
struct GroundWindBanner: View {
    let alert: GroundWindAlert
    let open: () -> Void
    let putAway: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Button(action: open) {
                HStack(alignment: .top, spacing: VentiSpace.s3) {
                    Image("icon-wind")
                        .resizable()
                        .frame(width: VentiSize.icon, height: VentiSize.icon)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(alert.title)
                            .font(.ventiType(.heading))
                            .multilineTextAlignment(.leading)
                        Text(alert.detail)
                            .font(.ventiType(.label))
                            .multilineTextAlignment(.leading)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("ground-wind-warning")
            Button("Put the warning away", systemImage: "xmark", action: putAway)
                .labelStyle(.iconOnly)
                .font(.ventiType(.heading))
                .frame(width: VentiSize.touch, height: VentiSize.touch)
                .contentShape(.rect)
        }
        .foregroundStyle(.venti(.caution))
        .padding(.leading, VentiSpace.s4)
        .padding(.trailing, VentiSpace.s1)
        .padding(.vertical, VentiSpace.s2)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.venti(.cautionSoft), in: .rect(cornerRadius: VentiRadius.m))
    }
}

/// The landing to think of first: its name and the height the glider is
/// expected to arrive above it, or that none of the pilot's landings is
/// within reach. By the glider's own numbers and the wind last measured,
/// in a straight line; where the iPhone has the ground, a landing behind
/// a ridge is not within reach.
struct LandingBadge: View {
    let landing: LandingOption

    var body: some View {
        Group {
            if let height = Self.height(landing) {
                // The name where it fits whole beside the wind's badge;
                // half a name says less than the height alone.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 6) {
                        Self.sign
                        Text(landing.name)
                        Text(height)
                    }
                    HStack(spacing: 6) {
                        Self.sign
                        Text(height)
                    }
                }
            } else {
                HStack(spacing: 6) {
                    Self.sign
                    Text("No landing in reach")
                }
            }
        }
        .lineLimit(1)
        .font(.venti(.subheadline, .semibold))
        .monospacedDigit()
        .foregroundStyle(.venti(Self.role(landing.reach.reach)))
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .ventiFloating(in: .capsule)
        .frame(maxWidth: 190, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Landing")
        .accessibilityValue(Self.spoken(landing))
        .accessibilityIdentifier("landing-badge")
    }

    /// The height to spare on arrival, "+320 m", for a landing within
    /// reach; none for one that is not.
    static func height(_ landing: LandingOption) -> String? {
        guard landing.reach.reach == .within || landing.reach.reach == .just,
              let arrival = landing.reach.arrivalHeightM
        else { return nil }
        // To ten metres: the prediction is no finer.
        return "\(Format.signedMetres((arrival / 10).rounded() * 10)) m"
    }

    /// The design language's sign for a landing.
    private static var sign: some View {
        Image("icon-landing")
            .resizable()
            .frame(width: 20, height: 20)
    }

    /// Plain while there is height to spare, the colour of caution when
    /// there is little, of danger when no landing is within reach. The
    /// words say the same.
    static func role(_ reach: Reach) -> VentiColor.Role {
        switch reach {
        case .within: .text
        case .just: .caution
        case .beyond: .danger
        case .unknown: .textSecondary
        }
    }

    static func spoken(_ landing: LandingOption) -> String {
        guard let height = height(landing) else { return "No landing in reach" }
        return "\(landing.name), arriving \(height) above it"
    }
}

/// A wind that leaves the glider little way into it, in the core's words:
/// "Into the wind 4 km/h".
private struct StrongWindBadge: View {
    let words: String

    var body: some View {
        Label {
            Text(words)
        } icon: {
            Image("icon-caution")
                .resizable()
                .frame(width: 20, height: 20)
        }
        .font(.venti(.subheadline, .semibold))
        .monospacedDigit()
        .foregroundStyle(.venti(.caution))
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .ventiFloating(in: .capsule)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Strong wind")
        .accessibilityValue(words)
        .accessibilityIdentifier("strong-wind-badge")
    }
}

/// A line of the core's words over the cockpit's map, as the badges are.
private struct MapWords: View {
    let text: String
    let identifier: String
    /// Whether the words are a caution, and so on the caution's ground.
    var isCaution = false

    var body: some View {
        Text(text)
            .font(.ventiType(.label))
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, VentiSpace.s3)
            .padding(.vertical, VentiSpace.s2)
            .ventiFloating(in: .rect(cornerRadius: VentiRadius.m), ground: isCaution ? .cautionSoft : .surface)
            .accessibilityIdentifier(identifier)
    }
}

/// A caution of the core among the cockpit's notices, in its words: the
/// caution sign and the sentence, with nothing to tap.
private struct CautionBand: View {
    let text: String

    var body: some View {
        HStack(alignment: .center, spacing: VentiSpace.s3) {
            Image("icon-caution")
                .resizable()
                .frame(width: VentiSize.icon, height: VentiSize.icon)
                .foregroundStyle(.venti(.caution))
                .accessibilityHidden(true)
            Text(text)
                .font(.ventiType(.heading))
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, VentiSpace.s4)
        .padding(.vertical, VentiSpace.s2)
        .frame(minHeight: VentiSize.touch)
        .foregroundStyle(.venti(.text))
        .background(.venti(.cautionSoft), in: .rect(cornerRadius: VentiRadius.m))
        .accessibilityElement(children: .combine)
    }
}

/// The task page, as an instrument's competition page has it: the next
/// turnpoint large (its name, distance, how far over or under the glide
/// to it the glider is, the glide needed and the arrival), goal the same in
/// a smaller row, before the start the start's words large, and the speed
/// section's average. Every word and number is the core's.
struct TaskReadingsPage: View {
    let readings: TaskReadings

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            if let start = readings.start {
                Text(start.words)
                    .font(.ventiType(.heading))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("task-start-words")
            }
            if let next = readings.next {
                Text(next.name)
                    .font(.ventiType(.heading))
                    .lineLimit(1)
                    .accessibilityIdentifier("task-next-name")
                HStack(alignment: .top, spacing: VentiSpace.s3) {
                    Reading(
                        name: next.isGoal ? "To goal" : "To next",
                        value: Format.kilometres(metres: next.distanceM),
                        unit: "km",
                        note: next.arrivesInS.map { "in \(Format.hoursMinutes(seconds: $0))" }
                    )
                    OverNeeded(leg: next, isLarge: true, stillAir: !readings.windKnown)
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("task-next")
                HStack(alignment: .top, spacing: VentiSpace.s3) {
                    Reading(
                        name: "Glide needed",
                        value: next.glideNeeded.map(Format.glideRatio),
                        unit: "",
                        size: .medium
                    )
                    Reading(
                        name: "Arrival",
                        value: next.arrivesUnixS.map { Format.time(unixSeconds: $0) },
                        unit: "",
                        size: .medium
                    )
                    Reading(
                        name: "Speed section",
                        value: readings.speedSectionAverageWords,
                        unit: "",
                        size: .medium
                    )
                }
            }
            if let goal = readings.goal, readings.next.map({ !$0.isGoal }) ?? true {
                HStack(alignment: .top, spacing: VentiSpace.s3) {
                    Reading(name: "Goal", value: Format.kilometres(metres: goal.distanceM), unit: "km", size: .medium)
                    OverNeeded(leg: goal, isLarge: false, stillAir: !readings.windKnown)
                    Reading(name: "Glide needed", value: goal.glideNeeded.map(Format.glideRatio), unit: "", size: .medium)
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("task-goal")
            }
        }
        .padding(.horizontal, VentiSpace.s4)
        .padding(.bottom, VentiSpace.s2)
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// How far over or under the altitude needed the glider is, in the core's
/// words: green over it, red under it, by the sign of the core's number.
private struct OverNeeded: View {
    let leg: TaskLegReading
    let isLarge: Bool
    /// Whether still air was assumed, the wind not being known.
    let stillAir: Bool

    var body: some View {
        Reading(
            name: "Over needed",
            value: leg.overNeededWords,
            unit: "",
            note: stillAir ? "Still air" : nil,
            size: isLarge ? .large : .medium,
            ink: ink
        )
        .accessibilityIdentifier(isLarge ? "task-next-over" : "task-goal-over")
    }

    private var ink: VentiColor.Role {
        guard let over = leg.overNeededM else { return .text }
        return over >= 0 ? .good : .danger
    }
}
