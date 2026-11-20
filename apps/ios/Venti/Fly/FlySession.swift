import Foundation
import Observation
import Security
import UIKit
import VentiCore

/// One flying session as the cockpit screen sees it.
///
/// It owns the phone's sensors and passes what they report to the core's
/// session; five times a second it asks the core what to show and what the
/// vario should sound like. It contains no flying logic.
///
/// A session is either live, fed by the sensors and recorded, or a replay
/// of a recorded flight, fed by the core and not recorded. A live one
/// records by itself: the core begins the flight file at the take-off and
/// says when the flight is over.
@MainActor
@Observable
final class FlySession {
    enum Mode: Equatable {
        case live
        /// Replaying a flight at this many times real speed.
        case replay(speed: Double)
    }

    /// What is running, if anything.
    private(set) var mode: Mode?
    /// What the cockpit shows. Present while a session runs.
    private(set) var view: CockpitView?
    /// The way to the place the cockpit navigates to: the one the pilot
    /// chose, or the take-off. Absent until there is one to go to.
    private(set) var way: WayToTarget?
    /// How the flight scores as a cross-country flight so far.
    private(set) var score: LiveScore?
    /// Where to fly for an FAI triangle, once the flight has covered a
    /// few kilometres in a straight line.
    private(set) var faiAreas: FaiAreas?
    /// How much of the day is left where the glider is.
    private(set) var daylight: DaylightLeft?
    /// The airspace warnings of the moment, the most urgent first.
    private(set) var airspaceWarnings: [AirspaceWarning] = []
    /// Whether the glider is being checked against airspace right now.
    /// Anything but `checking` means that silence is not "no airspace
    /// near", and the cockpit says which it is.
    private(set) var airspaceCheck = AirspaceCheck.noAirspace
    /// The airspace to warn about, asked for when a session starts. The
    /// app sets this to the pilot's airspace files.
    @ObservationIgnored var airspace: () -> AirspaceSet? = { nil }
    /// The ground, as far as the iPhone has it. Without it the cockpit
    /// knows no height above ground.
    @ObservationIgnored var terrain: () -> Terrain? = { nil }
    /// Asks for the ground around a place to be brought, where it is not
    /// there yet.
    @ObservationIgnored var wantTerrain: (Coordinate) -> Void = { _ in }
    /// The task to fly, asked for when a session starts. The app sets
    /// this to the task the pilot keeps.
    @ObservationIgnored var taskFile: () -> TaskFile? = { nil }
    /// Where the flight stands in its task. Absent without a task, and
    /// until there is a position.
    private(set) var task: TaskStatusView?
    /// The name of the place the pilot chose to fly to, if any.
    private(set) var chosenPlace: String?
    /// How the flight is shared with others while it is under way. Absent
    /// while it is not.
    private(set) var live: LiveShareView?
    /// The landing to think of first, among the pilot's own: the one the
    /// glider would arrive highest above. Absent on the ground, and for a
    /// pilot who keeps no landings with a height.
    private(set) var landing: LandingOption?
    /// How far the glider glides over the ground as it is, for a line on
    /// the map. Absent on the ground and where the iPhone has no ground.
    private(set) var range: GlideRange?
    /// Other pilots in the air around the glider who show their flight to
    /// everyone, as the pilot's server last listed them.
    private(set) var gliders: [LiveGlider] = []
    /// Asks the server who is in the air around a place. The app sets
    /// this; `nil` when there is no answer.
    @ObservationIgnored var others: (Coordinate) async -> [LiveGlider]? = { _ in nil }
    /// The wind on the ground around the glider and where the pilot means
    /// to land, as the wind stations of the pilot's server last measured
    /// it. Absent without a server, without a network, and on the ground.
    private(set) var windBelow: GroundWind?
    /// Whose data those stations are: the licences ask for the credit
    /// beside the data.
    private(set) var windBelowCredits: [Credit] = []
    /// Whether the pilot has put the warning of a strong wind at the
    /// landing away.
    private(set) var isWindWarningPutAway = false
    /// Asks the server for the wind stations around a place and around a
    /// second one. The app sets this; `nil` when there is no answer.
    @ObservationIgnored var windOnTheGround: (Coordinate, Coordinate?) async -> WindMap? = { _, _ in nil }
    /// The pilot's landings, asked for every few seconds in the air. The
    /// app sets this to the waypoints that are places to land.
    @ObservationIgnored var landings: () -> [WaypointView] = { [] }
    /// The address of the pilot's Venti server, if this iPhone is signed
    /// in to one. The app sets this.
    @ObservationIgnored var server: () -> String? = { nil }
    /// Carries a request to that server and brings back its answer, or
    /// `nil` if none came. The app sets this.
    @ObservationIgnored var carry: @MainActor (ServerRequest) async -> AccountModel.Answer? = { _ in nil }
    /// Takes the recording of a flight that ended by itself, to put it
    /// into the logbook as one the pilot ended. The Fly tab sets this.
    /// Without it the file waits in the data folder and is found at the
    /// next launch.
    @ObservationIgnored var savedByItself: @MainActor (PendingRecording) -> Void = { _ in }
    /// Something the pilot must be told, such as location being denied.
    var problem: String?
    /// Whether the vario is silent. Remembered between flights.
    var isMuted = UserDefaults.standard.bool(forKey: Preferences.varioMuted) {
        didSet {
            sound.isMuted = isMuted
            UserDefaults.standard.set(isMuted, forKey: Preferences.varioMuted)
        }
    }

