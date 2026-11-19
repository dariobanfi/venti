import Foundation
import Observation
import UIKit
import VentiCore

/// One flying session as the cockpit screen sees it.
///
/// It owns the phone's sensors and passes what they report to the core's
/// session; five times a second it asks the core what to show and what the
/// vario should sound like. It contains no flying logic.
///
/// A session is either live, fed by the sensors and recorded, or a replay
/// of a recorded flight, fed by the core and not recorded.
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
    /// The airspace warnings of the moment, the most urgent first.
    private(set) var airspaceWarnings: [AirspaceWarning] = []
    /// The airspace to warn about, asked for when a session starts. The
    /// app sets this to the pilot's airspace files.
    @ObservationIgnored var airspace: () -> AirspaceSet? = { nil }
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
    /// Refreshes of the screen since the score was last asked for.
    @ObservationIgnored private var ticksSinceScore = 0

    /// How often the screen and the vario tone are refreshed.
    private static let refreshInterval = Duration.milliseconds(200)

    init(dataFolder: URL) {
        self.dataFolder = dataFolder
    }

    /// Switches the sensors on and starts recording.
    func startFlight() {
        guard mode == nil else { return }
        let session = FlightSession(dataFolder: dataFolder.path)
        do {
            let defaults = UserDefaults.standard
            try session.startRecording(
                pilot: defaults.string(forKey: Preferences.pilot),
                glider: defaults.string(forKey: Preferences.glider)
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

    /// Chooses the place to navigate to; `nil` goes back to the take-off.
    func goTo(_ place: TargetPlace?) {
        guard let session else { return }
        session.goTo(place: place)
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

        var recording: PendingRecording?
        do {
            recording = try session.stopRecording()
        } catch {
            // The file is still in the data folder and is picked up at the
            // next launch.
            problem = error.words
        }
        self.session = nil
        replay = nil
        mode = nil
        view = nil
        way = nil
        score = nil
        airspaceWarnings = []
        return recording
    }

    private func begin(_ session: FlightSession, mode: Mode) {
        self.session = session
        self.mode = mode
        session.useAirspace(airspace: airspace())
        // How the pilot's glider glides, for predicting arrival heights.
        // Unset values are zero, which the core ignores.
        let defaults = UserDefaults.standard
        session.setGlider(
            trimSpeedMs: defaults.double(forKey: Preferences.trimSpeed) / 3.6,
            glideRatio: defaults.double(forKey: Preferences.glideRatio)
        )
        sound.isMuted = isMuted
        // A cockpit that dims and locks is no use.
        UIApplication.shared.isIdleTimerDisabled = true
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
            _ = replay.advance(session: session, seconds: elapsed * speed)
            now = replay.now()
        } else {
            now = Date().timeIntervalSince1970
        }
        let view = session.view(nowUnixS: now)
        self.view = view
        way = session.navigation(nowUnixS: now)
        sound.play(view.tone)

        // The core works the score out anew every half minute of flight;
        // asking every few seconds is enough to show it promptly.
        ticksSinceScore += 1
        if ticksSinceScore >= 10 {
            ticksSinceScore = 0
            let latest = session.liveScore()
            if latest != score {
                score = latest
            }
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
