import SwiftUI
import VentiCore

/// The cockpit: what the pilot looks at in the air.
///
/// Few numbers, very large, in fixed places. The map follows the glider by
/// itself. While the core says the glider is circling in lift, the lower
/// row changes from cruise numbers to thermal numbers and the map zooms in;
/// nothing needs to be touched in flight.
struct CockpitScreen: View {
    /// Called when the pilot has confirmed the end of the session.
    let onFinish: () -> Void

    @Environment(FlySession.self) private var fly
    @Environment(AirspaceModel.self) private var airspace
    @Environment(TaskModel.self) private var tasks
    @Environment(OfflineMapsModel.self) private var maps
    @State private var isConfirmingFinish = false
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
    @AppStorage(Preferences.hidesFaiAreas) private var hidesFaiAreas = false
    @AppStorage(Preferences.cockpitForSun) private var isForSun = false

    /// Whether the cockpit's map is drawn from a region kept on the iPhone.
    private var mapIsKept: Bool {
        if case .kept = maps.flyingSource {
            return true
        }
        return false
    }

    var body: some View {
        @Bindable var fly = fly
        if let view = fly.view {
            VStack(spacing: 0) {
                StatusLine(
                    view: view, score: fly.score, mode: fly.mode, varioName: fly.vario.connectedName,
                    mapIsKept: mapIsKept
                )
                CockpitMap(
                    view: view, way: fly.way, airspace: fly.airspace(),
                    airspaceDistance: fly.airspaceWarnings.first(where: { !$0.silenced })?.distanceM,
                    route: fly.score?.route ?? [],
                    fai: hidesFaiAreas ? nil : fly.faiAreas,
                    range: fly.range,
                    gliders: fly.gliders,
                    showsWholeFlight: showsWholeFlight,
                    task: fly.task == nil ? nil : tasks.task,
                    taskStatus: fly.task,
                    source: maps.flyingSource
                )
                    // Drawn from the region kept on the iPhone wherever
                    // one covers the glider: the map is made anew when
                    // the glider comes into one or leaves it.
                    .id(maps.flyingSource.key)
                    // The map is left out of the accessibility tree: what it
                    // shows is in the instruments, and its hundreds of map
                    // labels made reading the screen by code or by VoiceOver
                    // time out. The badges laid over it stay in.
                    .accessibilityHidden(true)
                    .overlay(alignment: .topTrailing) {
                        VStack(alignment: .trailing, spacing: 8) {
                            if let wind = view.wind {
                                // A tap lists the wind of every band of
                                // height the glider has circled in.
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
                            // A wind that leaves the glider little way
                            // into it, said where the wind is read.
                            if let words = view.strongWind {
                                StrongWindBadge(words: words)
                            }
                            // The wind on the ground: the station that
                            // speaks for the landing, else the nearest.
                            // A tap lists the stations around.
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
                            // While circling: which way the lift is
                            // stronger, in words beside the picture on the
                            // map.
                            if let centre = view.lift?.centre {
                                LiftBadge(centre: centre)
                            }
                        }
                        .padding(10)
                    }
                    .overlay(alignment: .leading) {
                        VStack(spacing: 10) {
                            // The whole flight at a glance, with the scored
                            // route and where to go for an FAI triangle;
                            // again for the close view that follows the
                            // glider.
                            Button {
                                showsWholeFlight.toggle()
                            } label: {
                                Image(
                                    systemName: showsWholeFlight
                                        ? "location.fill" : "arrow.up.left.and.arrow.down.right"
                                )
                                .font(.headline)
                                .frame(width: 44, height: 44)
                                .contentShape(.circle)
                            }
                            .buttonStyle(.plain)
                            .glassEffect(.regular, in: .circle)
                            .accessibilityLabel(showsWholeFlight ? "Follow the glider" : "Show the whole flight")
                            // With airspace to warn from, or the ground
                            // to draw: what lies on the line ahead, seen
                            // from the side.
                            if fly.airspaceCheck != .noAirspace || view.groundElevationM != nil {
                                Button {
                                    isShowingAhead = true
                                } label: {
                                    Image(systemName: "chart.line.downtrend.xyaxis")
                                        .font(.headline)
                                        .frame(width: 44, height: 44)
                                        .contentShape(.circle)
                                }
                                .buttonStyle(.plain)
                                .glassEffect(.regular, in: .circle)
                                .accessibilityLabel("Ahead, from the side")
                            }
                        }
                        .padding(10)
                    }
                    .overlay(alignment: .topLeading) {
                        VStack(alignment: .leading, spacing: 8) {
                            if let daylight = fly.daylight {
                                DaylightBadge(daylight: daylight)
                            }
                            // The landing to think of first, and whether
                            // the glide reaches it. A tap lists the places
                            // to fly to, each with its reach.
                            if let landing = fly.landing {
                                LandingBadge(landing: landing)
                                    .contentShape(.capsule)
                                    .onTapGesture {
                                        isChoosingTarget = true
                                    }
                                    .accessibilityAddTraits(.isButton)
                                    .accessibilityHint("Lists the places to fly to")
                            }
                        }
                        .padding(10)
                    }
                    .overlay(alignment: .bottom) {
                        VStack(alignment: .leading, spacing: 6) {
                            // Whose airspace the map draws: its licence
                            // asks for the credit next to the data. Beside
                            // the map's own credits, in the corner.
                            if let credit = Self.airspaceCredit(airspace.credits) {
                                Text(credit)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .background(.regularMaterial, in: .rect(cornerRadius: 8))
                                    .padding(.leading, 34)
                                    .accessibilityIdentifier("cockpit-airspace-credit")
                            }
                            // A strong wind where the pilot means to land,
                            // over the airspace's place on the map.
                            if let warning = fly.windWarning {
                                GroundWindBanner(alert: warning) {
                                    windsBeginOnTheGround = true
                                    isShowingWinds = true
                                } putAway: {
                                    fly.putWindWarningAway()
                                }
                            }
                            AirspaceBanner(warnings: fly.airspaceWarnings, check: fly.airspaceCheck) {
                                isShowingAirspace = true
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(10)
                    }
                    .overlay(alignment: .top) {
                        if view.phase != .flying {
                            Text(view.phase == .ground ? "Waiting for take-off" : "Landed")
                                .font(.headline)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 8)
                                .glassEffect(.regular, in: .capsule)
                                .padding(10)
                        }
                    }
                // With a task: where the flight stands in it, above the
                // line that says where to fly, which is then its next
                // turnpoint unless the pilot chose another place.
                if let status = fly.task {
                    TaskStrip(status: status) {
                        isShowingTask = true
                    }
                    Divider()
                }
                TargetStrip(way: fly.way, courseDeg: Self.course(of: view)) {
                    isChoosingTarget = true
                }
                Instruments(view: view, way: fly.way)
                HStack(spacing: 12) {
                    Button(isReplay ? "End replay" : "Finish flight") {
                        isConfirmingFinish = true
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)
                    .frame(maxWidth: .infinity)

                    // Nothing on the screen answers a touch until it is
                    // unlocked by holding a finger on the lock.
                    Button {
                        isLocked = true
                    } label: {
                        Label("Lock the screen", systemImage: "lock.open")
                            .labelStyle(.iconOnly)
                            .frame(width: 30, height: 30)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)

                    // Who watches the flight, and whether it is going
                    // out. Not in a replay: what is replayed is not shared.
                    if !isReplay {
                        LiveButton(live: fly.live) {
                            isShowingLive = true
                        }
                    }
                    Toggle(isOn: $fly.isMuted) {
                        Label("Mute the vario", systemImage: fly.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                            .labelStyle(.iconOnly)
                            .frame(width: 30, height: 30)
                    }
                    .toggleStyle(.button)
                    .controlSize(.large)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
            }
            .background(Color(.systemBackground))
            // For bright sun: dark type on a light ground whatever the
            // iPhone's appearance, and every letter and number in bold.
            // A pilot who has bold text switched on in the system keeps it
            // either way.
            .transformEnvironment(\.legibilityWeight) { weight in
                if isForSun {
                    weight = .bold
                }
            }
            .preferredColorScheme(isForSun ? .light : nil)
            // With the first position, and a network, the airspace around
            // it is fetched if what the iPhone keeps is old or from
            // elsewhere. A change reaches the running session by itself.
            .task(id: view.position != nil) {
                if let position = view.position {
                    await airspace.refresh(around: position)
                }
            }
            // Locked: a clear sheet over everything takes the touches, and
            // a lock at the foot of the screen opens it when held. The
            // cockpit goes on showing and sounding as before.
            .overlay {
                if isLocked {
                    Color.clear
                        .contentShape(.rect)
                        .ignoresSafeArea()
                        .accessibilityHidden(true)
                        .overlay(alignment: .bottom) {
                            Label("Locked. Hold to unlock", systemImage: "lock.fill")
                                .font(.headline)
                                .padding(.horizontal, 20)
                                .padding(.vertical, 14)
                                .glassEffect(.regular, in: .capsule)
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
                                .padding(.bottom, 14)
                        }
                }
            }
            .sheet(isPresented: $isShowingAirspace) {
                AirspaceWarningsSheet()
                    .presentationDetents([.medium, .large])
            }
            .sheet(isPresented: $isShowingWinds) {
                WindByHeightSheet(groundFirst: windsBeginOnTheGround)
                    .presentationDetents([.medium, .large])
            }
            .sheet(isPresented: $isShowingAhead) {
                AirspaceAheadSheet()
                    .presentationDetents([.large])
            }
            .sheet(isPresented: $isShowingTask) {
                TaskProgressSheet()
                    .presentationDetents([.medium, .large])
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
            }
            .sheet(isPresented: $isChoosingTarget) {
                TargetPicker(
                    position: view.position,
                    chosen: fly.chosenPlace,
                    fliesTask: fly.task != nil
                ) { waypoint in
                    fly.goTo(waypoint?.place)
                }
            }
            .confirmationDialog(
                isReplay ? "End the replay?" : "Finish the flight?",
                isPresented: $isConfirmingFinish,
                titleVisibility: .visible
            ) {
                Button(isReplay ? "Close the cockpit" : "Save flight") {
                    onFinish()
                }
            } message: {
                if !isReplay {
                    Text("Recording stops and the flight is added to the logbook.")
                }
            }
        }
    }

    private var isReplay: Bool {
        fly.mode != .live
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
            .tint(.primary)
            .accessibilityIdentifier("replay-speed")
        }
    }
}

/// The line at the top: is it recording, how long, and are the sensors there.
private struct StatusLine: View {
    let view: CockpitView
    /// How the flight scores so far.
    let score: LiveScore?
    let mode: FlySession.Mode?
    let varioName: String?
    /// Whether the map is drawn from a region kept on the iPhone, and so
    /// stays when the network goes.
    var mapIsKept = false

    var body: some View {
        HStack(spacing: 8) {
            if case .replay = mode {
                ReplaySpeedMenu()
            } else if view.recording {
                Label("Recording", systemImage: "record.circle.fill")
                    .symbolRenderingMode(.multicolor)
            }
            if let seconds = view.flightTimeS {
                Text(Format.stopwatch(seconds: seconds))
                    .monospacedDigit()
            }
            // The cross-country distance so far, once there is a kilometre
            // of it, with a triangle when it scores as one.
            if let score, score.distanceM >= 1000 {
                Label(
                    "\(Format.kilometres(metres: score.distanceM)) km",
                    systemImage: score.kind == .freeFlight ? "arrow.up.right" : "triangle"
                )
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Cross-country distance")
                .accessibilityValue(
                    "\(Format.kilometres(metres: score.distanceM)) kilometres, \(Self.kindWords(score))"
                )
                // An open triangle: how far it is to where it began.
                if let gap = score.closingM, !score.closed, gap >= 100 {
                    Text("\(Format.kilometres(metres: gap)) to close")
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            if positionIsLost {
                Label("No position", systemImage: "location.slash.fill")
                    .foregroundStyle(.red)
            }
            if mapIsKept {
                // One glyph: the line is full as it is.
                Image(systemName: "arrow.down.circle.fill")
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Map from this iPhone")
                    .accessibilityIdentifier("map-kept")
            }
            Text(varioText)
                .foregroundStyle(varioIsLost ? Color.red : Color.secondary)
        }
        .font(.subheadline.weight(.medium))
        .lineLimit(1)
        // With everything on it the line is full; it shrinks a little
        // before anything is cut off.
        .minimumScaleFactor(0.75)
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    private static func kindWords(_ score: LiveScore) -> String {
        switch score.kind {
        case .freeFlight: "free flight"
        case .freeTriangle: score.closed ? "closed triangle" : "triangle"
        case .faiTriangle: score.closed ? "closed FAI triangle" : "FAI triangle"
        }
    }

    /// No position for five seconds means the fix is gone.
    private var positionIsLost: Bool {
        view.positionAgeS.map { $0 > 5 } ?? true
    }

    /// An external vario that was talking and has stopped.
    private var varioIsLost: Bool {
        view.instrument.map { !$0.talking } ?? false
    }

    private var varioText: String {
        if let instrument = view.instrument {
            let name = varioName ?? "Vario"
            guard instrument.talking else { return "\(name) silent" }
            return instrument.batteryPercent.map { "\(name) \($0)%" } ?? name
        }
        // In a replay the pressure is the recording's, not this phone's.
        if case .replay = mode {
            return "Recorded data"
        }
        return switch view.varioSource {
        case .pressure: "Phone barometer"
        case .satellite: "Satellite vario"
        case .instrument, nil: "No vario yet"
        }
    }
}

/// The numbers. The upper pair never changes; the lower row is for cruising
/// or for thermalling.
private struct Instruments: View {
    let view: CockpitView
    let way: WayToTarget?

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            HStack(spacing: 0) {
                Instrument(
                    title: "Altitude",
                    value: view.altitudeM.map(Format.metres),
                    unit: "m",
                    // Over the ground where the iPhone has the ground;
                    // over the take-off where it has not.
                    detail: view.heightAboveGroundM.map { "\(Format.metres(max($0, 0))) m over ground" }
                        ?? view.heightAboveTakeoffM.map { "\(Format.signedMetres($0)) m over take-off" },
                    size: .large
                )
                Divider()
                HStack(spacing: 10) {
                    VarioBar(climb: view.climbMs)
                    Instrument(
                        title: "Climb",
                        value: view.climbMs.map(Format.climb(metresPerSecond:)),
                        unit: "m/s",
                        detail: view.averageClimbMs.map { "\(Format.climb(metresPerSecond: $0)) average" },
                        size: .large
                    )
                }
                .padding(.leading, 12)
            }
            Divider()
            HStack(spacing: 0) {
                if view.circling {
                    Instrument(
                        title: "Gained",
                        value: view.thermal.map { Format.signedMetres($0.gainM) },
                        unit: "m"
                    )
                    Divider()
                    Instrument(
                        title: "Thermal",
                        value: view.thermal.map { Format.climb(metresPerSecond: $0.climbMs) },
                        unit: "m/s"
                    )
                    Divider()
                    Instrument(
                        title: "Wind",
                        value: view.wind.map { Format.kilometresPerHour(metresPerSecond: $0.speedMs) },
                        unit: view.wind.map { "km/h \(Format.compassPoint(degrees: $0.fromDeg))" } ?? "km/h"
                    )
                } else {
                    Instrument(
                        title: "Speed",
                        value: view.groundSpeedMs.map(Format.kilometresPerHour(metresPerSecond:)),
                        unit: "km/h"
                    )
                    Divider()
                    Instrument(
                        title: "Glide",
                        value: view.glideRatio.map(Format.glideRatio),
                        unit: ""
                    )
                    Divider()
                    // Next to the glide the glider is making: the glide
                    // it needs to reach the place it is flying to.
                    Instrument(
                        title: "Needs",
                        value: way?.requiredGlide.map(Format.glideRatio),
                        unit: ""
                    )
                }
            }
            Divider()
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// One number in the cockpit. The sizes are fixed on purpose: an instrument
/// has to be in the same place at the same size every time the pilot
/// glances at it.
private struct Instrument: View {
    enum Size {
        case large
        case regular
    }

    let title: String
    /// The value as text, or `nil` while the sensors have not supplied it.
    let value: String?
    let unit: String
    var detail: String?
    var size: Size = .regular

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(value ?? "–")
                    .font(.system(size: size == .large ? 46 : 32, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .minimumScaleFactor(0.5)
                if !unit.isEmpty {
                    Text(unit)
                        .font(.system(size: size == .large ? 17 : 14, weight: .medium, design: .rounded))
                        .foregroundStyle(.secondary)
                }
            }
            .lineLimit(1)
            if size == .large {
                Text(detail ?? " ")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(value.map { unit.isEmpty ? $0 : "\($0) \(unit)" } ?? "not available")
    }
}

/// Climb and sink as a bar that grows up or down from the middle, in the
/// colours the track has on the map. Full scale is 5 m/s.
private struct VarioBar: View {
    let climb: Double?

    var body: some View {
        GeometryReader { proxy in
            let half = proxy.size.height / 2
            let rate = climb ?? 0
            let length = min(abs(rate) / 5, 1) * half
            ZStack(alignment: .top) {
                Capsule()
                    .fill(Color(.tertiarySystemFill))
                RoundedRectangle(cornerRadius: 3)
                    .fill(ClimbBand(metresPerSecond: rate).color)
                    .frame(height: max(length, 2))
                    .offset(y: rate >= 0 ? half - length : half)
            }
            .clipShape(Capsule())
        }
        .frame(width: 10)
        .padding(.vertical, 10)
        .accessibilityHidden(true)
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
                .font(.system(.subheadline, design: .rounded, weight: .semibold))
                .monospacedDigit()
                .foregroundStyle(text.isLate ? Color.red : Color.primary)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .glassEffect(.regular, in: .capsule)
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
                .font(.subheadline.weight(.bold))
                // The map is north-up, so the arrow points the way to move.
                .rotationEffect(.degrees(centre.isCentred ? 0 : centre.towardDeg))
            Text(centre.words)
                .font(.system(.subheadline, design: .rounded, weight: .semibold))
                .monospacedDigit()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassEffect(.regular, in: .capsule)
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
            Image(systemName: "arrow.up")
                .font(.headline.weight(.bold))
                // The arrow shows where the wind goes; the map is north-up.
                .rotationEffect(.degrees(wind.fromDeg + 180))
            Text("\(Format.kilometresPerHour(metresPerSecond: wind.speedMs)) km/h")
                .font(.system(.headline, design: .rounded))
                .monospacedDigit()
            // A wind from a while ago is a memory of another place and
            // height: its age stands beside it.
            if let age = Self.age(ageS) {
                Text(age)
                    .font(.system(.subheadline, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassEffect(.regular, in: .capsule)
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

/// The wind on the ground in one badge: a flag, where the wind goes and
/// its mean and gusts, as the station that speaks for the landing
/// measured it, or else the nearest. Red when it is strong for a landing.
struct GroundWindBadge: View {
    let station: GroundWindStation

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "flag.fill")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            if let from = station.fromDeg {
                Image(systemName: "arrow.up")
                    .font(.subheadline.weight(.bold))
                    .rotationEffect(.degrees(from + 180))
            }
            // Mean and gusts, and how far off the station is when it is
            // neither the landing's nor near.
            Text(station.badge)
                .font(.system(.subheadline, design: .rounded).weight(.semibold))
                .monospacedDigit()
        }
        .foregroundStyle(station.isStrong ? Color.red : Color.primary)
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .glassEffect(.regular, in: .capsule)
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
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: "wind")
                        .font(.title3)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(alert.title)
                            .font(.headline)
                        Text(alert.detail)
                            .font(.footnote)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("ground-wind-warning")
            Button("Put the warning away", systemImage: "xmark", action: putAway)
                .labelStyle(.iconOnly)
                .font(.headline)
                .frame(width: 36, height: 36)
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(red: 0.72, green: 0.32, blue: 0.06), in: .rect(cornerRadius: 14))
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
                        Image(systemName: "airplane.arrival")
                        Text(landing.name)
                        Text(height)
                    }
                    HStack(spacing: 6) {
                        Image(systemName: "airplane.arrival")
                        Text(height)
                    }
                }
            } else {
                HStack(spacing: 6) {
                    Image(systemName: "airplane.arrival")
                    Text("No landing in reach")
                }
            }
        }
        .lineLimit(1)
        .font(.system(.subheadline, design: .rounded, weight: .semibold))
        .monospacedDigit()
        .foregroundStyle(Self.color(landing.reach.reach))
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassEffect(.regular, in: .capsule)
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

    /// Plain while there is height to spare, amber when there is little,
    /// red when no landing is within reach.
    static func color(_ reach: Reach) -> Color {
        switch reach {
        case .within: .primary
        case .just: .orange
        case .beyond: .red
        case .unknown: .secondary
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
        Label(words, systemImage: "exclamationmark.triangle.fill")
            .font(.system(.subheadline, design: .rounded, weight: .semibold))
            .monospacedDigit()
            .foregroundStyle(.orange)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .glassEffect(.regular, in: .capsule)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Strong wind")
            .accessibilityValue(words)
            .accessibilityIdentifier("strong-wind-badge")
    }
}