    let vario = VarioLink()

    @ObservationIgnored private let dataFolder: URL
    @ObservationIgnored private var session: FlightSession?
    @ObservationIgnored private var replay: Replay?
    @ObservationIgnored private var ticker: Task<Void, Never>?
    @ObservationIgnored private let location = LocationProvider()
    @ObservationIgnored private let barometer = BarometerProvider()
    @ObservationIgnored private let sound = VarioSound()
    @ObservationIgnored private let voice = AirspaceVoice()
    /// The airspace last handed to the core's session.
    @ObservationIgnored private var handedAirspace: AirspaceSet?
    /// Refreshes of the screen since the score was last asked for.
    @ObservationIgnored private var ticksSinceScore = 0
    /// Refreshes of the screen since the ground was last asked for.
    @ObservationIgnored private var ticksSinceTerrain = 0
    /// Refreshes of the screen since the other gliders were asked for,
    /// and whether an answer is still awaited.
    @ObservationIgnored private var ticksSinceOthers = 0
    @ObservationIgnored private var isAskingForOthers = false
    /// Refreshes of the screen since the wind on the ground was asked
    /// for, whether an answer is still awaited, and the station whose
    /// strong wind was last said aloud.
    @ObservationIgnored private var ticksSinceWindBelow = 0
    @ObservationIgnored private var isAskingForWindBelow = false
    @ObservationIgnored private var warnedOfWindAt: String?
    /// How bright the screen was before the cockpit turned it up.
    @ObservationIgnored private var brightnessBefore: CGFloat?

    /// How often the screen and the vario tone are refreshed.
    private static let refreshInterval = Duration.milliseconds(200)

    /// The ground around the glider is asked for this many refreshes
    /// apart: every ten seconds, in which a glider goes two hundred
    /// metres and the tiles reach thirteen kilometres.
    private static let ticksBetweenTerrain = 50

    /// Who else is in the air is asked this many refreshes apart: every
    /// fifteen seconds, which is what the server asks for.
    private static let ticksBetweenOthers = 75

    /// The wind on the ground is asked for this many refreshes apart:
    /// every two minutes. Stations report every five to ten.
    private static let ticksBetweenWindBelow = 600

    /// The longest stretch of the clock a replay moves on by at once, in
    /// seconds. When the phone was busy with something else for longer,
    /// the replay carries on from where it was instead of leaping over
    /// minutes of the flight that nobody saw.
    static let replayStepAtMostS = 2.0

    init(dataFolder: URL) {
        self.dataFolder = dataFolder
    }

    /// Switches the sensors on and makes the session ready: it waits on
    /// the ground and records from the take-off.
    func startFlight() {
        guard mode == nil else { return }
        let session = FlightSession(dataFolder: dataFolder.path)
        do {
            let defaults = UserDefaults.standard
            try session.recordFromTakeoff(
                pilot: defaults.string(forKey: Preferences.pilot),
                glider: defaults.string(forKey: Preferences.glider),
                nowUnixS: Date().timeIntervalSince1970
            )
        } catch {
            problem = error.words
            return
        }

        location.onSample = { sample in session.onLocation(sample: sample) }
        location.onProblem = { [weak self] problem in self?.problem = problem }
        barometer.onPressure = { time, pressure in session.onPressure(timeUnixS: time, pressureHpa: pressure) }
        vario.onBytes = { bytes in
            session.onInstrumentBytes(timeUnixS: Date().timeIntervalSince1970, bytes: bytes)
        }
        location.start()
        barometer.start()
        vario.reconnectToRememberedVario()

        begin(session, mode: .live)
        // Shown to others from take-off, if the pilot chose so.
        share(with: LiveChoice.stored.audience)
    }

