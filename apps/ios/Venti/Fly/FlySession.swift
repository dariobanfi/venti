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
    /// Whether the live session runs only to notice a take-off, with the
    /// cockpit shut: on any tab, while the app is in front. The core's
    /// own take-off detection decides; once it has seen one, the session
    /// is the flight's and the cockpit opens (`onTakeoffSeen`).
    private(set) var isWatching = false
    /// Whether the cockpit is open: a session runs and does more than
    /// watch for a take-off.
    var isCockpitOpen: Bool {
        mode != nil && !isWatching
    }
    /// Called when a take-off was seen while the session only watched:
    /// the app brings the Fly tab, and with it the cockpit, to the front.
    @ObservationIgnored var onTakeoffSeen: @MainActor () -> Void = {}
    /// Called when the core took back a take-off that had opened the
    /// cockpit by itself (a cable car, a drive): the cockpit has shut and
    /// the session watches again; the app goes back to the tab the pilot
    /// was on.
    @ObservationIgnored var onTakenBack: @MainActor () -> Void = {}
    /// Whether the cockpit was opened by a take-off seen on any tab, and
    /// not by the pilot: a take-off taken back then shuts it again.
    @ObservationIgnored private var openedForTakeoff = false
    /// What the cockpit shows. Present while a session runs.
    private(set) var view: CockpitView?
    /// Whether a flight is being recorded: `view.recording`, kept apart
    /// so that a screen that needs only this is not drawn anew with every
    /// refresh (a menu drawn anew five times a second never comes to rest).
    private(set) var isRecording = false
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
    /// The cable ahead, by the core's rule, while there is one.
    private(set) var cableWarning: CableWarning?
    /// The cables to warn of, asked for when a session starts and handed
    /// on whenever they change. The app sets this.
    @ObservationIgnored var cables: () -> CableSet? = { nil }
    /// The other networks' aircraft around the glider, with their tracks,
    /// as the core's session keeps them.
    private(set) var othersNear: OthersNear?
    /// Asks the server for the other networks' aircraft around a place.
    /// The app sets this; `nil` when there is no answer.
    @ObservationIgnored var otherNetworks: (Coordinate) async -> LiveOthers? = { _ in nil }
    /// Where others climb near the glider, as the core's session ages
    /// them. Absent while the pilot has them switched off, and while
    /// there is none.
    private(set) var climbsNear: ClimbsNear?
    /// Asks the server for the climbs of the last minutes around a place.
    /// The app sets this; `nil` when there is no answer.
    @ObservationIgnored var climbsAround: (Coordinate) async -> LiveClimbs? = { _ in nil }
    /// Whether the pilot has "Climbs of others in the cockpit" switched
    /// on: off until then, and off nothing is asked or drawn.
    @ObservationIgnored var showsClimbs: () -> Bool = {
        UserDefaults.standard.bool(forKey: Preferences.cockpitShowsClimbs)
    }
    /// The take-offs around the glider, for the cockpit's map.
    private(set) var takeoffsNear: CockpitTakeoffs?
    /// Works out the take-offs around the glider from what the app holds.
    /// The app sets this.
    @ObservationIgnored var takeoffs: (Coordinate) -> CockpitTakeoffs? = { _ in nil }
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
    /// The landing fields around the glider, as the core lists them: the
    /// core keeps a standing reading of the nearest one within glide. The
    /// app sets this to the core's `cockpitLandings`.
    @ObservationIgnored var cockpitLandings: (Coordinate) -> [TargetPlace] = { _ in [] }
    /// The landings last handed to the core's session.
    @ObservationIgnored private var handedLandings: [TargetPlace]?
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
    /// Something said quietly in the cockpit, put away with a tap: the
    /// core's words for a take-off it took back.
    var quietNotice: String?

    let vario = VarioLink()

    @ObservationIgnored private let dataFolder: URL
    @ObservationIgnored private var session: FlightSession?
    @ObservationIgnored private var replay: Replay?
    /// The moment of the replayed flight at which it holds by itself, in
    /// seconds since 1970; `nil` once it has, and for a replay that runs on.
    @ObservationIgnored private var replayHoldsAt: Double?
    @ObservationIgnored private var ticker: Task<Void, Never>?
    @ObservationIgnored private let location = LocationProvider()
    @ObservationIgnored private let barometer = BarometerProvider()
    @ObservationIgnored private let sound = VarioSound()
    @ObservationIgnored private let voice = AirspaceVoice()
    /// The airspace last handed to the core's session.
    @ObservationIgnored private var handedAirspace: AirspaceSet?
    /// The cables last handed to it.
    @ObservationIgnored private var handedCables: CableSet?
    /// Refreshes since the other networks were asked for, and whether an
    /// answer is still awaited.
    @ObservationIgnored private var ticksSinceNetworks = 0
    @ObservationIgnored private var isAskingForNetworks = false
    /// Refreshes since the climbs of others were asked for, whether an
    /// answer is still awaited, and whether the core's session was handed
    /// one in this flight.
    @ObservationIgnored private var ticksSinceClimbs = 0
    @ObservationIgnored private var isAskingForClimbs = false
    @ObservationIgnored private var hasHandedClimbs = false
    /// Refreshes since the take-offs around were worked out.
    @ObservationIgnored private var ticksSinceTakeoffs = 0
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

    /// The climbs of others around the glider are asked for this many
    /// refreshes apart: every minute, while the pilot has them on.
    static let ticksBetweenClimbs = 300

    /// The take-offs around the glider are worked out again this many
    /// refreshes apart: every half minute.
    private static let ticksBetweenTakeoffs = 150

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

    /// Opens the cockpit: switches the sensors on and makes the session
    /// ready, waiting on the ground and recording from the take-off. A
    /// session that was only watching for the take-off becomes the
    /// cockpit's as it is.
    func startFlight() {
        if mode == .live, isWatching {
            isWatching = false
            openedForTakeoff = false
            openTheCockpit()
            share(with: LiveChoice.stored.audience)
            return
        }
        guard mode == nil else { return }
        startLive(watching: false)
    }

    /// Watches for a take-off with the cockpit shut, while the app is in
    /// front on any tab: the same live session the cockpit opens, fed by
    /// the position and the pressure, whose take-off the core notices.
    /// Only where the pilot has allowed the position already: nothing is
    /// asked for here.
    func watchForTakeoff() {
        guard mode == nil, LocationProvider.isAllowed else { return }
        startLive(watching: true)
    }

    /// Stops watching for a take-off, as the app goes to the back. A
    /// session that has seen one is the flight's and goes on.
    func stopWatching() {
        guard isWatching, view?.recording != true else { return }
        _ = finish()
    }

    private func startLive(watching: Bool) {
        let session = FlightSession(dataFolder: dataFolder.path)
        do {
            let defaults = UserDefaults.standard
            try session.recordFromTakeoff(
                pilot: defaults.string(forKey: Preferences.pilot),
                glider: defaults.string(forKey: Preferences.glider),
                nowUnixS: Date().timeIntervalSince1970
            )
        } catch {
            if !watching {
                problem = error.words
            }
            return
        }

        location.onSample = { sample in session.onLocation(sample: sample) }
        // What went wrong with the position is the cockpit's to say; a
        // watch says nothing on the other tabs.
        location.onProblem = { [weak self] problem in
            if self?.isWatching == false {
                self?.problem = problem
            }
        }
        barometer.onPressure = { time, pressure in session.onPressure(timeUnixS: time, pressureHpa: pressure) }
        vario.onBytes = { bytes in
            session.onInstrumentBytes(timeUnixS: Date().timeIntervalSince1970, bytes: bytes)
        }
        // The pilot's choice of vario, and what its link does: the core
        // decides from them whether the phone sounds and what the vario's
        // mark says. The choice first, then the link as it stands.
        session.setVarioChoice(choice: vario.choice)
        vario.onChoice = { choice in session.setVarioChoice(choice: choice) }
        vario.onLink = { state in
            session.onVarioLink(timeUnixS: Date().timeIntervalSince1970, state: state)
        }
        location.start()
        // The pressure sensor asks the pilot first on an iPhone: a watch
        // uses it only where that was answered already.
        if !watching || BarometerProvider.isAllowed {
            barometer.start()
        }
        vario.reconnectToRememberedVario()

        isWatching = watching
        begin(session, mode: .live)
        if !watching {
            // Shown to others from take-off, if the pilot chose so.
            share(with: LiveChoice.stored.audience)
        }
    }

    /// What the cockpit does to the iPhone while it is open: the screen
    /// stays on and as bright as it goes, and the vario sounds.
    private func openTheCockpit() {
        // A cockpit that dims and locks is no use, and one that cannot be
        // read in the sun is not much more: the screen goes all the way
        // up, unless the pilot would rather keep the battery.
        UIApplication.shared.isIdleTimerDisabled = true
        if !LaunchOptions.isUnderTest, !UserDefaults.standard.bool(forKey: Preferences.cockpitKeepsBrightness),
           brightnessBefore == nil, let screen = Self.screen
        {
            brightnessBefore = screen.brightness
            screen.brightness = 1
        }
        if !LaunchOptions.isUnderTest {
            sound.start()
        }
    }

    /// Gives the iPhone back what the cockpit took: the screen may dim and
    /// lock again, at its old brightness, and the vario is silent.
    private func closeTheCockpit() {
        sound.stop()
        UIApplication.shared.isIdleTimerDisabled = false
        if let before = brightnessBefore {
            Self.screen?.brightness = before
            brightnessBefore = nil
        }
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

    /// Plays a recorded flight through the cockpit, to try it on the ground:
    /// from `fromS` seconds into it, the seconds before fed to the session
    /// at once, and held by itself `holdingAfterS` seconds of the flight
    /// later where that is given (`LaunchOptions.replayFrom`, for the UI
    /// tests).
    func startReplay(igcText: String, speed: Double, fromS: Double = 0, holdingAfterS: Double? = nil) {
        // A session waiting on the ground, watched for or open, gives way:
        // nothing of it was recorded. A flight that is on does not.
        if mode == .live, view?.recording != true {
            _ = finish()
        }
        guard mode == nil else { return }
        do {
            replay = try Replay(igcText: igcText)
        } catch {
            problem = error.words
            return
        }
        replayHoldsAt = nil
        begin(FlightSession(dataFolder: dataFolder.path), mode: .replay(speed: speed))
        if let replay, let session {
            if fromS > 0 {
                _ = replay.advance(session: session, seconds: fromS)
            }
            if let holdingAfterS {
                replayHoldsAt = replay.now() + holdingAfterS
            }
            refresh(elapsed: 0)
        }
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

    /// "I have seen it" for the cable ahead: quiet for ten minutes, as
    /// the airspace warning's "Seen" is.
    func acknowledgeCable(_ id: String) {
        session?.acknowledgeCable(id: id, minutes: 10)
        cableWarning = session?.cableWarning()
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
        vario.onLink = nil
        vario.onChoice = nil
        voice.stop()
        closeTheCockpit()

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
        isWatching = false
        openedForTakeoff = false
        quietNotice = nil
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
        isRecording = false
        way = nil
        score = nil
        faiAreas = nil
        daylight = nil
        task = nil
        chosenPlace = nil
        airspaceWarnings = []
        airspaceCheck = .noAirspace
        handedAirspace = nil
        handedCables = nil
        cableWarning = nil
        othersNear = nil
        climbsNear = nil
        hasHandedClimbs = false
        ticksSinceClimbs = Self.ticksBetweenClimbs
        isAskingForClimbs = false
        takeoffsNear = nil
        ticksSinceNetworks = Self.ticksBetweenOthers
        isAskingForNetworks = false
        ticksSinceTakeoffs = Self.ticksBetweenTakeoffs
        return recording
    }

    /// The warning of a strong wind where the pilot means to land, unless
    /// the pilot has put it away.
    var windWarning: GroundWindAlert? {
        isWindWarningPutAway ? nil : windBelow?.alert
    }

    /// Hands the session the forecast for the glider's place, each time it
    /// is fetched: the thermal view's wind (`CockpitView.windShown`) comes
    /// from it while no station speaks for the glider's air and the flight
    /// has not measured one. Which hour and level is the core's.
    func useForecast(_ forecast: ForecastView?) {
        session?.useForecast(forecast: forecast)
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

    /// Hands the core's session the landing fields it keeps a standing
    /// reading of. Which they are is the core's (`cockpitLandings`); here
    /// they are only handed over, and again only when they have changed,
    /// since the session begins its reading anew with each.
    private func handLandings(to session: FlightSession, around glider: Coordinate) {
        let places = cockpitLandings(glider)
        if places != handedLandings {
            handedLandings = places
            session.useLandings(landings: places)
        }
    }

    private func begin(_ session: FlightSession, mode: Mode) {
        self.session = session
        self.mode = mode
        handedAirspace = airspace()
        session.useAirspace(airspace: handedAirspace)
        handedCables = cables()
        session.useCables(cables: handedCables)
        ticksSinceNetworks = Self.ticksBetweenOthers
        // The climbs of others are asked for as soon as the glider flies,
        // where the pilot has them on.
        ticksSinceClimbs = Self.ticksBetweenClimbs
        isAskingForClimbs = false
        hasHandedClimbs = false
        ticksSinceTakeoffs = Self.ticksBetweenTakeoffs
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
        // Only for a pilot who set a class: the air's own rise and fall
        // and a speed to fly. And hike and fly, which never lands.
        session.setGliderClass(class: GliderChoice.kind(defaults.string(forKey: Preferences.gliderClass)))
        session.setNeverLand(neverLand: defaults.bool(forKey: Preferences.neverLand))
        handedLandings = nil
        // The task of the day, on the day of the flight: a replay's day is
        // the day it was flown.
        do {
            try session.flyTask(
                fileText: taskFile()?.text, nowUnixS: replay?.now() ?? Date().timeIntervalSince1970
            )
        } catch {
            problem = error.words
        }
        // A watch leaves the screen and the sound alone until it has
        // seen the take-off.
        if !isWatching {
            openTheCockpit()
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
            var step = min(elapsed, Self.replayStepAtMostS) * speed
            if let hold = replayHoldsAt {
                step = min(step, max(0, hold - replay.now()))
            }
            _ = replay.advance(session: session, seconds: step)
            if let hold = replayHoldsAt, replay.now() >= hold {
                replayHoldsAt = nil
                mode = .replay(speed: 0)
            }
            now = replay.now()
        } else {
            now = Date().timeIntervalSince1970
        }
        let view = session.view(nowUnixS: now)
        // A take-off the core took back (a cable car, a drive): the session
        // is on the ground again. What was shared of it is taken off the
        // server with its recording. A cockpit the take-off opened by
        // itself shuts quietly and the session watches again; one the
        // pilot opened says it quietly and waits for the take-off.
        if mode == .live, let falseStart = session.takeFalseStart() {
            if falseStart.recordingDropped, live != nil {
                deleteLiveTrack()
            }
            if openedForTakeoff, falseStart.recordingDropped {
                openedForTakeoff = false
                isWatching = true
                closeTheCockpit()
                onTakenBack()
            } else {
                quietNotice = falseStart.words
            }
        }
        if isWatching {
            // Nobody took off while the app was in front for two hours:
            // the watch begins anew, without a word.
            if view.over != nil {
                _ = finish()
                watchForTakeoff()
                return
            }
            // The core has seen the take-off and records: the session is
            // the flight's, and the cockpit opens over whatever tab is in
            // front.
            if view.recording || view.phase == .flying {
                isWatching = false
                openedForTakeoff = true
                openTheCockpit()
                share(with: LiveChoice.stored.audience)
                onTakeoffSeen()
            }
        }
        // A flight that records itself also ends by itself: some minutes
        // after the landing, or when nobody took off.
        if let over = view.over, mode == .live {
            endByItself(over)
            return
        }
        self.view = view
        if view.recording != isRecording {
            isRecording = view.recording
        }
        way = session.navigation(nowUnixS: now)
        // The core decides when the phone sounds (`phoneTone`): in flight,
        // and not while a connected vario beeps itself. Its tone is absent
        // otherwise; there is no mute of the app's own.
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
            if !LaunchOptions.isUnderTest, !isWatching {
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

        // The other networks' aircraft, in flight only, as often as
        // Venti's own pilots: the core keeps their tracks.
        ticksSinceNetworks += 1
        if ticksSinceNetworks >= Self.ticksBetweenOthers, !isAskingForNetworks, view.phase == .flying,
           let position = view.position
        {
            ticksSinceNetworks = 0
            isAskingForNetworks = true
            Task { [weak self] in
                guard let self else { return }
                let found = await self.otherNetworks(position)
                self.isAskingForNetworks = false
                guard let found, self.mode != nil, let session = self.session else { return }
                session.onOthers(others: found, nowUnixS: self.replay?.now() ?? Date().timeIntervalSince1970)
            }
        }
        // The climbs of others around the glider: only where the pilot
        // switched them on, in flight only, once a minute. Off, nothing
        // is asked and nothing is kept to draw.
        let wantsClimbs = showsClimbs()
        if !wantsClimbs {
            if climbsNear != nil {
                climbsNear = nil
            }
        } else {
            ticksSinceClimbs += 1
            if ticksSinceClimbs >= Self.ticksBetweenClimbs, !isAskingForClimbs, view.phase == .flying,
               let position = view.position
            {
                ticksSinceClimbs = 0
                isAskingForClimbs = true
                Task { [weak self] in
                    guard let self else { return }
                    let found = await self.climbsAround(position)
                    self.isAskingForClimbs = false
                    guard let found, self.mode != nil, let session = self.session else { return }
                    session.onClimbs(climbs: found, nowUnixS: self.replay?.now() ?? Date().timeIntervalSince1970)
                    self.hasHandedClimbs = true
                }
            }
        }
        // The take-offs around the glider, whenever it has a position.
        ticksSinceTakeoffs += 1
        if ticksSinceTakeoffs >= Self.ticksBetweenTakeoffs, let position = view.position {
            ticksSinceTakeoffs = 0
            let found = takeoffs(position)
            if found != takeoffsNear {
                takeoffsNear = found
            }
            handLandings(to: session, around: position)
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
                // The thermal view's wind takes a station's until the
                // flight has measured one: the core's rule, handed the
                // stations each time they are picked.
                self.session?.useGroundWind(stations: wind.stations)
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
            // The other networks' aircraft near, aged to now.
            let near = session.othersNear(nowUnixS: now)
            let shownNear = near.aircraft.isEmpty ? nil : near
            if shownNear != othersNear {
                othersNear = shownNear
            }
            // The climbs of others near, aged to now by the core: read
            // every two seconds, and only once an answer was handed over.
            if wantsClimbs, hasHandedClimbs {
                let climbing = session.climbsNear(nowUnixS: now)
                let shownClimbs = climbing.climbs.isEmpty ? nil : climbing
                if shownClimbs != climbsNear {
                    climbsNear = shownClimbs
                }
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
        let latestCables = cables()
        if latestCables !== handedCables {
            handedCables = latestCables
            session.useCables(cables: latestCables)
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
        // A watch on another tab says nothing aloud.
        if session.takeAirspaceAlert() != nil, !LaunchOptions.isUnderTest, !isWatching,
           let loudest = warnings.first(where: { !$0.silenced })
        {
            voice.say("Airspace. \(loudest.headline).")
        }
        // The cable ahead, sounded once as a new airspace warning is.
        let cable = session.cableWarning()
        if cable != cableWarning {
            cableWarning = cable
        }
        if session.takeCableAlert(), !LaunchOptions.isUnderTest, !isWatching, let cable {
            voice.say(cable.words)
        }
        // The vario the pilot chose, lost in the air, is said aloud once,
        // when the core says so; the mark on the top line says it for as
        // long as it lasts, and the phone sounds meanwhile.
        if mode == .live, session.takeVarioLost(nowUnixS: now), !isWatching, !LaunchOptions.isUnderTest {
            voice.say("Vario lost.")
        }
    }
}
