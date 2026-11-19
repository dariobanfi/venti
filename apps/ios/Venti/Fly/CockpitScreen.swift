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
    @State private var isConfirmingFinish = false
    @State private var isShowingTask = false
    @State private var isChoosingTarget = false
    @State private var isShowingAirspace = false
    @State private var isShowingWinds = false
    /// Whether the map shows the whole flight instead of following closely.
    @State private var showsWholeFlight = false
    @AppStorage(Preferences.hidesFaiAreas) private var hidesFaiAreas = false

    var body: some View {
        @Bindable var fly = fly
        if let view = fly.view {
            VStack(spacing: 0) {
                StatusLine(view: view, score: fly.score, mode: fly.mode, varioName: fly.vario.connectedName)
                CockpitMap(
                    view: view, way: fly.way, airspace: fly.airspace(),
                    airspaceDistance: fly.airspaceWarnings.first(where: { !$0.silenced })?.distanceM,
                    route: fly.score?.route ?? [],
                    fai: hidesFaiAreas ? nil : fly.faiAreas,
                    showsWholeFlight: showsWholeFlight,
                    task: fly.task == nil ? nil : tasks.task,
                    taskStatus: fly.task
                )
                    // The map is left out of the accessibility tree: what it
                    // shows is in the instruments, and its hundreds of map
                    // labels made reading the screen by code or by VoiceOver
                    // time out. The badges laid over it stay in.
                    .accessibilityHidden(true)
                    .overlay(alignment: .topTrailing) {
                        if let wind = view.wind {
                            // A tap lists the wind of every band of height
                            // the glider has circled in.
                            WindBadge(wind: wind)
                                .contentShape(.capsule)
                                .onTapGesture {
                                    isShowingWinds = true
                                }
                                .accessibilityAddTraits(.isButton)
                                .accessibilityHint("Shows the wind by height")
                                .accessibilityIdentifier("wind-badge")
                                .padding(10)
                        }
                    }
                    .overlay(alignment: .leading) {
                        // The whole flight at a glance, with the scored
                        // route and where to go for an FAI triangle; again
                        // for the close view that follows the glider.
                        Button {
                            showsWholeFlight.toggle()
                        } label: {
                            Image(systemName: showsWholeFlight ? "location.fill" : "arrow.up.left.and.arrow.down.right")
                                .font(.headline)
                                .frame(width: 44, height: 44)
                                .contentShape(.circle)
                        }
                        .buttonStyle(.plain)
                        .glassEffect(.regular, in: .circle)
                        .accessibilityLabel(showsWholeFlight ? "Follow the glider" : "Show the whole flight")
                        .padding(10)
                    }
                    .overlay(alignment: .topLeading) {
                        if let daylight = fly.daylight {
                            DaylightBadge(daylight: daylight)
                                .padding(10)
                        }
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
            // With the first position, and a network, the airspace around
            // it is fetched if what the iPhone keeps is old or from
            // elsewhere. A change reaches the running session by itself.
            .task(id: view.position != nil) {
                if let position = view.position {
                    await airspace.refresh(around: position)
                }
            }
            .sheet(isPresented: $isShowingAirspace) {
                AirspaceWarningsSheet()
                    .presentationDetents([.medium, .large])
            }
            .sheet(isPresented: $isShowingWinds) {
                WindByHeightSheet()
                    .presentationDetents([.medium, .large])
            }
            .sheet(isPresented: $isShowingTask) {
                TaskProgressSheet()
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

/// The line at the top: is it recording, how long, and are the sensors there.
private struct StatusLine: View {
    let view: CockpitView
    /// How the flight scores so far.
    let score: LiveScore?
    let mode: FlySession.Mode?
    let varioName: String?

    var body: some View {
        HStack(spacing: 8) {
            if case .replay(let speed) = mode {
                Label(speed == 1 ? "Replay" : "Replay ×\(Int(speed))", systemImage: "play.fill")
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
                    detail: view.heightAboveTakeoffM.map { "\(Format.signedMetres($0)) m over take-off" },
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

/// The wind over the map: an arrow pointing the way the air moves, and its
/// speed.
private struct WindBadge: View {
    let wind: WindView

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "arrow.up")
                .font(.headline.weight(.bold))
                // The arrow shows where the wind goes; the map is north-up.
                .rotationEffect(.degrees(wind.fromDeg + 180))
            Text("\(Format.kilometresPerHour(metresPerSecond: wind.speedMs)) km/h")
                .font(.system(.headline, design: .rounded))
                .monospacedDigit()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassEffect(.regular, in: .capsule)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Wind")
        .accessibilityValue(
            "\(Format.spokenKilometresPerHour(metresPerSecond: wind.speedMs)) from \(Format.compassPoint(degrees: wind.fromDeg))"
        )
    }
}