    /// Records from now, for a pilot who would rather not wait for the
    /// take-off to be noticed. The flight file begins with the half minute
    /// the session has kept.
    func recordNow() {
        guard let session, mode == .live else { return }
        do {
            let defaults = UserDefaults.standard
            try session.startRecording(
                pilot: defaults.string(forKey: Preferences.pilot),
                glider: defaults.string(forKey: Preferences.glider)
            )
        } catch {
            problem = error.words
        }
        refresh(elapsed: 0)
    }

    /// Ends a flight that is over by itself: landed and saved as if the
    /// pilot had ended it, or given up on because nobody took off.
    private func endByItself(_ over: SessionOver) {
        let recording = finish()
        switch over {
        case .landed:
            if let recording {
                savedByItself(recording)
            }
        case .noTakeoff:
            problem = "Nobody took off in two hours, so the cockpit closed to save the battery. Nothing was recorded."
        }
    }

    /// How each of some places lies within the glider's reach now, in
    /// their order: for the list of places to fly to. Empty without a
    /// position.
    func reach(of places: [WaypointView]) -> [ReachView] {
        guard let session else { return [] }
        return session.reach(places: places.map(\.place), nowUnixS: replay?.now() ?? Date().timeIntervalSince1970)
    }

    /// The screen the app is on.
    private static var screen: UIScreen? {
        UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.screen }.first
    }

    // MARK: Live tracking

    /// Whether the flight in the air is going out to others.
    var isSharingLive: Bool {
        live.map { $0.state != .stopped } ?? false
    }

    /// Shows the flight in the air to these people, or to others than
    /// before. Does nothing without a flight, in a replay, without an
    /// account, and for nobody: taking a flight away from everyone is
    /// `deleteLiveTrack`.
    func share(with audience: LiveAudience?) {
        guard let session, mode == .live, let audience else { return }
        if isSharingLive {
            session.setLiveAudience(audience: audience)
        } else if let server = server() {
            // The link is made of these: from the system's source of
            // random numbers for secrets.
            var random = [UInt8](repeating: 0, count: 26)
            guard SecRandomCopyBytes(kSecRandomDefault, random.count, &random) == errSecSuccess else {
                problem = "The iPhone could not make a link for this flight."
                return
            }
            do {
                live = try session.shareLive(server: server, random: Data(random), audience: audience)
            } catch {
                problem = error.words
            }
        }
        live = session.liveShare(nowUnixS: Date().timeIntervalSince1970)
    }

    /// Takes the flight away from everyone who could watch it: the track
    /// is removed from the server and nothing more is sent.
    func deleteLiveTrack() {
        guard let session, let request = session.deleteLive() else { return }
        live = session.liveShare(nowUnixS: Date().timeIntervalSince1970)
        Task { [carry] in
            // Twice, in case the network was away the first time. The
            // server removes the track by itself after two days.
            for _ in 0..<2 {
                if await carry(request) != nil {
                    break
                }
                try? await Task.sleep(for: .seconds(10))
            }
        }
    }

    /// Sends what the core says is due for the shared flight, and hands
    /// the answer back. The core hands out nothing further until it has
    /// one of the two.
    private func sendLive(from session: FlightSession, now: Double) {
        guard let request = session.liveRequest(nowUnixS: now) else { return }
        Task { [carry] in
            let answer = await carry(request)
            let now = Date().timeIntervalSince1970
            if let answer {
                session.liveAnswer(
                    status: answer.status, body: answer.body, retryAfterS: answer.retryAfter, nowUnixS: now
                )
            } else {
                session.liveNoAnswer(nowUnixS: now)
            }
        }
    }

    /// Tells the server that the flight is over, with what is still
    /// waiting, after the cockpit has closed: for a minute and a half at
    /// most, then the track simply goes quiet.
    private func sayLanded(_ session: FlightSession) {
        guard session.liveShare(nowUnixS: Date().timeIntervalSince1970) != nil else { return }
        session.endLive()
        Task { [carry] in
            let clock = ContinuousClock()
            let deadline = clock.now + .seconds(90)
            while clock.now < deadline {
                let now = Date().timeIntervalSince1970
                if let request = session.liveRequest(nowUnixS: now) {
                    if let answer = await carry(request) {
                        session.liveAnswer(
                            status: answer.status, body: answer.body, retryAfterS: answer.retryAfter,
                            nowUnixS: Date().timeIntervalSince1970
                        )
                    } else {
                        session.liveNoAnswer(nowUnixS: Date().timeIntervalSince1970)
                    }
                } else if let share = session.liveShare(nowUnixS: now),
                          share.state == .waiting || (share.state == .sharing && share.waiting > 0)
                {
                    // Asked to wait, or positions from a gap in the network
                    // are still to go out, a batch every quarter minute.
                    try? await Task.sleep(for: .seconds(2))
                } else {
                    break
                }
            }
        }
    }

    /// Plays a recorded flight through the cockpit, to try it on the ground.
    func startReplay(igcText: String, speed: Double) {
        guard mode == nil else { return }
        do {
            replay = try Replay(igcText: igcText)
        } catch {
            problem = error.words
            return
        }
        begin(FlightSession(dataFolder: dataFolder.path), mode: .replay(speed: speed))
    }

    /// The airspace on the line ahead, seen from the side: on the glider's
    /// course on a glide, towards the place the cockpit navigates to while
    /// circling. `nil` without airspace, and without a position and a
    /// height.
    func airspaceAhead() -> AirspaceAhead? {
        session?.airspaceAhead(nowUnixS: replay?.now() ?? Date().timeIntervalSince1970)
    }

    /// Changes how fast a replay runs: 1 is the flight's own speed, and 0
    /// holds it where it is. Does nothing while a flight is recorded.
    func setReplaySpeed(_ speed: Double) {
        guard case .replay = mode, speed.isFinite, speed >= 0 else { return }
        mode = .replay(speed: speed)
    }

    /// Chooses the place to navigate to; `nil` goes back to the task, or
    /// without one to the take-off.
    func goTo(_ place: TargetPlace?) {
        guard let session else { return }
        session.goTo(place: place)
        chosenPlace = place?.name
        refresh(elapsed: 0)
    }

    /// Moves the task on to its next turnpoint, or back to the one before.
    func stepTask(forward: Bool) {
        guard let session else { return }
        session.stepTask(forward: forward)
        refresh(elapsed: 0)
    }

    /// "I have seen it": the warning stays on screen and is quiet for ten
    /// minutes, unless it becomes more urgent.
    func acknowledgeAirspace(_ id: String) {
        session?.acknowledgeAirspace(id: id, minutes: 10)
        airspaceWarnings = session?.airspaceWarnings() ?? []
    }

    /// "This airspace does not apply to me today": quiet until midnight.
    func ignoreAirspaceToday(_ id: String) {
        session?.ignoreAirspaceToday(id: id, utcOffsetS: Int32(TimeZone.current.secondsFromGMT()))
        airspaceWarnings = session?.airspaceWarnings() ?? []
    }

    func unsilenceAirspace(_ id: String) {
        session?.unsilenceAirspace(id: id)
        airspaceWarnings = session?.airspaceWarnings() ?? []
    }

    /// Ends the session. For a live flight, returns the recording to put
    /// into the logbook, if any position was recorded.
    func finish() -> PendingRecording? {
        guard let session else { return nil }
        ticker?.cancel()
        ticker = nil
        location.stop()
        barometer.stop()
        vario.onBytes = nil
        sound.stop()
        voice.stop()
        UIApplication.shared.isIdleTimerDisabled = false
        if let before = brightnessBefore {
            Self.screen?.brightness = before
            brightnessBefore = nil
        }

        var recording: PendingRecording?
        do {
            recording = try session.stopRecording()
        } catch {
            // The file is still in the data folder and is picked up at the
            // next launch.
            problem = error.words
        }
        if mode == .live {
            sayLanded(session)
        }
        self.session = nil
        replay = nil
        mode = nil
        live = nil
        landing = nil
        range = nil
        gliders = []
        ticksSinceOthers = Self.ticksBetweenOthers
        windBelow = nil
        windBelowCredits = []
        isWindWarningPutAway = false
        warnedOfWindAt = nil
        view = nil
        way = nil
        score = nil
        faiAreas = nil
        daylight = nil
        task = nil
        chosenPlace = nil
        airspaceWarnings = []
        airspaceCheck = .noAirspace
        handedAirspace = nil
        return recording
    }

    /// The warning of a strong wind where the pilot means to land, unless
    /// the pilot has put it away.
    var windWarning: GroundWindAlert? {
        isWindWarningPutAway ? nil : windBelow?.alert
    }

    /// Puts the warning away. It comes back for another station, and for
    /// this one after its wind has dropped and risen again.
    func putWindWarningAway() {
        isWindWarningPutAway = true
    }

    /// Where the pilot means to land, as far as the cockpit knows: the
    /// place it navigates to, unless that is the take-off, else the
    /// landing it names as the one to think of first.
    private func placeToLand(from view: CockpitView) -> GroundWindPlace? {
        if let way, !way.isTakeoff {
            let elevation = view.altitudeM.flatMap { altitude in way.heightAboveM.map { altitude - $0 } }
            return GroundWindPlace(name: way.name, position: way.position, elevationM: elevation)
        }
        let places = landings()
        if let landing, places.indices.contains(Int(landing.index)) {
            let place = places[Int(landing.index)]
            return GroundWindPlace(name: place.name, position: place.position, elevationM: place.elevationM)
        }
        return nil
    }

    /// Takes the wind on the ground as it was just worked out. A warning
    /// is said aloud when it comes up, and again only for another station
    /// or after it has been gone.
    private func take(_ wind: GroundWind, from credits: [Credit]) {
        if wind != windBelow {
            windBelow = wind
        }
        if credits != windBelowCredits {
            windBelowCredits = credits
        }
        guard let alert = wind.alert else {
            warnedOfWindAt = nil
            return
        }
        if alert.stationId != warnedOfWindAt {
            warnedOfWindAt = alert.stationId
            isWindWarningPutAway = false
            voice.sayAfter(alert.spoken)
        }
    }

    private func begin(_ session: FlightSession, mode: Mode) {
        self.session = session
        self.mode = mode
        handedAirspace = airspace()
        session.useAirspace(airspace: handedAirspace)
        // The ground, as far as the iPhone has it; more of it arrives
        // while the session runs.
        session.useTerrain(terrain: terrain())
        ticksSinceTerrain = Self.ticksBetweenTerrain
        // The wind on the ground is asked for as soon as the glider flies.
        ticksSinceWindBelow = Self.ticksBetweenWindBelow
        isAskingForWindBelow = false
        // How the pilot's glider glides, for predicting arrival heights.
        // Unset values are zero, which the core ignores.
        let defaults = UserDefaults.standard
        session.setGlider(
            trimSpeedMs: defaults.double(forKey: Preferences.trimSpeed) / 3.6,
            glideRatio: defaults.double(forKey: Preferences.glideRatio)
        )
        // The task of the day, on the day of the flight: a replay's day is
        // the day it was flown.
        do {
            try session.flyTask(
                fileText: taskFile()?.text, nowUnixS: replay?.now() ?? Date().timeIntervalSince1970
            )
        } catch {
            problem = error.words
        }
        sound.isMuted = isMuted
        // A cockpit that dims and locks is no use, and one that cannot be
        // read in the sun is not much more: the screen goes all the way
        // up, unless the pilot would rather keep the battery.
        UIApplication.shared.isIdleTimerDisabled = true
        if !LaunchOptions.isUnderTest, !UserDefaults.standard.bool(forKey: Preferences.cockpitKeepsBrightness),
           let screen = Self.screen
        {
            brightnessBefore = screen.brightness
            screen.brightness = 1
        }
        if !LaunchOptions.isUnderTest {
            sound.start()
        }
        refresh(elapsed: 0)

        ticker = Task { [weak self] in
            let clock = ContinuousClock()
            var last = clock.now
            while !Task.isCancelled {
                try? await Task.sleep(for: Self.refreshInterval)
                let now = clock.now
                let elapsed = now - last
                last = now
                self?.refresh(elapsed: Double(elapsed.components.seconds) + Double(elapsed.components.attoseconds) / 1e18)
            }
        }
    }

    private func refresh(elapsed: Double) {
        guard let session else { return }
        let now: Double
        if let replay, case .replay(let speed) = mode {
            _ = replay.advance(session: session, seconds: min(elapsed, Self.replayStepAtMostS) * speed)
            now = replay.now()
        } else {
            now = Date().timeIntervalSince1970
        }
        let view = session.view(nowUnixS: now)
        // A flight that records itself also ends by itself: some minutes
        // after the landing, or when nobody took off.
        if let over = view.over, mode == .live {
            endByItself(over)
            return
        }
        self.view = view
        way = session.navigation(nowUnixS: now)
        sound.play(view.tone)

        let status = session.taskStatus(nowUnixS: now)
        if status != task {
            task = status
        }
        // A flight shared with others: what is due goes out, and the
        // cockpit shows how it stands.
        if mode == .live {
            sendLive(from: session, now: now)
            let shared = session.liveShare(nowUnixS: now)
            if shared != live {
                live = shared
            }
        }
        // What happens in the task is said once: the start taken, a
        // turnpoint reached, the goal. Tests have nobody listening.
        while let event = session.takeTaskEvent() {
            if !LaunchOptions.isUnderTest {
                voice.say(event.words)
            }
        }

        // The ground around the glider: what is not there yet is brought.
        ticksSinceTerrain += 1
        if ticksSinceTerrain >= Self.ticksBetweenTerrain, let position = view.position {
            ticksSinceTerrain = 0
            wantTerrain(position)
        }

        // Who else is in the air around the glider, in flight only: on
        // the ground the map tab shows them.
        ticksSinceOthers += 1
        if ticksSinceOthers >= Self.ticksBetweenOthers, !isAskingForOthers, view.phase == .flying,
           let position = view.position
        {
            ticksSinceOthers = 0
            isAskingForOthers = true
            // The pilot's own glider, if the flight is shown to everyone,
            // is on the server's list too: it is not another pilot.
            let own = live?.link.split(separator: "/").last.map(String.init)
            Task { [weak self] in
                guard let self else { return }
                let found = await self.others(position)
                self.isAskingForOthers = false
                if let found, self.mode != nil {
                    let others = found.filter { $0.id != own }
                    if others != self.gliders {
                        self.gliders = others
                    }
                }
            }
        }

        // The wind on the ground, in flight only: around the glider, and
        // where the pilot means to land.
        ticksSinceWindBelow += 1
        if ticksSinceWindBelow >= Self.ticksBetweenWindBelow, !isAskingForWindBelow, view.phase == .flying,
           let position = view.position
        {
            ticksSinceWindBelow = 0
            isAskingForWindBelow = true
            let place = placeToLand(from: view)
            let trimSpeedMs = UserDefaults.standard.double(forKey: Preferences.trimSpeed) / 3.6
            Task { [weak self] in
                guard let self else { return }
                let found = await self.windOnTheGround(position, place?.position)
                self.isAskingForWindBelow = false
                guard let found, self.mode != nil else { return }
                let wind = groundWind(
                    stations: found.stations, position: position, landing: place, trimSpeedMs: trimSpeedMs
                )
                self.take(wind, from: found.credits)
            }
        }

        // The core works the score out anew every half minute of flight;
        // asking every few seconds is enough to show it promptly.
        ticksSinceScore += 1
        if ticksSinceScore >= 10 {
            ticksSinceScore = 0
            let latest = session.liveScore()
            if latest != score {
                score = latest
            }
            let areas = session.faiAreas()
            if areas != faiAreas {
                faiAreas = areas
            }
            let light = session.daylight(nowUnixS: now)
            if light != daylight {
                daylight = light
            }
            // Which of the pilot's landings the glider could go to now.
            let option = session.landingOption(landings: landings().map(\.place), nowUnixS: now)
            if option != landing {
                landing = option
            }
            // And how far it glides, every way, over the ground as it is.
            let reach = session.glideRange(nowUnixS: now)
            if reach != range {
                range = reach
            }
        }

        // Files brought in or removed in the middle of a session reach the
        // core at once; what the pilot silenced stays silenced.
        let latest = airspace()
        if latest !== handedAirspace {
            handedAirspace = latest
            session.useAirspace(airspace: latest)
        }
        let check = session.airspaceCheck(nowUnixS: now)
        if check != airspaceCheck {
            airspaceCheck = check
        }

        let warnings = session.airspaceWarnings()
        if warnings != airspaceWarnings {
            airspaceWarnings = warnings
        }
        // A warning that is new, or more urgent than it was, is said aloud
        // once. Tests have nobody listening.
        if session.takeAirspaceAlert() != nil, !LaunchOptions.isUnderTest,
           let loudest = warnings.first(where: { !$0.silenced })
        {
            voice.say("Airspace. \(loudest.headline).")
        }
    }
}
